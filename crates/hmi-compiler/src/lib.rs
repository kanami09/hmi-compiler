//! 页面与全局代码编译。第一阶段支持空白页属性及页面自身事件。

use hmi_analysis::{ScriptSource, analyze};
use hmi_artifact::{CompiledObject, ObjectKind};
use hmi_core::{
    Result, error, file,
    model::{PageSource, check_name, check_schema},
    target::Target,
};
use hmi_format::write;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub const PAGE_EVENTS: &[&str] = &["load", "loadend", "down", "up", "unload"];
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

fn compile_page_inner(path: &Path, target: &Target) -> Result<Compilation> {
    target.validate()?;
    let page: PageSource = file::read_toml(path)?;
    check_schema(&page.schema, "hmi.page/1")?;
    check_name(&page.name)?;
    if !page.objects.is_empty() || page.original_record.is_some() {
        return Err(error(
            "page_feature",
            "当前阶段仅支持新建空白页及页面事件，尚未接入普通控件和原始数据附件",
        ));
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
    for slot in page.events.keys() {
        if !PAGE_EVENTS.contains(&slot.as_str()) {
            return Err(error("event_slot", format!("页面不支持事件槽 {slot}")));
        }
    }
    let directory = path.parent().unwrap_or(Path::new("."));
    let mut inputs = vec![path.to_path_buf()];
    let mut events = Vec::new();
    let mut facts = Vec::new();
    for slot in PAGE_EVENTS {
        let lines = if let Some(relative) = page.events.get(*slot) {
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
                object: None,
                event: Some((*slot).into()),
                file: label(&script),
                text,
            }));
            inputs.push(script);
            lines
        } else {
            Vec::new()
        };
        events.push(((*slot).to_string(), lines));
    }
    let object = write::object(&page_attributes(target, &page.name, bco), &events)?;
    let payload = write::page(&page.name, &[object])?;
    Ok(Compilation {
        object: CompiledObject::new(ObjectKind::Page, Some(page.name), target, facts, payload),
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
