//! 原引用表确定各类逻辑顺序；原生包保留运行成员和已有编辑源，查看副本不成为隐式输入。
use hmi_artifact::{asset::AssetObject, native};
use hmi_core::{
    Result,
    asset::ResourceReference,
    controls::{self, Storage},
    error,
    model::{AssetSource, Properties},
    target::Target,
};
use hmi_format::{container::Container, page::Object, project::Project};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
pub struct ImportedMedia {
    pub assets: Vec<AssetSource>,
    pub tables: BTreeMap<String, Vec<String>>,
    pub managed: BTreeSet<String>,
    pub files: BTreeMap<PathBuf, Vec<u8>>,
}
pub fn collect(container: &Container, project: &Project, target: &Target) -> Result<ImportedMedia> {
    let mut result = ImportedMedia {
        assets: Vec::new(),
        tables: BTreeMap::new(),
        managed: BTreeSet::new(),
        files: BTreeMap::new(),
    };
    for (reference_kind, kind) in [
        ("i", "image"),
        ("zi", "font"),
        ("gmov", "animation"),
        ("v", "video"),
        ("wav", "audio"),
    ] {
        for (id, reference) in project
            .references
            .iter()
            .filter(|r| r.kind == reference_kind)
            .enumerate()
        {
            let entry = container
                .active(&reference.resource)
                .ok_or_else(|| error("import_media", "媒体引用缺少活动资源"))?;
            let bytes = container.data(entry)?;
            let stem = entry.name.rsplit_once('.').unwrap().0;
            let source = match kind {
                "image" => container.active(&format!("{stem}.is")),
                "animation" => container.active(&format!("{stem}.gmovs")),
                _ => None,
            };
            let source_bytes = source.map(|e| container.data(e)).transpose()?;
            let mut object: AssetObject = match kind {
                "image" => native::image(bytes, source_bytes),
                "font" => native::font(bytes),
                "animation" => native::animation(bytes, source_bytes),
                "video" => native::video(bytes, Default::default()),
                "audio" => native::audio(bytes, Default::default()),
                _ => unreachable!(),
            }
            .map_err(|e| -> hmi_core::Error {
                let mut d = e.as_diagnostic().cloned().unwrap_or_else(|| {
                    hmi_core::diagnostic::Diagnostic::error("import_media", e.to_string())
                });
                d.location.file = Some(entry.name.clone());
                d.into()
            })?;
            object.metadata.members[0].directory_unknown = entry.unknown;
            if let Some(source) = source {
                object.metadata.members[1].directory_unknown = source.unknown;
                result.managed.insert(source.name.clone());
            }
            object.validate_for(target)?;
            result.managed.insert(entry.name.clone());
            let key = format!("{kind}_{id:05}");
            result
                .tables
                .entry(kind.into())
                .or_default()
                .push(key.clone());
            let package = format!(".preserve/assets/{key}.hma");
            result
                .files
                .insert(PathBuf::from(&package), object.encode()?);
            let description = format!("assets/{key}.asset.toml");
            result.files.insert(PathBuf::from(&description),format!("schema=\"hmi.asset/1\"\nkind=\"{kind}\"\ninput=\"native\"\nsource=\"../{package}\"\n").into_bytes());
            result.assets.push(AssetSource {
                key: key.clone(),
                source: description,
            });
            let view = match kind {
                "image" => source_bytes.map(|b| {
                    let format = std::str::from_utf8(&b[24..27]).unwrap();
                    (format.to_string(), b[27..].to_vec())
                }),
                "animation" => {
                    source_bytes.map(|b| ("gif".into(), b[hmi_format::gmov::HEADER..].to_vec()))
                }
                "font" => Some(("zi".into(), bytes.to_vec())),
                "video" => Some(("video".into(), bytes.to_vec())),
                "audio" => Some(("wav".into(), bytes.to_vec())),
                _ => None,
            };
            if let Some((extension, bytes)) = view {
                result.files.insert(
                    PathBuf::from(format!("assets/view/{key}.{extension}")),
                    bytes,
                );
            }
        }
    }
    Ok(result)
}
fn reference(
    tables: &BTreeMap<String, Vec<String>>,
    kind: &str,
    id: usize,
) -> Result<ResourceReference> {
    let key = tables
        .get(kind)
        .and_then(|items| items.get(id))
        .ok_or_else(|| {
            error(
                "import_resource",
                format!("{kind}引用ID{id}不在当前HMI引用表中"),
            )
        })?;
    Ok(ResourceReference {
        name: Some(key.clone()),
        id: None,
        none: None,
    })
}
fn value(reference: ResourceReference) -> Result<toml::Value> {
    toml::Value::try_from(reference).map_err(|e| error("import_resource", e.to_string()))
}
pub fn properties(object: &Object, media: &ImportedMedia) -> Result<Properties> {
    let definition = controls::record_definition(object.type_code)?;
    let attrs = object
        .attributes
        .iter()
        .map(|a| (a.name.clone(), a.value.clone()))
        .collect::<Vec<_>>();
    let mut result = definition.properties_from_record(&attrs)?;
    for property in definition.all_properties() {
        let kind = match property.storage {
            Storage::Image => Some("image"),
            Storage::Font => Some("font"),
            Storage::Animation => Some("animation"),
            Storage::Video => Some("video"),
            Storage::Audio => Some("audio"),
            _ => None,
        };
        if let Some(kind) = kind {
            let id = result[property.name].as_integer().unwrap();
            let r = if id == 65535 && property.allows_none() {
                ResourceReference {
                    name: None,
                    id: None,
                    none: Some(true),
                }
            } else {
                reference(&media.tables, kind, id as usize)?
            };
            result.insert(property.name.into(), value(r)?);
        }
        if matches!(property.storage, Storage::ImageMap) {
            let mut entries =
                hmi_core::icon_map::parse_native(result[property.name].as_str().unwrap())?;
            for entry in &mut entries {
                entry.pic = reference(&media.tables, "image", usize::from(entry.pic.id.unwrap()))?;
            }
            result.insert(
                property.name.into(),
                toml::Value::try_from(entries)
                    .map_err(|e| error("import_resource", e.to_string()))?,
            );
        }
    }
    Ok(result)
}
/// 公开属性按源码表达引用；范围检查使用等价占位，不能把有效引用误转为none。
pub fn native_properties(
    definition: &controls::Definition,
    properties: &Properties,
) -> Result<Properties> {
    let mut result = properties.clone();
    for property in definition.all_properties() {
        match property.storage {
            Storage::Image
            | Storage::Font
            | Storage::Animation
            | Storage::Video
            | Storage::Audio => {
                let reference: ResourceReference = result[property.name]
                    .clone()
                    .try_into()
                    .map_err(|_| error("import_resource", "资源引用格式无效"))?;
                result.insert(
                    property.name.into(),
                    if reference.none == Some(true) {
                        65535.into()
                    } else {
                        0.into()
                    },
                );
            }
            Storage::ImageMap => {
                let entries = hmi_core::icon_map::parse_source(&result[property.name])?;
                result.insert(
                    property.name.into(),
                    hmi_core::icon_map::render(&entries, |_| Ok(0))?.into(),
                );
            }
            _ => {}
        }
    }
    Ok(result)
}
