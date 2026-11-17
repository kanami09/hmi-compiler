//! 页面目标文件：版本标识、JSON 描述、二进制载荷以及内容摘要。
//!
//! 页面目标保存对象记录和重定位表；最终 ID、页面索引及 CRC 由链接器生成。

use hmi_analysis::ScriptFacts;
use hmi_core::dependency::{RuntimeDependency, RuntimeFile};
use hmi_core::{Result, error, file, model::check_schema, target::Target};
use serde::{Deserialize, Serialize};
use std::path::Path;
pub mod asset;
mod envelope;

const MAGIC: &[u8; 8] = b"HMIOBJ02";

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
    ImageId,
    ImageMap,
    FontId,
    ExternalPath,
    AnimationId,
    VideoId,
    AudioId,
}

/// object 指目标文件内的记录序号，不是预先分配的 HMI 控件 ID。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relocation {
    pub object: usize,
    pub property: String,
    pub kind: RelocationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<hmi_core::asset::ResourceReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_map: Option<Vec<hmi_core::icon_map::ImageMapEntry>>,
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
    /// 文件存在于屏幕运行环境，不是 HMI 内嵌资源或构建机输入。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_dependencies: Vec<RuntimeDependency>,
}

#[derive(Clone, Debug)]
pub struct CompiledObject {
    pub metadata: Metadata,
    pub payload: Vec<u8>,
}

