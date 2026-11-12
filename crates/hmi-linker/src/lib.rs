//! 按最终页面顺序解析引用、生成容器并校验输出，不读取页面或脚本源码。

use hmi_analysis::{Context, check_facts};
use hmi_artifact::{CompiledObject, ObjectKind};
use hmi_core::{
    Result,
    diagnostic::{Diagnostic, Severity},
    error, file,
    model::{LinkSource, check_schema},
    target::Target,
};
use hmi_format::{container::Container, page::Page, project::Project, write};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Linked {
    pub bytes: Vec<u8>,
    pub diagnostics: Vec<Diagnostic>,
    pub map: serde_json::Value,
    pub inputs: Vec<PathBuf>,
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
        let page = Page::parse(&object.payload)?;
        if !page.crc_valid
            || !page.length_valid
            || page.objects.len() != 1
            || page.objects[0].type_code != 121
            || page.objects[0].id != 0
            || page.objects[0].name != page.name
            || Some(&page.name) != object.metadata.name.as_ref()
        {
            return Err(error(
                "page_object",
                "页面目标的名称、结构或校验无效，当前阶段仅支持页面自身对象",
            ));
        }
        if !names.insert(page.name.clone()) {
            return Err(error("page_name", format!("页面名称 {} 重复", page.name)));
        }
        let mut slots = BTreeSet::new();
        let mut bound = Vec::new();
        for facts in &object.metadata.scripts {
            let slot = facts
                .source
                .event
                .as_deref()
                .ok_or_else(|| error("script_facts", "页面脚本缺少事件槽"))?;
            let event = page.objects[0]
                .events
                .iter()
                .find(|e| e.kind == slot)
                .ok_or_else(|| error("script_facts", "页面脚本引用了不存在的事件槽"))?;
            if !slots.insert(slot) || file::script_lines(&facts.source.text)? != event.lines {
                return Err(error(
                    "script_facts",
                    "页面事件与目标文件中的分析数据不一致",
                ));
            }
            let mut facts = facts.clone();
            facts.source.page = Some(id);
            facts.source.object = Some(0);
            bound.push(facts);
        }
        if page.objects[0]
            .events
            .iter()
            .any(|event| !event.lines.is_empty() && !slots.contains(event.kind.as_str()))
        {
            return Err(error("script_facts", "页面事件缺少分析数据"));
        }
        all_facts.push(bound);
        resources.push((format!("{id}.pa"), object.payload.clone()));
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
    let mut diagnostics = check_facts(&context, None, &global);
    let mut static_errors = global
        .iter()
        .flat_map(|facts| facts.static_page_errors(project.pages.len()))
        .collect::<Vec<_>>();
    for (id, facts) in all_facts.iter().enumerate() {
        diagnostics.extend(check_facts(&context, Some(id), facts));
        static_errors.extend(
            facts
                .iter()
                .flat_map(|facts| facts.static_page_errors(project.pages.len())),
        );
    }
    if let Some(error) = static_errors.into_iter().next() {
        return Err(error.into());
    }
    if deny_warnings && let Some(diagnostic) = diagnostics.first() {
        let mut diagnostic = diagnostic.clone();
        diagnostic.original_severity = Some(diagnostic.severity);
        diagnostic.severity = Severity::Error;
        return Err(diagnostic.into());
    }
    let map = serde_json::json!({
        "schema":"hmi.link-map/1", "target":target,
        "pages":project.pages.iter().enumerate().map(|(id,(entry,page))| serde_json::json!({"id":id,"name":page.name,"resource":container.entries[*entry].name,"objects":[{"id":0,"name":page.name}]})).collect::<Vec<_>>(),
        "resources":container.entries, "output_sha256":hmi_core::sha256(&bytes),
    });
    Ok(Linked {
        bytes,
        diagnostics,
        map,
        inputs: Vec::new(),
    })
}
