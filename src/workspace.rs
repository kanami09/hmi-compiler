//! 版本 1 的 TOML 项目、保真快照与事务式文件输出。
mod format;
pub use format::page_to_toml;

use crate::container::Container;
use crate::diagnostic::{Diagnostic, Severity};
use crate::page::{Attribute, Object, type_name};
use crate::project::Project;
use crate::{Error, Result, hex, sha256};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
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

struct Bundle {
    files: BTreeMap<String, Vec<u8>>,
    manifest: Manifest,
    project: ProjectFile,
    pages: Vec<PageFile>,
}

fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    toml::to_string_pretty(value)
        .map(String::into_bytes)
        .map_err(|e| Error(format!("TOML 序列化失败：{e}")))
}

fn deserialize<T: serde::de::DeserializeOwned>(bytes: &[u8], path: &str) -> Result<T> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error(format!("{path}：文件不是 UTF-8")))?;
    toml::from_str(text).map_err(|e| Error(format!("{path}：{e}")))
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
                .map_err(|_| Error(format!("{}.{} 不是有效 UTF-8", object.name, attr.name)))?
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
        std::str::from_utf8(line).map_err(|_| Error("事件代码不是有效 UTF-8".into()))?;
        if line.contains(&b'\n') || line.contains(&b'\r') {
            return Err(Error("单条事件记录内嵌换行，首版不能无歧义导出".into()));
        }
        result.extend_from_slice(line);
        result.push(b'\n');
    }
    Ok(result)
}

/// 空文件是零行；末尾的一个分隔符结束当前行，不额外产生一行。
pub fn script_lines(bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error("脚本不是有效 UTF-8".into()))?;
    if text.contains('\0') {
        return Err(Error("脚本不能包含 NUL".into()));
    }
    let normalized = text.replace("\r\n", "\n");
    if normalized.contains('\r') {
        return Err(Error("脚本包含独立 CR，请使用 LF 或 CRLF".into()));
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

fn make_bundle(container: &Container, project: &Project) -> Result<Bundle> {
    let mut files = BTreeMap::new();
    files.insert(SOURCE.into(), container.bytes.clone());
    let program = container.data(
        &container.entries[project
            .program_index
            .ok_or_else(|| Error("缺少 Program.s".into()))?],
    )?;
    std::str::from_utf8(program).map_err(|_| Error("Program.s 不是有效 UTF-8".into()))?;
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
            return Err(Error(format!("导出文件名在 Windows 下发生冲突：{path}")));
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
        return Err(Error(format!("不安全的项目相对路径：{path}")));
    }
    let value = Path::new(path);
    if value
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error(format!("路径不能越出项目目录：{path}")));
    }
    Ok(value)
}

