use crate::binary::*;
use crate::{Error, Result};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug)]
pub struct Attribute {
    pub name: String,
    pub record: usize,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub kind: String,
    pub record: usize,
    pub lines: Vec<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct Object {
    pub name: String,
    pub type_code: u8,
    pub id: u8,
    pub records: Vec<Vec<u8>>,
    pub attributes: Vec<Attribute>,
    pub events: Vec<Event>,
    pub raw: Vec<u8>,
}

fn count_marker(bytes: &[u8], prefix: &str) -> Result<usize> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| Error::Message("记录标记不是 UTF-8".into()))?;
    let digits = text
        .strip_prefix(prefix)
        .ok_or_else(|| Error::Message(format!("缺少 {prefix} 标记")))?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::Message(format!("无效的记录数量：{text}")));
    }
    digits
        .parse()
        .map_err(|_| Error::Message(format!("记录数量溢出：{text}")))
}

impl Object {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let mut records = Vec::new();
        let mut cursor = 0;
        while cursor < raw.len() {
            let length = usize32(raw, cursor)?;
            cursor += 4;
            records.push(slice(raw, cursor, length)?.to_vec());
            cursor += length;
        }
        let first = records
            .first()
            .ok_or_else(|| Error::Message("对象记录为空".into()))?;
        let count = count_marker(first, "att-")?;
        if count > records.len().saturating_sub(1) {
            return Err(Error::Message("属性计数超出对象记录边界".into()));
        }
        let mut attributes = Vec::new();
        let mut names = HashSet::new();
        for (i, record) in records.iter().enumerate().skip(1).take(count) {
            let property = name(slice(record, 0, 16)?)?;
            if property.is_empty() || !names.insert(property.clone()) {
                return Err(Error::Message(format!("属性名为空或重复：{property}")));
            }
            attributes.push(Attribute {
                name: property,
                record: i,
                value: record[16..].to_vec(),
            });
        }
        let get = |key: &str| -> Result<&[u8]> {
            attributes
                .iter()
                .find(|a| a.name == key)
                .map(|a| a.value.as_slice())
                .ok_or_else(|| Error::Message(format!("对象缺少属性 {key}")))
        };
        let object_name = std::str::from_utf8(get("objname")?)
            .map_err(|_| Error::Message("objname 不是 UTF-8".into()))?
            .to_owned();
        let type_code = *slice(get("type")?, 0, 1)?.first().unwrap();
        let id = *slice(get("id")?, 0, 1)?.first().unwrap();
        if get("type")?.len() != 1 || get("id")?.len() != 1 {
            return Err(Error::Message("type/id 宽度必须为 1 字节".into()));
        }
        let mut events = Vec::new();
        let mut kinds = HashSet::new();
        let mut i = count + 1;
        while i < records.len() {
            let record = &records[i];
            if record.starts_with(b"codes") {
                let marker = std::str::from_utf8(record)
                    .map_err(|_| Error::Message("事件标记不是 UTF-8".into()))?;
                let (label, _) = marker
                    .rsplit_once('-')
                    .ok_or_else(|| Error::Message("事件标记缺少行数".into()))?;
                let kind = label.strip_prefix("codes").unwrap().to_owned();
                if kind.is_empty() || !kinds.insert(kind.clone()) {
                    return Err(Error::Message(format!("事件槽为空或重复：{kind}")));
                }
                let lines = count_marker(record, &format!("{label}-"))?;
                if lines > records.len() - i - 1 {
                    return Err(Error::Message(format!("事件 {kind} 行数超出对象边界")));
                }
                events.push(Event {
                    kind,
                    record: i,
                    lines: records[i + 1..i + 1 + lines].to_vec(),
                });
                i += lines + 1;
            } else {
                i += 1;
            }
        }
        Ok(Self {
            name: object_name,
            type_code,
            id,
            records,
            attributes,
            events,
            raw: raw.to_vec(),
        })
    }

    pub fn attribute(&self, key: &str) -> Option<&Attribute> {
        self.attributes.iter().find(|a| a.name == key)
    }

    pub fn integer(&self, key: &str) -> Option<u32> {
        let bytes = &self.attribute(key)?.value;
        if bytes.len() > 4 {
            return None;
        }
        Some(
            bytes
                .iter()
                .enumerate()
                .fold(0, |v, (i, b)| v | (u32::from(*b) << (i * 8))),
        )
    }

    pub fn rebuild(
        &self,
        values: &BTreeMap<String, Vec<u8>>,
        codes: &BTreeMap<String, Vec<Vec<u8>>>,
    ) -> Result<Vec<u8>> {
        let mut records = Vec::new();
        let mut i = 0;
        while i < self.records.len() {
            if let Some(event) = self.events.iter().find(|event| event.record == i) {
                if let Some(lines) = codes.get(&event.kind) {
                    if lines != &event.lines {
                        records.push(format!("codes{}-{}", event.kind, lines.len()).into_bytes());
                        records.extend(lines.iter().cloned());
                    } else {
                        records.extend(self.records[i..i + 1 + event.lines.len()].iter().cloned());
                    }
                } else {
                    records.extend(self.records[i..i + 1 + event.lines.len()].iter().cloned());
                }
                i += event.lines.len() + 1;
            } else {
                let mut record = self.records[i].clone();
                if let Some(attr) = self.attributes.iter().find(|attr| attr.record == i)
                    && let Some(value) = values.get(&attr.name)
                {
                    record.truncate(16);
                    record.extend_from_slice(value);
                }
                records.push(record);
                i += 1;
            }
        }
        let mut bytes = Vec::new();
        for record in records {
            let length = u32::try_from(record.len())
                .map_err(|_| Error::Message("对象记录超过 u32 长度".into()))?;
            bytes.extend_from_slice(&length.to_le_bytes());
            bytes.extend(record);
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug)]
pub struct ObjectIndex {
    pub offset: usize,
    pub length: usize,
    pub unknown: u32,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub name: String,
    pub objects: Vec<Object>,
    pub indices: Vec<ObjectIndex>,
    pub index_start: usize,
    pub raw: Vec<u8>,
    pub crc_valid: bool,
    pub length_valid: bool,
}

