//! 按最终页面顺序解析引用、生成容器并校验输出，不读取页面或脚本源码。

use hmi_analysis::{Context, check_linked_facts};
use hmi_artifact::{CompiledObject, ObjectKind, RelocationKind};
use hmi_core::{
    Result, controls,
    diagnostic::{Diagnostic, Severity},
    error, file,
    model::{LinkSource, check_schema},
    target::Target,
};
use hmi_format::{container::Container, page::Object, project::Project, write};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Linked {
    pub bytes: Vec<u8>,
    pub diagnostics: Vec<Diagnostic>,
    pub map: serde_json::Value,
    pub inputs: Vec<PathBuf>,
}

/// 检查对象记录、应用控件 ID 重定位，再生成页面头、索引和 CRC。
fn assemble_page(
    object: &CompiledObject,
    target: &Target,
    page_id: usize,
) -> Result<(Vec<u8>, Vec<hmi_analysis::ScriptFacts>)> {
    let layout = object
        .metadata
        .layout
        .as_ref()
        .ok_or_else(|| error("object_layout", "页面目标缺少对象布局"))?;
    let name = object
        .metadata
        .name
        .as_deref()
        .ok_or_else(|| error("page_name", "页面目标缺少名称"))?;
    let mut parsed = Vec::new();
    for (index, record) in layout.objects.iter().enumerate() {
        let current = Object::parse(object.record(index)?)?;
        if current.name != record.name || current.type_code != record.type_code || current.id != 0 {
            return Err(error(
                "object_record",
                format!(
                    "页面 {name} 中控件 {} 的名称、类型或 ID 占位值无效，请重新编译",
                    record.name
                ),
            ));
        }
        let attributes: Vec<_> = current
            .attributes
            .iter()
            .map(|a| (a.name.clone(), a.value.clone()))
            .collect();
        let events: Vec<_> = current
            .events
            .iter()
            .map(|e| (e.kind.clone(), e.lines.clone()))
            .collect();
        let slots = if index == 0 {
            let dimensions = [
                ("x", 0),
                ("y", 0),
                ("w", u32::from(target.canvas.width)),
                ("h", u32::from(target.canvas.height)),
                ("endx", u32::from(target.canvas.width) - 1),
                ("endy", u32::from(target.canvas.height) - 1),
            ];
            if current.attributes.len() != 28
                || current.integer("sta") != Some(1)
                || dimensions.iter().any(|(key, value)| {
                    current.integer(key) != Some(*value)
                        || current.attribute(key).is_none_or(|a| a.value.len() != 2)
                })
            {
                return Err(error(
                    "page_object",
                    format!("页面 {name} 的自身对象与目标几何或单色背景布局不一致"),
                ));
            }
            controls::PAGE_EVENTS
        } else {
            let definition = controls::by_code(current.type_code)?;
            if !target
                .capabilities
                .control_types
                .iter()
                .any(|name| name == definition.name)
            {
                return Err(error(
                    "control_capability",
                    format!("目标未开放控件类型 {}", definition.name),
                ));
            }
            definition.validate_record(&current.name, &attributes)?;
            definition.events
        };
        if current
            .events
            .iter()
            .map(|e| e.kind.as_str())
            .collect::<Vec<_>>()
            != slots
            || write::object(&attributes, &events)? != current.raw
        {
            return Err(error(
                "object_layout",
                format!("页面 {name} 中控件 {} 的事件或记录布局无效", current.name),
            ));
        }
        parsed.push(current);
    }
    let mut bound = Vec::new();
    let mut slots = BTreeSet::new();
    for facts in &object.metadata.scripts {
        let owner = facts
            .source
            .object
            .ok_or_else(|| error("script_facts", "页面脚本缺少所属对象"))?;
        let slot = facts
            .source
            .event
            .as_deref()
            .ok_or_else(|| error("script_facts", "页面脚本缺少事件槽"))?;
        let current = parsed
            .get(owner)
            .ok_or_else(|| error("script_facts", "页面脚本所属对象不存在"))?;
        let event = current
            .events
            .iter()
            .find(|e| e.kind == slot)
            .ok_or_else(|| error("script_facts", "页面脚本引用了不存在的事件槽"))?;
        if !slots.insert((owner, slot)) || file::script_lines(&facts.source.text)? != event.lines {
            return Err(error(
                "script_facts",
                "对象事件与目标文件中的分析数据不一致",
            ));
        }
        let mut facts = facts.clone();
        facts.source.page = Some(page_id);
        bound.push(facts);
    }
    for (index, current) in parsed.iter().enumerate() {
        if current
            .events
            .iter()
            .any(|e| !e.lines.is_empty() && !slots.contains(&(index, e.kind.as_str())))
        {
            return Err(error(
                "script_facts",
                format!("页面 {name} 中控件 {} 的事件缺少分析数据", current.name),
            ));
        }
    }
    let mut values = vec![BTreeMap::new(); parsed.len()];
    for relocation in &object.metadata.relocations {
        match relocation.kind {
            RelocationKind::ObjectId => {
                let id = u8::try_from(relocation.object)
                    .map_err(|_| error("object_count", "控件 ID 超出 u8 范围"))?;
                values[relocation.object].insert(relocation.property.clone(), vec![id]);
            }
        }
    }
    let objects = parsed
        .iter()
        .zip(&values)
        .map(|(object, values)| object.rebuild(values, &BTreeMap::new()))
        .collect::<Result<Vec<_>>>()?;
    Ok((write::page(name, &objects)?, bound))
}

