//! 版本 1 的 TOML 项目、保真快照与事务式文件输出。
mod format;
pub mod source;
pub use format::page_to_toml;

use crate::container::Container;
use crate::diagnostic::{Diagnostic, Severity};
use crate::page::{Attribute, Object, type_name};
use crate::project::Project;
use crate::{Error, Result, hex, sha256};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

const META: &str = ".hmi/manifest.toml";
const SOURCE: &str = ".hmi/container.bin";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    pub format_version: u32,
    pub encoding: String,
    pub pages: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ObjectFile {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub properties: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub events: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PageFile {
    pub name: String,
    pub properties: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub events: BTreeMap<String, String>,
    #[serde(default)]
    pub objects: Vec<ObjectFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PageMapping {
    pub path: String,
    pub resource: String,
    pub object_indices: Vec<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub source_sha256: String,
    pub pages: Vec<PageMapping>,
    pub assets: Vec<Asset>,
    pub managed_files: Vec<String>,
}

pub(crate) struct Bundle {
    pub(crate) files: BTreeMap<String, Vec<u8>>,
    pub(crate) manifest: Manifest,
    pub(crate) project: ProjectFile,
    pub(crate) pages: Vec<PageFile>,
}

fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    toml::to_string_pretty(value)
        .map(String::into_bytes)
        .map_err(|e| Error::Message(format!("TOML 序列化失败：{e}")))
}

pub(crate) fn deserialize<T: serde::de::DeserializeOwned>(bytes: &[u8], path: &str) -> Result<T> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::Message(format!("{path}：文件不是 UTF-8")))?;
    toml::from_str(text).map_err(|e| Error::Message(format!("{path}：{e}")))
}

/// 目录与文件名不承担工程标识的作用；映射在 manifest 中显式记录。
fn filename(name: &str) -> String {
    let mut result = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' {
            result.push(byte as char);
        } else {
            result.push_str(&format!("_{byte:02x}"));
        }
    }
    if result.is_empty() {
        result.push('_');
    }
    result
}

#[derive(Clone, Copy)]
enum Scalar {
    Unsigned(usize),
    Signed32,
    Text,
}

fn scalar(object: &Object, name: &str) -> Option<Scalar> {
    if name == "txt" {
        return Some(Scalar::Text);
    }
    if name == "val" {
        return match object.type_code {
            52 => Some(Scalar::Signed32),
            1 => Some(Scalar::Unsigned(2)),
            106 | 98 | 53 | 57 | 5 | 56 => Some(Scalar::Unsigned(1)),
            _ => None,
        };
    }
    let width = match name {
        "vscope" | "drag" | "sendkey" | "aph" | "effect" | "first" | "lockobj" | "up" | "down"
        | "left" | "right" | "sta" | "style" | "key" | "borderw" | "font" | "xcen" | "ycen"
        | "pw" | "isbr" | "spax" | "spay" | "dez" | "dis" | "en" | "mode" | "psta" | "wid"
        | "hig" | "ch" | "dir" => 1,
        "movex" | "movey" | "x" | "y" | "w" | "h" | "endx" | "endy" | "time" | "bco" | "pic"
        | "borderc" | "picc" | "pco" | "txt_maxl" | "bpic" | "ppic" | "pic2" | "picc2" | "bco2"
        | "pco2" | "tim" | "vvs0" | "vvs1" | "pic1" | "picc1" | "bco1" | "maxval" | "minval"
        | "vvs2" | "vvs3" => 2,
        "groupid0" | "groupid1" => 4,
        _ => return None,
    };
    Some(Scalar::Unsigned(width))
}

fn editable(name: &str) -> bool {
    matches!(
        name,
        "txt"
            | "txt_maxl"
            | "x"
            | "y"
            | "w"
            | "h"
            | "vscope"
            | "font"
            | "pic"
            | "pic2"
            | "picc"
            | "picc2"
            | "bpic"
            | "ppic"
            | "pic1"
            | "picc1"
            | "bco"
            | "bco2"
            | "bco1"
            | "pco"
            | "pco2"
            | "pco1"
            | "borderc"
            | "val"
            | "tim"
            | "en"
    )
}