fn is_link(metadata: &fs::Metadata) -> bool {
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

fn resolve(root: &Path, path: &str) -> Result<PathBuf> {
    let relative = relative(path)?;
    let mut cursor = root.to_owned();
    for component in relative.components() {
        cursor.push(component);
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if is_link(&metadata) => {
                return Err(Error(format!("项目内不允许符号链接或目录联接：{path}")));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(cursor)
}

fn read(root: &Path, path: &str) -> Result<Vec<u8>> {
    fs::read(resolve(root, path)?).map_err(|e| Error(format!("{path}：{e}")))
}

fn parent(path: &Path) -> &Path {
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
            return Err(Error(format!(
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
            return Err(Error("导出目标含不支持的文件类型".into()));
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
            return Err(Error("导出目标不能是符号链接或目录联接".into()));
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
        return Err(Error(
            "输出目录已存在；使用 --force 覆盖工具管理的文件".into(),
        ));
    }
    let existing = if output.exists() {
        let metadata = fs::symlink_metadata(output)?;
        if !metadata.is_dir() || is_link(&metadata) {
            return Err(Error("输出目标必须是普通目录".into()));
        }
        let old: Manifest = deserialize(&read(output, META)?, META)?;
        if old.format_version != 1 {
            return Err(Error("不能覆盖未知版本的工作目录".into()));
        }
        let source = read(output, SOURCE)?;
        if sha256(&source) != old.source_sha256 {
            return Err(Error("不能覆盖元数据已损坏的项目".into()));
        }
        let old_container = Container::parse(source)?;
        old_container.strict()?;
        let old_project = Project::parse(&old_container)?;
        old_project.strict()?;
        if make_bundle(&old_container, &old_project)?.manifest != old {
            return Err(Error("不能覆盖文件清单或映射已改变的项目".into()));
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
                return Err(Error(format!("原工具文件变成了目录：{path}")));
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
            return Err(Error(format!(
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
        return Err(Error("首版中此属性只读".into()));
    }
    let result = match scalar(object, &attr.name) {
        Some(Scalar::Text) => input
            .as_str()
            .ok_or_else(|| Error("需要字符串".into()))?
            .as_bytes()
            .to_vec(),
        Some(Scalar::Signed32) if attr.value.len() == 4 => {
            let number = input.as_integer().ok_or_else(|| Error("需要整数".into()))?;
            i32::try_from(number)
                .map_err(|_| Error("超出有符号 32 位范围".into()))?
                .to_le_bytes()
                .to_vec()
        }
        Some(Scalar::Unsigned(width)) if attr.value.len() == width => {
            let number = input.as_integer().ok_or_else(|| Error("需要整数".into()))?;
            let max = (1u64 << (width * 8)) - 1;
            if number < 0 || number as u64 > max {
                return Err(Error(format!("超出 {width} 字节无符号整数范围")));
            }
            (number as u32).to_le_bytes()[..width].to_vec()
        }
        _ => return Err(Error("此类型或属性宽度尚未验证，不允许修改".into())),
    };
    Ok(result)
}

fn update_object(
    root: &Path,
    directory: &str,
    object: &Object,
    baseline: &ObjectFile,
    edited: &ObjectFile,
    project: &Project,
    input_files: &mut BTreeSet<String>,
) -> Result<Vec<u8>> {
    let location = format!("{directory}/page.toml：控件 {}", object.name);
    if edited.name != baseline.name || edited.type_name != baseline.type_name {
        return Err(Error(format!("{location}：首版禁止改变名称或类型")));
    }
    if edited.properties.keys().ne(baseline.properties.keys()) {
        return Err(Error(format!("{location}：禁止添加或删除属性")));
    }
    let mut values = BTreeMap::new();
    for (key, input) in &edited.properties {
        if Some(input) == baseline.properties.get(key) {
            continue;
        }
        let attr = object
            .attribute(key)
            .ok_or_else(|| Error(format!("{location}：未知属性 {key}")))?;
        let bytes = encode_value(object, attr, input)
            .map_err(|e| Error(format!("{location}.{key}：{e}")))?;
        if matches!(key.as_str(), "vscope" | "en") && bytes[0] > 1 {
            return Err(Error(format!("{location}.{key}：只允许 0 或 1")));
        }
        if key == "font"
            || matches!(
                key.as_str(),
                "pic" | "pic2" | "picc" | "picc2" | "pic1" | "picc1" | "bpic" | "ppic"
            )
        {
            let kind = if key == "font" { "zi" } else { "i" };
            let count = project.references.iter().filter(|r| r.kind == kind).count();
            let number = input.as_integer().unwrap() as usize;
            if number >= count && !(kind == "i" && number == 65535) {
                return Err(Error(format!(
                    "{location}.{key}：资源 ID {number} 不存在，{kind} 资源共 {count} 个"
                )));
            }
        }
        values.insert(key.clone(), bytes);
    }
    let integer = |key: &str| -> Option<u32> {
        if let Some(bytes) = values.get(key) {
            Some(
                bytes
                    .iter()
                    .enumerate()
                    .fold(0, |n, (i, b)| n | (u32::from(*b) << (i * 8))),
            )
        } else {
            object.integer(key)
        }
    };
    if let (Some(text), Some(capacity)) = (
        values
            .get("txt")
            .or_else(|| object.attribute("txt").map(|a| &a.value)),
        integer("txt_maxl"),
    ) && text.len() > capacity as usize
    {
        return Err(Error(format!(
            "{location}.txt：文本 {} 字节，超过 txt_maxl={capacity}",
            text.len()
        )));
    }
    let geometry_changed = ["x", "y", "w", "h"]
        .iter()
        .any(|key| values.contains_key(*key));
    if geometry_changed {
        let mut endpoints = Vec::new();
        for (position, size, endpoint) in [("x", "w", "endx"), ("y", "h", "endy")] {
            let start =
                integer(position).ok_or_else(|| Error(format!("{location}：缺少 {position}")))?;
            let length = integer(size).ok_or_else(|| Error(format!("{location}：缺少 {size}")))?;
            let end = start
                .checked_add(length)
                .and_then(|n| n.checked_sub(1))
                .filter(|n| length > 0 && *n <= 65535)
                .ok_or_else(|| {
                    Error(format!("{location}：尺寸必须大于 0，且端点不能超过 65535"))
                })?;
            if object
                .attribute(endpoint)
                .is_none_or(|a| a.value.len() != 2)
            {
                return Err(Error(format!("{location}：未验证的几何字段")));
            }
            endpoints.push((endpoint.to_owned(), (end as u16).to_le_bytes().to_vec()));
        }
        values.extend(endpoints);
    }
    let mut codes = BTreeMap::new();
    for kind in edited.events.keys() {
        if !object.events.iter().any(|e| &e.kind == kind) {
            return Err(Error(format!("{location}：对象没有 {kind} 事件槽")));
        }
    }
    for event in &object.events {
        let lines = if let Some(path) = edited.events.get(&event.kind) {
            let path = format!(
                "{directory}/{}",
                relative(path)?.to_string_lossy().replace('\\', "/")
            );
            if !path.ends_with(".s") {
                return Err(Error(format!("事件文件必须使用 .s 后缀：{path}")));
            }
            input_files.insert(path.clone());
            script_lines(&read(root, &path)?).map_err(|e| Error(format!("{path}：{e}")))?
        } else {
            Vec::new()
        };
        codes.insert(event.kind.clone(), lines);
    }
    object.rebuild(&values, &codes)
}

pub fn prepare(root: &Path) -> Result<Prepared> {
    let manifest: Manifest = deserialize(&read(root, META)?, META)?;
    if manifest.format_version != 1 {
        return Err(Error("不支持的原始元数据版本".into()));
    }
    let source = read(root, SOURCE)?;
    if sha256(&source) != manifest.source_sha256 {
        return Err(Error(
            ".hmi/container.bin 摘要不匹配，原始快照已改变".into(),
        ));
    }
    let container = Container::parse(source)?;
    container.strict()?;
    let project = Project::parse(&container)?;
    project.strict()?;
    let expected = make_bundle(&container, &project)?;
    if manifest != expected.manifest {
        return Err(Error("只读元数据或资源映射已改变".into()));
    }
    let description: ProjectFile = deserialize(&read(root, "project.toml")?, "project.toml")?;
    if description != expected.project {
        return Err(Error(
            "工程格式、编码或页面顺序发生变化；首版禁止结构编辑".into(),
        ));
    }
    for asset in &manifest.assets {
        if sha256(&read(root, &asset.path)?) != asset.sha256 {
            return Err(Error(format!("只读素材已修改：{}", asset.path)));
        }
    }
    let mut inputs: BTreeSet<_> = manifest.managed_files.iter().cloned().collect();
    let mut replacements = BTreeMap::new();
    let mut script_sources = Vec::new();
    for (i, ((entry_index, page), mapping)) in project.pages.iter().zip(&manifest.pages).enumerate()
    {
        let path = format!("{}/page.toml", mapping.path);
        let edited: PageFile = deserialize(&read(root, &path)?, &path)?;
        let baseline = &expected.pages[i];
        if edited.name != baseline.name || edited.objects.len() != baseline.objects.len() {
            return Err(Error(format!("{path}：首版禁止改变页面名或对象数量")));
        }
        let baseline_own = ObjectFile {
            name: baseline.name.clone(),
            type_name: "page".into(),
            properties: baseline.properties.clone(),
            events: baseline.events.clone(),
        };
        let edited_own = ObjectFile {
            name: edited.name.clone(),
            type_name: "page".into(),
            properties: edited.properties,
            events: edited.events,
        };
        let all_baseline: Vec<_> = std::iter::once(&baseline_own)
            .chain(baseline.objects.iter())
            .collect();
        let all_edited: Vec<_> = std::iter::once(&edited_own)
            .chain(edited.objects.iter())
            .collect();
        let mut objects = Vec::new();
        for (object_index, ((object, baseline), edited)) in page
            .objects
            .iter()
            .zip(all_baseline)
            .zip(all_edited)
            .enumerate()
        {
            objects.push(update_object(
                root,
                &mapping.path,
                object,
                baseline,
                edited,
                &project,
                &mut inputs,
            )?);
            for path in edited.events.values() {
                let file = format!("{}/{path}", mapping.path);
                let text = String::from_utf8(read(root, &file)?)
                    .map_err(|_| Error(format!("{file} 不是 UTF-8")))?;
                script_sources.push((Some(i), Some(object_index), file, text));
            }
        }
        let bytes = page.rebuild(&objects)?;
        if bytes != page.raw {
            replacements.insert(*entry_index, bytes);
        }
    }
    let program = read(root, "program.s")?;
    let program_text =
        std::str::from_utf8(&program).map_err(|_| Error("program.s 不是 UTF-8".into()))?;
    if program_text.contains('\0') {
        return Err(Error("program.s 不能包含 NUL".into()));
    }
    let index = project.program_index.unwrap();
    if program != container.data(&container.entries[index])? {
        replacements.insert(index, program.clone());
    }
    script_sources.push((None, None, "program.s".into(), program_text.into()));
    let bytes = container.rebuild(&replacements)?;
    // 回编输出必须能够重新通过全部结构与 CRC 检查。
    let rebuilt = Container::parse(bytes.clone())?;
    rebuilt.strict()?;
    let rebuilt_project = Project::parse(&rebuilt)?;
    rebuilt_project.strict()?;
    let diagnostics = crate::script::check(&rebuilt_project, &script_sources);
    for entry in &container.entries {
        let expected = replacements
            .get(&entry.index)
            .map(Vec::as_slice)
            .unwrap_or(container.data(entry)?);
        if rebuilt.data(&rebuilt.entries[entry.index])? != expected {
            return Err(Error(format!("回编验证发现非预期资源变化：{}", entry.name)));
        }
    }
    Ok(Prepared {
        bytes,
        diagnostics,
        changed_resources: replacements
            .keys()
            .map(|i| container.entries[*i].name.clone())
            .collect(),
        input_files: inputs,
    })
}

#[derive(Debug, Serialize)]
pub struct CheckReport {
    pub changed_resources: Vec<String>,
    pub byte_identical: bool,
    pub output_size: usize,
}

pub fn check(root: &Path, deny_warnings: bool) -> Result<(CheckReport, Vec<Diagnostic>)> {
    let prepared = prepare(root)?;
    if deny_warnings && !prepared.diagnostics.is_empty() {
        return Err(Error(format!(
            "--deny-warnings：发现 {} 条警告",
            prepared.diagnostics.len()
        )));
    }
    let report = CheckReport {
        byte_identical: prepared.changed_resources.is_empty(),
        changed_resources: prepared.changed_resources,
        output_size: prepared.bytes.len(),
    };
    Ok((report, prepared.diagnostics))
}

#[derive(Debug, Serialize)]
pub struct BuildReport {
    pub output: String,
    pub sha256: String,
    pub size: usize,
    pub changed_resources: Vec<String>,
    pub byte_identical: bool,
}

pub fn build(
    root: &Path,
    output: &Path,
    force: bool,
    deny_warnings: bool,
) -> Result<(BuildReport, Vec<Diagnostic>)> {
    let prepared = prepare(root)?;
    if deny_warnings
        && prepared
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning)
    {
        return Err(Error(format!(
            "--deny-warnings：发现 {} 条警告",
            prepared.diagnostics.len()
        )));
    }
    if output.exists() {
        if !force {
            return Err(Error("输出文件已存在；使用 --force 允许覆盖".into()));
        }
        let canonical_output = fs::canonicalize(output)?;
        for path in &prepared.input_files {
            if let Ok(input) = fs::canonicalize(resolve(root, path)?)
                && input == canonical_output
            {
                return Err(Error("构建输出不能覆盖项目输入文件".into()));
            }
        }
    }
    fs::create_dir_all(parent(output))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent(output))?;
    temporary.write_all(&prepared.bytes)?;
    temporary.as_file_mut().sync_all()?;
    if force {
        temporary
            .persist(output)
            .map_err(|e| Error(format!("发布输出失败：{}", e.error)))?;
    } else {
        temporary
            .persist_noclobber(output)
            .map_err(|e| Error(format!("发布输出失败：{}", e.error)))?;
    }
    Ok((
        BuildReport {
            output: output.display().to_string(),
            sha256: sha256(&prepared.bytes),
            size: prepared.bytes.len(),
            byte_identical: prepared.changed_resources.is_empty(),
            changed_resources: prepared.changed_resources,
        },
        prepared.diagnostics,
    ))
}
