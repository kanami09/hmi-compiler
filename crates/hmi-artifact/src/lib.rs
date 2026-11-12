//! 页面目标文件：版本标识、JSON 描述、二进制载荷以及内容摘要。
//!
//! 页面目标保存对象记录和重定位表；最终 ID、页面索引及 CRC 由链接器生成。

use hmi_analysis::ScriptFacts;
use hmi_core::{Result, error, file, model::check_schema, sha256, target::Target};
use serde::{Deserialize, Serialize};
use std::path::Path;

const MAGIC: &[u8; 8] = b"HMIOBJ02";
const HEADER_SIZE: usize = 80;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Page,
    Program,
}

/// 每段载荷是一条完整对象记录，不包含原生页面头或对象索引。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectRecord {
    pub name: String,
    pub type_code: u8,
    pub offset: u32,
    pub length: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageLayout {
    pub objects: Vec<ObjectRecord>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelocationKind {
    ObjectId,
}

/// object 指目标文件内的记录序号，不是预先分配的 HMI 控件 ID。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relocation {
    pub object: usize,
    pub property: String,
    pub kind: RelocationKind,
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
    pub layout: Option<PageLayout>,
    pub relocations: Vec<Relocation>,
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
                schema: "hmi.object/2".into(),
                kind,
                name,
                target: target.clone(),
                producer: format!("hmi-compile/{}", env!("CARGO_PKG_VERSION")),
                scripts,
                layout: None,
                relocations: Vec::new(),
            },
            payload,
        }
    }

    /// 编译器只打包对象记录；ID 使用零占位，链接器应用重定位后再生成 .pa。
    pub fn page(
        name: String,
        target: &Target,
        scripts: Vec<ScriptFacts>,
        records: Vec<(u8, String, Vec<u8>)>,
    ) -> Result<Self> {
        let mut object = Self::new(ObjectKind::Page, Some(name), target, scripts, Vec::new());
        let mut layout = PageLayout {
            objects: Vec::new(),
        };
        for (index, (type_code, name, bytes)) in records.into_iter().enumerate() {
            let offset = u32::try_from(object.payload.len())
                .map_err(|_| error("object_length", "对象载荷偏移超过 u32 范围"))?;
            let length = u32::try_from(bytes.len())
                .map_err(|_| error("object_length", "对象记录长度超过 u32 范围"))?;
            layout.objects.push(ObjectRecord {
                name,
                type_code,
                offset,
                length,
            });
            object.payload.extend(bytes);
            if index > 0 {
                object.metadata.relocations.push(Relocation {
                    object: index,
                    property: "id".into(),
                    kind: RelocationKind::ObjectId,
                });
            }
        }
        object.metadata.layout = Some(layout);
        object.validate()?;
        Ok(object)
    }

    pub fn record(&self, index: usize) -> Result<&[u8]> {
        let record = self
            .metadata
            .layout
            .as_ref()
            .and_then(|layout| layout.objects.get(index))
            .ok_or_else(|| error("object_record", "目标文件中不存在指定的对象记录"))?;
        let start = record.offset as usize;
        let end = start
            .checked_add(record.length as usize)
            .ok_or_else(|| error("object_length", "对象记录范围溢出"))?;
        self.payload
            .get(start..end)
            .ok_or_else(|| error("object_length", "对象记录超出目标文件载荷范围"))
    }

    pub fn validate(&self) -> Result<()> {
        check_schema(&self.metadata.schema, "hmi.object/2")?;
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
        if self.metadata.kind == ObjectKind::Program {
            if self.metadata.layout.is_some()
                || !self.metadata.relocations.is_empty()
                || self.metadata.scripts.iter().any(|f| {
                    f.source.page.is_some() || f.source.object.is_some() || f.source.event.is_some()
                })
            {
                return Err(error(
                    "object_layout",
                    "全局代码目标不能包含页面布局、对象事件或重定位",
                ));
            }
            return Ok(());
        }
        let layout = self
            .metadata
            .layout
            .as_ref()
            .ok_or_else(|| error("object_layout", "页面目标缺少对象布局"))?;
        if layout.objects.is_empty() || layout.objects.len() > 251 {
            return Err(error(
                "object_count",
                "页面须包含页面自身对象，且最多包含 250 个普通控件",
            ));
        }
        let mut cursor = 0usize;
        let mut names = std::collections::BTreeSet::new();
        for (index, record) in layout.objects.iter().enumerate() {
            hmi_core::model::check_name(&record.name)?;
            if !names.insert(&record.name) || record.offset as usize != cursor || record.length == 0
            {
                return Err(error("object_layout", "对象记录名称重复或载荷布局无效"));
            }
            if index == 0 {
                if record.type_code != 121 || Some(&record.name) != self.metadata.name.as_ref() {
                    return Err(error("page_object", "页面自身对象的类型或名称无效"));
                }
            } else {
                hmi_core::controls::by_code(record.type_code)?;
            }
            cursor = cursor
                .checked_add(record.length as usize)
                .filter(|end| *end <= self.payload.len())
                .ok_or_else(|| error("object_length", "对象记录超出载荷范围"))?;
        }
        if cursor != self.payload.len() {
            return Err(error("object_length", "对象记录未完整覆盖页面载荷"));
        }
        let mut relocated = std::collections::BTreeSet::new();
        for relocation in &self.metadata.relocations {
            if relocation.object == 0
                || relocation.object >= layout.objects.len()
                || relocation.property != "id"
                || !relocated.insert(relocation.object)
            {
                return Err(error(
                    "object_relocation",
                    "控件 ID 重定位重复、缺失或指向无效记录",
                ));
            }
        }
        if relocated.len() != layout.objects.len() - 1 {
            return Err(error("object_relocation", "普通控件缺少 ID 重定位"));
        }
        for facts in &self.metadata.scripts {
            if facts.source.page.is_some()
                || facts
                    .source
                    .object
                    .is_none_or(|index| index >= layout.objects.len())
                || facts.source.event.is_none()
            {
                return Err(error("script_facts", "页面脚本的所属对象或事件槽无效"));
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
            return Err(error(
                "object_format",
                "不是受支持的 .hmo 目标文件或版本，请使用当前编译器重新编译",
            ));
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
        unknown[7] = b'9';
        assert!(CompiledObject::decode(&unknown).is_err());
        let mut overflow = bytes;
        overflow[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(CompiledObject::decode(&overflow).is_err());
    }
}
