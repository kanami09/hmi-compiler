//! 页面与全局代码编译。模板检查后生成对象记录，最终 ID 和页面由链接器生成。

use hmi_analysis::{ScriptSource, analyze};
use hmi_artifact::{CompiledObject, ObjectKind};
use hmi_core::{
    Result, controls, error, file,
    model::{PageSource, check_name, check_schema},
    target::Target,
};
use hmi_format::write;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub use hmi_core::controls::PAGE_EVENTS;
pub use hmi_core::file::script_lines;

pub struct Compilation {
    pub object: CompiledObject,
    pub inputs: Vec<PathBuf>,
}

fn label(path: &Path) -> String {
    let path = if path.is_absolute() {
        Path::new(path.file_name().unwrap_or_default())
    } else {
        path
    };
    path.display().to_string().replace('\\', "/")
}

pub fn compile_program(path: &Path, target: &Target) -> Result<Compilation> {
    target.validate()?;
    let text = file::text(path)?;
    script_lines(&text).map_err(|e| file::at_file(e, path))?;
    let source = ScriptSource {
        page: None,
        object: None,
        event: None,
        file: label(path),
        text: text.clone(),
    };
    Ok(Compilation {
        object: CompiledObject::new(
            ObjectKind::Program,
            None,
            target,
            vec![analyze(&source)],
            text.into_bytes(),
        ),
        inputs: vec![path.into()],
    })
}

pub fn compile_page(path: &Path, target: &Target) -> Result<Compilation> {
    compile_page_with_original(path, target, None)
}
pub fn compile_page_with_original(
    path: &Path,
    target: &Target,
    original: Option<&hmi_artifact::original::OriginalBundle>,
) -> Result<Compilation> {
    compile_page_inner(path, target, original).map_err(|e| file::at_file(e, path))
}

type EventRecords = Vec<(String, Vec<Vec<u8>>)>;

/// 事件归属保存为本目标内的对象序号；链接器随后绑定最终页面上下文。
fn compile_events(
    events: &BTreeMap<String, String>,
    slots: &[&str],
    directory: &Path,
    owner: usize,
    inputs: &mut Vec<PathBuf>,
    facts: &mut Vec<hmi_analysis::ScriptFacts>,
) -> Result<EventRecords> {
    for slot in events.keys() {
        if !slots.contains(&slot.as_str()) {
            return Err(error("event_slot", format!("不支持事件槽 {slot}")));
        }
    }
    let mut records = Vec::new();
    for slot in slots {
        let lines = if let Some(relative) = events.get(*slot) {
            if Path::new(relative).is_absolute() || !relative.ends_with(".s") {
                return Err(error(
                    "event_path",
                    format!("事件 {slot} 必须引用相对路径的 .s 文件"),
                ));
            }
            let script = directory.join(relative);
            let text = file::text(&script)?;
            let lines = script_lines(&text).map_err(|e| file::at_file(e, &script))?;
            facts.push(analyze(&ScriptSource {
                page: None,
                object: Some(owner),
                event: Some((*slot).into()),
                file: label(&script),
                text,
            }));
            inputs.push(script);
            lines
        } else {
            Vec::new()
        };
        records.push(((*slot).to_string(), lines));
    }
    Ok(records)
}

fn at_object(error: hmi_core::Error, page: &str, name: &str, index: usize) -> hmi_core::Error {
    let mut diagnostic = error.as_diagnostic().cloned().unwrap_or_else(|| {
        hmi_core::diagnostic::Diagnostic::error("compile_object", error.to_string())
    });
    diagnostic.location.page = Some(page.into());
    diagnostic.location.object = Some(name.into());
    diagnostic.location.object_index = Some(index);
    diagnostic.into()
}