fn value(object: &Object, attr: &Attribute) -> Result<toml::Value> {
    let value = match scalar(object, &attr.name) {
        Some(Scalar::Text) => toml::Value::String(
            std::str::from_utf8(&attr.value)
                .map_err(|_| {
                    Error::Message(format!("{}.{} 不是有效 UTF-8", object.name, attr.name))
                })?
                .into(),
        ),
        Some(Scalar::Signed32) if attr.value.len() == 4 => toml::Value::Integer(
            i32::from_le_bytes(attr.value.as_slice().try_into().unwrap()) as i64,
        ),
        Some(Scalar::Unsigned(width)) if attr.value.len() == width => {
            toml::Value::Integer(object.integer(&attr.name).unwrap() as i64)
        }
        _ => toml::Value::String(format!("raw:{}", hex(&attr.value))),
    };
    Ok(value)
}

pub fn properties(object: &Object) -> Result<BTreeMap<String, toml::Value>> {
    object
        .attributes
        .iter()
        .filter(|a| !matches!(a.name.as_str(), "type" | "id" | "objname" | "endx" | "endy"))
        .map(|a| Ok((a.name.clone(), value(object, a)?)))
        .collect()
}

fn script_file(lines: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    for line in lines {
        std::str::from_utf8(line).map_err(|_| Error::Message("事件代码不是有效 UTF-8".into()))?;
        if line.contains(&b'\n') || line.contains(&b'\r') {
            return Err(Error::Message(
                "单条事件记录内嵌换行，首版不能无歧义导出".into(),
            ));
        }
        result.extend_from_slice(line);
        result.push(b'\n');
    }
    Ok(result)
}

/// 空文件是零行；末尾的一个分隔符结束当前行，不额外产生一行。
pub fn script_lines(bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| Error::Message("脚本不是有效 UTF-8".into()))?;
    if text.contains('\0') {
        return Err(Error::Message("脚本不能包含 NUL".into()));
    }
    let normalized = text.replace("\r\n", "\n");
    if normalized.contains('\r') {
        return Err(Error::Message("脚本包含独立 CR，请使用 LF 或 CRLF".into()));
    }
    if normalized.is_empty() {
        return Ok(Vec::new());
    }
    let mut lines: Vec<_> = normalized
        .split('\n')
        .map(|s| s.as_bytes().to_vec())
        .collect();
    if normalized.ends_with('\n') {
        lines.pop();
    }
    Ok(lines)
}

fn png_crc(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xEDB8_8320 } else { 0 };
        }
    }
    !crc
}

fn embedded_pngs(bytes: &[u8]) -> Vec<Vec<u8>> {
    let signature = b"\x89PNG\r\n\x1a\n";
    let mut images = Vec::new();
    let mut search = 0;
    while search + 8 <= bytes.len() {
        let Some(relative) = bytes[search..].windows(8).position(|s| s == signature) else {
            break;
        };
        let start = search + relative;
        let mut cursor = start + 8;
        let mut valid = false;
        while let Some(header) = bytes.get(cursor..cursor.saturating_add(8)) {
            let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
            let Some(end) = cursor.checked_add(12).and_then(|n| n.checked_add(length)) else {
                break;
            };
            let Some(chunk) = bytes.get(cursor..end) else {
                break;
            };
            if png_crc(&chunk[4..8 + length])
                != u32::from_be_bytes(chunk[8 + length..].try_into().unwrap())
            {
                break;
            }
            cursor = end;
            if &chunk[4..8] == b"IEND" {
                valid = true;
                break;
            }
        }
        if valid {
            images.push(bytes[start..cursor].to_vec());
            search = cursor;
        } else {
            search = start + 8;
        }
    }
    images
}

