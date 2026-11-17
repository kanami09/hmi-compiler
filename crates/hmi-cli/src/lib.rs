//! 工程初始化、执行计划和完整构建。
//!
//! 编译和链接直接调用共享库；计划中的独立命令使用相同的输入与格式。

use hmi_artifact::asset::{AssetInfo, AssetObject as EncodedAsset};
use hmi_core::{
    Result, error, file,
    model::*,
    target::{DEFAULT_PROGRAM, Target, TargetConfig},
};
use hmi_page_compiler::{Compilation, compile_program};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

mod cache;
pub use cache::{BuildStats, clean};

pub const BUILD_DIRECTORY: &str = ".build/hmi-toolchain";

pub struct Prepared {
    pub original: Option<hmi_artifact::original::OriginalBundle>,
    pub original_path: Option<PathBuf>,
    pub root: PathBuf,
    pub project: ProjectSource,
    pub target: Target,
    pub program: Compilation,
    pub pages: Vec<Compilation>,
    pub inputs: Vec<PathBuf>,
    pub assets: Vec<AssetInput>,
}
pub struct AssetInput {
    pub key: String,
    pub source: AssetOrigin,
    pub probe: hmi_media::AssetProbe,
    encoded: Option<EncodedAsset>,
}
pub enum AssetOrigin {
    File(PathBuf),
    Builtin(hmi_core::builtin::BuiltinImage),
}

/// 显式目录直接使用；未指定时向上查找工程清单。
pub fn locate_root(directory: Option<&Path>) -> Result<PathBuf> {
    if let Some(directory) = directory {
        return fs::canonicalize(directory).map_err(|e| file::at_file(e.into(), directory));
    }
    let current = std::env::current_dir()?;
    current
        .ancestors()
        .find(|dir| dir.join("hmi.toml").is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| error("project_missing", "当前目录及其父目录中没有 hmi.toml"))
}

pub fn init(directory: &Path, config: &TargetConfig) -> Result<()> {
    Target::resolve(config)?;
    if directory.exists() && fs::read_dir(directory)?.next().is_some() {
        return Err(error(
            "init_directory",
            "工程目录必须不存在或为空，已有文件不会被覆盖",
        ));
    }
    let project = ProjectSource {
        original: None,
        schema: "hmi.project/1".into(),
        name: "Panel".into(),
        target: config.clone(),
        program: ProgramSource {
            source: "program.s".into(),
        },
        build: BuildConfig::default(),
        pages: PageSources {
            sources: vec!["pages/page0/page.toml".into()],
        },
        assets: AssetSources::default(),
    };
    let files = [
        ("hmi.toml", file::toml(&project)?),
        ("program.s", DEFAULT_PROGRAM.as_bytes().to_vec()),
        (
            "pages/page0/page.toml",
            file::toml(&hmi_page_compiler::empty_page("page0"))?,
        ),
    ];
    // 使用 create_new，目录检查后的并发文件创建也不能导致原文件被覆盖。
    for (relative, bytes) in files {
        let path = directory.join(relative);
        fs::create_dir_all(path.parent().unwrap())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| file::at_file(e.into(), &path))?;
        output
            .write_all(&bytes)
            .map_err(|e| file::at_file(e.into(), &path))?;
        output.sync_all()?;
    }
    Ok(())
}

pub fn prepare(root: &Path) -> Result<Prepared> {
    prepare_inner(root, None)
}

