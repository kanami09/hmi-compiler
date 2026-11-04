//! 每工程独占、可删除的派生缓存；不从清单中读取任意文件路径。
use crate::{
    Error, Result,
    binary::{page_crc, u32_at},
    build::Baseline,
    container::Container,
    diagnostic::{Diagnostic, Location},
    sha256, workspace,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub const DIRECTORY: &str = ".build/hmi-compiler";
const LOCK: &str = ".build/hmi-compiler.lock";
const VERSION: u32 = 1;

#[derive(Clone, Copy)]
pub(crate) enum Unit {
    Page(usize),
    Program,
}

impl Unit {
    fn artifact(self) -> String {
        match self {
            Self::Page(i) => format!("pages/{i}.pa"),
            Self::Program => "Program.s".into(),
        }
    }
    fn diagnostics(self) -> String {
        match self {
            Self::Page(i) => format!("diagnostics/{i}.json"),
            Self::Program => "diagnostics/program.json".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UnitRecord {
    key: String,
    artifact: String,
    diagnostic_key: Option<String>,
    diagnostics: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Index {
    version: u32,
    tool: String,
    source: String,
    baseline: Option<String>,
    pages: BTreeMap<usize, UnitRecord>,
    program: Option<UnitRecord>,
}

#[derive(Serialize, Deserialize)]
struct DiagnosticRecord {
    key: String,
    diagnostics: Vec<Diagnostic>,
}

pub(crate) struct Cache {
    root: PathBuf,
    index: Index,
    _lock: File,
}

fn path_error(path: &Path, message: impl Into<String>) -> Error {
    Diagnostic::error("cache_path", message)
        .at(Location {
            file: Some(path.display().to_string()),
            ..Location::default()
        })
        .into()
}

pub(crate) fn checked(root: &Path, relative: &str) -> Result<PathBuf> {
    let mut result = root.to_owned();
    for component in workspace::relative(relative)?.components() {
        result.push(component);
        match fs::symlink_metadata(&result) {
            Ok(metadata) if workspace::is_link(&metadata) => {
                return Err(path_error(&result, "构建目录不能包含符号链接或目录联接"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(result)
}

pub fn is_unsafe(error: &Error) -> bool {
    error
        .as_diagnostic()
        .is_some_and(|d| d.code == "cache_path")
}

pub(crate) fn absolute(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(fs::canonicalize(path)?);
    }
    let absolute = std::path::absolute(path)?;
    let mut ancestor = absolute.as_path();
    let mut tail = Vec::new();
    while !ancestor.exists() {
        tail.push(
            ancestor
                .file_name()
                .ok_or_else(|| Error::Message("输出路径无法解析".into()))?
                .to_owned(),
        );
        ancestor = ancestor
            .parent()
            .ok_or_else(|| Error::Message("输出路径无法解析".into()))?;
    }
    let mut result = fs::canonicalize(ancestor)?;
    for component in tail.into_iter().rev() {
        result.push(component);
    }
    // absolute() 在部分平台保留 ..，通过组件归一化避免缓存路径绕过。
    let mut normalized = PathBuf::new();
    for component in result.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            _ => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

pub(crate) fn protect_output(root: &Path, output: &Path) -> Result<()> {
    let root = fs::canonicalize(root)?;
    let target = absolute(output)?;
    if target.starts_with(root.join(DIRECTORY))
        || target == root.join(LOCK)
        || target.starts_with(root.join(".hmi"))
    {
        return Err(path_error(
            output,
            "构建输出不能覆盖原始元数据、内部缓存或构建锁",
        ));
    }
    Ok(())
}

fn acquire(root: &Path, info: &mut dyn FnMut(&str)) -> Result<File> {
    checked(root, DIRECTORY)?;
    let path = checked(root, LOCK)?;
    fs::create_dir_all(workspace::parent(&path))?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            info("等待同一工程的构建或清理操作释放锁");
            file.lock()?;
        }
        Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
    }
    // 等待期间目录可能已被另一个命令清理，重新检查安全边界。
    checked(root, DIRECTORY)?;
    Ok(file)
}

pub(crate) fn tool_id() -> Option<&'static str> {
    static ID: OnceLock<Option<String>> = OnceLock::new();
    ID.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| fs::read(p).ok())
            .map(|bytes| sha256(&bytes))
    })
    .as_deref()
}

fn serialize(value: &impl Serialize) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|e| Error::Message(e.to_string()))
}

impl Cache {
    pub fn open(
        root: &Path,
        source: &str,
        full: bool,
        info: &mut dyn FnMut(&str),
    ) -> Result<Option<Self>> {
        let Some(tool) = tool_id() else {
            info("无法读取工具构建标识，本次执行无缓存全量构建");
            return Ok(None);
        };
        let lock = acquire(root, info)?;
        let directory = checked(root, DIRECTORY)?;
        fs::create_dir_all(&directory)?;
        let path = checked(&directory, "manifest.json")?;
        let previous = if full {
            None
        } else {
            match fs::read(path) {
                Ok(bytes) => {
                    let index = serde_json::from_slice::<Index>(&bytes).ok();
                    if index.is_none() {
                        info("缓存清单损坏，重新生成构建缓存");
                    }
                    index
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            }
        };
        let index = previous
            .filter(|index| {
                index.version == VERSION && index.tool == tool && index.source == source
            })
            .unwrap_or_else(|| Index {
                version: VERSION,
                tool: tool.into(),
                source: source.into(),
                baseline: None,
                pages: BTreeMap::new(),
                program: None,
            });
        Ok(Some(Self {
            root: directory,
            index,
            _lock: lock,
        }))
    }

    fn record(&self, unit: Unit) -> Option<&UnitRecord> {
        match unit {
            Unit::Page(i) => self.index.pages.get(&i),
            Unit::Program => self.index.program.as_ref(),
        }
    }
    fn write(&self, relative: &str, bytes: &[u8]) -> Result<()> {
        let path = checked(&self.root, relative)?;
        fs::create_dir_all(workspace::parent(&path))?;
        let mut temporary = tempfile::NamedTempFile::new_in(workspace::parent(&path))?;
        temporary.write_all(bytes)?;
        temporary.as_file_mut().sync_all()?;
        temporary
            .persist(path)
            .map_err(|e| Error::Message(e.error.to_string()))?;
        Ok(())
    }
    fn verified(&self, relative: &str, digest: &str) -> Result<Option<Vec<u8>>> {
        let path = checked(&self.root, relative)?;
        match fs::read(path) {
            Ok(bytes) => Ok((sha256(&bytes) == digest).then_some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn flush(&self) -> Result<()> {
        self.write("manifest.json", &serialize(&self.index)?)
    }

    pub fn baseline(&self, container: &Container) -> Result<Option<Baseline>> {
        let Some(digest) = &self.index.baseline else {
            return Ok(None);
        };
        let Some(bytes) = self.verified("baseline.json", digest)? else {
            return Ok(None);
        };
        let Ok(baseline) = serde_json::from_slice::<Baseline>(&bytes) else {
            return Ok(None);
        };
        let count = baseline.pages.len();
        let valid =
            count == baseline.entries.len()
                && count == baseline.manifest.pages.len()
                && count == baseline.symbols.pages.len()
                && count == baseline.project.pages.len()
                && baseline.program_index < container.entries.len()
                && !container.entries[baseline.program_index].deleted
                && container.entries[baseline.program_index].name == "Program.s"
                && baseline.entries.iter().enumerate().all(|(i, entry)| {
                    container.entries.get(*entry).is_some_and(|e| {
                        !e.deleted && e.name == baseline.manifest.pages[i].resource
                    }) && baseline.project.pages[i] == baseline.manifest.pages[i].path
                        && baseline.pages[i].objects.len() + 1
                            == baseline.symbols.pages[i].objects.len()
                        && baseline.manifest.pages[i].object_indices
                            == (0..baseline.symbols.pages[i].objects.len()).collect::<Vec<_>>()
                });
        Ok(valid.then_some(baseline))
    }
    pub fn save_baseline(&mut self, baseline: &Baseline) -> Result<()> {
        let bytes = serialize(baseline)?;
        self.write("baseline.json", &bytes)?;
        self.index.baseline = Some(sha256(&bytes));
        self.flush()
    }
    pub fn artifact(&self, unit: Unit, key: &str) -> Result<Option<Vec<u8>>> {
        let Some(record) = self.record(unit).filter(|r| r.key == key) else {
            return Ok(None);
        };
        let Some(bytes) = self.verified(&unit.artifact(), &record.artifact)? else {
            return Ok(None);
        };
        if matches!(unit, Unit::Page(_))
            && (page_crc(&bytes).ok() != u32_at(&bytes, 0).ok()
                || u32_at(&bytes, 4).ok().map(|n| n as usize) != Some(bytes.len())
                || bytes.len() < 56)
        {
            return Ok(None);
        }
        Ok(Some(bytes))
    }
    pub fn diagnostics(&self, unit: Unit, key: &str) -> Result<Option<Vec<Diagnostic>>> {
        let Some(record) = self
            .record(unit)
            .filter(|r| r.diagnostic_key.as_deref() == Some(key))
        else {
            return Ok(None);
        };
        let Some(digest) = &record.diagnostics else {
            return Ok(None);
        };
        let Some(bytes) = self.verified(&unit.diagnostics(), digest)? else {
            return Ok(None);
        };
        let Ok(record) = serde_json::from_slice::<DiagnosticRecord>(&bytes) else {
            return Ok(None);
        };
        if record.key != key
            || record.diagnostics.iter().any(|d| {
                d.original_severity.is_some() || d.severity != crate::diagnostic::Severity::Warning
            })
        {
            return Ok(None);
        }
        Ok(Some(record.diagnostics))
    }
    pub fn save(
        &mut self,
        unit: Unit,
        key: &str,
        bytes: &[u8],
        diagnostics: Option<(&str, &[Diagnostic])>,
        artifact_reused: bool,
    ) -> Result<()> {
        if !artifact_reused {
            self.write(&unit.artifact(), bytes)?;
        }
        let (diagnostic_key, diagnostic_digest) = if let Some((key, diagnostics)) = diagnostics {
            let contents = serialize(&DiagnosticRecord {
                key: key.into(),
                diagnostics: diagnostics.to_vec(),
            })?;
            self.write(&unit.diagnostics(), &contents)?;
            (Some(key.into()), Some(sha256(&contents)))
        } else {
            (None, None)
        };
        let record = UnitRecord {
            key: key.into(),
            artifact: sha256(bytes),
            diagnostic_key,
            diagnostics: diagnostic_digest,
        };
        match unit {
            Unit::Page(i) => {
                self.index.pages.insert(i, record);
            }
            Unit::Program => self.index.program = Some(record),
        }
        self.flush()
    }
}

#[derive(Debug, Serialize)]
pub struct CleanReport {
    pub directory: String,
    pub removed: bool,
}

pub fn clean(root: &Path, info: &mut dyn FnMut(&str)) -> Result<CleanReport> {
    let root = fs::canonicalize(root)?;
    let directory = checked(&root, DIRECTORY)?;
    if !directory.exists() && !checked(&root, LOCK)?.exists() {
        return Ok(CleanReport {
            directory: directory.display().to_string(),
            removed: false,
        });
    }
    let _lock = acquire(&root, info)?;
    let directory = checked(&root, DIRECTORY)?;
    let removed = directory.exists();
    if removed {
        let resolved = fs::canonicalize(&directory)?;
        if resolved == root
            || !resolved.starts_with(root.join(".build"))
            || !resolved.ends_with("hmi-compiler")
        {
            return Err(path_error(&directory, "缓存清理目标超出工程构建目录"));
        }
        fs::remove_dir_all(&directory)?;
    }
    Ok(CleanReport {
        directory: directory.display().to_string(),
        removed,
    })
}
