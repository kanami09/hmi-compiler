use crate::binary::*;
use crate::diagnostic::{Diagnostic, Location, has_errors};
use crate::{Error, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

pub const DATA_START: usize = 0x700000;
pub const BACKUP_START: usize = 0x80000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub index: usize,
    pub name: String,
    pub offset: usize,
    pub length: usize,
    pub deleted: bool,
    pub flags: u8,
    pub unknown: [u8; 3],
}

#[derive(Clone, Debug, Serialize)]
pub struct DirectoryStatus {
    pub count: usize,
    pub stored_crc: u32,
    pub computed_crc: u32,
    pub crc_valid: bool,
}

#[derive(Debug)]
pub struct Container {
    pub bytes: Vec<u8>,
    pub entries: Vec<Entry>,
    pub primary: Option<DirectoryStatus>,
    pub backup: Option<DirectoryStatus>,
    pub contiguous: bool,
    pub diagnostics: Vec<Diagnostic>,
}

fn directory(bytes: &[u8], base: usize) -> Result<(Vec<Entry>, DirectoryStatus)> {
    let count = usize32(bytes, base)?;
    let table_end = checked_table(bytes, base + 4, count, 28)?;
    if table_end + 4 > base + BACKUP_START {
        return Err(Error("目录超出其保留空间".into()));
    }
    let stored_crc = u32_at(bytes, table_end)?;
    let computed_crc = directory_crc(&bytes[base..table_end])?;
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let offset = base + 4 + i * 28;
        let flags = bytes[offset + 24];
        let raw_name = &bytes[offset..offset + 16];
        let resource_name = if flags == 0 {
            name(raw_name)?
        } else {
            String::from_utf8_lossy(raw_name.split(|b| *b == 0).next().unwrap_or_default())
                .into_owned()
        };
        entries.push(Entry {
            index: i,
            name: resource_name,
            offset: usize32(bytes, offset + 16)?,
            length: usize32(bytes, offset + 20)?,
            deleted: flags != 0,
            flags,
            unknown: bytes[offset + 25..offset + 28].try_into().unwrap(),
        });
    }
    Ok((
        entries,
        DirectoryStatus {
            count,
            stored_crc,
            computed_crc,
            crc_valid: stored_crc == computed_crc,
        },
    ))
}

