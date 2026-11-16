//! 屏幕运行时文件需求。这里的路径是设备路径，不能用作构建机的读写路径。
use crate::{Result, error};
use serde::{Deserialize, Serialize};

/// 数据文件的结构参数；record_bytes 对应 lenth，不包含槽位末尾的 NUL。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DataRecordStructure {
    pub record_bytes: u32,
    pub max_records: u32,
    pub columns: u16,
}

/// 与媒体资产分开记录的文件需求，不表示链接器已经生成或部署了文件。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeFile {
    ExternalPicture {
        path: String,
    },
    DataRecord {
        path: String,
        structure: DataRecordStructure,
        create_if_missing: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeDependency {
    /// 页面目标中的对象序号；最终页面和控件 ID 由链接器确定。
    pub object: usize,
    pub file: RuntimeFile,
}

impl RuntimeFile {
    pub fn path(&self) -> &str {
        match self {
            Self::ExternalPicture { path } => path,
            Self::DataRecord { path, .. } => path,
        }
    }

    /// 仅从已编译记录提取依赖，独立链接不需要回读页面源码或访问虚拟 SD。
    pub fn from_control(code: u8, attributes: &[(String, Vec<u8>)]) -> Result<Option<Self>> {
        if !matches!(code, 60 | 66) {
            return Ok(None);
        }
        let bytes = |key: &str| -> Result<&[u8]> {
            attributes
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, b)| b.as_slice())
                .ok_or_else(|| error("runtime_dependency", format!("控件缺少文件依赖属性 {key}")))
        };
        let integer = |key: &str, width: usize| -> Result<u32> {
            let b = bytes(key)?;
            if b.len() != width {
                return Err(error(
                    "runtime_dependency",
                    format!("数据记录依赖属性 {key} 的宽度无效"),
                ));
            }
            Ok(b.iter()
                .enumerate()
                .fold(0, |n, (i, b)| n | (u32::from(*b) << (i * 8))))
        };
        let path = std::str::from_utf8(bytes("path")?)
            .map_err(|_| error("runtime_file_path", "运行时文件路径必须是 UTF-8 文本"))?;
        // 空字符串仅用于外部资产符号的重定位占位，最终链接仍须绑定非空路径。
        if code == 60 && path.is_empty() {
            return Ok(None);
        }
        if path.is_empty() || path.len() > 255 || path.as_bytes().contains(&0) {
            return Err(error(
                "runtime_file_path",
                "运行时文件路径不能为空、包含 NUL 或超过 255 字节",
            ));
        }
        if code == 60 {
            return Ok(Some(Self::ExternalPicture { path: path.into() }));
        }
        let record_bytes = integer("lenth", 4)?;
        let max_records = integer("maxval", 4)?;
        let columns = integer("dez", 2)?;
        let mode = integer("mode", 1)?;
        if record_bytes > 255 || !(1..=12).contains(&columns) || mode > 1 {
            return Err(error(
                "runtime_dependency",
                "数据记录文件的结构参数或创建模式无效",
            ));
        }
        Ok(Some(Self::DataRecord {
            path: path.into(),
            structure: DataRecordStructure {
                record_bytes,
                max_records,
                columns: columns as u16,
            },
            create_if_missing: mode == 1,
        }))
    }
}
