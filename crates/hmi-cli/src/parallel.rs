//! 有界任务调度。执行顺序可以交错，结果始终按工程清单顺序组装。
use super::{AssetInput, AssetOrigin, Prepared, cache::Cache, describe_project};
use hmi_core::{Result, error, file, model::*, report::Report, target::Target};
use hmi_page_compiler::Compilation;
use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Condvar, Mutex},
};

#[derive(Clone, Copy)]
enum Kind {
    Program,
    Page,
    Asset,
    Builtin(hmi_core::builtin::BuiltinImage),
}
struct Job {
    kind: Kind,
    path: PathBuf,
    name: String,
    key: (u8, PathBuf),
}
enum Product {
    Compiled(Box<Compilation>),
    Encoded(Box<hmi_media::EncodingResult>),
}
impl Job {
    fn stage(&self) -> &'static str {
        if matches!(self.kind, Kind::Program | Kind::Page) {
            "compile"
        } else {
            "media"
        }
    }
    fn media(&self) -> bool {
        self.stage() == "media"
    }
    fn run(
        &self,
        cache: &mut Cache,
        target: &Target,
        original: Option<&hmi_artifact::original::OriginalBundle>,
        original_path: Option<&Path>,
    ) -> Result<Product> {
        match self.kind {
            Kind::Program => cache
                .compile(&self.path, target, None, None, true)
                .map(|p| Product::Compiled(Box::new(p))),
            Kind::Page => cache
                .compile(&self.path, target, original, original_path, false)
                .map(|p| Product::Compiled(Box::new(p))),
            Kind::Asset => cache
                .encode(&self.path, target, &self.name)
                .map(|p| Product::Encoded(Box::new(p))),
            Kind::Builtin(id) => cache
                .encode_builtin(id, target)
                .map(|p| Product::Encoded(Box::new(p))),
        }
        .map_err(|e| e.with_stage(self.stage()))
    }
}
struct QueueState {
    pending: VecDeque<usize>,
    active: BTreeSet<(u8, PathBuf)>,
    media: usize,
    stopped: bool,
}
struct Queue<'a> {
    jobs: &'a [Job],
    media_limit: usize,
    state: Mutex<QueueState>,
    ready: Condvar,
}
impl<'a> Queue<'a> {
    fn new(jobs: &'a [Job], media_limit: usize) -> Self {
        Self {
            jobs,
            media_limit,
            state: Mutex::new(QueueState {
                pending: (0..jobs.len()).collect(),
                active: BTreeSet::new(),
                media: 0,
                stopped: false,
            }),
            ready: Condvar::new(),
        }
    }
    fn next(&self) -> Option<usize> {
        let mut state = self.state.lock().unwrap();
        loop {
            if state.stopped || state.pending.is_empty() {
                return None;
            }
            // 不让等待媒体名额或同一输入的任务占住工作线程，页面可继续运行。
            if let Some(position) = state.pending.iter().position(|&i| {
                let job = &self.jobs[i];
                !state.active.contains(&job.key) && (!job.media() || state.media < self.media_limit)
            }) {
                let index = state.pending.remove(position).unwrap();
                let job = &self.jobs[index];
                state.active.insert(job.key.clone());
                if job.media() {
                    state.media += 1;
                }
                return Some(index);
            }
            state = self.ready.wait(state).unwrap();
        }
    }
    fn finish(&self, index: usize, failed: bool) {
        let mut state = self.state.lock().unwrap();
        let job = &self.jobs[index];
        state.active.remove(&job.key);
        if job.media() {
            state.media -= 1;
        }
        state.stopped |= failed;
        self.ready.notify_all();
    }
}

fn execute(
    jobs: &[Job],
    cache: &mut Cache,
    target: &Target,
    original: Option<&hmi_artifact::original::OriginalBundle>,
    original_path: Option<&Path>,
    workers: usize,
) -> Result<Vec<Product>> {
    if jobs.is_empty() {
        return Ok(vec![]);
    }
    let queue = Queue::new(jobs, workers.min(2));
    let results = Mutex::new(
        (0..jobs.len())
            .map(|_| None)
            .collect::<Vec<Option<Result<Product>>>>(),
    );
    let forks = (0..workers.min(jobs.len()))
        .map(|_| cache.fork())
        .collect::<Vec<_>>();
    let completed = std::thread::scope(|scope| {
        let handles = forks
            .into_iter()
            .map(|mut worker| {
                let queue = &queue;
                let results = &results;
                scope.spawn(move || {
                    while let Some(index) = queue.next() {
                        let job = &jobs[index];
                        let outcome =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                job.run(&mut worker, target, original, original_path)
                            }))
                            .unwrap_or_else(|_| {
                                Err(error("worker_panic", "构建任务异常终止，输出文件未更新")
                                    .with_stage(job.stage()))
                            });
                        let failed = outcome.is_err();
                        results.lock().unwrap()[index] = Some(outcome);
                        queue.finish(index, failed);
                    }
                    worker
                })
            })
            .collect::<Vec<_>>();
        handles.into_iter().map(|h| h.join()).collect::<Vec<_>>()
    });
    let mut diagnostics = vec![];
    // 即使某个任务失败，其他线程已经验证的缓存也要登记并保留。
    for worker in completed {
        match worker {
            Ok(worker) => {
                if let Err(error) = cache.absorb(worker) {
                    diagnostics.extend(error.with_stage("cache").diagnostics());
                }
            }
            Err(_) => diagnostics.extend(
                error("worker_panic", "构建工作线程异常终止")
                    .with_stage("compile")
                    .diagnostics(),
            ),
        }
    }
    if let Err(error) = cache.flush() {
        diagnostics.extend(error.with_stage("cache").diagnostics());
    }
    let mut products = vec![];
    for result in results.into_inner().unwrap().into_iter().flatten() {
        match result {
            Ok(product) => products.push(product),
            Err(error) => diagnostics.extend(error.diagnostics()),
        }
    }
    if !diagnostics.is_empty() {
        return Err(hmi_core::Error::Diagnostics(diagnostics));
    }
    if products.len() != jobs.len() {
        return Err(error(
            "build_schedule",
            "构建任务未全部完成，输出文件未更新",
        ));
    }
    Ok(products)
}