/// 页面自身使用 owner=0，普通控件使用最终页内序号，共用资源占位和重定位规则。
fn bind_resources(
    definition: &controls::Definition,
    properties: &mut hmi_core::model::Properties,
    owner: usize,
    resources: &mut Vec<hmi_artifact::Relocation>,
) -> Result<()> {
    for property in definition.all_properties() {
        if matches!(property.storage, controls::Storage::ImageMap) {
            let empty = "".into();
            let entries =
                hmi_core::icon_map::parse_source(properties.get(property.name).unwrap_or(&empty))?;
            let placeholder = hmi_core::icon_map::render(&entries, |_| Ok(0))?;
            properties.insert(property.name.into(), placeholder.into());
            resources.push(hmi_artifact::Relocation {
                object: owner,
                property: property.name.into(),
                kind: hmi_artifact::RelocationKind::ImageMap,
                reference: None,
                image_map: Some(entries),
            });
            continue;
        }
        if matches!(property.storage, controls::Storage::ExternalPath(_)) {
            let active = property
                .resource_rule
                .and_then(|r| r.active_when)
                .is_none_or(|(mode, expected)| {
                    let value = properties
                        .get(mode)
                        .and_then(|v| v.as_integer())
                        .or_else(|| {
                            definition
                                .all_properties()
                                .find(|p| p.name == mode)
                                .and_then(|p| match p.default {
                                    controls::DefaultValue::Integer(n) => Some(n),
                                    _ => None,
                                })
                        });
                    value == Some(i64::from(expected))
                });
            let fallback = match property.default {
                controls::DefaultValue::Text(text) => Some(text.into()),
                _ => None,
            };
            let value = properties
                .get(property.name)
                .or(fallback.as_ref())
                .ok_or_else(|| error("property_required", "必须指定外部文件 path"))?;
            if let Some(path) = value.as_str() {
                if active && path.is_empty() {
                    return Err(error("runtime_file_path", "当前模式要求非空的外部文件路径"));
                }
            } else {
                let reference: hmi_core::asset::ResourceReference =
                    value.clone().try_into().map_err(|_| {
                        error(
                            "resource_reference",
                            "外部文件 path 必须为字符串或 ref 引用表",
                        )
                    })?;
                reference.validate_source()?;
                if reference.name.is_none() {
                    return Err(error(
                        "resource_reference",
                        "外部文件路径只接受 ref，不接受 ID 或 none",
                    ));
                }
                resources.push(hmi_artifact::Relocation {
                    object: owner,
                    property: property.name.into(),
                    kind: hmi_artifact::RelocationKind::ExternalPath,
                    reference: Some(reference),
                    image_map: None,
                });
                properties.insert(property.name.into(), "".into());
            }
        }
        if matches!(
            property.storage,
            controls::Storage::Image
                | controls::Storage::Font
                | controls::Storage::Animation
                | controls::Storage::Video
                | controls::Storage::Audio
        ) {
            let reference: hmi_core::asset::ResourceReference =
                if let Some(value) = properties.get(property.name) {
                    value.clone().try_into().map_err(|_| {
                        error(
                            "resource_reference",
                            format!("属性 {} 必须使用 ref、id 或 none 的引用表", property.name),
                        )
                    })?
                } else if let controls::DefaultValue::BuiltinImage(id) = property.default {
                    let builtin = hmi_core::builtin::BuiltinImage::from_id(id)
                        .ok_or_else(|| error("builtin_asset", "未知的内置图标请求"))?;
                    hmi_core::asset::ResourceReference {
                        name: Some(builtin.key().into()),
                        id: None,
                        none: None,
                    }
                } else if property.allows_none() {
                    hmi_core::asset::ResourceReference {
                        name: None,
                        id: None,
                        none: Some(true),
                    }
                } else {
                    return Err(error(
                        "property_required",
                        format!("必须指定资源属性 {}", property.name),
                    ));
                };
            reference.validate()?;
            if properties.contains_key(property.name) {
                reference.validate_source()?;
            }
            let font = matches!(property.storage, controls::Storage::Font);
            if (reference.none == Some(true) && !property.allows_none())
                || reference
                    .id
                    .is_some_and(|id| id >= if font { 255 } else { 65535 })
            {
                return Err(error(
                    "resource_reference",
                    "资源属性必须引用有效的媒体 ID，不能使用空资源",
                ));
            }
            properties.insert(
                property.name.into(),
                if reference.none == Some(true) {
                    65535_i64.into()
                } else {
                    0_i64.into()
                },
            );
            resources.push(hmi_artifact::Relocation {
                object: owner,
                property: property.name.into(),
                kind: if font {
                    hmi_artifact::RelocationKind::FontId
                } else if matches!(property.storage, controls::Storage::Audio) {
                    hmi_artifact::RelocationKind::AudioId
                } else if matches!(property.storage, controls::Storage::Video) {
                    hmi_artifact::RelocationKind::VideoId
                } else if matches!(property.storage, controls::Storage::Animation) {
                    hmi_artifact::RelocationKind::AnimationId
                } else {
                    hmi_artifact::RelocationKind::ImageId
                },
                reference: Some(reference),
                image_map: None,
            });
        }
    }
    Ok(())
}

