//! 已有HMI到新版源码的入口。已知属性可编辑，未知记录通过显式附件保留。
use hmi_artifact::original::OriginalBundle;
use hmi_core::{
    Result,
    controls::{self, Storage},
    error, file,
    model::*,
    target::{PROFILES, Target, TargetConfig},
};
use hmi_format::{binary, container::Container, page::Object, project::Project};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub struct Parsed {
    pub container: Container,
    pub project: Project,
    pub target: TargetConfig,
}
#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub directory: PathBuf,
    pub target: TargetConfig,
    pub pages: usize,
    pub objects: usize,
    pub extra_resources: usize,
    pub original_bundle: String,
}
pub fn read(path: &Path) -> Result<Parsed> {
    let run = || -> Result<_> {
        let container = Container::parse(file::read(path)?)?;
        if container.diagnostics.iter().any(|d| {
            matches!(
                d.code.as_str(),
                "directory_crc" | "directory_structure" | "directory_mirror"
            )
        }) {
            return Err(error(
                "import_container",
                "原HMI目录校验或主备一致性检查失败",
            ));
        }
        // 导入只读取有效资源，删除项和空闲区不约束重新分配；活动区间仍不能重叠。
        let mut active = container
            .entries
            .iter()
            .filter(|e| !e.deleted)
            .collect::<Vec<_>>();
        active.sort_by_key(|e| e.offset);
        let mut end = hmi_format::container::DATA_START;
        let mut names = BTreeSet::new();
        for entry in active {
            if entry.offset < end || !names.insert(&entry.name) {
                return Err(error("import_container", "活动资源重叠或名称重复"));
            }
            container.data(entry)?;
            end = entry.offset + entry.length;
        }
        let project = Project::parse(&container)?;
        project.strict()?;
        let main = container.data(container.active("main.HMI").unwrap())?;
        let code = binary::u32_at(main, 16)?;
        let profile = PROFILES
            .iter()
            .find(|p| binary::crc_bytes(&[p.as_bytes()]) == code)
            .ok_or_else(|| error("import_target", format!("型号代码0x{code:08X}尚无工程配置")))?;
        if main[12] > 3 {
            return Err(error("import_target", "工程方向代码不受支持"));
        }
        let target = TargetConfig {
            profile: (*profile).into(),
            format: "ver21234".into(),
            encoding: "utf-8".into(),
            orientation: u16::from(main[12]) * 90,
        };
        Target::resolve(&target)?;
        Ok(Parsed {
            container,
            project,
            target,
        })
    };
    run().map_err(|e| file::at_file(e, path))
}
pub fn inspect(path: &Path) -> Result<serde_json::Value> {
    let p = read(path)?;
    Ok(
        serde_json::json!({"target":p.target,"pages":p.project.pages.iter().enumerate().map(|(id,(_,page))|serde_json::json!({"id":id,"name":page.name,"objects":page.objects.iter().map(|o|serde_json::json!({"id":o.id,"name":o.name,"type":hmi_format::page::type_name(o.type_code)})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"active_resources":p.container.entries.iter().filter(|e|!e.deleted).collect::<Vec<_>>() }),
    )
}
fn properties(object: &Object) -> Result<Properties> {
    let d = controls::record_definition(object.type_code)?;
    let attrs = object
        .attributes
        .iter()
        .map(|a| (a.name.clone(), a.value.clone()))
        .collect::<Vec<_>>();
    let mut p = d.properties_from_record(&attrs)?;
    for property in d.all_properties() {
        if matches!(
            property.storage,
            Storage::Image | Storage::Font | Storage::Animation | Storage::Video | Storage::Audio
        ) {
            let id = p[property.name].as_integer().unwrap();
            if id == 65535 && property.allows_none() {
                p.insert(
                    property.name.into(),
                    toml::Value::Table(toml::map::Map::from_iter([("none".into(), true.into())])),
                );
            } else {
                return Err(error(
                    "import_media",
                    format!(
                        "{}.{}含媒体引用{id}，本批尚未接入媒体资产导入",
                        object.name, property.name
                    ),
                ));
            }
        }
        if matches!(property.storage, Storage::ImageMap) {
            let entries = hmi_core::icon_map::parse_native(p[property.name].as_str().unwrap())?;
            if !entries.is_empty() {
                return Err(error("import_media", "本批尚未接入自定义图标所引用的媒体"));
            }
        }
    }
    Ok(p)
}
fn events(
    object: &Object,
    prefix: &str,
    base: &Path,
    files: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<BTreeMap<String, String>> {
    let definition = controls::record_definition(object.type_code)?;
    if object
        .events
        .iter()
        .map(|e| e.kind.as_str())
        .collect::<Vec<_>>()
        != definition.events
    {
        return Err(error(
            "import_events",
            format!("{}含未接入的事件槽布局", object.name),
        ));
    }
    let mut result = BTreeMap::new();
    for event in &object.events {
        let relative = format!("{prefix}/{}.s", event.kind);
        let mut bytes = Vec::new();
        for line in &event.lines {
            std::str::from_utf8(line).map_err(|_| error("script_encoding", "事件代码不是UTF-8"))?;
            bytes.extend(line);
            bytes.extend(b"\r\n");
        }
        file::script_lines(std::str::from_utf8(&bytes).unwrap())?;
        files.insert(base.join(&relative), bytes);
        result.insert(event.kind.clone(), relative);
    }
    Ok(result)
}
pub fn export(path: &Path, output: &Path) -> Result<ImportReport> {
    let parsed = read(path)?;
    let target = Target::resolve(&parsed.target)?;
    // 首批明确拒绝媒体，不能把未导入的有效资源丢掉后伪装成成功。
    for entry in parsed.container.entries.iter().filter(|e| !e.deleted) {
        if entry.name.rsplit_once('.').is_some_and(|(_, ext)| {
            matches!(ext, "i" | "is" | "zi" | "gmov" | "gmovs" | "v" | "wav")
        }) {
            return Err(error(
                "import_media",
                format!(
                    "活动媒体资源 {} 需由下一批媒体导入处理，当前未写出工程",
                    entry.name
                ),
            ));
        }
    }
    if output.exists() && (!output.is_dir() || fs::read_dir(output)?.next().is_some()) {
        return Err(error(
            "import_output",
            "导入目标须不存在或为空目录，不覆盖已有工程",
        ));
    }
    let bundle =
        OriginalBundle::capture(&parsed.container, &parsed.project, parsed.target.clone())?;
    let mut files = BTreeMap::new();
    files.insert(PathBuf::from(".preserve/origin.hpo"), bundle.encode()?);
    let program = parsed
        .container
        .data(&parsed.container.entries[parsed.project.program_index.unwrap()])?
        .to_vec();
    file::script_lines(
        std::str::from_utf8(&program)
            .map_err(|_| error("script_encoding", "Program.s不是UTF-8"))?,
    )?;
    files.insert(PathBuf::from("program.s"), program);
    let mut sources = Vec::new();
    let mut object_count = 0;
    for (index, (_, page)) in parsed.project.pages.iter().enumerate() {
        check_name(&page.name)?;
        if page.objects[0].integer("w") != Some(u32::from(target.canvas.width))
            || page.objects[0].integer("h") != Some(u32::from(target.canvas.height))
        {
            return Err(error(
                "import_geometry",
                "原始页面几何与目标方向不同，本批尚未接入此类方向迁移",
            ));
        }
        let directory = PathBuf::from(format!("pages/p{index:05}"));
        let source = format!("pages/p{index:05}/page.toml");
        sources.push(source.clone());
        let raw_attrs = page.objects[0]
            .attributes
            .iter()
            .map(|a| (a.name.clone(), a.value.clone()))
            .collect::<Vec<_>>();
        let raw_props = controls::PAGE.properties_from_record(&raw_attrs)?;
        let checked = controls::PAGE.attributes(&page.name, &raw_props)?;
        target.check_control_properties(121, &checked)?;
        let mut own = properties(&page.objects[0])?;
        own.retain(|key, _| matches!(key.as_str(), "sta" | "bco" | "pic" | "effect"));
        let mut objects = Vec::new();
        for (object_index, object) in page.objects.iter().enumerate().skip(1) {
            check_name(&object.name)?;
            let definition = controls::by_code(object.type_code)?;
            if !target
                .capabilities
                .control_types
                .iter()
                .any(|name| name == definition.name)
            {
                return Err(error(
                    "import_control",
                    format!("当前目标未开放控件类型 {}", definition.name),
                ));
            }
            let props = properties(object)?;
            // 校验已公开属性的范围，未知字段仍留在原始记录中。
            let generated = definition.attributes(
                &object.name,
                &props
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            if matches!(
                                definition
                                    .all_properties()
                                    .find(|p| p.name == k)
                                    .unwrap()
                                    .storage,
                                Storage::Image
                                    | Storage::Animation
                                    | Storage::Video
                                    | Storage::Audio
                            ) {
                                65535.into()
                            } else {
                                v.clone()
                            },
                        )
                    })
                    .collect(),
            )?;
            target.check_control_properties(object.type_code, &generated)?;
            objects.push(ObjectFile {
                original_record: Some(format!("pages/{index}/objects/{object_index}")),
                name: object.name.clone(),
                type_name: definition.name.into(),
                properties: props,
                events: events(
                    object,
                    &format!("objects/o{object_index:05}"),
                    &directory,
                    &mut files,
                )?,
            });
            object_count += 1;
        }
        let page = PageSource {
            schema: "hmi.page/1".into(),
            name: page.name.clone(),
            properties: own,
            events: events(&page.objects[0], "page", &directory, &mut files)?,
            objects,
            original_record: Some(format!("pages/{index}")),
        };
        files.insert(PathBuf::from(source), file::toml(&page)?);
    }
    let project = ProjectSource {
        original: Some(OriginalConfig {
            bundle: ".preserve/origin.hpo".into(),
        }),
        schema: "hmi.project/1".into(),
        name: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported")
            .into(),
        target: parsed.target.clone(),
        program: ProgramSource {
            source: "program.s".into(),
        },
        build: BuildConfig::default(),
        pages: PageSources { sources },
        assets: AssetSources::default(),
    };
    files.insert(PathBuf::from("hmi.toml"), file::toml(&project)?);
    let output = file::absolute(output)?;
    let parent = output
        .parent()
        .ok_or_else(|| error("import_output", "无法确定导入目录的父目录"))?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".hmi-import-")
        .tempdir_in(parent)?;
    for (relative, bytes) in files {
        let dest = stage.path().join(relative);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::write(dest, bytes)?;
    }
    if output.exists() {
        fs::remove_dir(&output)?;
    }
    fs::rename(stage.path(), &output)?;
    Ok(ImportReport {
        directory: output,
        target: parsed.target,
        pages: parsed.project.pages.len(),
        objects: object_count,
        extra_resources: bundle.metadata.extras.len(),
        original_bundle: ".preserve/origin.hpo".into(),
    })
}