pub fn prepare(
    root: &Path,
    cache: &mut Cache,
    report: &Report,
    workers: usize,
) -> Result<Prepared> {
    let manifest = root.join("hmi.toml");
    let project: ProjectSource = file::read_toml(&manifest)?;
    check_schema(&project.schema, "hmi.project/1")?;
    if project.name.trim().is_empty() {
        return Err(error("project_name", "工程名称不能为空"));
    }
    if project.pages.sources.is_empty() {
        return Err(error("page_count", "工程至少需要一个页面"));
    }
    describe_project(root, &project, report)?;
    let target = Target::resolve(&project.target)?;
    let original_path = project.original.as_ref().map(|o| root.join(&o.bundle));
    let original = original_path
        .as_deref()
        .map(hmi_artifact::original::OriginalBundle::read)
        .transpose()?;
    let program_path = root.join(&project.program.source);
    let mut jobs = vec![Job {
        kind: Kind::Program,
        key: (
            0,
            fs::canonicalize(&program_path).map_err(|e| file::at_file(e.into(), &program_path))?,
        ),
        path: program_path,
        name: String::new(),
    }];
    let mut keys = BTreeSet::new();
    for asset in &project.assets.items {
        hmi_core::asset::check_key(&asset.key)?;
        if !keys.insert(&asset.key) {
            return Err(error("asset_key", format!("资产符号 {} 重复", asset.key)));
        }
        let path = root.join(&asset.source);
        let canonical = fs::canonicalize(&path).map_err(|e| file::at_file(e.into(), &path))?;
        jobs.push(Job {
            kind: Kind::Asset,
            key: (2, canonical),
            path,
            name: asset.key.clone(),
        });
    }
    let mut paths = BTreeSet::new();
    for source in &project.pages.sources {
        let path = root.join(source);
        let canonical = fs::canonicalize(&path).map_err(|e| file::at_file(e.into(), &path))?;
        if !paths.insert(canonical.clone()) {
            return Err(error("page_source", format!("页面输入 {source} 重复")));
        }
        jobs.push(Job {
            kind: Kind::Page,
            key: (1, canonical),
            path,
            name: String::new(),
        });
    }
    let mut products = execute(
        &jobs,
        cache,
        &target,
        original.as_ref(),
        original_path.as_deref(),
        workers,
    )?
    .into_iter();
    let Product::Compiled(program) = products.next().unwrap() else {
        unreachable!()
    };
    let mut assets = vec![];
    for asset in &project.assets.items {
        let Product::Encoded(encoded) = products.next().unwrap() else {
            unreachable!()
        };
        assets.push(AssetInput {
            key: asset.key.clone(),
            source: AssetOrigin::File(root.join(&asset.source)),
            probe: hmi_media::AssetProbe {
                action: encoded.action,
                info: encoded.object.metadata.info.clone(),
                inputs: encoded.inputs,
            },
            encoded: Some(encoded.object),
        });
    }
    let mut pages = vec![];
    for product in products {
        let Product::Compiled(page) = product else {
            unreachable!()
        };
        pages.push(*page);
    }
    let needed = pages
        .iter()
        .flat_map(|p| p.object.implicit_images())
        .collect::<BTreeSet<_>>();
    let builtins = hmi_core::builtin::BuiltinImage::ALL
        .into_iter()
        .filter(|b| needed.contains(b))
        .collect::<Vec<_>>();
    report.add_planned("media", builtins.len());
    let jobs = builtins
        .iter()
        .map(|&id| Job {
            kind: Kind::Builtin(id),
            key: (3, root.join(id.id())),
            path: root.join(id.id()),
            name: id.key().into(),
        })
        .collect::<Vec<_>>();
    for (id, product) in builtins.into_iter().zip(execute(
        &jobs,
        cache,
        &target,
        original.as_ref(),
        original_path.as_deref(),
        workers,
    )?) {
        let Product::Encoded(encoded) = product else {
            unreachable!()
        };
        assets.push(AssetInput {
            key: id.key().into(),
            source: AssetOrigin::Builtin(id),
            probe: hmi_media::AssetProbe {
                action: encoded.action,
                info: encoded.object.metadata.info.clone(),
                inputs: encoded.inputs,
            },
            encoded: Some(encoded.object),
        });
    }
    let mut inputs = vec![manifest];
    inputs.extend(program.inputs.iter().cloned());
    if let Some(path) = &original_path {
        inputs.push(path.clone());
    }
    for page in &pages {
        inputs.extend(page.inputs.iter().cloned());
    }
    for asset in &assets {
        inputs.extend(asset.probe.inputs.iter().cloned());
    }
    Ok(Prepared {
        original,
        original_path,
        root: root.into(),
        project,
        target,
        program: *program,
        pages,
        assets,
        inputs,
    })
}
