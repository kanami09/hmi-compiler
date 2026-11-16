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

/// 按原生创建顺序建立页面自身对象；此 ID 始终为 0，不是工程页面 ID。
fn page_attributes(target: &Target, name: &str, bco: u16) -> Vec<(String, Vec<u8>)> {
    let (w, h) = (target.canvas.width, target.canvas.height);
    let attrs: Vec<(&str, Vec<u8>)> = vec![
        ("type", vec![121]),
        ("id", vec![0]),
        ("objname", name.as_bytes().to_vec()),
        ("vscope", vec![0]),
        ("drag", vec![0]),
        ("sendkey", vec![0]),
        ("aph", vec![127]),
        ("movex", 0u16.to_le_bytes().to_vec()),
        ("movey", 0u16.to_le_bytes().to_vec()),
        ("x", 0u16.to_le_bytes().to_vec()),
        ("y", 0u16.to_le_bytes().to_vec()),
        ("w", w.to_le_bytes().to_vec()),
        ("h", h.to_le_bytes().to_vec()),
        ("endx", (w - 1).to_le_bytes().to_vec()),
        ("endy", (h - 1).to_le_bytes().to_vec()),
        ("effect", vec![0]),
        ("first", vec![0]),
        ("time", 300u16.to_le_bytes().to_vec()),
        ("lockobj", vec![0]),
        ("groupid0", 0u32.to_le_bytes().to_vec()),
        ("groupid1", 0u32.to_le_bytes().to_vec()),
        ("up", vec![255]),
        ("down", vec![255]),
        ("left", vec![255]),
        ("right", vec![255]),
        ("sta", vec![1]),
        ("bco", bco.to_le_bytes().to_vec()),
        ("pic", u16::MAX.to_le_bytes().to_vec()),
    ];
    attrs
        .into_iter()
        .map(|(name, bytes)| (name.into(), bytes))
        .collect()
}

pub fn compile_page(path: &Path, target: &Target) -> Result<Compilation> {
    compile_page_inner(path, target).map_err(|e| file::at_file(e, path))
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

fn compile_page_inner(path: &Path, target: &Target) -> Result<Compilation> {
    target.validate()?;
    let page: PageSource = file::read_toml(path)?;
    check_schema(&page.schema, "hmi.page/1")?;
    check_name(&page.name)?;
    if page.original_record.is_some() {
        return Err(error("page_feature", "当前尚未接入原始数据附件"));
    }
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
    let mut bco = u16::MAX;
    for (name, value) in &page.properties {
        match (name.as_str(), value.as_integer()) {
            ("sta", Some(1)) => {}
            ("bco", Some(n)) if (0..=65535).contains(&n) => bco = n as u16,
            ("sta", _) => {
                return Err(error(
                    "page_background",
                    "当前阶段页面 sta 仅支持 1（单色背景）",
                ));
            }
            ("bco", _) => return Err(error("property_range", "页面 bco 必须为 0～65535 的整数")),
            _ => {
                return Err(error(
                    "page_property",
                    format!("当前阶段不支持页面属性 {name}"),
                ));
            }
        }
    }
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
    let mut records = vec![(
        121,
        page.name.clone(),
        write::object(&page_attributes(target, &page.name, bco), &events)?,
    )];
    let mut resources = Vec::new();
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
            for property in definition.properties {
                if matches!(property.storage, controls::Storage::ExternalPath) {
                    let value = properties
                        .get(property.name)
                        .ok_or_else(|| error("property_required", "必须指定外部图片 path"))?;
                    if let Some(path) = value.as_str() {
                        if path.is_empty() {
                            return Err(error("runtime_file_path", "外部图片路径不能为空"));
                        }
                    } else {
                        let reference: hmi_core::asset::ResourceReference =
                            value.clone().try_into().map_err(|_| {
                                error(
                                    "resource_reference",
                                    "外部图片 path 必须为字符串或 ref 引用表",
                                )
                            })?;
                        reference.validate()?;
                        if reference.name.is_none() {
                            return Err(error(
                                "resource_reference",
                                "外部图片路径只接受 ref，不接受 ID 或 none",
                            ));
                        }
                        resources.push(hmi_artifact::Relocation {
                            object: index + 1,
                            property: property.name.into(),
                            kind: hmi_artifact::RelocationKind::ExternalPath,
                            reference: Some(reference),
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
                ) {
                    let value = properties.get(property.name).ok_or_else(|| {
                        error(
                            "property_required",
                            format!("必须指定资源属性 {}", property.name),
                        )
                    })?;
                    let reference: hmi_core::asset::ResourceReference =
                        value.clone().try_into().map_err(|_| {
                            error(
                                "resource_reference",
                                format!("属性 {} 必须使用 ref、id 或 none 的引用表", property.name),
                            )
                        })?;
                    reference.validate()?;
                    let font = matches!(property.storage, controls::Storage::Font);
                    if reference.none == Some(true)
                        || reference
                            .id
                            .is_some_and(|id| id >= if font { 255 } else { 65535 })
                    {
                        return Err(error(
                            "resource_reference",
                            "资源属性必须引用有效的媒体 ID，不能使用空资源",
                        ));
                    }
                    properties.insert(property.name.into(), 0_i64.into());
                    resources.push(hmi_artifact::Relocation {
                        object: index + 1,
                        property: property.name.into(),
                        kind: if font {
                            hmi_artifact::RelocationKind::FontId
                        } else if matches!(property.storage, controls::Storage::Video) {
                            hmi_artifact::RelocationKind::VideoId
                        } else if matches!(property.storage, controls::Storage::Animation) {
                            hmi_artifact::RelocationKind::AnimationId
                        } else {
                            hmi_artifact::RelocationKind::ImageId
                        },
                        reference: Some(reference),
                    });
                }
            }
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
            Ok((
                definition.code,
                object.name.clone(),
                write::object(&attributes, &events)?,
            ))
        })()
        .map_err(|e| at_object(e, &page.name, &object.name, index + 1))?;
        records.push(compiled);
    }
    Ok(Compilation {
        object: CompiledObject::page(page.name, target, facts, records, resources)?,
        inputs,
    })
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