pub(crate) fn make_bundle(container: &Container, project: &Project) -> Result<Bundle> {
    let mut files = BTreeMap::new();
    files.insert(SOURCE.into(), container.bytes.clone());
    let program = container.data(
        &container.entries[project
            .program_index
            .ok_or_else(|| Error::Message("缺少 Program.s".into()))?],
    )?;
    std::str::from_utf8(program).map_err(|_| Error::Message("Program.s 不是有效 UTF-8".into()))?;
    files.insert("program.s".into(), program.to_vec());
    let mut pages = Vec::new();
    let mut mappings = Vec::new();
    let mut paths = Vec::new();
    for (page_id, (entry_index, page)) in project.pages.iter().enumerate() {
        let path = format!("pages/p{page_id:03}_{}", filename(&page.name));
        paths.push(path.clone());
        let mut objects = Vec::new();
        for (i, object) in page.objects.iter().enumerate() {
            let mut events = BTreeMap::new();
            for event in &object.events {
                if !event.lines.is_empty() {
                    let event_path = if i == 0 {
                        format!("events/page.{}.s", filename(&event.kind))
                    } else {
                        format!(
                            "events/o{i:03}_{}.{}.s",
                            filename(&object.name),
                            filename(&event.kind)
                        )
                    };
                    files.insert(format!("{path}/{event_path}"), script_file(&event.lines)?);
                    events.insert(event.kind.clone(), event_path);
                }
            }
            objects.push(ObjectFile {
                name: object.name.clone(),
                type_name: type_name(object.type_code),
                properties: properties(object)?,
                events,
            });
        }
        let own = objects.remove(0);
        let description = PageFile {
            name: page.name.clone(),
            properties: own.properties,
            events: own.events,
            objects,
        };
        files.insert(
            format!("{path}/page.toml"),
            page_to_toml(&description)?.into_bytes(),
        );
        pages.push(description);
        mappings.push(PageMapping {
            path,
            resource: container.entries[*entry_index].name.clone(),
            object_indices: (0..page.objects.len()).collect(),
        });
    }
    let mut assets = Vec::new();
    for entry in container.entries.iter().filter(|e| !e.deleted) {
        let bytes = container.data(entry)?;
        if entry.name.ends_with(".zi") {
            let path = format!(
                "assets/fonts/resource_{}.zi",
                filename(entry.name.trim_end_matches(".zi"))
            );
            assets.push(Asset {
                path: path.clone(),
                sha256: sha256(bytes),
            });
            files.insert(path, bytes.to_vec());
        } else if entry.name.ends_with(".is") || entry.name.ends_with(".ib") {
            for (i, png) in embedded_pngs(bytes).into_iter().enumerate() {
                let path = format!("assets/images/resource_{}_{i}.png", filename(&entry.name));
                assets.push(Asset {
                    path: path.clone(),
                    sha256: sha256(&png),
                });
                files.insert(path, png);
            }
        }
    }
    let project_file = ProjectFile {
        format_version: 1,
        encoding: "utf-8".into(),
        pages: paths,
    };
    files.insert("project.toml".into(), serialize(&project_file)?);
    let mut managed_files: Vec<String> = files.keys().cloned().collect();
    managed_files.push(META.into());
    managed_files.sort();
    let mut portable_paths = BTreeSet::new();
    for path in &managed_files {
        if !portable_paths.insert(path.to_ascii_lowercase()) {
            return Err(Error::Message(format!(
                "导出文件名在 Windows 下发生冲突：{path}"
            )));
        }
    }
    let manifest = Manifest {
        format_version: 1,
        source_sha256: sha256(&container.bytes),
        pages: mappings,
        assets,
        managed_files,
    };
    files.insert(META.into(), serialize(&manifest)?);
    Ok(Bundle {
        files,
        manifest,
        project: project_file,
        pages,
    })
}

pub fn relative(path: &str) -> Result<&Path> {
    if path.is_empty()
        || path.contains(['\\', ':', '\0'])
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(Error::Message(format!("不安全的项目相对路径：{path}")));
    }
    let value = Path::new(path);
    if value
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::Message(format!("路径不能越出项目目录：{path}")));
    }
    Ok(value)
}

