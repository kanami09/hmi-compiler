//! 工程语义差异。
use crate::container::Container;
use crate::page::{Object, Page};
use crate::project::Project;
use crate::{Result, hex, sha256};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct Difference {
    pub kind: String,
    pub resource: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub before: Value,
    pub after: Value,
}

#[derive(Debug, Serialize)]
pub struct DiffReport {
    pub byte_identical: bool,
    pub active_resources_equal: bool,
    pub container_changed: bool,
    pub old_sha256: String,
    pub new_sha256: String,
    pub changes: Vec<Difference>,
}

fn change(
    kind: &str,
    resource: &str,
    page: Option<&str>,
    object: Option<&str>,
    field: Option<&str>,
    before: Value,
    after: Value,
) -> Difference {
    Difference {
        kind: kind.into(),
        resource: resource.into(),
        page: page.map(str::to_owned),
        object: object.map(str::to_owned),
        field: field.map(str::to_owned),
        before,
        after,
    }
}

fn attributes(object: &Object) -> BTreeMap<String, Value> {
    let mut props: BTreeMap<_, _> = crate::workspace::properties(object)
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, json!(v)))
        .collect();
    for attr in &object.attributes {
        props
            .entry(attr.name.clone())
            .or_insert_with(|| match attr.name.as_str() {
                "objname" => json!(object.name),
                "id" | "type" | "endx" | "endy" => json!(object.integer(&attr.name)),
                _ => json!({"raw":hex(&attr.value)}),
            });
    }
    props
}

fn lines(lines: &[Vec<u8>]) -> Value {
    json!(
        lines
            .iter()
            .map(|line| match std::str::from_utf8(line) {
                Ok(text) => json!(text),
                Err(_) => json!({"raw":hex(line)}),
            })
            .collect::<Vec<_>>()
    )
}

fn object_metadata(object: &Object) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < object.records.len() {
        if let Some(attr) = object.attributes.iter().find(|a| a.record == i) {
            result.push(b'A');
            result.extend_from_slice(&object.records[attr.record][..16]);
            i += 1;
        } else if let Some(event) = object.events.iter().find(|e| e.record == i) {
            result.push(b'E');
            result.extend_from_slice(&(event.kind.len() as u64).to_le_bytes());
            result.extend_from_slice(event.kind.as_bytes());
            i += 1 + event.lines.len();
        } else {
            // att-N 的计数单独由属性集合体现；其余原始记录不能漏报。
            if i != 0 {
                result.push(b'R');
                result.extend_from_slice(&(object.records[i].len() as u64).to_le_bytes());
                result.extend_from_slice(&object.records[i]);
            }
            i += 1;
        }
    }
    result
}

fn page_metadata(page: &Page) -> Vec<u8> {
    let mut result = page.raw[..page.index_start].to_vec();
    result[..8].fill(0); // CRC、总长度属于派生数据。
    let mut cursor = page.index_start + page.indices.len() * 12;
    for index in &page.indices {
        result.extend_from_slice(&index.unknown.to_le_bytes());
        if index.offset >= cursor {
            let gap = &page.raw[cursor..index.offset];
            result.extend_from_slice(&(gap.len() as u64).to_le_bytes());
            result.extend_from_slice(gap);
        }
        cursor = index.offset + index.length;
    }
    result.extend_from_slice(&page.raw[cursor..]);
    result
}

fn page_diff(resource: &str, old: &Page, new: &Page, changes: &mut Vec<Difference>) {
    let initial = changes.len();
    if old.name != new.name {
        changes.push(change(
            "page_name",
            resource,
            Some(&old.name),
            None,
            None,
            json!(old.name),
            json!(new.name),
        ));
    }
    let old_order: Vec<_> = old.objects.iter().map(|o| &o.name).collect();
    let new_order: Vec<_> = new.objects.iter().map(|o| &o.name).collect();
    if old_order != new_order {
        changes.push(change(
            "object_order",
            resource,
            Some(&new.name),
            None,
            None,
            json!(old_order),
            json!(new_order),
        ));
    }
    let old_objects: BTreeMap<_, _> = old.objects.iter().map(|o| (&o.name, o)).collect();
    let new_objects: BTreeMap<_, _> = new.objects.iter().map(|o| (&o.name, o)).collect();
    let names: BTreeSet<_> = old_objects
        .keys()
        .chain(new_objects.keys())
        .copied()
        .collect();
    for name in names {
        match (old_objects.get(name), new_objects.get(name)) {
            (Some(a), Some(b)) => {
                if a.raw == b.raw {
                    continue;
                }
                let start = changes.len();
                let ap = attributes(a);
                let bp = attributes(b);
                for key in ap.keys().chain(bp.keys()).collect::<BTreeSet<_>>() {
                    if ap.get(key) != bp.get(key) {
                        changes.push(change(
                            "property",
                            resource,
                            Some(&new.name),
                            Some(name),
                            Some(key),
                            ap.get(key).cloned().unwrap_or(Value::Null),
                            bp.get(key).cloned().unwrap_or(Value::Null),
                        ));
                    }
                }
                let ae: BTreeMap<_, _> = a.events.iter().map(|e| (&e.kind, &e.lines)).collect();
                let be: BTreeMap<_, _> = b.events.iter().map(|e| (&e.kind, &e.lines)).collect();
                for kind in ae.keys().chain(be.keys()).copied().collect::<BTreeSet<_>>() {
                    if ae.get(kind) != be.get(kind) {
                        changes.push(change(
                            "event",
                            resource,
                            Some(&new.name),
                            Some(name),
                            Some(kind),
                            ae.get(kind).map(|v| lines(v)).unwrap_or(Value::Null),
                            be.get(kind).map(|v| lines(v)).unwrap_or(Value::Null),
                        ));
                    }
                }
                if object_metadata(a) != object_metadata(b) || start == changes.len() {
                    changes.push(change(
                        "object_metadata",
                        resource,
                        Some(&new.name),
                        Some(name),
                        None,
                        json!(sha256(&a.raw)),
                        json!(sha256(&b.raw)),
                    ));
                }
            }
            (Some(a), None) => changes.push(change(
                "object_removed",
                resource,
                Some(&old.name),
                Some(name),
                None,
                json!({"type":a.type_code,"id":a.id}),
                Value::Null,
            )),
            (None, Some(b)) => changes.push(change(
                "object_added",
                resource,
                Some(&new.name),
                Some(name),
                None,
                Value::Null,
                json!({"type":b.type_code,"id":b.id}),
            )),
            _ => {}
        }
    }
    if page_metadata(old) != page_metadata(new) || changes.len() == initial {
        changes.push(change(
            "page_metadata",
            resource,
            Some(&new.name),
            None,
            None,
            json!(sha256(&old.raw)),
            json!(sha256(&new.raw)),
        ));
    }
}

