//! HPO只保存有效原始记录与活动资源，不保存原HMI的目录布局、空闲区或删除历史。
use hmi_core::{
    Result,
    controls::{Definition, FieldValue},
    error, file,
    model::check_schema,
    sha256,
    target::TargetConfig,
};
use hmi_format::{
    container::Container,
    page::{Object, Page},
    project::Project,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
const MAGIC: &[u8; 8] = b"HMIORG01";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blob {
    offset: usize,
    length: usize,
    sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalPage {
    pub resource: String,
    pub directory_unknown: [u8; 3],
    blob: Blob,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraResource {
    pub name: String,
    pub directory_unknown: [u8; 3],
    blob: Blob,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub managed_resources: Vec<String>,
    pub schema: String,
    pub target: TargetConfig,
    pub source_sha256: String,
    main: Blob,
    pub main_directory_unknown: [u8; 3],
    pub program_directory_unknown: [u8; 3],
    pub pages: Vec<OriginalPage>,
    pub extras: Vec<ExtraResource>,
}
#[derive(Clone, Debug)]
pub struct OriginalBundle {
    pub metadata: OriginalMetadata,
    payload: Vec<u8>,
}
fn push(payload: &mut Vec<u8>, bytes: &[u8]) -> Blob {
    let offset = payload.len();
    payload.extend(bytes);
    Blob {
        offset,
        length: bytes.len(),
        sha256: sha256(bytes),
    }
}
impl OriginalBundle {
    pub fn capture(container: &Container, project: &Project, target: TargetConfig) -> Result<Self> {
        Self::capture_managed(container, project, target, &BTreeSet::new())
    }
    pub fn capture_managed(
        container: &Container,
        project: &Project,
        target: TargetConfig,
        managed: &BTreeSet<String>,
    ) -> Result<Self> {
        let mut payload = Vec::new();
        let main = container.active("main.HMI").unwrap();
        let main_blob = push(&mut payload, container.data(main)?);
        let mut pages = Vec::new();
        let mut used = BTreeSet::from([main.index, project.program_index.unwrap()]);
        for (index, page) in &project.pages {
            let entry = &container.entries[*index];
            used.insert(*index);
            pages.push(OriginalPage {
                resource: entry.name.clone(),
                directory_unknown: entry.unknown,
                blob: push(&mut payload, &page.raw),
            });
        }
        let mut extras = Vec::new();
        for entry in container
            .entries
            .iter()
            .filter(|e| !e.deleted && !used.contains(&e.index) && !managed.contains(&e.name))
        {
            extras.push(ExtraResource {
                name: entry.name.clone(),
                directory_unknown: entry.unknown,
                blob: push(&mut payload, container.data(entry)?),
            });
        }
        let result = Self {
            metadata: OriginalMetadata {
                managed_resources: managed.iter().cloned().collect(),
                schema: "hmi.original/1".into(),
                target,
                source_sha256: sha256(&container.bytes),
                main: main_blob,
                main_directory_unknown: main.unknown,
                program_directory_unknown: container.entries[project.program_index.unwrap()]
                    .unknown,
                pages,
                extras,
            },
            payload,
        };
        result.validate()?;
        Ok(result)
    }
    fn data(&self, blob: &Blob) -> Result<&[u8]> {
        self.payload
            .get(
                blob.offset
                    ..blob
                        .offset
                        .checked_add(blob.length)
                        .ok_or_else(|| error("original_length", "附件成员范围溢出"))?,
            )
            .ok_or_else(|| error("original_length", "附件成员超出载荷"))
    }
    pub fn main(&self) -> Result<&[u8]> {
        self.data(&self.metadata.main)
    }
    pub fn page(&self, index: usize) -> Result<Page> {
        let p = self
            .metadata
            .pages
            .get(index)
            .ok_or_else(|| error("original_record", "原始页面定位不存在"))?;
        Page::parse(self.data(&p.blob)?)
    }
    pub fn page_index(locator: &str) -> Result<usize> {
        locator
            .strip_prefix("pages/")
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| error("original_record", "页面定位须为pages/序号"))
    }
    pub fn object(&self, locator: &str) -> Result<OriginalRecord> {
        let (page, object) = locator
            .split_once("/objects/")
            .ok_or_else(|| error("original_record", "控件定位须为pages/序号/objects/序号"))?;
        let index = object
            .parse::<usize>()
            .map_err(|_| error("original_record", "原始控件序号无效"))?;
        let page = self.page(Self::page_index(page)?)?;
        let object = page
            .objects
            .get(index)
            .ok_or_else(|| error("original_record", "原始控件定位不存在"))?;
        Ok(OriginalRecord {
            raw: object.raw.clone(),
            index_unknown: page.indices[index].unknown,
        })
    }
    pub fn extra_bytes(&self, extra: &ExtraResource) -> Result<&[u8]> {
        self.data(&extra.blob)
    }
    pub fn validate(&self) -> Result<()> {
        check_schema(&self.metadata.schema, "hmi.original/1")?;
        hmi_core::target::Target::resolve(&self.metadata.target)?;
        let main = self.main()?;
        if main.len() < 96
            || hmi_format::binary::main_crc(main)? != hmi_format::binary::u32_at(main, 0)?
            || hmi_format::binary::usize32(main, 24)? != 96
        {
            return Err(error("original_main", "附件中的主工程头或CRC无效"));
        }
        hmi_format::binary::checked_table(main, 96, hmi_format::binary::usize32(main, 28)?, 16)?;
        let mut cursor = 0;
        let mut names = BTreeSet::from(["main.HMI", "Program.s"]);
        for blob in std::iter::once(&self.metadata.main)
            .chain(self.metadata.pages.iter().map(|p| &p.blob))
            .chain(self.metadata.extras.iter().map(|e| &e.blob))
        {
            if blob.offset != cursor || sha256(self.data(blob)?) != blob.sha256 {
                return Err(error("original_hash", "原始附件的成员布局或摘要无效"));
            }
            cursor += blob.length;
        }
        if cursor != self.payload.len() {
            return Err(error("original_length", "原始附件存在未声明载荷"));
        }
        for (index, page) in self.metadata.pages.iter().enumerate() {
            if !names.insert(&page.resource) {
                return Err(error("original_resource", "原始附件资源名重复"));
            }
            let p = self.page(index)?;
            if p.index_start != 56 || !p.crc_valid || !p.length_valid {
                return Err(error("original_page", "原始页面结构或校验无效"));
            }
        }
        for extra in &self.metadata.extras {
            if !names.insert(&extra.name) {
                return Err(error("original_resource", "原始附件资源名重复"));
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        super::envelope::encode(MAGIC, &self.metadata, &self.payload, "original")
    }
    pub fn read(path: &Path) -> Result<Self> {
        let run = || {
            let (metadata, payload) =
                super::envelope::decode(MAGIC, &file::read(path)?, "original")?;
            let r = Self { metadata, payload };
            r.validate()?;
            Ok(r)
        };
        run().map_err(|e| file::at_file(e, path))
    }
    pub fn write(&self, path: &Path, inputs: &[&Path]) -> Result<()> {
        file::write_atomic(path, &self.encode()?, inputs)
    }
}

/// HMO携带编译所需的记录本身，链接器不再读取源码或原HMI。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalRecord {
    pub raw: Vec<u8>,
    pub index_unknown: u32,
}

pub fn merge_record(
    definition: &Definition,
    name: &str,
    attributes: &[(String, Vec<u8>)],
    events: &[(String, Vec<Vec<u8>>)],
    original: &OriginalRecord,
) -> Result<Vec<u8>> {
    let old = Object::parse(&original.raw)?;
    if old.type_code != definition.code {
        return Err(error("original_type", "原始记录类型与当前控件不一致"));
    }
    if old
        .events
        .iter()
        .map(|e| e.kind.as_str())
        .collect::<Vec<_>>()
        != definition.events
    {
        return Err(error("original_events", "原始事件槽与当前支持的类型不一致"));
    }
    let mut values = BTreeMap::new();
    for field in definition.fields {
        if old.attribute(field.name).is_none() {
            return Err(error(
                "original_layout",
                format!("原始记录缺少当前类型所需的属性 {}", field.name),
            ));
        }
        let editable = match field.value {
            FieldValue::Constant(_) => false,
            FieldValue::PropertyOrConstant(_) => {
                definition.all_properties().any(|p| p.name == field.name)
            }
            _ => true,
        };
        if editable {
            let value = attributes
                .iter()
                .find(|(k, _)| k == field.name)
                .unwrap()
                .1
                .clone();
            values.insert(field.name.into(), value);
        }
    }
    let codes = events.iter().cloned().collect();
    let raw = old.rebuild(&values, &codes)?;
    if Object::parse(&raw)?.name != name {
        return Err(error("original_layout", "当前名称未正确写入原始记录"));
    }
    Ok(raw)
}
pub fn validate_record(
    definition: &Definition,
    current: &Object,
    original: Option<&OriginalRecord>,
) -> Result<()> {
    let attrs = current
        .attributes
        .iter()
        .map(|a| (a.name.clone(), a.value.clone()))
        .collect::<Vec<_>>();
    let Some(original) = original else {
        return definition.validate_record(&current.name, &attrs);
    };
    let properties = definition.properties_from_record(&attrs)?;
    let generated = definition.attributes(&current.name, &properties)?;
    let events = current
        .events
        .iter()
        .map(|e| (e.kind.clone(), e.lines.clone()))
        .collect::<Vec<_>>();
    if merge_record(definition, &current.name, &generated, &events, original)? != current.raw {
        return Err(error(
            "original_record",
            "目标记录与当前属性或声明的原始数据不一致",
        ));
    }
    Ok(())
}
