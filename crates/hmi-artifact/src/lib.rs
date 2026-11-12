//! 页面目标文件：版本标识、JSON 描述、二进制载荷以及内容摘要。
//!
//! 当前阶段仅有页面自身对象，无跨资源重定位；普通控件接入时扩展结构化记录。

use hmi_analysis::ScriptFacts;
use hmi_core::{Result, error, file, model::check_schema, sha256, target::Target};
use serde::{Deserialize, Serialize};
use std::path::Path;

const MAGIC: &[u8; 8] = b"HMIOBJ01";
const HEADER_SIZE: usize = 80;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Page,
    Program,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub schema: String,
    pub kind: ObjectKind,
    pub name: Option<String>,
    pub target: Target,
    pub producer: String,
    pub scripts: Vec<ScriptFacts>,
}

#[derive(Clone, Debug)]
pub struct CompiledObject {
    pub metadata: Metadata,
    pub payload: Vec<u8>,
}

impl CompiledObject {
    pub fn new(
        kind: ObjectKind,
        name: Option<String>,
        target: &Target,
        scripts: Vec<ScriptFacts>,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            metadata: Metadata {
                schema: "hmi.object/1".into(),
                kind,
                name,
                target: target.clone(),
                producer: format!("hmi-compile/{}", env!("CARGO_PKG_VERSION")),
                scripts,
            },
            payload,
        }
    }

    pub fn validate(&self) -> Result<()> {
        check_schema(&self.metadata.schema, "hmi.object/1")?;
        self.metadata.target.validate()?;
        for facts in &self.metadata.scripts {
            facts.validate()?;
        }
        match (&self.metadata.kind, &self.metadata.name) {
            (ObjectKind::Page, Some(name)) => hmi_core::model::check_name(name)?,
            (ObjectKind::Program, None) => {}
            _ => {
                return Err(error(
                    "object_kind",
                    "页面目标须有页面名称，全局代码目标不能指定页面名称",
                ));
            }
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let metadata = serde_json::to_vec(&self.metadata)
            .map_err(|e| error("object_metadata", e.to_string()))?;
        let meta_len = u32::try_from(metadata.len())
            .map_err(|_| error("object_length", "目标文件描述超过 u32 范围"))?;
        let payload_len = u32::try_from(self.payload.len())
            .map_err(|_| error("object_length", "目标文件载荷超过 u32 范围"))?;
        let mut output = vec![0; HEADER_SIZE];
        output[..8].copy_from_slice(MAGIC);
        output[8..12].copy_from_slice(&meta_len.to_le_bytes());
        output[12..16].copy_from_slice(&payload_len.to_le_bytes());
        output.extend(metadata);
        output.extend(&self.payload);
        let digest = sha256(&output[HEADER_SIZE..]);
        output[16..80].copy_from_slice(digest.as_bytes());
        Ok(output)
    }

    /// 先检查实际边界和摘要，再解析描述；不根据外部长度预分配载荷。
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER_SIZE || &bytes[..8] != MAGIC {
            return Err(error("object_format", "不是受支持的 .hmo 目标文件"));
        }
        let meta_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let payload_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let meta_end = HEADER_SIZE
            .checked_add(meta_len)
            .ok_or_else(|| error("object_length", "目标文件描述长度溢出"))?;
        if meta_end.checked_add(payload_len) != Some(bytes.len()) {
            return Err(error("object_length", "目标文件长度与头部记录不一致"));
        }
        if sha256(&bytes[HEADER_SIZE..]).as_bytes() != &bytes[16..80] {
            return Err(error("object_hash", "目标文件摘要不匹配，请重新编译"));
        }
        let metadata = serde_json::from_slice(&bytes[HEADER_SIZE..meta_end])
            .map_err(|e| error("object_metadata", format!("目标文件描述无效：{e}")))?;
        let object = Self {
            metadata,
            payload: bytes[meta_end..].to_vec(),
        };
        object.validate()?;
        Ok(object)
    }

    pub fn read(path: &Path) -> Result<Self> {
        Self::decode(&file::read(path)?).map_err(|e| file::at_file(e, path))
    }

    pub fn write(&self, path: &Path, inputs: &[&Path]) -> Result<()> {
        file::write_atomic(path, &self.encode()?, inputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmi_core::target::TargetConfig;

    #[test]
    fn target_file_rejects_corruption_truncation_and_unknown_version() {
        let target = Target::resolve(&TargetConfig {
            profile: "TJC4827X543_011".into(),
            format: "ver21234".into(),
            encoding: "utf-8".into(),
            orientation: 0,
        })
        .unwrap();
        let object = CompiledObject::new(
            ObjectKind::Program,
            None,
            &target,
            vec![],
            b"page 0\n".to_vec(),
        );
        let bytes = object.encode().unwrap();
        assert_eq!(
            CompiledObject::decode(&bytes).unwrap().payload,
            object.payload
        );
        for n in 0..bytes.len() {
            assert!(CompiledObject::decode(&bytes[..n]).is_err());
        }
        let mut corrupt = bytes.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert_eq!(
            CompiledObject::decode(&corrupt)
                .unwrap_err()
                .as_diagnostic()
                .unwrap()
                .code,
            "object_hash"
        );
        let mut unknown = bytes.clone();
        unknown[7] = b'2';
        assert!(CompiledObject::decode(&unknown).is_err());
        let mut overflow = bytes;
        overflow[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(CompiledObject::decode(&overflow).is_err());
    }
}