impl Page {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        slice(bytes, 0, 56)?;
        let index_start = usize32(bytes, 8)?;
        if index_start < 56 {
            return Err(Error::Message("页面索引与页头重叠".into()));
        }
        let count = usize32(bytes, 12)?;
        let end = checked_table(bytes, index_start, count, 12)?;
        let mut objects = Vec::with_capacity(count);
        let mut indices = Vec::with_capacity(count);
        let mut names = HashSet::new();
        let mut ranges = Vec::new();
        for i in 0..count {
            let index = index_start + i * 12;
            let offset = usize32(bytes, index)?
                .checked_add(56)
                .ok_or_else(|| Error::Message("对象偏移溢出".into()))?;
            let length = usize32(bytes, index + 4)?;
            if offset < end {
                return Err(Error::Message(format!("对象 #{i} 与页头或索引重叠")));
            }
            let object = Object::parse(slice(bytes, offset, length)?)
                .map_err(|e| Error::Message(format!("对象 #{i}：{e}")))?;
            if !names.insert(object.name.clone()) {
                return Err(Error::Message(format!("对象名重复：{}", object.name)));
            }
            ranges.push((offset, offset + length));
            objects.push(object);
            indices.push(ObjectIndex {
                offset,
                length,
                unknown: u32_at(bytes, index + 8)?,
            });
        }
        ranges.sort_unstable();
        if ranges.windows(2).any(|r| r[0].1 > r[1].0) {
            return Err(Error::Message("对象数据范围重叠".into()));
        }
        let page_name = name(&bytes[24..40])?;
        Ok(Self {
            name: page_name,
            objects,
            indices,
            index_start,
            raw: bytes.to_vec(),
            crc_valid: page_crc(bytes)? == u32_at(bytes, 0)?,
            length_valid: bytes.len() == usize32(bytes, 4)?,
        })
    }

    pub fn rebuild(&self, objects: &[Vec<u8>]) -> Result<Vec<u8>> {
        if objects.len() != self.objects.len() {
            return Err(Error::Message("首版禁止改变对象数量".into()));
        }
        if objects.iter().zip(&self.objects).all(|(a, b)| a == &b.raw) {
            return Ok(self.raw.clone());
        }
        let end = self.index_start + self.indices.len() * 12;
        let mut result = self.raw[..end].to_vec();
        let mut cursor = end;
        for (i, (index, object)) in self.indices.iter().zip(objects).enumerate() {
            if index.offset < cursor {
                return Err(Error::Message(
                    "首版不支持对象物理顺序与索引顺序不一致的页面修改".into(),
                ));
            }
            result.extend_from_slice(&self.raw[cursor..index.offset]);
            let offset = result.len() - 56;
            put_u32(&mut result, self.index_start + i * 12, offset)?;
            put_u32(&mut result, self.index_start + i * 12 + 4, object.len())?;
            result.extend_from_slice(object);
            cursor = index.offset + index.length;
        }
        result.extend_from_slice(&self.raw[cursor..]);
        let length = result.len();
        put_u32(&mut result, 4, length)?;
        let crc = page_crc(&result)?;
        result[..4].copy_from_slice(&crc.to_le_bytes());
        Ok(result)
    }
}

pub fn type_name(code: u8) -> String {
    let name = match code {
        116 => "text",
        55 => "scrolling_text",
        54 => "number",
        59 => "xfloat",
        98 => "button",
        106 => "progress_bar",
        112 => "picture",
        113 => "crop",
        109 => "hotspot",
        5 => "touch_cap",
        122 => "gauge",
        0 => "waveform",
        1 => "slider",
        51 => "timer",
        52 => "variable",
        53 => "dual_state_button",
        56 => "checkbox",
        57 => "radio",
        58 => "qrcode",
        67 => "switch",
        61 => "combo_box",
        68 => "text_select",
        62 => "sliding_text",
        66 => "data_record",
        65 => "file_browser",
        63 => "file_stream",
        2 => "animation",
        3 => "video",
        4 => "audio",
        60 => "external_picture",
        121 => "page",
        _ => return format!("unknown_0x{code:02x}"),
    };
    name.into()
}