fn prepare_inner(root: &Path, mut cache: Option<&mut cache::Cache>) -> Result<Prepared> {
    let root = fs::canonicalize(root)?;
    let manifest = root.join("hmi.toml");
    let project: ProjectSource = file::read_toml(&manifest)?;
    check_schema(&project.schema, "hmi.project/1")?;
    if project.name.trim().is_empty() {
        return Err(error("project_name", "工程名称不能为空"));
    }
    if project.pages.sources.is_empty() {
        return Err(error("page_count", "工程至少需要一个页面"));
    }
    let target = Target::resolve(&project.target)?;
    let original_path = project.original.as_ref().map(|o| root.join(&o.bundle));
    let original = original_path
        .as_deref()
        .map(hmi_artifact::original::OriginalBundle::read)
        .transpose()?;
    let program_path = root.join(&project.program.source);
    let program = match cache.as_deref_mut() {
        Some(cache) => cache.compile(&program_path, &target, None, None, true)?,
        None => compile_program(&program_path, &target)?,
    };
    let mut inputs = vec![manifest];
    inputs.extend(program.inputs.iter().cloned());
    if let Some(path) = &original_path {
        inputs.push(path.clone());
    }
    let mut pages = Vec::new();
    let mut assets = Vec::new();
    let mut keys = std::collections::BTreeSet::new();
    for asset in &project.assets.items {
        hmi_core::asset::check_key(&asset.key)?;
        if !keys.insert(&asset.key) {
            return Err(error("asset_key", format!("资产符号 {} 重复", asset.key)));
        }
        let source = root.join(&asset.source);
        let (probe, encoded) = match cache.as_deref_mut() {
            Some(cache) => {
                let encoded = cache.encode(&source, &target)?;
                (
                    hmi_media::AssetProbe {
                        action: encoded.action,
                        info: encoded.object.metadata.info.clone(),
                        inputs: encoded.inputs,
                    },
                    Some(encoded.object),
                )
            }
            None => (hmi_media::probe(&source, &target)?, None),
        };
        inputs.extend(probe.inputs.iter().cloned());
        assets.push(AssetInput {
            key: asset.key.clone(),
            source: AssetOrigin::File(source),
            probe,
            encoded,
        });
    }
    let mut paths = std::collections::BTreeSet::new();
    for source in &project.pages.sources {
        let path = root.join(source);
        let canonical = fs::canonicalize(&path).map_err(|e| file::at_file(e.into(), &path))?;
        if !paths.insert(canonical) {
            return Err(error("page_source", format!("页面输入 {source} 重复")));
        }
        let compilation = match cache.as_deref_mut() {
            Some(cache) => cache.compile(
                &path,
                &target,
                original.as_ref(),
                original_path.as_deref(),
                false,
            )?,
            None => {
                hmi_page_compiler::compile_page_with_original(&path, &target, original.as_ref())?
            }
        };
        inputs.extend(compilation.inputs.iter().cloned());
        pages.push(compilation);
    }
    // 显式资产顺序决定原有ID；内置依赖在编译后收集，以固定顺序追加并共享。
    let needed = pages
        .iter()
        .flat_map(|p| p.object.implicit_images())
        .collect::<std::collections::BTreeSet<_>>();
    for builtin in hmi_core::builtin::BuiltinImage::ALL
        .into_iter()
        .filter(|b| needed.contains(b))
    {
        let encoded = cache
            .as_deref_mut()
            .map(|cache| cache.encode_builtin(builtin, &target))
            .transpose()?;
        assets.push(AssetInput {
            key: builtin.key().into(),
            source: AssetOrigin::Builtin(builtin),
            probe: hmi_media::builtin::probe(builtin, &target)?,
            encoded: encoded.map(|e| e.object),
        });
    }
    Ok(Prepared {
        original,
        original_path,
        root,
        project,
        target,
        program,
        pages,
        inputs,
        assets,
    })
}

impl Prepared {
    pub fn encode_assets(&self) -> Result<Vec<(String, EncodedAsset)>> {
        self.assets
            .iter()
            .map(|asset| {
                if let Some(encoded) = &asset.encoded {
                    return Ok((asset.key.clone(), encoded.clone()));
                }
                let encoded = match &asset.source {
                    AssetOrigin::File(path) => hmi_media::encode(path, &self.target)?,
                    AssetOrigin::Builtin(id) => hmi_media::builtin::encode(*id, &self.target)?,
                };
                if encoded.object.metadata.info != asset.probe.info
                    || encoded.inputs != asset.probe.inputs
                {
                    return Err(error(
                        "input_changed",
                        format!("资产 {} 在构建期间发生变化，请重试", asset.key),
                    ));
                }
                Ok((asset.key.clone(), encoded.object))
            })
            .collect()
    }
    pub fn check(&self, deny_warnings: bool) -> Result<Vec<hmi_core::diagnostic::Diagnostic>> {
        let images = self
            .assets
            .iter()
            .map(|asset| (asset.key.clone(), asset.probe.info.clone()))
            .collect::<Vec<(String, AssetInfo)>>();
        hmi_linker::check_with_original(
            &self.target,
            &self.program.object,
            &self
                .pages
                .iter()
                .map(|p| p.object.clone())
                .collect::<Vec<_>>(),
            &images,
            deny_warnings,
            self.original.as_ref(),
        )
    }
    pub fn link(&self, deny_warnings: bool) -> Result<hmi_linker::Linked> {
        hmi_linker::link_with_original(
            &self.target,
            &self.program.object,
            &self
                .pages
                .iter()
                .map(|p| p.object.clone())
                .collect::<Vec<_>>(),
            &self.encode_assets()?,
            deny_warnings,
            self.original.as_ref(),
        )
    }

