//! 按最终页面顺序解析引用、生成容器并校验输出，不读取页面或脚本源码。

use hmi_analysis::{Context, check_linked_facts};
use hmi_artifact::asset::{AssetInfo, AssetObject, FontInfo, ImageInfo};
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
    images: &[(String, ImageInfo)],
    fonts: &[(String, FontInfo)],
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
    let mut touch_capture = false;
    for (index, record) in layout.objects.iter().enumerate() {
        let current = Object::parse(object.record(index)?)?;
        if current.type_code == 5 && std::mem::replace(&mut touch_capture, true) {
            return Err(error(
                "control_count",
                format!("页面 {name} 最多支持一个触摸捕捉控件"),
            ));
        }
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
            RelocationKind::ImageId => {
                let reference = relocation.reference.as_ref().unwrap();
                let current = &parsed[relocation.object];
                let id = resolve_resource(images, reference, "图片").map_err(
                    |error| -> hmi_core::Error {
                        let mut diagnostic = error.as_diagnostic().cloned().unwrap_or_else(|| {
                            Diagnostic::error("resource_reference", error.to_string())
                        });
                        diagnostic.location.page = Some(name.into());
                        diagnostic.location.object = Some(current.name.clone());
                        diagnostic.location.object_index = Some(relocation.object);
                        diagnostic.location.property = Some(relocation.property.clone());
                        diagnostic.into()
                    },
                )?;
                if current
                    .attribute(&relocation.property)
                    .is_none_or(|a| a.value != [0, 0])
                {
                    return Err(error("object_relocation", "图片资源 ID 占位值必须为零"));
                }
                if current.type_code == 113 {
                    let info = &images[usize::from(id)].1;
                    if info.width != target.canvas.width || info.height != target.canvas.height {
                        return Err(error(
                            "crop_image",
                            format!(
                                "切图控件 {} 要求使用与当前页面同尺寸的全屏图片",
                                current.name
                            ),
                        ));
                    }
                    if current
                        .integer("endx")
                        .is_none_or(|x| x >= u32::from(info.width))
                        || current
                            .integer("endy")
                            .is_none_or(|y| y >= u32::from(info.height))
                    {
                        return Err(error(
                            "crop_geometry",
                            format!("切图控件 {} 的区域超出全屏图片范围", current.name),
                        ));
                    }
                }
                values[relocation.object]
                    .insert(relocation.property.clone(), id.to_le_bytes().to_vec());
            }
            RelocationKind::FontId => {
                let reference = relocation.reference.as_ref().unwrap();
                let current = &parsed[relocation.object];
                let id = resolve_resource(fonts, reference, "字库").map_err(
                    |error| -> hmi_core::Error {
                        let mut d = error.as_diagnostic().cloned().unwrap_or_else(|| {
                            Diagnostic::error("resource_reference", error.to_string())
                        });
                        d.location.page = Some(name.into());
                        d.location.object = Some(current.name.clone());
                        d.location.object_index = Some(relocation.object);
                        d.location.property = Some(relocation.property.clone());
                        d.into()
                    },
                )?;
                if current
                    .attribute(&relocation.property)
                    .is_none_or(|a| a.value != [0])
                {
                    return Err(error("object_relocation", "字体 ID 必须使用单字节零占位"));
                }
                let id =
                    u8::try_from(id).map_err(|_| error("resource_id", "字体 ID 超过单字节范围"))?;
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

fn resolve_resource<T>(
    assets: &[(String, T)],
    reference: &hmi_core::asset::ResourceReference,
    kind: &str,
) -> Result<u16> {
    reference.validate()?;
    if let Some(name) = &reference.name {
        return assets
            .iter()
            .position(|(key, _)| key == name)
            .map(|i| i as u16)
            .ok_or_else(|| error("resource_symbol", format!("{kind}资产 {name} 不存在")));
    }
    if let Some(id) = reference.id
        && usize::from(id) < assets.len()
    {
        return Ok(id);
    }
    Err(error(
        "resource_id",
        format!(
            "{kind} ID {:?} 不存在，当前共有 {} 个{kind}资源",
            reference.id,
            assets.len()
        ),
    ))
}

struct Layout {
    pages: Vec<Vec<u8>>,
    diagnostics: Vec<Diagnostic>,
}

fn layout(
    target: &Target,
    program: &CompiledObject,
    pages: &[CompiledObject],
    assets: &[(String, AssetInfo)],
    deny_warnings: bool,
) -> Result<Layout> {
    target.validate()?;
    let images = assets
        .iter()
        .filter_map(|(key, info)| match info {
            AssetInfo::Image { image } => Some((key.clone(), image.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let fonts = assets
        .iter()
        .filter_map(|(key, info)| match info {
            AssetInfo::Font { font } => Some((key.clone(), font.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    if pages.is_empty() {
        return Err(error("page_count", "工程至少需要一个页面"));
    }
    let resource_count = images
        .len()
        .checked_mul(2)
        .and_then(|n| n.checked_add(pages.len() + fonts.len() + 2))
        .ok_or_else(|| error("resource_count", "资源数量溢出"))?;
    if resource_count > (hmi_format::container::BACKUP_START - 8) / 28
        || images.len() >= 65535
        || fonts.len() > 255
    {
        return Err(error("resource_count", "资源数量超过当前目录容量"));
    }
    let mut keys = BTreeSet::new();
    for (key, _) in assets {
        hmi_core::asset::check_key(key)?;
        if !keys.insert(key) {
            return Err(error("asset_key", format!("资产符号 {key} 重复")));
        }
    }
    for (key, info) in &images {
        if info.width == 0
            || info.height == 0
            || info.orientation != target.orientation
            || !target.capabilities.image_codecs.contains(&info.codec)
        {
            return Err(error(
                "asset_target",
                format!("图片资产 {key} 的尺寸、编码或方向与目标不匹配"),
            ));
        }
    }
    for (key, info) in &fonts {
        if info.height == 0
            || info.glyph_count == 0
            || info.glyph_count > 65536
            || info.orientation != target.orientation
            || info.encoding != target.encoding
            || !target.capabilities.font_versions.contains(&info.version)
        {
            return Err(error(
                "asset_target",
                format!("字库资产 {key} 的格式或方向与目标不匹配"),
            ));
        }
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
    let mut native = Vec::new();
    let mut facts_by_page = Vec::new();
    for (index, object) in pages.iter().enumerate() {
        object.validate()?;
        if object.metadata.kind != ObjectKind::Page || object.metadata.target != *target {
            return Err(error(
                "link_target",
                format!("第 {index} 个页面目标类型或目标配置不匹配"),
            ));
        }
        let name = object.metadata.name.as_ref().unwrap();
        if !names.insert(name) {
            return Err(error("page_name", format!("页面名称 {name} 重复")));
        }
        let (bytes, facts) = assemble_page(object, target, index, &images, &fonts)?;
        native.push(bytes);
        facts_by_page.push(facts);
    }
    let parsed = native
        .iter()
        .map(|bytes| hmi_format::page::Page::parse(bytes))
        .collect::<Result<Vec<_>>>()?;
    let mut references = images
        .iter()
        .enumerate()
        .map(|(id, _)| hmi_format::project::Reference {
            kind: "i".into(),
            resource: format!("{id}.i"),
        })
        .collect::<Vec<_>>();
    references.extend((0..fonts.len()).map(|id| hmi_format::project::Reference {
        kind: "zi".into(),
        resource: format!("{id}.zi"),
    }));
    references.extend((0..pages.len()).map(|id| hmi_format::project::Reference {
        kind: "pa".into(),
        resource: format!("{id}.pa"),
    }));
    let context = Context::from_pages(parsed.iter(), references);
    let mut diagnostics = check_linked_facts(&context, None, &program.metadata.scripts);
    let mut static_errors = program
        .metadata
        .scripts
        .iter()
        .flat_map(|f| f.static_page_errors(pages.len()))
        .collect::<Vec<_>>();
    for (id, facts) in facts_by_page.iter().enumerate() {
        diagnostics.extend(check_linked_facts(&context, Some(id), facts));
        static_errors.extend(facts.iter().flat_map(|f| f.static_page_errors(pages.len())));
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
                | "script_readonly"
                | "script_scope"
                | "property_resource"
                | "script_facts"
        ) {
            diagnostic.original_severity = Some(diagnostic.severity.clone());
            diagnostic.severity = Severity::Error;
        }
    }
    if let Some(d) = diagnostics.iter().find(|d| d.severity == Severity::Error) {
        return Err(d.clone().into());
    }
    if deny_warnings && let Some(d) = diagnostics.first() {
        let mut d = d.clone();
        d.original_severity = Some(d.severity);
        d.severity = Severity::Error;
        return Err(d.into());
    }
    Ok(Layout {
        pages: native,
        diagnostics,
    })
}

/// 只使用媒体探测信息检查符号、几何、ID 和脚本，不要求先执行媒体编码。
pub fn check(
    target: &Target,
    program: &CompiledObject,
    pages: &[CompiledObject],
    assets: &[(String, AssetInfo)],
    deny_warnings: bool,
) -> Result<Vec<Diagnostic>> {
    Ok(layout(target, program, pages, assets, deny_warnings)?.diagnostics)
}

pub fn from_manifest(path: &Path, deny_warnings: bool) -> Result<Linked> {
    let source: LinkSource = file::read_toml(path)?;
    check_schema(&source.schema, "hmi.link/1")?;
    let directory = path.parent().unwrap_or(Path::new("."));
    let target_path = directory.join(&source.target);
    let target = file::read_json(&target_path)?;
    let program_path = directory.join(&source.program);
    let program = CompiledObject::read(&program_path)?;
    let mut inputs = vec![path.to_path_buf(), target_path, program_path];
    let mut pages = Vec::new();
    let mut assets = Vec::new();
    for name in source.pages.objects {
        let input = directory.join(name);
        pages.push(CompiledObject::read(&input)?);
        inputs.push(input);
    }
    for asset in source.assets.items {
        let input = directory.join(asset.object);
        assets.push((asset.key, AssetObject::read(&input)?));
        inputs.push(input);
    }
    let mut linked = link_with_assets(&target, &program, &pages, &assets, deny_warnings)?;
    linked.inputs = inputs;
    Ok(linked)
}

pub fn link(
    target: &Target,
    program: &CompiledObject,
    pages: &[CompiledObject],
    deny_warnings: bool,
) -> Result<Linked> {
    link_with_assets(target, program, pages, &[], deny_warnings)
}

pub fn link_with_assets(
    target: &Target,
    program: &CompiledObject,
    pages: &[CompiledObject],
    assets: &[(String, AssetObject)],
    deny_warnings: bool,
) -> Result<Linked> {
    for (_, asset) in assets {
        asset.validate_for(target)?;
    }
    let infos = assets
        .iter()
        .map(|(key, asset)| (key.clone(), asset.metadata.info.clone()))
        .collect::<Vec<_>>();
    let layout = layout(target, program, pages, &infos, deny_warnings)?;
    let mut resources = vec![("Program.s".into(), program.payload.clone())];
    resources.extend(
        layout
            .pages
            .into_iter()
            .enumerate()
            .map(|(id, bytes)| (format!("{id}.pa"), bytes)),
    );
    let mut next_id = BTreeMap::new();
    let mut references = Vec::new();
    let mut asset_map = Vec::new();
    for (key, asset) in assets {
        // 数字 ID 按资源类别分配，不能直接使用混合输入列表的序号。
        let kind = asset.metadata.info.kind();
        let id = next_id.entry(kind).or_insert(0usize);
        for (index, member) in asset.metadata.members.iter().enumerate() {
            let name = format!("{id}.{}", member.extension);
            resources.push((name.clone(), asset.member(index)?.to_vec()));
            if let Some(reference_kind) = member.reference_kind {
                let end = reference_kind.iter().position(|&b| b == 0).unwrap_or(8);
                references.push((
                    String::from_utf8_lossy(&reference_kind[..end]).into_owned(),
                    name,
                ));
            }
        }
        asset_map
            .push(serde_json::json!({"key":key,"kind":kind,"id":id,"info":asset.metadata.info}));
        *id += 1;
    }
    references.extend((0..pages.len()).map(|id| ("pa".to_string(), format!("{id}.pa"))));
    let borrowed = references
        .iter()
        .map(|(kind, name)| (kind.as_str(), name.as_str()))
        .collect::<Vec<_>>();
    resources.push((
        "main.HMI".into(),
        write::main_resource(
            &target.profile,
            target.header_tag()?,
            target.orientation,
            &borrowed,
        )?,
    ));
    let bytes = write::container(&resources)?;
    let container = Container::parse(bytes.clone())?;
    let project = Project::parse(&container)?;
    project.strict()?;
    let map = serde_json::json!({"schema":"hmi.link-map/1","target":target,
        "pages":project.pages.iter().enumerate().map(|(id,(entry,page))|serde_json::json!({"id":id,"name":page.name,"resource":container.entries[*entry].name,"objects":page.objects.iter().map(|o|serde_json::json!({"id":o.id,"name":o.name,"type":hmi_format::page::type_name(o.type_code)})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "assets":asset_map,
        "resources":container.entries,"output_sha256":hmi_core::sha256(&bytes)});
    Ok(Linked {
        bytes,
        diagnostics: layout.diagnostics,
        map,
        inputs: Vec::new(),
    })
}
