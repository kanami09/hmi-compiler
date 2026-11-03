//! 所有来自文件的长度与偏移在分配或切片前检查。
use crate::{Error, Result};

pub fn slice(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error(format!("偏移溢出：0x{offset:X} + {length}")))?;
    bytes.get(offset..end).ok_or_else(|| {
        Error(format!(
            "文件越界：0x{offset:X}..0x{end:X}，可用 {} 字节",
            bytes.len()
        ))
    })
}

pub fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        slice(bytes, offset, 4)?.try_into().unwrap(),
    ))
}

pub fn usize32(bytes: &[u8], offset: usize) -> Result<usize> {
    Ok(u32_at(bytes, offset)? as usize)
}

pub fn checked_table(bytes: &[u8], offset: usize, count: usize, width: usize) -> Result<usize> {
    let length = count
        .checked_mul(width)
        .ok_or_else(|| Error("表格长度溢出".into()))?;
    slice(bytes, offset, length)?;
    Ok(offset + length)
}

pub fn put_u32(bytes: &mut [u8], offset: usize, value: usize) -> Result<()> {
    let value = u32::try_from(value).map_err(|_| Error("输出超过 u32 格式限制".into()))?;
    let end = offset
        .checked_add(4)
        .ok_or_else(|| Error("写入偏移溢出".into()))?;
    bytes
        .get_mut(offset..end)
        .ok_or_else(|| Error("写入越界".into()))?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

pub fn name(bytes: &[u8]) -> Result<String> {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8(bytes[..end].to_vec()).map_err(|_| Error("名称不是有效 UTF-8".into()))
}

const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut value = (i as u32) << 24;
        let mut bit = 0;
        while bit < 8 {
            value = (value << 1)
                ^ if value & 0x8000_0000 != 0 {
                    0x04C1_1DB7
                } else {
                    0
                };
            bit += 1;
        }
        table[i] = value;
        i += 1;
    }
    table
}
const CRC_TABLE: [u32; 256] = crc_table();

fn word(mut crc: u32, value: u32) -> u32 {
    crc ^= value;
    for _ in 0..4 {
        crc = (crc << 8) ^ CRC_TABLE[(crc >> 24) as usize];
    }
    crc
}

/// 每个输入字节都执行 32 次移位，而不是标准 CRC32 的 8 次。
pub fn crc_bytes(parts: &[&[u8]]) -> u32 {
    let mut crc = u32::MAX;
    for part in parts {
        for byte in *part {
            crc = word(crc, u32::from(*byte));
        }
    }
    crc
}

pub fn directory_crc(bytes: &[u8]) -> Result<u32> {
    if !bytes.len().is_multiple_of(4) {
        return Err(Error("目录 CRC 输入必须为 4 字节倍数".into()));
    }
    let mut crc = u32::MAX;
    for chunk in bytes.as_chunks::<4>().0 {
        crc = word(crc, u32::from_le_bytes(*chunk));
    }
    Ok(word(crc, u32::from_le_bytes(*b"ADEC")))
}

pub fn page_crc(bytes: &[u8]) -> Result<u32> {
    slice(bytes, 0, 56)?;
    Ok(crc_bytes(&[
        &bytes[4..],
        &bytes[4..8],
        &bytes[12..16],
        &bytes[20..22],
    ]))
}

pub fn main_crc(bytes: &[u8]) -> Result<u32> {
    slice(bytes, 0, 32)?;
    Ok(crc_bytes(&[
        &bytes[4..],
        &bytes[16..20],
        &bytes[4..8],
        &bytes[10..11],
        &bytes[14..15],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference(parts: &[&[u8]], unit: usize) -> u32 {
        let mut crc = u32::MAX;
        for part in parts {
            for chunk in part.chunks_exact(unit) {
                let mut value = 0;
                for (i, byte) in chunk.iter().enumerate() {
                    value |= u32::from(*byte) << (i * 8);
                }
                crc ^= value;
                for _ in 0..32 {
                    crc = (crc << 1)
                        ^ if crc & 0x8000_0000 != 0 {
                            0x04C1_1DB7
                        } else {
                            0
                        };
                }
            }
        }
        crc
    }
    #[test]
    fn crc_matches_bitwise_algorithm() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(crc_bytes(&[&bytes]), reference(&[&bytes], 1));
        assert_eq!(
            directory_crc(&bytes).unwrap(),
            reference(&[&bytes, b"ADEC"], 4)
        );
    }
    #[test]
    fn bounds_are_checked() {
        assert!(slice(&[], usize::MAX, 4).is_err());
        assert!(checked_table(&[0; 8], 0, usize::MAX, 28).is_err());
        assert!(u32_at(&[0; 3], 0).is_err());
    }
}