    pub fn output_path(&self, requested: Option<&Path>) -> Result<PathBuf> {
        if let Some(path) = requested {
            return file::absolute(path);
        }
        if let Some(path) = &self.project.build.output {
            return Ok(self.root.join(path));
        }
        let name = &self.project.name;
        if name.contains(['/', '\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
            || matches!(name.as_str(), "." | "..")
        {
            return Err(error(
                "output_path",
                "工程名称不能用作默认输出路径，请在 build.output 中指定合法路径",
            ));
        }
        Ok(self
            .root
            .join("dist")
            .join(name)
            .join(format!("{name}.HMI")))
    }

    fn plan_paths(&self, directory: &Path) -> Vec<PathBuf> {
        let mut paths = vec![
            directory.join("target.json"),
            directory.join("link.toml"),
            directory.join("jobs.json"),
            directory.join("obj/program.hmo"),
        ];
        if self.original.is_some() {
            paths.push(directory.join("original.hpo"));
        }
        paths.extend((0..self.pages.len()).map(|id| directory.join(format!("obj/p{id:05}.hmo"))));
        paths.extend(
            (0..self.assets.len()).map(|id| directory.join(format!("assets/a{id:05}.hma"))),
        );
        paths
    }

    /// 计划是显式的工具调用参数，不包含可执行的 shell 字符串。
    pub fn save_plan(
        &self,
        directory: &Path,
        write_objects: bool,
        requested_output: Option<&Path>,
    ) -> Result<()> {
        let assets = if write_objects {
            self.encode_assets()?
        } else {
            Vec::new()
        };
        self.save_plan_inner(directory, write_objects, requested_output, &assets)
    }

    fn save_plan_inner(
        &self,
        directory: &Path,
        write_objects: bool,
        requested_output: Option<&Path>,
        assets: &[(String, EncodedAsset)],
    ) -> Result<()> {
        let directory = file::absolute(directory)?;
        let protected: Vec<_> = self.inputs.iter().map(|p| p.as_path()).collect();
        let paths = self.plan_paths(&directory);
        for path in &paths {
            file::protect_output(path, &protected)?;
        }
        let target = directory.join("target.json");
        let program = directory.join("obj/program.hmo");
        let manifest = LinkSource {
            original: self.original.as_ref().map(|_| OriginalConfig {
                bundle: "original.hpo".into(),
            }),
            schema: "hmi.link/1".into(),
            name: self.project.name.clone(),
            target: "target.json".into(),
            program: "obj/program.hmo".into(),
            pages: PageObjects {
                objects: (0..self.pages.len())
                    .map(|id| format!("obj/p{id:05}.hmo"))
                    .collect(),
            },
            assets: AssetObjects {
                items: self
                    .assets
                    .iter()
                    .enumerate()
                    .map(|(id, asset)| AssetObject {
                        key: asset.key.clone(),
                        object: format!("assets/a{id:05}.hma"),
                    })
                    .collect(),
            },
        };
        let mut jobs = vec![
            serde_json::json!({"command":"hmi-compile","args":["program",self.root.join(&self.project.program.source),"--target",target,"-o",program]}),
        ];
        for (id, source) in self.project.pages.sources.iter().enumerate() {
            let output = directory.join(format!("obj/p{id:05}.hmo"));
            let mut args = vec![
                serde_json::json!("page"),
                serde_json::json!(self.root.join(source)),
                serde_json::json!("--target"),
                serde_json::json!(target),
                serde_json::json!("-o"),
                serde_json::json!(output),
            ];
            if let Some(path) = &self.original_path {
                args.extend([serde_json::json!("--original"), serde_json::json!(path)]);
            }
            jobs.push(serde_json::json!({"command":"hmi-compile","args":args}));
            if write_objects {
                self.pages[id].object.write(&output, &protected)?;
            }
        }
        for (id, asset) in self.assets.iter().enumerate() {
            let output = directory.join(format!("assets/a{id:05}.hma"));
            jobs.push(match &asset.source {
                AssetOrigin::File(source)=>serde_json::json!({"command":"hmi-media","args":["encode",source,"--target",target,"-o",output]}),
                AssetOrigin::Builtin(id)=>serde_json::json!({"command":"hmi-media","args":["builtin",id.id(),"--target",target,"-o",output]}),
            });
            if write_objects {
                assets[id].1.write(&output, &protected)?;
            }
        }
        let output = self.output_path(requested_output)?;
        jobs.push(serde_json::json!({"command":"hmi-link","args":[directory.join("link.toml"),"-o",output]}));
        file::write_atomic(&target, &file::json(&self.target)?, &protected)?;
        if let Some(original) = &self.original {
            original.write(&directory.join("original.hpo"), &protected)?;
        }
        file::write_atomic(
            &directory.join("link.toml"),
            &file::toml(&manifest)?,
            &protected,
        )?;
        file::write_atomic(
            &directory.join("jobs.json"),
            &file::json(&serde_json::json!({"schema":"hmi.build-plan/1","jobs":jobs}))?,
            &protected,
        )?;
        if write_objects {
            self.program.object.write(&program, &protected)?;
        }
        Ok(())
    }
}

/// 默认按内容摘要复用编译及编码产物，所有阶段通过后才更新最终 HMI。
pub fn build(
    root: &Path,
    requested: Option<&Path>,
    deny_warnings: bool,
) -> Result<(PathBuf, hmi_linker::Linked)> {
    let (output, linked, _) = build_with_options(root, requested, deny_warnings, false)?;
    Ok((output, linked))
}

/// force 忽略派生缓存，不改变显式原始附件的输入语义。
pub fn build_with_options(
    root: &Path,
    requested: Option<&Path>,
    deny_warnings: bool,
    force: bool,
) -> Result<(PathBuf, hmi_linker::Linked, BuildStats)> {
    let root = fs::canonicalize(root)?;
    let declared = cache::declared_inputs(&root)?;
    let _lock = cache::ProjectLock::acquire(&root, &declared)?;
    let mut cache = cache::Cache::new(&root, force, &declared)?;
    let prepared = prepare_inner(&root, Some(&mut cache))?;
    let assets = prepared.encode_assets()?;
    let linked = hmi_linker::link_with_original(
        &prepared.target,
        &prepared.program.object,
        &prepared
            .pages
            .iter()
            .map(|p| p.object.clone())
            .collect::<Vec<_>>(),
        &assets,
        deny_warnings,
        prepared.original.as_ref(),
    )?;
    let output = prepared.output_path(requested)?;
    let map = output.parent().unwrap().join("link-map.json");
    if file::absolute(&output)? == file::absolute(&map)? {
        return Err(error("output_path", "HMI 与链接映射不能使用同一输出路径"));
    }
    let build_directory = prepared.root.join(BUILD_DIRECTORY);
    let mut protected_paths = prepared.inputs.clone();
    protected_paths.extend(cache.protected_paths());
    protected_paths.extend(prepared.plan_paths(&build_directory));
    let protected: Vec<_> = protected_paths.iter().map(|p| p.as_path()).collect();
    let publication =
        hmi_linker::publish::Publication::prepare(&linked, &output, Some(&map), &protected)?;
    cache.verify_inputs()?;
    prepared.save_plan_inner(&build_directory, true, Some(&output), &assets)?;
    cache.record(&prepared.plan_paths(&build_directory))?;
    publication.write()?;
    Ok((output, linked, cache.stats))
}
