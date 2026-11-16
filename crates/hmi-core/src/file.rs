//! 文件读取和原子输出。输入缺失或解析失败时保留具体路径。

use crate::{Error, Result, diagnostic::Location, error};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

/// 比较尚未创建的输出位置时，先消除相对路径中的 . 和 ..。
pub fn absolute(path: &Path) -> Result<PathBuf> {
    let mut result = PathBuf::new();
    for component in std::path::absolute(path)?.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            _ => result.push(component.as_os_str()),
        }
    }
    Ok(result)
}

pub fn protect_output(path: &Path, inputs: &[&Path]) -> Result<()> {
    let output = absolute(path)?;
    let canonical = fs::canonicalize(path).ok();
    for input in inputs {
        if output == absolute(input)?
            || (canonical.is_some() && fs::canonicalize(input).ok() == canonical)
        {
            return Err(at_file(
                error("output_input", "输出文件不能覆盖输入文件"),
                path,
            ));
        }
    }
    Ok(())
}

/// 保留中间空行，末尾换行不额外生成空记录。
pub fn script_lines(text: &str) -> Result<Vec<Vec<u8>>> {
    if text.contains('\0') || text.starts_with('\u{feff}') {
        return Err(error(
            "script_encoding",
            "脚本须为不含 NUL 的 UTF-8 文件，且不带 BOM",
        ));
    }
    Ok(text
        .split_terminator('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).as_bytes().to_vec())
        .collect())
}

pub fn at_file(error: Error, path: &Path) -> Error {
    let mut diagnostic = error
        .as_diagnostic()
        .cloned()
        .unwrap_or_else(|| crate::diagnostic::Diagnostic::error("io", error.to_string()));
    if diagnostic.location.file.is_none() {
        diagnostic.location.file = Some(path.display().to_string());
    }
    diagnostic.into()
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|e| at_file(error("read_file", format!("无法读取文件：{e}")), path))
}

pub fn text(path: &Path) -> Result<String> {
    String::from_utf8(read(path)?)
        .map_err(|_| at_file(error("encoding", "文件必须使用 UTF-8 编码"), path))
}

pub fn read_toml<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let source = text(path)?;
    toml::from_str(&source).map_err(|e| {
        let line = e
            .span()
            .map(|span| source[..span.start].bytes().filter(|b| *b == b'\n').count() + 1);
        crate::diagnostic::Diagnostic::error("toml", format!("TOML 格式错误：{e}"))
            .at(Location {
                file: Some(path.display().to_string()),
                line,
                ..Default::default()
            })
            .into()
    })
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&read(path)?)
        .map_err(|e| at_file(error("json", format!("JSON 格式错误：{e}")), path))
}

pub fn json(value: &impl Serialize) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|e| error("serialize", e.to_string()))
}

pub fn toml(value: &impl Serialize) -> Result<Vec<u8>> {
    toml::to_string_pretty(value)
        .map(String::into_bytes)
        .map_err(|e| error("serialize", e.to_string()))
}

/// 输出通过校验后再替换原文件，禁止输出覆盖任一已声明输入。
pub fn write_atomic(path: &Path, bytes: &[u8], inputs: &[&Path]) -> Result<()> {
    stage_atomic(path, bytes, inputs)?.publish(inputs)
}

/// 多文件输出先全部暂存，任一暂存失败时不替换已有产物。
pub struct StagedFile {
    path: PathBuf,
    temporary: tempfile::NamedTempFile,
}

impl StagedFile {
    pub fn publish(self, inputs: &[&Path]) -> Result<()> {
        protect_output(&self.path, inputs)?;
        self.temporary.persist(&self.path).map_err(|e| {
            at_file(
                error("write_file", format!("无法写入文件：{}", e.error)),
                &self.path,
            )
        })?;
        Ok(())
    }
}

pub fn stage_atomic(path: &Path, bytes: &[u8], inputs: &[&Path]) -> Result<StagedFile> {
    protect_output(path, inputs)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| at_file(e.into(), parent))?;
    protect_output(path, inputs)?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| at_file(e.into(), path))?;
    temporary
        .write_all(bytes)
        .map_err(|e| at_file(e.into(), path))?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|e| at_file(e.into(), path))?;
    Ok(StagedFile {
        path: path.to_path_buf(),
        temporary,
    })
}
