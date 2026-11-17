//! 新工程的记录和容器写入，不复刻原文件的已删除资源与分配历史。

use crate::{
    Result,
    binary::*,
    container::{BACKUP_START, Container, DATA_START},
};
use hmi_core::error;
use std::collections::BTreeSet;

fn field(name: &str, capacity: usize) -> Result<Vec<u8>> {
    if name.is_empty() || name.len() > capacity || name.contains('\0') {
        return Err(error(
            "field_name",
            format!("名称 {name:?} 无效，字段容量为 {capacity} 字节且不允许 NUL"),
        ));
    }
    let mut bytes = vec![0; capacity];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    Ok(bytes)
}

fn record(output: &mut Vec<u8>, payload: &[u8]) -> Result<()> {
    let length = u32::try_from(payload.len())
        .map_err(|_| error("record_length", "记录长度超过 u32 范围"))?;
    output.extend(length.to_le_bytes());
    output.extend(payload);
    Ok(())
}

/// 属性和事件均按传入顺序写入；空代码行仍是一个有效记录。
pub fn object(
    attributes: &[(String, Vec<u8>)],
    events: &[(String, Vec<Vec<u8>>)],
) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    record(&mut output, format!("att-{}", attributes.len()).as_bytes())?;
    let mut names = BTreeSet::new();
    for (name, value) in attributes {
        if !names.insert(name) {
            return Err(error("attribute_name", format!("属性 {name} 重复")));
        }
        let mut payload = field(name, 16)?;
        payload.extend(value);
        record(&mut output, &payload)?;
    }
    let mut slots = BTreeSet::new();
    for (event, lines) in events {
        if event.is_empty()
            || !event.bytes().all(|b| b.is_ascii_alphabetic())
            || !slots.insert(event)
        {
            return Err(error("event_slot", format!("事件槽 {event:?} 无效或重复")));
        }
        record(
            &mut output,
            format!("codes{event}-{}", lines.len()).as_bytes(),
        )?;
        for line in lines {
            record(&mut output, line)?;
        }
    }
    record(&mut output, b"")?;
    Ok(output)
}

/// 页面索引偏移以页头之后的 +56 为基址，不以索引末尾为基址。
pub fn page(name: &str, objects: &[Vec<u8>]) -> Result<Vec<u8>> {
    page_preserved(name, objects, None, &[])
}

/// 页头未知字节和各记录索引的第三字随原始记录保留，偏移、长度、名称与CRC重新生成。
pub fn page_preserved(
    name: &str,
    objects: &[Vec<u8>],
    original_header: Option<&[u8]>,
    index_unknown: &[u32],
) -> Result<Vec<u8>> {
    if objects.is_empty() || objects.len() > 251 {
        return Err(error(
            "object_count",
            "页面须包含页面自身对象，且最多包含 250 个普通控件",
        ));
    }
    let mut output = vec![0; 56 + objects.len() * 12];
    put_u32(&mut output, 8, 56)?;
    put_u32(&mut output, 12, objects.len())?;
    output[21..23].copy_from_slice(&[0x55, 0x21]);
    output[24..40].copy_from_slice(&field(name, 16)?);
    // 当前 ver21234 配置的已观察固定字段，与对象记录分别保存。
    output[40..44].copy_from_slice(&[0x01, 0x44, 0x02, 0x18]);
    if let Some(header) = original_header {
        if header.len() != 56 || header[21..23] != [0x55, 0x21] {
            return Err(error("original_page", "原始页头格式无效"));
        }
        output[..56].copy_from_slice(header);
        put_u32(&mut output, 8, 56)?;
        put_u32(&mut output, 12, objects.len())?;
        output[24..40].copy_from_slice(&field(name, 16)?);
    }
    for (i, object) in objects.iter().enumerate() {
        let offset = output.len() - 56;
        put_u32(&mut output, 56 + i * 12, offset)?;
        put_u32(&mut output, 60 + i * 12, object.len())?;
        if let Some(value) = index_unknown.get(i) {
            output[64 + i * 12..68 + i * 12].copy_from_slice(&value.to_le_bytes());
        }
        output.extend(object);
    }
    let length = output.len();
    put_u32(&mut output, 4, length)?;
    let crc = page_crc(&output)?;
    output[..4].copy_from_slice(&crc.to_le_bytes());
    Ok(output)
}