pub(crate) fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub(crate) fn resolve(root: &Path, path: &str) -> Result<PathBuf> {
    let relative = relative(path)?;
    let mut cursor = root.to_owned();
    for component in relative.components() {
        cursor.push(component);
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if is_link(&metadata) => {
                return Err(Error::Message(format!(
                    "项目内不允许符号链接或目录联接：{path}"
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(cursor)
}

pub(crate) fn read(root: &Path, path: &str) -> Result<Vec<u8>> {
    fs::read(resolve(root, path)?).map_err(|e| Error::Message(format!("{path}：{e}")))
}

pub(crate) fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if is_link(&metadata) {
            return Err(Error::Message(format!(
                "导出目标中存在符号链接：{}",
                entry.path().display()
            )));
        }
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if metadata.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return Err(Error::Message("导出目标含不支持的文件类型".into()));
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct ExportReport {
    pub output: String,
    pub pages: usize,
    pub files: usize,
    pub source_sha256: String,
}

pub fn export(input: &Path, output: &Path, force: bool) -> Result<ExportReport> {
    // 将 existing/. 归一化，避免把事务目录创建在被复制的目标目录内部。
    let normalized = if output.exists() {
        if is_link(&fs::symlink_metadata(output)?) {
            return Err(Error::Message("导出目标不能是符号链接或目录联接".into()));
        }
        fs::canonicalize(output)?
    } else {
        std::path::absolute(output)?
    };
    let output = normalized.as_path();
    let container = Container::parse(fs::read(input)?)?;
    container.strict()?;
    let project = Project::parse(&container)?;
    project.strict()?;
    let bundle = make_bundle(&container, &project)?;
    if output.exists() && !force {
        return Err(Error::Message(
            "输出目录已存在；使用 --force 覆盖工具管理的文件".into(),
        ));
    }
    let existing = if output.exists() {
        let metadata = fs::symlink_metadata(output)?;
        if !metadata.is_dir() || is_link(&metadata) {
            return Err(Error::Message("输出目标必须是普通目录".into()));
        }
        let old: Manifest = deserialize(&read(output, META)?, META)?;
        if old.format_version != 1 {
            return Err(Error::Message("不能覆盖未知版本的工作目录".into()));
        }
        let source = read(output, SOURCE)?;
        if sha256(&source) != old.source_sha256 {
            return Err(Error::Message("不能覆盖元数据已损坏的项目".into()));
        }
        let old_container = Container::parse(source)?;
        old_container.strict()?;
        let old_project = Project::parse(&old_container)?;
        old_project.strict()?;
        if make_bundle(&old_container, &old_project)?.manifest != old {
            return Err(Error::Message("不能覆盖文件清单或映射已改变的项目".into()));
        }
        Some(old)
    } else {
        None
    };
    fs::create_dir_all(parent(output))?;
    let temporary = tempfile::Builder::new()
        .prefix(".hmi-export-")
        .tempdir_in(parent(output))?;
    let staged = temporary.path().join("result");
    if let Some(old) = existing.as_ref() {
        copy_tree(output, &staged)?;
        for path in &old.managed_files {
            let target = resolve(&staged, path)?;
            if target.is_file() {
                fs::remove_file(target)?;
            } else if target.exists() {
                return Err(Error::Message(format!("原工具文件变成了目录：{path}")));
            }
        }
    } else {
        fs::create_dir(&staged)?;
    }
    for (path, bytes) in &bundle.files {
        let target = resolve(&staged, path)?;
        fs::create_dir_all(parent(&target))?;
        fs::write(target, bytes)?;
    }
    let backup = temporary.path().join("previous");
    if existing.is_some() {
        fs::rename(output, &backup)?;
    }
    if let Err(error) = fs::rename(&staged, output) {
        if existing.is_some()
            && let Err(restore) = fs::rename(&backup, output)
        {
            let retained = temporary.keep();
            return Err(Error::Message(format!(
                "输出发布失败：{error}；恢复失败：{restore}；旧目录保留在 {}",
                retained.join("previous").display()
            )));
        }
        return Err(error.into());
    }
    Ok(ExportReport {
        output: output.display().to_string(),
        pages: bundle.pages.len(),
        files: bundle.files.len(),
        source_sha256: bundle.manifest.source_sha256,
    })
}

pub struct Prepared {
    pub bytes: Vec<u8>,
    pub diagnostics: Vec<Diagnostic>,
    pub changed_resources: Vec<String>,
    pub input_files: BTreeSet<String>,
}

fn encode_value(object: &Object, attr: &Attribute, input: &toml::Value) -> Result<Vec<u8>> {
    if !editable(&attr.name) {
        return Err(Error::Message("首版中此属性只读".into()));
    }
    let result = match scalar(object, &attr.name) {
        Some(Scalar::Text) => input
            .as_str()
            .ok_or_else(|| Error::Message("需要字符串".into()))?
            .as_bytes()
            .to_vec(),
        Some(Scalar::Signed32) if attr.value.len() == 4 => {
            let number = input
                .as_integer()
                .ok_or_else(|| Error::Message("需要整数".into()))?;
            i32::try_from(number)
                .map_err(|_| Error::Message("超出有符号 32 位范围".into()))?
                .to_le_bytes()
                .to_vec()
        }
        Some(Scalar::Unsigned(width)) if attr.value.len() == width => {
            let number = input
                .as_integer()
                .ok_or_else(|| Error::Message("需要整数".into()))?;
            let max = (1u64 << (width * 8)) - 1;
            if number < 0 || number as u64 > max {
                return Err(Error::Message(format!("超出 {width} 字节无符号整数范围")));
            }
            (number as u32).to_le_bytes()[..width].to_vec()
        }
        _ => {
            return Err(Error::Message(
                "此类型或属性宽度尚未验证，不允许修改".into(),
            ));
        }
    };
    Ok(result)
}

pub(crate) struct ObjectUpdate<'a> {
    pub root: &'a Path,
    pub document: &'a source::PageDocument,
    pub index: usize,
    pub references: &'a [crate::project::Reference],
    pub input_files: &'a mut BTreeSet<String>,
    pub scripts: Option<&'a BTreeMap<String, Vec<u8>>>,
}

pub(crate) fn update_object(
    ctx: ObjectUpdate<'_>,
    object: &Object,
    baseline: &ObjectFile,
    edited: &ObjectFile,
) -> Result<Vec<u8>> {
    let error = |field: Option<&str>, code, message: String| {
        ctx.document.error(ctx.index, field, code, message)
    };
    if edited.name != baseline.name {
        return Err(error(
            Some("name"),
            "object_name",
            "当前不支持修改控件名称".into(),
        ));
    }
    if edited.type_name != baseline.type_name {
        return Err(error(
            Some("type"),
            "object_type",
            "当前不支持修改控件类型".into(),
        ));
    }
    if edited.properties.keys().ne(baseline.properties.keys()) {
        let key = edited
            .properties
            .keys()
            .find(|key| !baseline.properties.contains_key(*key))
            .or_else(|| {
                baseline
                    .properties
                    .keys()
                    .find(|key| !edited.properties.contains_key(*key))
            })
            .unwrap();
        return Err(error(
            Some(key),
            "property_set",
            format!("属性集合发生变化，当前不支持添加或删除属性 {key}"),
        ));
    }
    let mut values = BTreeMap::new();
    for (key, input) in &edited.properties {
        if Some(input) == baseline.properties.get(key) {
            continue;
        }
        let attr = object
            .attribute(key)
            .ok_or_else(|| error(Some(key), "unknown_property", format!("控件没有属性 {key}")))?;
        let bytes = encode_value(object, attr, input)
            .map_err(|e| error(Some(key), "property_value", e.to_string()))?;
        if matches!(key.as_str(), "vscope" | "en") && bytes[0] > 1 {
            return Err(error(
                Some(key),
                "property_range",
                format!("属性值为 {}，允许值为 0 或 1", bytes[0]),
            ));
        }
        if key == "font"
            || matches!(
                key.as_str(),
                "pic" | "pic2" | "picc" | "picc2" | "pic1" | "picc1" | "bpic" | "ppic"
            )
        {
            let kind = if key == "font" { "zi" } else { "i" };
            let count = ctx.references.iter().filter(|r| r.kind == kind).count();
            let number = input.as_integer().unwrap() as usize;
            if number >= count && !(kind == "i" && number == 65535) {
                return Err(error(
                    Some(key),
                    "resource_id",
                    format!("资源 ID {number} 不存在，对应资源数量为 {count}，ID 必须小于 {count}"),
                ));
            }
        }
        values.insert(key.clone(), bytes);
    }
    let integer = |key: &str| -> Option<u32> {
        values
            .get(key)
            .map(|bytes| {
                bytes
                    .iter()
                    .enumerate()
                    .fold(0, |n, (i, b)| n | (u32::from(*b) << (i * 8)))
            })
            .or_else(|| object.integer(key))
    };
    if let (Some(text), Some(capacity)) = (
        values
            .get("txt")
            .or_else(|| object.attribute("txt").map(|a| &a.value)),
        integer("txt_maxl"),
    ) && text.len() > capacity as usize
    {
        let field = if values.contains_key("txt") {
            "txt"
        } else {
            "txt_maxl"
        };
        return Err(error(
            Some(field),
            "text_capacity",
            format!(
                "UTF-8 文本占用 {} 字节，超过 txt_maxl 允许的 {capacity} 字节（超出 {} 字节）",
                text.len(),
                text.len() - capacity as usize
            ),
        ));
    }
    if ["x", "y", "w", "h"]
        .iter()
        .any(|key| values.contains_key(*key))
    {
        let mut endpoints = Vec::new();
        for (position, size, endpoint) in [("x", "w", "endx"), ("y", "h", "endy")] {
            let start = integer(position).ok_or_else(|| {
                error(
                    Some(position),
                    "geometry",
                    format!("缺少位置属性 {position}"),
                )
            })?;
            let length = integer(size)
                .ok_or_else(|| error(Some(size), "geometry", format!("缺少尺寸属性 {size}")))?;
            if length == 0 {
                return Err(error(Some(size), "geometry", "尺寸必须大于 0".into()));
            }
            let field = if values.contains_key(position) {
                position
            } else {
                size
            };
            let end = start.checked_add(length).and_then(|n| n.checked_sub(1)).filter(|n| *n <= 65535)
                .ok_or_else(|| error(Some(field), "geometry", format!("计算 {endpoint} = {position} + {size} - 1 时超出 u16 范围（最大 65535）")))?;
            if object
                .attribute(endpoint)
                .is_none_or(|a| a.value.len() != 2)
            {
                return Err(error(
                    Some(field),
                    "geometry",
                    format!("端点属性 {endpoint} 的二进制布局尚未验证"),
                ));
            }
            endpoints.push((endpoint.to_owned(), (end as u16).to_le_bytes().to_vec()));
        }
        values.extend(endpoints);
    }
    let directory = ctx
        .document
        .file
        .rsplit_once('/')
        .map(|(directory, _)| directory)
        .unwrap_or("");
    let mut codes = BTreeMap::new();
    for kind in edited.events.keys() {
        if !object.events.iter().any(|e| &e.kind == kind) {
            return Err(error(
                Some(&format!("events.{kind}")),
                "event_slot",
                format!("该控件不存在 {kind} 事件槽"),
            ));
        }
    }
    for event in &object.events {
        let field = format!("events.{}", event.kind);
        let lines = if let Some(path) = edited.events.get(&event.kind) {
            relative(path).map_err(|e| error(Some(&field), "event_path", e.to_string()))?;
            let path = format!("{directory}/{path}");
            if !path.ends_with(".s") {
                return Err(error(
                    Some(&field),
                    "event_path",
                    "事件文件必须使用 .s 后缀".into(),
                ));
            }
            ctx.input_files.insert(path.clone());
            let bytes = if let Some(scripts) = ctx.scripts {
                scripts.get(&path).cloned().ok_or_else(|| {
                    error(
                        Some(&field),
                        "event_file",
                        format!("事件文件未读取：{path}"),
                    )
                })?
            } else {
                read(ctx.root, &path)
                    .map_err(|e| error(Some(&field), "event_file", e.to_string()))?
            };
            script_lines(&bytes)
                .map_err(|e| error(Some(&field), "script_encoding", e.to_string()))?
        } else {
            Vec::new()
        };
        codes.insert(event.kind.clone(), lines);
    }
    object.rebuild(&values, &codes)
}

/// 完整校验并在内存中生成工程，不写缓存或输出。
pub fn prepare(root: &Path) -> Result<Prepared> {
    let outcome = crate::build::run(
        root,
        None,
        &crate::build::BuildOptions {
            mode: crate::build::BuildMode::Check,
            ..Default::default()
        },
        &mut |_| {},
    );
    if !outcome.success {
        return Err(outcome
            .diagnostics
            .into_iter()
            .find(|d| d.severity == Severity::Error)
            .unwrap_or_else(|| Diagnostic::error("build_failed", "工程构建失败"))
            .into());
    }
    Ok(Prepared {
        bytes: outcome.bytes.expect("成功构建具有输出字节"),
        diagnostics: outcome.diagnostics,
        changed_resources: outcome.report.changed_resources,
        input_files: outcome.inputs,
    })
}

pub fn check(root: &Path, options: &crate::build::BuildOptions) -> crate::build::BuildOutcome {
    crate::build::run(
        root,
        None,
        &crate::build::BuildOptions {
            mode: crate::build::BuildMode::Check,
            warnings: options.warnings.clone(),
        },
        &mut |_| {},
    )
}

pub fn build(
    root: &Path,
    output: &Path,
    options: &crate::build::BuildOptions,
) -> crate::build::BuildOutcome {
    crate::build::run(root, Some(output), options, &mut |_| {})
}