fn compile_page_inner(
    path: &Path,
    target: &Target,
    original: Option<&hmi_artifact::original::OriginalBundle>,
) -> Result<Compilation> {
    target.validate()?;
    let page: PageSource = file::read_toml(path)?;
    check_schema(&page.schema, "hmi.page/1")?;
    check_name(&page.name)?;
    let require_original = || {
        original.ok_or_else(|| {
            error(
                "original_required",
                "源码声明了original_record，但未提供original附件",
            )
        })
    };
    let original_page = page
        .original_record
        .as_ref()
        .map(|locator| -> Result<_> {
            let bundle = require_original()?;
            let index = hmi_artifact::original::OriginalBundle::page_index(locator)?;
            let old = bundle.page(index)?;
            if old.objects[0].integer("w") != Some(u32::from(target.canvas.width))
                || old.objects[0].integer("h") != Some(u32::from(target.canvas.height))
            {
                return Err(error(
                    "original_geometry",
                    "原始页面几何与当前目标不一致，本批尚未接入此类方向迁移",
                ));
            }
            Ok((index, old))
        })
        .transpose()?;
    if page.objects.len() > 250 {
        return Err(error("object_count", "页面最多包含 250 个普通控件"));
    }
    let mut names = BTreeSet::from([page.name.as_str()]);
    if page
        .objects
        .iter()
        .filter(|o| o.type_name == "data_record")
        .count()
        > controls::DATA_RECORD_LIMIT
    {
        return Err(error("control_count", "一个工程最多支持八个数据记录控件"));
    }
    let mut touch_capture = false;
    for object in &page.objects {
        if object.type_name == "touch_cap" && std::mem::replace(&mut touch_capture, true) {
            return Err(error(
                "control_count",
                format!("页面 {} 最多支持一个触摸捕捉控件", page.name),
            ));
        }
        check_name(&object.name)?;
        if !names.insert(&object.name) {
            return Err(error(
                "object_name",
                format!("页面 {} 中名称 {} 重复", page.name, object.name),
            ));
        }
    }
    for property in page.properties.keys() {
        if !matches!(property.as_str(), "sta" | "bco" | "pic" | "effect") {
            return Err(error(
                "page_property",
                format!("不支持页面属性 {property}；几何由目标固定"),
            ));
        }
    }
    let mut page_properties = page.properties.clone();
    // 页面几何只取目标画布，源码不能覆盖；背景图片在链接时核对同一逻辑尺寸。
    page_properties.insert("w".into(), i64::from(target.canvas.width).into());
    page_properties.insert("h".into(), i64::from(target.canvas.height).into());
    let mut resources = Vec::new();
    bind_resources(&controls::PAGE, &mut page_properties, 0, &mut resources)?;
    let attributes = controls::PAGE.attributes(&page.name, &page_properties)?;
    target.check_control_properties(121, &attributes)?;
    let directory = path.parent().unwrap_or(Path::new("."));
    let mut inputs = vec![path.to_path_buf()];
    let mut facts = Vec::new();
    let events = compile_events(
        &page.events,
        PAGE_EVENTS,
        directory,
        0,
        &mut inputs,
        &mut facts,
    )?;
    let own_original =
        original_page
            .as_ref()
            .map(|(_, p)| hmi_artifact::original::OriginalRecord {
                raw: p.objects[0].raw.clone(),
                index_unknown: p.indices[0].unknown,
            });
    let own = if let Some(old) = &own_original {
        hmi_artifact::original::merge_record(
            &controls::PAGE,
            &page.name,
            &attributes,
            &events,
            old,
        )?
    } else {
        write::object(&attributes, &events)?
    };
    let mut originals = vec![own_original];
    let mut records = vec![(121, page.name.clone(), own)];
    for (index, object) in page.objects.iter().enumerate() {
        let compiled = (|| {
            let definition = controls::by_name(&object.type_name)?;
            if !target
                .capabilities
                .control_types
                .contains(&object.type_name)
            {
                return Err(error(
                    "control_capability",
                    format!("当前目标未开放控件类型 {}", object.type_name),
                ));
            }
            let mut properties = object.properties.clone();
            bind_resources(definition, &mut properties, index + 1, &mut resources)?;
            let attributes = definition.attributes(&object.name, &properties)?;
            target.check_control_properties(definition.code, &attributes)?;
            let events = compile_events(
                &object.events,
                definition.events,
                directory,
                index + 1,
                &mut inputs,
                &mut facts,
            )?;
            let old = object
                .original_record
                .as_deref()
                .map(|locator| require_original()?.object(locator))
                .transpose()?;
            let bytes = if let Some(old) = &old {
                hmi_artifact::original::merge_record(
                    definition,
                    &object.name,
                    &attributes,
                    &events,
                    old,
                )?
            } else {
                write::object(&attributes, &events)?
            };
            originals.push(old);
            Ok((definition.code, object.name.clone(), bytes))
        })()
        .map_err(|e| at_object(e, &page.name, &object.name, index + 1))?;
        records.push(compiled);
    }
    let mut object = CompiledObject::page(page.name, target, facts, records, resources)?;
    for (record, original) in object
        .metadata
        .layout
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .zip(originals)
    {
        record.original = original;
    }
    if let Some((index, page)) = original_page {
        object.metadata.original_page_header = Some(page.raw[..56].to_vec());
        object.metadata.original_page_directory =
            Some(require_original()?.metadata.pages[index].directory_unknown);
    }
    if object.metadata.original_page_header.is_some()
        || object
            .metadata
            .layout
            .as_ref()
            .unwrap()
            .objects
            .iter()
            .any(|r| r.original.is_some())
    {
        object.metadata.original_bundle_sha256 =
            Some(hmi_core::sha256(&require_original()?.encode()?));
    }
    object.validate()?;
    Ok(Compilation { object, inputs })
}

/// init 的页面源码不依赖字体或图片，用户可直接完成第一次构建。
pub fn empty_page(name: &str) -> PageSource {
    PageSource {
        schema: "hmi.page/1".into(),
        name: name.into(),
        properties: BTreeMap::new(),
        events: BTreeMap::new(),
        objects: Vec::new(),
        original_record: None,
    }
}