pub fn compare(old: &Container, new: &Container) -> Result<DiffReport> {
    old.strict()?;
    new.strict()?;
    let old_project = Project::parse(old)?;
    old_project.strict()?;
    let new_project = Project::parse(new)?;
    new_project.strict()?;
    let old_resources: BTreeMap<_, _> = old
        .entries
        .iter()
        .filter(|e| !e.deleted)
        .map(|e| (e.name.as_str(), e))
        .collect();
    let new_resources: BTreeMap<_, _> = new
        .entries
        .iter()
        .filter(|e| !e.deleted)
        .map(|e| (e.name.as_str(), e))
        .collect();
    let old_pages: BTreeMap<_, _> = old_project
        .pages
        .iter()
        .map(|(i, p)| (old.entries[*i].name.as_str(), p))
        .collect();
    let new_pages: BTreeMap<_, _> = new_project
        .pages
        .iter()
        .map(|(i, p)| (new.entries[*i].name.as_str(), p))
        .collect();
    let mut changes = Vec::new();
    let mut equal = true;
    for name in old_resources
        .keys()
        .chain(new_resources.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        match (old_resources.get(name), new_resources.get(name)) {
            (Some(a), Some(b)) => {
                let left = old.data(a)?;
                let right = new.data(b)?;
                if left == right {
                    continue;
                }
                equal = false;
                if let (Some(a), Some(b)) = (old_pages.get(name), new_pages.get(name)) {
                    page_diff(name, a, b, &mut changes);
                } else if name == "Program.s" {
                    changes.push(change(
                        "global_script",
                        name,
                        None,
                        None,
                        None,
                        json!(String::from_utf8_lossy(left)),
                        json!(String::from_utf8_lossy(right)),
                    ));
                } else {
                    changes.push(change(
                        "resource",
                        name,
                        None,
                        None,
                        None,
                        json!({"sha256":sha256(left),"size":left.len()}),
                        json!({"sha256":sha256(right),"size":right.len()}),
                    ));
                }
            }
            (Some(a), None) => {
                equal = false;
                changes.push(change(
                    if name.ends_with(".pa") {
                        "page_removed"
                    } else {
                        "resource_removed"
                    },
                    name,
                    None,
                    None,
                    None,
                    json!({"sha256":sha256(old.data(a)?),"size":a.length}),
                    Value::Null,
                ));
            }
            (None, Some(b)) => {
                equal = false;
                changes.push(change(
                    if name.ends_with(".pa") {
                        "page_added"
                    } else {
                        "resource_added"
                    },
                    name,
                    None,
                    None,
                    None,
                    Value::Null,
                    json!({"sha256":sha256(new.data(b)?),"size":b.length}),
                ));
            }
            _ => {}
        }
    }
    let old_order: Vec<_> = old_project
        .pages
        .iter()
        .map(|(i, _)| &old.entries[*i].name)
        .collect();
    let new_order: Vec<_> = new_project
        .pages
        .iter()
        .map(|(i, _)| &new.entries[*i].name)
        .collect();
    if old_order != new_order {
        changes.push(change(
            "page_order",
            "main.HMI",
            None,
            None,
            None,
            json!(old_order),
            json!(new_order),
        ));
    }
    let identical = old.bytes == new.bytes;
    Ok(DiffReport {
        byte_identical: identical,
        active_resources_equal: equal,
        container_changed: !identical,
        old_sha256: sha256(&old.bytes),
        new_sha256: sha256(&new.bytes),
        changes,
    })
}

pub fn diff(old: &Path, new: &Path) -> Result<DiffReport> {
    compare(
        &Container::parse(std::fs::read(old)?)?,
        &Container::parse(std::fs::read(new)?)?,
    )
}
