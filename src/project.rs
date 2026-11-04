use crate::binary::*;
use crate::container::{Container, DirectoryStatus, Entry};
use crate::diagnostic::{Diagnostic, Location, has_errors};
use crate::page::Page;
use crate::{Error, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, Serialize)]
pub struct Reference {
    pub kind: String,
    pub resource: String,
}

#[derive(Debug)]
pub struct Project {
    pub encoding: u8,
    pub references: Vec<Reference>,
    pub pages: Vec<(usize, Page)>,
    pub program_index: Option<usize>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Project {
    pub fn parse(container: &Container) -> Result<Self> {
        let main = container
            .active("main.HMI")
            .ok_or_else(|| Error::Message("缺少活动资源 main.HMI".into()))?;
        let bytes = container.data(main)?;
        slice(bytes, 0, 32)?;
        let mut diagnostics = Vec::new();
        if main_crc(bytes)? != u32_at(bytes, 0)? {
            diagnostics.push(Diagnostic::error("main_crc", "main.HMI CRC 错误"));
        }
        let encoding = bytes[13];
        if encoding != 0x18 {
            diagnostics.push(Diagnostic::warning(
                "encoding",
                format!("工程编码 0x{encoding:02X} 尚未验证，仅支持诊断"),
            ));
        }
        let start = usize32(bytes, 24)?;
        if start < 32 {
            return Err(Error::Message("工程引用表与头部重叠".into()));
        }
        let count = usize32(bytes, 28)?;
        checked_table(bytes, start, count, 16)?;
        let mut references = Vec::with_capacity(count);
        let mut referenced = HashSet::new();
        for i in 0..count {
            let offset = start + i * 16;
            let kind = name(&bytes[offset..offset + 8])?;
            let resource = name(&bytes[offset + 8..offset + 16])?;
            if !referenced.insert(resource.clone()) {
                diagnostics.push(Diagnostic::error(
                    "duplicate_reference",
                    format!("资源引用重复：{resource}"),
                ));
            }
            if container.active(&resource).is_none() {
                diagnostics.push(Diagnostic::error(
                    "missing_resource",
                    format!("工程引用不存在的资源：{resource}"),
                ));
            }
            if resource.rsplit_once('.').is_none_or(|(_, ext)| ext != kind) {
                diagnostics.push(Diagnostic::error(
                    "reference_type",
                    format!("资源引用类型与后缀不一致：{kind} / {resource}"),
                ));
            }
            references.push(Reference { kind, resource });
        }
        let mut pages = Vec::new();
        let mut page_names = HashSet::new();
        let mut order: Vec<_> = references
            .iter()
            .filter(|r| r.kind == "pa")
            .map(|r| r.resource.clone())
            .collect();
        for entry in &container.entries {
            if !entry.deleted && entry.name.ends_with(".pa") && !order.contains(&entry.name) {
                diagnostics.push(Diagnostic::error(
                    "unreferenced_page",
                    format!("活动页面不在引用表内：{}", entry.name),
                ));
                order.push(entry.name.clone());
            }
        }
        for resource in order {
            let Some(entry) = container.active(&resource) else {
                continue;
            };
            match container.data(entry).and_then(Page::parse) {
                Ok(page) => {
                    let location = Location {
                        page: Some(page.name.clone()),
                        file: Some(resource.clone()),
                        ..Location::default()
                    };
                    if page.raw[21] != 0x55 || page.raw[22] != 0x21 || page.index_start != 56 {
                        diagnostics.push(
                            Diagnostic::error(
                                "page_format",
                                "未验证的页面格式或索引版本，首版只允许诊断",
                            )
                            .at(location.clone()),
                        );
                    }
                    if !page.crc_valid {
                        diagnostics.push(
                            Diagnostic::error("page_crc", "页面 CRC 错误").at(location.clone()),
                        );
                    }
                    if !page.length_valid {
                        diagnostics.push(
                            Diagnostic::error("page_length", "页面总长度错误").at(location.clone()),
                        );
                    }
                    if !page_names.insert(page.name.clone()) {
                        diagnostics.push(
                            Diagnostic::error("page_name", "页面名称重复").at(location.clone()),
                        );
                    }
                    if page
                        .objects
                        .first()
                        .is_none_or(|o| o.type_code != 121 || o.id != 0 || o.name != page.name)
                    {
                        diagnostics.push(
                            Diagnostic::error(
                                "page_object",
                                "页面自身对象的名称、类型或 ID 不正确",
                            )
                            .at(location.clone()),
                        );
                    }
                    if page.objects.len() > 251
                        || page
                            .objects
                            .iter()
                            .enumerate()
                            .any(|(i, o)| o.id as usize != i)
                    {
                        diagnostics.push(
                            Diagnostic::error(
                                "object_id",
                                "对象 ID 必须连续，且页面最多包含 250 个普通控件",
                            )
                            .at(location),
                        );
                    }
                    pages.push((entry.index, page));
                }
                Err(error) => diagnostics.push(
                    Diagnostic::error("page_structure", error.to_string()).at(Location {
                        file: Some(resource),
                        ..Location::default()
                    }),
                ),
            }
        }
        let program_index = container.active("Program.s").map(|e| e.index);
        if program_index.is_none() {
            diagnostics.push(Diagnostic::error("missing_program", "缺少 Program.s"));
        }
        Ok(Self {
            encoding,
            references,
            pages,
            program_index,
            diagnostics,
        })
    }

    pub fn strict(&self) -> Result<()> {
        if has_errors(&self.diagnostics) {
            return Err(Error::Message(
                "工程或页面校验失败，请使用 inspect 查看诊断".into(),
            ));
        }
        if self.encoding != 0x18 {
            return Err(Error::Message("首版仅支持编码 0x18（UTF-8）的工程".into()));
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct PageSummary {
    pub id: usize,
    pub resource: String,
    pub name: String,
    pub objects: usize,
    pub events: usize,
    pub nonempty_events: usize,
    pub crc_valid: bool,
}

#[derive(Serialize)]
pub struct Inspection {
    pub version: &'static str,
    pub file_size: usize,
    pub entries: usize,
    pub active_resources: usize,
    pub deleted_resources: usize,
    pub encoding: Option<u8>,
    pub pages: usize,
    pub objects: usize,
    pub events: usize,
    pub nonempty_events: usize,
    pub contiguous: bool,
    pub primary_directory: Option<DirectoryStatus>,
    pub backup_directory: Option<DirectoryStatus>,
    pub resource_types: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_list: Option<Vec<PageSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_list: Option<Vec<Entry>>,
}

pub fn inspect(
    container: &Container,
    include_pages: bool,
    include_resources: bool,
) -> (Inspection, Vec<Diagnostic>) {
    let mut diagnostics = container.diagnostics.clone();
    let project = match Project::parse(container) {
        Ok(project) => {
            diagnostics.extend(project.diagnostics.clone());
            Some(project)
        }
        Err(error) => {
            diagnostics.push(Diagnostic::error("project_structure", error.to_string()));
            None
        }
    };
    let page_list: Vec<_> = project
        .as_ref()
        .map(|p| {
            p.pages
                .iter()
                .enumerate()
                .map(|(id, (index, page))| PageSummary {
                    id,
                    resource: container.entries[*index].name.clone(),
                    name: page.name.clone(),
                    objects: page.objects.len(),
                    events: page.objects.iter().map(|o| o.events.len()).sum(),
                    nonempty_events: page
                        .objects
                        .iter()
                        .flat_map(|o| &o.events)
                        .filter(|e| !e.lines.is_empty())
                        .count(),
                    crc_valid: page.crc_valid,
                })
                .collect()
        })
        .unwrap_or_default();
    let mut resource_types = BTreeMap::new();
    for entry in container.entries.iter().filter(|e| !e.deleted) {
        *resource_types
            .entry(entry.name.rsplit('.').next().unwrap_or("").to_owned())
            .or_default() += 1;
    }
    let active_resources = container.entries.iter().filter(|e| !e.deleted).count();
    (
        Inspection {
            version: "ver21234",
            file_size: container.bytes.len(),
            entries: container.entries.len(),
            active_resources,
            deleted_resources: container.entries.len() - active_resources,
            encoding: project.as_ref().map(|p| p.encoding),
            pages: page_list.len(),
            objects: page_list.iter().map(|p| p.objects).sum(),
            events: page_list.iter().map(|p| p.events).sum(),
            nonempty_events: page_list.iter().map(|p| p.nonempty_events).sum(),
            contiguous: container.contiguous,
            primary_directory: container.primary.clone(),
            backup_directory: container.backup.clone(),
            resource_types,
            page_list: include_pages.then_some(page_list),
            resource_list: include_resources.then(|| container.entries.clone()),
        },
        diagnostics,
    )
}