impl CompiledObject {
    /// 默认图标只由受控的保留符号请求；返回顺序与页／控件排列无关。
    pub fn implicit_images(&self) -> Vec<hmi_core::builtin::BuiltinImage> {
        let mut images = std::collections::BTreeSet::new();
        for relocation in &self.metadata.relocations {
            for reference in relocation
                .reference
                .iter()
                .chain(relocation.image_map.iter().flatten().map(|e| &e.pic))
            {
                if let Some(image) = reference
                    .name
                    .as_deref()
                    .and_then(hmi_core::builtin::BuiltinImage::from_key)
                {
                    images.insert(image);
                }
            }
        }
        images.into_iter().collect()
    }
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
                runtime_dependencies: Vec::new(),
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
        resources: Vec<Relocation>,
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
                    reference: None,
                    image_map: None,
                });
            }
        }
        object.metadata.layout = Some(layout);
        object.metadata.relocations.extend(resources);
        object.metadata.runtime_dependencies = object.record_dependencies()?;
        object.validate()?;
        Ok(object)
    }

    /// 从自包含记录提取需求，也用来核对目标文件声明，防止漏报或伪造依赖。
    fn record_dependencies(&self) -> Result<Vec<RuntimeDependency>> {
        let mut dependencies = Vec::new();
        if let Some(layout) = &self.metadata.layout {
            for (index, record) in layout.objects.iter().enumerate() {
                let object = hmi_format::page::Object::parse(self.record(index)?)?;
                let attributes = object
                    .attributes
                    .into_iter()
                    .map(|a| (a.name, a.value))
                    .collect::<Vec<_>>();
                if let Some(file) = RuntimeFile::from_control(record.type_code, &attributes)? {
                    dependencies.push(RuntimeDependency {
                        object: index,
                        file,
                    });
                }
            }
        }
        Ok(dependencies)
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
                || !self.metadata.runtime_dependencies.is_empty()
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
            if relocation.kind != RelocationKind::ImageMap && relocation.image_map.is_some() {
                return Err(error("object_relocation", "普通重定位不能附带图标映射"));
            }
            if (relocation.object == 0 && relocation.kind != RelocationKind::ImageId)
                || relocation.object >= layout.objects.len()
                || !relocated.insert((relocation.object, relocation.property.as_str()))
            {
                return Err(error(
                    "object_relocation",
                    "控件 ID 重定位重复、缺失或指向无效记录",
                ));
            }
            match relocation.kind {
                RelocationKind::ImageMap => {
                    let definition = hmi_core::controls::record_definition(
                        layout.objects[relocation.object].type_code,
                    )?;
                    if relocation.reference.is_some()
                        || !definition.all_properties().any(|p| {
                            p.name == relocation.property
                                && matches!(p.storage, hmi_core::controls::Storage::ImageMap)
                        })
                    {
                        return Err(error("object_relocation", "图标映射重定位的类型或属性无效"));
                    }
                    let entries = relocation
                        .image_map
                        .as_ref()
                        .ok_or_else(|| error("object_relocation", "图标映射重定位缺少条目"))?;
                    hmi_core::icon_map::validate(entries)?;
                }
                RelocationKind::ExternalPath => {
                    let definition = hmi_core::controls::record_definition(
                        layout.objects[relocation.object].type_code,
                    )?;
                    if !definition.all_properties().any(|p| {
                        p.name == relocation.property
                            && matches!(p.storage, hmi_core::controls::Storage::ExternalPath(_))
                    }) {
                        return Err(error("object_relocation", "外部路径重定位的类型或属性无效"));
                    }
                    let reference = relocation
                        .reference
                        .as_ref()
                        .ok_or_else(|| error("object_relocation", "外部路径重定位缺少资产符号"))?;
                    reference.validate()?;
                    if reference.name.is_none() {
                        return Err(error(
                            "resource_reference",
                            "外部路径只能使用 ref 资产符号，不能使用 ID 或 none",
                        ));
                    }
                }
                RelocationKind::ObjectId => {
                    if relocation.property != "id" || relocation.reference.is_some() {
                        return Err(error("object_relocation", "控件 ID 重定位无效"));
                    }
                }
                RelocationKind::ImageId
                | RelocationKind::FontId
                | RelocationKind::AnimationId
                | RelocationKind::VideoId
                | RelocationKind::AudioId => {
                    let font = relocation.kind == RelocationKind::FontId;
                    let definition = hmi_core::controls::record_definition(
                        layout.objects[relocation.object].type_code,
                    )?;
                    if !definition.all_properties().any(|p| {
                        p.name == relocation.property
                            && if font {
                                matches!(p.storage, hmi_core::controls::Storage::Font)
                            } else if relocation.kind == RelocationKind::AudioId {
                                matches!(p.storage, hmi_core::controls::Storage::Audio)
                            } else if relocation.kind == RelocationKind::VideoId {
                                matches!(p.storage, hmi_core::controls::Storage::Video)
                            } else if relocation.kind == RelocationKind::AnimationId {
                                matches!(p.storage, hmi_core::controls::Storage::Animation)
                            } else {
                                matches!(p.storage, hmi_core::controls::Storage::Image)
                            }
                    }) {
                        return Err(error("object_relocation", "资源重定位的类别与属性不符"));
                    }
                    let reference = relocation
                        .reference
                        .as_ref()
                        .ok_or_else(|| error("object_relocation", "资源重定位缺少引用"))?;
                    reference.validate()?;
                    let optional = definition
                        .all_properties()
                        .any(|p| p.name == relocation.property && p.allows_none());
                    if (reference.none == Some(true) && !optional)
                        || reference
                            .id
                            .is_some_and(|id| id >= if font { 255 } else { 65535 })
                    {
                        return Err(error(
                            "resource_reference",
                            "资源重定位不能引用空资源或保留 ID",
                        ));
                    }
                }
            }
        }
        for (index, record) in layout.objects.iter().enumerate() {
            if index > 0 && !relocated.contains(&(index, "id")) {
                return Err(error("object_relocation", "普通控件缺少 ID 重定位"));
            }
            for property in
                hmi_core::controls::record_definition(record.type_code)?.all_properties()
            {
                if matches!(
                    property.storage,
                    hmi_core::controls::Storage::Image
                        | hmi_core::controls::Storage::ImageMap
                        | hmi_core::controls::Storage::Font
                        | hmi_core::controls::Storage::Animation
                        | hmi_core::controls::Storage::Video
                        | hmi_core::controls::Storage::Audio
                ) && !relocated.contains(&(index, property.name))
                {
                    return Err(error("object_relocation", "资源属性缺少重定位"));
                }
            }
        }
        if self.metadata.runtime_dependencies != self.record_dependencies()? {
            return Err(error(
                "runtime_dependency",
                "目标文件的运行时文件声明与控件记录不一致，请重新编译",
            ));
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
        envelope::encode(MAGIC, &self.metadata, &self.payload, "object")
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let (metadata, payload) = envelope::decode(MAGIC, bytes, "object")?;
        let object = Self { metadata, payload };
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