/// 主引用表决定各类逻辑 ID；Program.s 和源副本不自动加入此表。
pub fn main_resource(
    model: &str,
    tag: u8,
    orientation: u16,
    references: &[(&str, &str)],
) -> Result<Vec<u8>> {
    let direction = match orientation {
        0 => 0,
        90 => 1,
        180 => 2,
        270 => 3,
        _ => return Err(error("orientation", "工程方向必须为 0、90、180 或 270")),
    };
    let length = references
        .len()
        .checked_mul(16)
        .and_then(|n| n.checked_add(96))
        .ok_or_else(|| error("reference_count", "主引用表长度溢出"))?;
    let mut output = vec![0; length];
    put_u32(&mut output, 4, 96)?;
    output[8..16].copy_from_slice(&[0x01, 0x44, 0x21, tag, direction, 0x18, 0x55, 0]);
    output[16..20].copy_from_slice(&crc_bytes(&[model.as_bytes()]).to_le_bytes());
    put_u32(&mut output, 24, 96)?;
    put_u32(&mut output, 28, references.len())?;
    put_u32(&mut output, 36, 2)?;
    output[65] = 3; // 固定观测值，不是当前工程方向。
    for (i, (kind, name)) in references.iter().enumerate() {
        let offset = 96 + i * 16;
        output[offset..offset + 8].copy_from_slice(&field(kind, 8)?);
        output[offset + 8..offset + 16].copy_from_slice(&field(name, 8)?);
    }
    let crc = main_crc(&output)?;
    output[..4].copy_from_slice(&crc.to_le_bytes());
    Ok(output)
}

/// 连续写入活动资源，生成主备目录和固定保留区，再回读验证容器。
pub fn container(resources: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let end = resources
        .len()
        .checked_mul(28)
        .and_then(|n| n.checked_add(4))
        .ok_or_else(|| error("resource_count", "资源目录长度溢出"))?;
    if end + 4 > BACKUP_START {
        return Err(error("resource_count", "资源数量超过目录容量"));
    }
    let mut names = BTreeSet::new();
    for (name, _) in resources {
        field(name, 16)?;
        if !names.insert(name) {
            return Err(error("resource_name", format!("资源名 {name} 重复")));
        }
    }
    let mut output = vec![0; DATA_START];
    output[0x380000..0x380004].fill(0xff);
    output[DATA_START - 8..].copy_from_slice(b"ver21234");
    for base in [0, BACKUP_START] {
        put_u32(&mut output, base, resources.len())?;
    }
    for (i, (name, payload)) in resources.iter().enumerate() {
        let offset = output.len();
        let new_end = offset
            .checked_add(payload.len())
            .filter(|n| *n <= u32::MAX as usize)
            .ok_or_else(|| error("resource_length", "资源总长度超过 HMI 的 u32 范围"))?;
        for base in [0, BACKUP_START] {
            let entry = base + 4 + i * 28;
            output[entry..entry + 16].copy_from_slice(&field(name, 16)?);
            put_u32(&mut output, entry + 16, offset)?;
            put_u32(&mut output, entry + 20, payload.len())?;
        }
        output.extend(payload);
        debug_assert_eq!(output.len(), new_end);
    }
    for base in [0, BACKUP_START] {
        let crc = directory_crc(&output[base..base + end])?;
        output[base + end..base + end + 4].copy_from_slice(&crc.to_le_bytes());
    }
    Container::parse(output.clone())?.strict()?;
    Ok(output)
}

/// 活动目录项的未知字段与资源绑定，不保留原目录位置或删除标志。
pub fn container_preserved(
    resources: &[(String, Vec<u8>)],
    unknown: &std::collections::BTreeMap<String, [u8; 3]>,
) -> Result<Vec<u8>> {
    let mut bytes = container(resources)?;
    for base in [0, BACKUP_START] {
        for (index, (name, _)) in resources.iter().enumerate() {
            if let Some(value) = unknown.get(name) {
                bytes[base + 4 + index * 28 + 25..base + 4 + index * 28 + 28]
                    .copy_from_slice(value);
            }
        }
        let end = base + 4 + resources.len() * 28;
        let crc = directory_crc(&bytes[base..end])?;
        bytes[end..end + 4].copy_from_slice(&crc.to_le_bytes());
    }
    Container::parse(bytes.clone())?.strict()?;
    Ok(bytes)
}

pub fn main_preserved(mut generated: Vec<u8>, original: &[u8]) -> Result<Vec<u8>> {
    if original.len() < 96 || usize32(original, 24)? != 96 {
        return Err(error("original_main", "原始主工程头格式不受支持"));
    }
    let known = generated[..96].to_vec();
    generated[..96].copy_from_slice(&original[..96]);
    for (start, end) in [(4, 8), (11, 14), (16, 20), (24, 32)] {
        generated[start..end].copy_from_slice(&known[start..end]);
    }
    let crc = main_crc(&generated)?;
    generated[..4].copy_from_slice(&crc.to_le_bytes());
    Ok(generated)
}
