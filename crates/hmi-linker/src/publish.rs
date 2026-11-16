//! 共用输出流程：检查路径，暂存全部文件，发布配套文件，最后替换 HMI。
use crate::Linked;
use hmi_core::{Result, error, file};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Publication {
    outputs: Vec<(PathBuf, Vec<u8>)>,
    inputs: Vec<PathBuf>,
}

/// 未创建的路径也通过最近存在的父目录解析，识别目录链接和祖先文件冲突。
fn resolved(path: &Path) -> Result<PathBuf> {
    let path = file::absolute(path)?;
    let mut cursor = path.as_path();
    let mut tail = Vec::new();
    loop {
        match fs::canonicalize(cursor) {
            Ok(mut base) => {
                if !tail.is_empty() && !base.is_dir() {
                    return Err(file::at_file(
                        error("output_path", "输出的父路径不是目录"),
                        cursor,
                    ));
                }
                for component in tail.into_iter().rev() {
                    base.push(component);
                }
                return Ok(base);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // 断开的目录链接也不作为普通待创建目录处理。
                if fs::symlink_metadata(cursor).is_ok() {
                    return Err(file::at_file(
                        error("output_path", "输出路径包含无效链接"),
                        cursor,
                    ));
                }
                tail.push(
                    cursor
                        .file_name()
                        .ok_or_else(|| error("output_path", "无法解析输出路径"))?
                        .to_os_string(),
                );
                cursor = cursor
                    .parent()
                    .ok_or_else(|| error("output_path", "无法解析输出目录"))?;
            }
            Err(e) => return Err(file::at_file(e.into(), cursor)),
        }
    }
}

fn path_key(path: &Path) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.into()
    }
}

impl Publication {
    /// 所有输出与输入、彼此之间的冲突在任何写入之前检查。
    pub fn prepare(
        linked: &Linked,
        output: &Path,
        map: Option<&Path>,
        inputs: &[&Path],
    ) -> Result<Self> {
        let output = file::absolute(output)?;
        let root = output
            .parent()
            .ok_or_else(|| error("output_path", "HMI 输出缺少父目录"))?;
        let canonical_root = path_key(&resolved(root)?);
        let mut outputs = Vec::new();
        for external in &linked.external_files {
            hmi_core::deployment::validate(&external.path)?;
            let path = root.join(&external.path);
            if !path_key(&resolved(&path)?).starts_with(&canonical_root) {
                return Err(file::at_file(
                    error("deployment_path", "外部输出通过目录链接指向了发布目录之外"),
                    &path,
                ));
            }
            outputs.push((path, external.bytes.clone()));
        }
        if let Some(map) = map {
            outputs.push((file::absolute(map)?, file::json(&linked.map)?));
        }
        outputs.push((output, linked.bytes.clone()));
        let protected = inputs
            .iter()
            .map(|p| resolved(p).map(|p| path_key(&p)))
            .collect::<Result<Vec<_>>>()?;
        let mut destinations: Vec<PathBuf> = Vec::new();
        for (path, _) in &outputs {
            file::protect_output(path, inputs)?;
            let canonical = path_key(&resolved(path)?);
            if protected
                .iter()
                .any(|p| canonical == *p || p.starts_with(&canonical) || canonical.starts_with(p))
            {
                return Err(file::at_file(
                    error("output_input", "输出路径不能覆盖输入文件或与其目录结构冲突"),
                    path,
                ));
            }
            if destinations
                .iter()
                .any(|p| canonical == *p || p.starts_with(&canonical) || canonical.starts_with(p))
            {
                return Err(file::at_file(
                    error("output_path", "输出文件之间存在同名或目录冲突"),
                    path,
                ));
            }
            if let Ok(metadata) = fs::metadata(path)
                && (metadata.is_dir() || metadata.permissions().readonly())
            {
                return Err(file::at_file(
                    error("output_path", "输出路径是目录或文件为只读"),
                    path,
                ));
            }
            destinations.push(canonical);
        }
        Ok(Self {
            outputs,
            inputs: inputs.iter().map(|p| p.to_path_buf()).collect(),
        })
    }

    pub fn write(self) -> Result<()> {
        let inputs = self.inputs.iter().map(PathBuf::as_path).collect::<Vec<_>>();
        let mut staged = Vec::new();
        for (path, bytes) in &self.outputs {
            staged.push((path.clone(), file::stage_atomic(path, bytes, &inputs)?));
        }
        let mut published = Vec::new();
        for (path, file) in staged {
            if let Err(e) = file.publish(&inputs) {
                if published.is_empty() {
                    return Err(e);
                }
                return Err(error(
                    "publish_partial",
                    format!(
                        "写入 {} 失败：{e}。已写入：{}",
                        path.display(),
                        published.join("、")
                    ),
                ));
            }
            published.push(path.display().to_string());
        }
        Ok(())
    }
}