impl Container {
    pub fn parse(bytes: Vec<u8>) -> Result<Self> {
        if slice(&bytes, DATA_START - 8, 8)? != b"ver21234" {
            return Err(Error("不支持的容器版本：需要 ver21234".into()));
        }
        let primary = directory(&bytes, 0);
        let backup = directory(&bytes, BACKUP_START);
        let mut diagnostics = Vec::new();
        for (label, base, result) in [("主目录", 0, &primary), ("备份目录", BACKUP_START, &backup)]
        {
            let location = Location {
                offset: Some(base),
                ..Location::default()
            };
            match result {
                Ok((_, status)) if !status.crc_valid => diagnostics.push(
                    Diagnostic::error(
                        "directory_crc",
                        format!(
                            "{label} CRC 错误：存储 0x{:08X}，计算 0x{:08X}",
                            status.stored_crc, status.computed_crc
                        ),
                    )
                    .at(location),
                ),
                Err(error) => diagnostics.push(
                    Diagnostic::error("directory_structure", format!("{label}：{error}"))
                        .at(location),
                ),
                _ => {}
            }
        }
        if let (Ok((a, _)), Ok((b, _))) = (&primary, &backup) {
            let count = a.len();
            if a != b
                || bytes[..4 + count * 28 + 4]
                    != bytes[BACKUP_START..BACKUP_START + 4 + count * 28 + 4]
            {
                diagnostics.push(Diagnostic::error("directory_mirror", "主备目录内容不一致"));
            }
        }
        let entries = match (&primary, &backup) {
            (Ok((entries, status)), _) if status.crc_valid => entries.clone(),
            (_, Ok((entries, status))) if status.crc_valid => entries.clone(),
            (Ok((entries, _)), _) | (_, Ok((entries, _))) => entries.clone(),
            _ => {
                return Err(Error(format!(
                    "主备目录均无法解析：{}；{}",
                    primary.unwrap_err(),
                    backup.unwrap_err()
                )));
            }
        };
        let mut names = HashSet::new();
        for entry in &entries {
            if entry.flags > 1 {
                diagnostics.push(Diagnostic::error(
                    "entry_flags",
                    format!("资源 #{} 删除标志未知：{}", entry.index, entry.flags),
                ));
            }
            if entry.offset < DATA_START || slice(&bytes, entry.offset, entry.length).is_err() {
                diagnostics.push(Diagnostic::error(
                    "resource_bounds",
                    format!("资源 #{} {} 范围越界", entry.index, entry.name),
                ));
            }
            if !entry.deleted && (entry.name.is_empty() || !names.insert(entry.name.clone())) {
                diagnostics.push(Diagnostic::error(
                    "resource_name",
                    format!("活动资源名称为空或重复：{}", entry.name),
                ));
            }
        }
        let mut physical: Vec<_> = entries.iter().collect();
        physical.sort_by_key(|e| (e.offset, e.index));
        let mut cursor = DATA_START;
        let mut contiguous = true;
        for entry in physical {
            if cursor != entry.offset {
                contiguous = false;
            }
            cursor = entry.offset.saturating_add(entry.length);
        }
        contiguous &= cursor == bytes.len();
        if !contiguous {
            diagnostics.push(Diagnostic::warning(
                "resource_layout",
                "资源不是连续完整布局，首版仅支持诊断，不支持导出回编",
            ));
        }
        Ok(Self {
            bytes,
            entries,
            primary: primary.ok().map(|(_, s)| s),
            backup: backup.ok().map(|(_, s)| s),
            contiguous,
            diagnostics,
        })
    }

    pub fn strict(&self) -> Result<()> {
        if has_errors(&self.diagnostics) {
            return Err(Error("容器校验失败，请使用 inspect 查看诊断".into()));
        }
        if !self.contiguous {
            return Err(Error("首版不支持非连续资源布局".into()));
        }
        Ok(())
    }

    pub fn active(&self, resource: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| !e.deleted && e.name == resource)
    }

    pub fn data(&self, entry: &Entry) -> Result<&[u8]> {
        slice(&self.bytes, entry.offset, entry.length)
    }

    pub fn rebuild(&self, replacements: &BTreeMap<usize, Vec<u8>>) -> Result<Vec<u8>> {
        self.strict()?;
        if replacements.iter().all(|(i, data)| {
            self.entries.get(*i).and_then(|e| self.data(e).ok()) == Some(data.as_slice())
        }) {
            return Ok(self.bytes.clone());
        }
        if replacements
            .keys()
            .any(|i| self.entries.get(*i).is_none_or(|e| e.deleted))
        {
            return Err(Error("只能替换已有活动资源".into()));
        }
        let mut result = self.bytes[..DATA_START].to_vec();
        let mut physical: Vec<_> = self.entries.iter().collect();
        physical.sort_by_key(|e| (e.offset, e.index));
        for entry in physical {
            let payload = replacements
                .get(&entry.index)
                .map(Vec::as_slice)
                .unwrap_or(self.data(entry)?);
            let offset = result.len();
            result.extend_from_slice(payload);
            for base in [0, BACKUP_START] {
                put_u32(&mut result, base + 4 + entry.index * 28 + 16, offset)?;
                put_u32(&mut result, base + 4 + entry.index * 28 + 20, payload.len())?;
            }
        }
        for base in [0, BACKUP_START] {
            let end = base + 4 + self.entries.len() * 28;
            let crc = directory_crc(&result[base..end])?;
            result[end..end + 4].copy_from_slice(&crc.to_le_bytes());
        }
        Ok(result)
    }
}