pub fn from_manifest(path: &Path, deny_warnings: bool) -> Result<Linked> {
    let source: LinkSource = file::read_toml(path)?;
    check_schema(&source.schema, "hmi.link/1")?;
    if !source.assets.items.is_empty() {
        return Err(error("link_assets", "当前阶段尚未接入资产链接"));
    }
    let directory = path.parent().unwrap_or(Path::new("."));
    let target_path = directory.join(&source.target);
    let target = file::read_json(&target_path)?;
    let program_path = directory.join(&source.program);
    let program = CompiledObject::read(&program_path)?;
    let mut inputs = vec![path.to_path_buf(), target_path, program_path];
    let mut pages = Vec::new();
    for object in source.pages.objects {
        let input = directory.join(object);
        pages.push(CompiledObject::read(&input)?);
        inputs.push(input);
    }
    let mut linked = link(&target, &program, &pages, deny_warnings)?;
    linked.inputs = inputs;
    Ok(linked)
}

pub fn link(
    target: &Target,
    program: &CompiledObject,
    pages: &[CompiledObject],
    deny_warnings: bool,
) -> Result<Linked> {
    target.validate()?;
    if pages.is_empty() {
        return Err(error("page_count", "工程至少需要一个页面"));
    }
    program.validate()?;
    if program.metadata.kind != ObjectKind::Program || program.metadata.target != *target {
        return Err(error("link_target", "全局代码目标类型或目标配置不匹配"));
    }
    if program.metadata.scripts.len() != 1
        || program.metadata.scripts[0].source.text.as_bytes() != program.payload
    {
        return Err(error(
            "script_facts",
            "全局代码与目标文件中的分析数据不一致",
        ));
    }
    file::script_lines(&program.metadata.scripts[0].source.text)?;
    let mut names = BTreeSet::new();
    let mut resources = vec![("Program.s".to_string(), program.payload.clone())];
    let mut all_facts = Vec::new();
    for (id, object) in pages.iter().enumerate() {
        object.validate()?;
        if object.metadata.kind != ObjectKind::Page || object.metadata.target != *target {
            return Err(error(
                "link_target",
                format!("第 {id} 个页面目标类型或目标配置不匹配"),
            ));
        }
        let name = object.metadata.name.as_ref().unwrap();
        if !names.insert(name) {
            return Err(error("page_name", format!("页面名称 {name} 重复")));
        }
        let (page, bound) = assemble_page(object, target, id)?;
        all_facts.push(bound);
        resources.push((format!("{id}.pa"), page));
    }
    let references: Vec<_> = resources[1..]
        .iter()
        .map(|(name, _)| ("pa", name.as_str()))
        .collect();
    let main = write::main_resource(
        &target.profile,
        target.header_tag()?,
        target.orientation,
        &references,
    )?;
    resources.push(("main.HMI".into(), main));
    let bytes = write::container(&resources)?;
    let container = Container::parse(bytes.clone())?;
    let project = Project::parse(&container)?;
    project.strict()?;
    let context = Context::from_project(&project);
    let mut global = program.metadata.scripts.clone();
    global[0].source.page = None;
    global[0].source.object = None;
    global[0].source.event = None;
    let mut diagnostics = check_linked_facts(&context, None, &global);
    let mut static_errors = global
        .iter()
        .flat_map(|facts| facts.static_page_errors(project.pages.len()))
        .collect::<Vec<_>>();
    for (id, facts) in all_facts.iter().enumerate() {
        diagnostics.extend(check_linked_facts(&context, Some(id), facts));
        static_errors.extend(
            facts
                .iter()
                .flat_map(|facts| facts.static_page_errors(project.pages.len())),
        );
    }
    if let Some(error) = static_errors.into_iter().next() {
        return Err(error.into());
    }
    for diagnostic in &mut diagnostics {
        if matches!(
            diagnostic.code.as_str(),
            "script_page"
                | "script_object"
                | "script_property"
                | "script_property_mode"
                | "script_scope"
                | "property_resource"
                | "script_facts"
        ) {
            diagnostic.original_severity = Some(diagnostic.severity.clone());
            diagnostic.severity = Severity::Error;
        }
    }
    if let Some(diagnostic) = diagnostics.iter().find(|d| d.severity == Severity::Error) {
        return Err(diagnostic.clone().into());
    }
    if deny_warnings && let Some(diagnostic) = diagnostics.first() {
        let mut diagnostic = diagnostic.clone();
        diagnostic.original_severity = Some(diagnostic.severity);
        diagnostic.severity = Severity::Error;
        return Err(diagnostic.into());
    }
    let map = serde_json::json!({
        "schema":"hmi.link-map/1", "target":target,
        "pages":project.pages.iter().enumerate().map(|(id,(entry,page))| serde_json::json!({"id":id,"name":page.name,"resource":container.entries[*entry].name,"objects":page.objects.iter().map(|o|serde_json::json!({"id":o.id,"name":o.name,"type":hmi_format::page::type_name(o.type_code)})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "resources":container.entries, "output_sha256":hmi_core::sha256(&bytes),
    });
    Ok(Linked {
        bytes,
        diagnostics,
        map,
        inputs: Vec::new(),
    })
}
