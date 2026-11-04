//! 按页面顺序执行的构建引擎，终端输出由事件观察者负责。
use crate::{
    Error, Result,
    container::Container,
    diagnostic::{Diagnostic, Location, Severity, has_errors},
    page::Page,
    project::Project,
    script::{self, Context, ScriptSource},
    sha256,
    warning::WarningPolicy,
    workspace::{
        self, Manifest, ObjectFile, PageFile, ProjectFile,
        source::{PageDocument, line_at},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::Path,
    time::Instant,
};

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildMode {
    #[default]
    Incremental,
    Full,
    Check,
}

#[derive(Clone, Debug, Default)]
pub struct BuildOptions {
    pub mode: BuildMode,
    pub warnings: WarningPolicy,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageReport {
    pub id: usize,
    pub name: String,
    pub resource: String,
    pub reused: bool,
    pub success: bool,
    pub errors: usize,
    pub warnings: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct BuildReport {
    pub mode: BuildMode,
    pub output: Option<String>,
    pub output_written: bool,
    pub size: usize,
    pub sha256: Option<String>,
    pub byte_identical: bool,
    pub changed_resources: Vec<String>,
    pub pages: Vec<PageReport>,
    pub rebuilt_pages: usize,
    pub reused_pages: usize,
    pub diagnostic_rechecks: usize,
    pub baseline_reused: bool,
    pub program_reused: bool,
    pub context_complete: bool,
    pub context_sha256: String,
    pub errors: usize,
    pub warnings: usize,
    pub elapsed_ms: u128,
}

#[derive(Debug)]
pub struct BuildOutcome {
    pub success: bool,
    pub report: BuildReport,
    pub diagnostics: Vec<Diagnostic>,
    pub(crate) bytes: Option<Vec<u8>>,
    pub(crate) inputs: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub enum BuildEvent {
    Info(String),
    Diagnostic(Diagnostic),
    Page {
        completed: usize,
        total: usize,
        name: String,
        diagnostics: Vec<Diagnostic>,
    },
    Finished(BuildReport),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Baseline {
    pub manifest: Manifest,
    pub project: ProjectFile,
    pub pages: Vec<PageFile>,
    pub symbols: Context,
    pub entries: Vec<usize>,
    pub program_index: usize,
    #[serde(skip)]
    pub originals: BTreeMap<usize, Page>,
}

impl Baseline {
    pub fn cold(container: &Container) -> Result<Self> {
        let project = Project::parse(container)?;
        if let Err(error) = project.strict() {
            return Err(project
                .diagnostics
                .iter()
                .find(|d| d.severity == Severity::Error)
                .cloned()
                .unwrap_or_else(|| Diagnostic::error("project_validation", error.to_string()))
                .into());
        }
        let bundle = workspace::make_bundle(container, &project)?;
        let symbols = Context::from_project(&project);
        let entries = project.pages.iter().map(|(i, _)| *i).collect();
        let program_index = project
            .program_index
            .ok_or_else(|| Error::Message("缺少 Program.s".into()))?;
        Ok(Self {
            manifest: bundle.manifest,
            project: bundle.project,
            pages: bundle.pages,
            symbols,
            entries,
            program_index,
            originals: project.pages.into_iter().collect(),
        })
    }
}

pub(crate) struct PageInput {
    pub document: Option<PageDocument>,
    pub scripts: BTreeMap<String, Vec<u8>>,
    pub sources: Vec<ScriptSource>,
    pub errors: Vec<Diagnostic>,
    pub key: String,
}

fn diagnostic(error: Error, file: &str) -> Diagnostic {
    let mut result = error
        .as_diagnostic()
        .cloned()
        .unwrap_or_else(|| Diagnostic::error("execution", error.to_string()));
    if result.location.file.is_none() {
        result.location.file = Some(file.into());
    }
    result
}

pub(crate) fn json_hash(value: &impl Serialize) -> Result<String> {
    Ok(sha256(
        &serde_json::to_vec(value).map_err(|e| Error::Message(e.to_string()))?,
    ))
}

impl PageInput {
    fn load(root: &Path, directory: &str, page_index: usize, name: &str) -> Self {
        let file = format!("{directory}/page.toml");
        let mut result = Self {
            document: None,
            scripts: BTreeMap::new(),
            sources: Vec::new(),
            errors: Vec::new(),
            key: String::new(),
        };
        let bytes = match workspace::read(root, &file) {
            Ok(bytes) => bytes,
            Err(e) => {
                result.errors.push(diagnostic(e, &file));
                return result;
            }
        };
        let document = match PageDocument::parse(&bytes, &file) {
            Ok(document) => document,
            Err(e) => {
                let mut d = diagnostic(e, &file);
                d.location.page = Some(name.into());
                result.errors.push(d);
                return result;
            }
        };
        let events = std::iter::once(&document.page.events)
            .chain(document.page.objects.iter().map(|o| &o.events));
        for (object, events) in events.enumerate() {
            for (kind, reference) in events {
                let field = format!("events.{kind}");
                if let Err(e) = workspace::relative(reference) {
                    result.errors.push(diagnostic(
                        document.error(object, Some(&field), "event_path", e.to_string()),
                        &file,
                    ));
                    continue;
                }
                let path = format!("{directory}/{reference}");
                if !path.ends_with(".s") {
                    result.errors.push(diagnostic(
                        document.error(
                            object,
                            Some(&field),
                            "event_path",
                            "事件文件必须使用 .s 后缀",
                        ),
                        &file,
                    ));
                    continue;
                }
                let contents = match workspace::read(root, &path) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        result.errors.push(diagnostic(
                            document.error(object, Some(&field), "event_file", e.to_string()),
                            &file,
                        ));
                        continue;
                    }
                };
                if let Err(e) = workspace::script_lines(&contents) {
                    let offset = std::str::from_utf8(&contents)
                        .err()
                        .map(|e| e.valid_up_to())
                        .or_else(|| contents.iter().position(|b| *b == 0))
                        .or_else(|| {
                            contents
                                .iter()
                                .enumerate()
                                .find(|(i, b)| **b == b'\r' && contents.get(*i + 1) != Some(&b'\n'))
                                .map(|(i, _)| i)
                        })
                        .unwrap_or(0);
                    let mut location = document.location(object, Some(&field));
                    location.file = Some(path);
                    location.line = Some(line_at(&contents, offset));
                    result
                        .errors
                        .push(Diagnostic::error("script_encoding", e.to_string()).at(location));
                    continue;
                }
                result.sources.push(ScriptSource {
                    page: Some(page_index),
                    object: Some(object),
                    event: Some(kind.clone()),
                    file: path.clone(),
                    text: String::from_utf8(contents.clone()).expect("已验证 UTF-8"),
                });
                result.scripts.insert(path, contents);
            }
        }
        result.key = json_hash(&(
            sha256(&bytes),
            result
                .scripts
                .iter()
                .map(|(path, bytes)| (path, sha256(bytes)))
                .collect::<Vec<_>>(),
        ))
        .expect("可序列化的字符串");
        result.document = Some(document);
        result
    }
}

fn compile_page(
    root: &Path,
    baseline: &Baseline,
    page_id: usize,
    original: &Page,
    input: &PageInput,
    inputs: &mut BTreeSet<String>,
) -> (Option<Vec<u8>>, Vec<Diagnostic>) {
    let mut errors = input.errors.clone();
    let Some(document) = &input.document else {
        return (None, errors);
    };
    let edited = &document.page;
    let expected = &baseline.pages[page_id];
    if edited.name != expected.name {
        errors.push(diagnostic(
            document.error(0, Some("name"), "page_name", "当前不支持修改页面名称"),
            &document.file,
        ));
    }
    if edited.objects.len() != expected.objects.len() {
        errors.push(diagnostic(
            document.error(0, None, "object_count", "当前不支持新增、删除或重排控件"),
            &document.file,
        ));
    }
    if !errors.is_empty() {
        return (None, errors);
    }
    let own = ObjectFile {
        name: edited.name.clone(),
        type_name: "page".into(),
        properties: edited.properties.clone(),
        events: edited.events.clone(),
    };
    let expected_own = ObjectFile {
        name: expected.name.clone(),
        type_name: "page".into(),
        properties: expected.properties.clone(),
        events: expected.events.clone(),
    };
    let descriptions = std::iter::once(&own).chain(edited.objects.iter());
    let expected_objects = std::iter::once(&expected_own).chain(expected.objects.iter());
    let mut objects = Vec::new();
    for (index, ((object, expected), description)) in original
        .objects
        .iter()
        .zip(expected_objects)
        .zip(descriptions)
        .enumerate()
    {
        let context = workspace::ObjectUpdate {
            root,
            document,
            index,
            references: &baseline.symbols.references,
            input_files: inputs,
            scripts: Some(&input.scripts),
        };
        match workspace::update_object(context, object, expected, description) {
            Ok(bytes) => objects.push(bytes),
            Err(error) => errors.push(diagnostic(error, &document.file)),
        }
    }
    if !errors.is_empty() {
        return (None, errors);
    }
    match original.rebuild(&objects) {
        Ok(bytes) => (Some(bytes), errors),
        Err(error) => {
            errors.push(diagnostic(
                document.error(0, None, "page_build", error.to_string()),
                &document.file,
            ));
            (None, errors)
        }
    }
}

struct Engine<'a> {
    root: &'a Path,
    output: Option<&'a Path>,
    options: &'a BuildOptions,
    observer: &'a mut dyn FnMut(&BuildEvent),
    report: BuildReport,
    diagnostics: Vec<Diagnostic>,
    inputs: BTreeSet<String>,
}

impl Engine<'_> {
    fn info(&mut self, text: impl Into<String>) {
        (self.observer)(&BuildEvent::Info(text.into()));
    }
    fn global(&mut self, raw: Diagnostic) {
        if let Some(diagnostic) = self.options.warnings.apply(&raw) {
            (self.observer)(&BuildEvent::Diagnostic(diagnostic.clone()));
            self.diagnostics.push(diagnostic);
        }
    }

    fn execute(&mut self) -> Result<Option<Vec<u8>>> {
        self.info("读取工程，验证原始快照与资源");
        let manifest: Manifest = workspace::deserialize(
            &workspace::read(self.root, ".hmi/manifest.toml")?,
            ".hmi/manifest.toml",
        )?;
        if manifest.format_version != 1 {
            return Err(Error::Message("不支持的原始元数据版本".into()));
        }
        let source = workspace::read(self.root, ".hmi/container.bin")?;
        if sha256(&source) != manifest.source_sha256 {
            return Err(Diagnostic::error(
                "snapshot_hash",
                "原始快照 SHA-256 不匹配，不能使用已改变的原始工程数据",
            )
            .at(Location {
                file: Some(".hmi/container.bin".into()),
                ..Location::default()
            })
            .into());
        }
        let container = Container::parse(source)?;
        if let Err(error) = container.strict() {
            return Err(container
                .diagnostics
                .iter()
                .find(|d| d.severity == Severity::Error)
                .cloned()
                .unwrap_or_else(|| Diagnostic::error("container_validation", error.to_string()))
                .into());
        }
        let baseline = Baseline::cold(&container)?;
        if manifest != baseline.manifest {
            return Err(Error::Message("只读元数据或资源映射已改变".into()));
        }
        let description: ProjectFile =
            workspace::deserialize(&workspace::read(self.root, "project.toml")?, "project.toml")?;
        if description != baseline.project {
            return Err(Diagnostic::error(
                "project_structure",
                "当前不支持更改工程版本、编码或页面顺序",
            )
            .at(Location {
                file: Some("project.toml".into()),
                ..Location::default()
            })
            .into());
        }
        for asset in &manifest.assets {
            if sha256(&workspace::read(self.root, &asset.path)?) != asset.sha256 {
                return Err(Diagnostic::error(
                    "readonly_asset",
                    "只读素材内容已改变，当前不支持素材替换",
                )
                .at(Location {
                    file: Some(asset.path.clone()),
                    ..Location::default()
                })
                .into());
            }
        }
        self.inputs.extend(manifest.managed_files.iter().cloned());
        let mut page_inputs = Vec::new();
        let mut context = baseline.symbols.clone();
        for (i, mapping) in manifest.pages.iter().enumerate() {
            let input = PageInput::load(self.root, &mapping.path, i, &baseline.pages[i].name);
            context.overlay(i, input.document.as_ref().map(|d| &d.page));
            if !input.errors.is_empty() {
                context.pages[i].valid = false;
            }
            self.inputs.extend(input.scripts.keys().cloned());
            page_inputs.push(input);
        }
        self.report.context_complete = context.complete();
        self.report.context_sha256 = context.fingerprint()?;
        let program = workspace::read(self.root, "program.s")?;
        let text = std::str::from_utf8(&program).map_err(|e| {
            Diagnostic::error("script_encoding", "全局代码不是有效的 UTF-8 文本").at(Location {
                file: Some("program.s".into()),
                line: Some(line_at(&program, e.valid_up_to())),
                ..Location::default()
            })
        })?;
        if let Some(offset) = program.iter().position(|b| *b == 0) {
            return Err(
                Diagnostic::error("script_encoding", "全局代码不能包含 NUL 字符")
                    .at(Location {
                        file: Some("program.s".into()),
                        line: Some(line_at(&program, offset)),
                        ..Location::default()
                    })
                    .into(),
            );
        }
        self.info(format!(
            "检查全局代码，按工程顺序处理 {} 个页面",
            baseline.pages.len()
        ));
        for raw in script::check_unit(
            &context,
            None,
            &[ScriptSource {
                page: None,
                object: None,
                event: None,
                file: "program.s".into(),
                text: text.into(),
            }],
        ) {
            self.global(raw);
        }
        let mut replacements = BTreeMap::new();
        for (i, input) in page_inputs.iter().enumerate() {
            let entry_index = baseline.entries[i];
            let parsed;
            let original = if let Some(page) = baseline.originals.get(&entry_index) {
                page
            } else {
                parsed = Page::parse(container.data(&container.entries[entry_index])?)?;
                &parsed
            };
            let (bytes, mut raw) =
                compile_page(self.root, &baseline, i, original, input, &mut self.inputs);
            self.report.rebuilt_pages += 1;
            if input.document.is_some() {
                raw.extend(script::check_unit(&context, Some(i), &input.sources));
                self.report.diagnostic_rechecks += 1;
            }
            if let Some(document) = &input.document {
                for diagnostic in &mut raw {
                    document.locate(diagnostic);
                }
            }
            raw.sort_by(|a, b| {
                (
                    &a.location.object_index,
                    &a.location.file,
                    &a.location.line,
                    &a.code,
                )
                    .cmp(&(
                        &b.location.object_index,
                        &b.location.file,
                        &b.location.line,
                        &b.code,
                    ))
            });
            let effective = self.options.warnings.filter(&raw);
            let errors = effective
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count();
            let warnings = effective.len() - errors;
            if let Some(bytes) = bytes
                && bytes.as_slice() != container.data(&container.entries[entry_index])?
            {
                replacements.insert(entry_index, bytes);
            }
            self.report.pages.push(PageReport {
                id: i,
                name: baseline.pages[i].name.clone(),
                resource: manifest.pages[i].resource.clone(),
                reused: false,
                success: errors == 0,
                errors,
                warnings,
            });
            (self.observer)(&BuildEvent::Page {
                completed: i + 1,
                total: baseline.pages.len(),
                name: baseline.pages[i].name.clone(),
                diagnostics: effective.clone(),
            });
            self.diagnostics.extend(effective);
        }
        if program != container.data(&container.entries[baseline.program_index])? {
            replacements.insert(baseline.program_index, program);
        }
        self.report.changed_resources = replacements
            .keys()
            .map(|i| container.entries[*i].name.clone())
            .collect();
        if has_errors(&self.diagnostics) {
            return Ok(None);
        }
        self.info("封装容器，更新目录偏移与 CRC");
        let bytes = container.rebuild(&replacements)?;
        self.info("完整回读并校验生成工程");
        let rebuilt = Container::parse(bytes.clone())?;
        rebuilt.strict()?;
        Project::parse(&rebuilt)?.strict()?;
        for entry in &container.entries {
            let expected = replacements
                .get(&entry.index)
                .map(Vec::as_slice)
                .unwrap_or(container.data(entry)?);
            if rebuilt.data(&rebuilt.entries[entry.index])? != expected {
                return Err(Error::Message(format!(
                    "输出验证发现非预期的资源变化：{}",
                    entry.name
                )));
            }
        }
        self.report.size = bytes.len();
        self.report.sha256 = Some(sha256(&bytes));
        self.report.byte_identical = bytes == container.bytes;
        if self.options.mode != BuildMode::Check
            && let Some(output) = self.output
        {
            publish(self.root, output, &bytes, &self.inputs)?;
            self.report.output_written = true;
        }
        Ok(Some(bytes))
    }
}

pub(crate) fn publish(
    root: &Path,
    output: &Path,
    bytes: &[u8],
    inputs: &BTreeSet<String>,
) -> Result<()> {
    let target = if output.exists() {
        fs::canonicalize(output)?
    } else {
        std::path::absolute(output)?
    };
    let cache = std::path::absolute(root)?.join(".build/hmi-compiler");
    if target.starts_with(&cache) {
        return Err(Error::Message("构建输出不能位于工具缓存目录中".into()));
    }
    for file in inputs {
        let source = workspace::resolve(root, file)?;
        if fs::canonicalize(&source).ok().as_ref() == Some(&target)
            || std::path::absolute(&source)? == target
        {
            return Err(Error::Message("构建输出不能覆盖项目输入文件".into()));
        }
    }
    fs::create_dir_all(workspace::parent(output))?;
    let mut temporary = tempfile::NamedTempFile::new_in(workspace::parent(output))?;
    temporary.write_all(bytes)?;
    temporary.as_file_mut().sync_all()?;
    temporary
        .persist(output)
        .map_err(|e| Error::Message(format!("发布输出失败：{}", e.error)))?;
    Ok(())
}

/// 所有页面事件均实时发送；失败时仍返回已收集的报告和诊断。
pub fn run(
    root: &Path,
    output: Option<&Path>,
    options: &BuildOptions,
    observer: &mut dyn FnMut(&BuildEvent),
) -> BuildOutcome {
    let started = Instant::now();
    let mut engine = Engine {
        root,
        output,
        options,
        observer,
        report: BuildReport {
            mode: options.mode,
            output: output.map(|p| p.display().to_string()),
            ..BuildReport::default()
        },
        diagnostics: Vec::new(),
        inputs: BTreeSet::new(),
    };
    let bytes = match engine.execute() {
        Ok(bytes) => bytes,
        Err(error) => {
            engine.global(diagnostic(error, &root.display().to_string()));
            None
        }
    };
    engine.report.errors = engine
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    engine.report.warnings = engine.diagnostics.len() - engine.report.errors;
    engine.report.elapsed_ms = started.elapsed().as_millis();
    (engine.observer)(&BuildEvent::Finished(engine.report.clone()));
    BuildOutcome {
        success: engine.report.errors == 0 && bytes.is_some(),
        report: engine.report,
        diagnostics: engine.diagnostics,
        bytes,
        inputs: engine.inputs,
    }
}
