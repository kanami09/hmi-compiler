//! RIFF/WAVE 输入与 HMI PCM16 包装。hmic 的 CRC 范围兼容原生实现，不替换成标准 CRC32。
use crate::binary::u32_at;
use hmi_core::{Result, error};
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub type Metadata = Vec<([u8; 4], Vec<u8>)>;
#[derive(Debug)]
pub struct Wave<'a> {
    pub format: u16,
    pub original_format: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits: u16,
    pub samples: u32,
    pub data: &'a [u8],
    pub metadata: Metadata,
    pub data_header: usize,
}
fn u16_at(b: &[u8], o: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        b.get(o..o + 2)
            .ok_or_else(|| error("wave_layout", "WAV 字段超出范围"))?
            .try_into()
            .unwrap(),
    ))
}

pub fn inspect(bytes: &[u8]) -> Result<Wave<'_>> {
    let invalid = || error("wave_layout", "WAV 头部、块边界或采样数据无效");
    if bytes.len() < 12
        || bytes.len() > MAX_BYTES + 4096
        || &bytes[..4] != b"RIFF"
        || &bytes[8..12] != b"WAVE"
        || u32_at(bytes, 4)? as usize + 8 != bytes.len()
    {
        return Err(invalid());
    }
    let mut pos = 12;
    let mut fmt = None;
    let mut data = None;
    let mut metadata = Vec::new();
    let mut hmic = false;
    while pos < bytes.len() {
        let head = bytes.get(pos..pos + 8).ok_or_else(invalid)?;
        let size = u32_at(head, 4)? as usize;
        let end = pos
            .checked_add(8)
            .and_then(|n| n.checked_add(size))
            .filter(|n| *n <= bytes.len())
            .ok_or_else(invalid)?;
        let block = &bytes[pos + 8..end];
        match &head[..4] {
            b"fmt " => {
                if fmt.replace(block).is_some() {
                    return Err(invalid());
                }
            }
            b"data" => {
                if data.replace((pos, block)).is_some() {
                    return Err(invalid());
                }
            }
            b"hmic" => {
                if hmic || size > 4096 {
                    return Err(invalid());
                }
                hmic = true;
                let mut p = 0;
                let mut ended = false;
                while p + 8 <= block.len() {
                    let key: [u8; 4] = block[p..p + 4].try_into().unwrap();
                    let n = u32_at(block, p + 4)? as usize;
                    p += 8;
                    if key == [0; 4] {
                        if n != 0 || block[p..].iter().any(|b| *b != 0) {
                            return Err(invalid());
                        }
                        ended = true;
                        break;
                    }
                    if !key
                        .iter()
                        .all(|b| b.is_ascii() && (!b.is_ascii_control() || *b == 0))
                        || metadata.iter().any(|(k, _)| *k == key)
                    {
                        return Err(invalid());
                    }
                    let end = p
                        .checked_add(n)
                        .filter(|n| *n <= block.len())
                        .ok_or_else(invalid)?;
                    metadata.push((key, block[p..end].to_vec()));
                    p = end;
                }
                if !ended {
                    return Err(invalid());
                }
            }
            _ => {}
        }
        pos = end + size % 2;
        if pos > bytes.len() {
            return Err(invalid());
        }
    }
    let fmt = fmt.ok_or_else(invalid)?;
    if fmt.len() < 16 {
        return Err(invalid());
    }
    let original_format = u16_at(fmt, 0)?;
    let mut format = original_format;
    let channels = u16_at(fmt, 2)?;
    let sample_rate = u32_at(fmt, 4)?;
    let bits = u16_at(fmt, 14)?;
    if format == 0xfffe {
        if fmt.len() < 40
            || u16_at(fmt, 16)? < 22
            || fmt[28..40] != [0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]
        {
            return Err(error("wave_format", "不支持该 WAVEFORMATEXTENSIBLE 变体"));
        }
        let sub = u32_at(fmt, 24)?;
        if ![1, 3].contains(&sub) {
            return Err(error("wave_format", "不支持该 WAV 子格式"));
        }
        format = sub as u16;
    }
    if !matches!((format, bits), (1, 8 | 16 | 24 | 32) | (3, 32 | 64)) {
        return Err(error(
            "wave_format",
            "当前支持 PCM 或浮点 WAV，不支持该压缩格式",
        ));
    }
    let align = u32::from(channels) * u32::from(bits / 8);
    if channels == 0
        || channels > 32
        || sample_rate == 0
        || sample_rate > 384000
        || u32::from(u16_at(fmt, 12)?) != align
        || u64::from(u32_at(fmt, 8)?) != u64::from(sample_rate) * u64::from(align)
    {
        return Err(invalid());
    }
    let (data_header, data) = data.ok_or_else(invalid)?;
    if data.is_empty() || data.len() > MAX_BYTES || !data.len().is_multiple_of(align as usize) {
        return Err(invalid());
    }
    Ok(Wave {
        format,
        original_format,
        channels,
        sample_rate,
        bits,
        samples: (data.len() / align as usize) as u32,
        data,
        metadata,
        data_header,
    })
}

/// 原生为 little-endian u32 的逐字处理，不追加目录 CRC 的 ADEC 标记。
fn checksum(bytes: &[u8]) -> Result<u32> {
    if !bytes.len().is_multiple_of(4) {
        return Err(error("wave_crc", "WAV 校验长度必须按四字节对齐"));
    }
    let mut crc = u32::MAX;
    for word in bytes.as_chunks::<4>().0 {
        crc ^= u32::from_le_bytes(*word);
        for _ in 0..32 {
            crc = (crc << 1)
                ^ if crc & 0x8000_0000 != 0 {
                    0x04c1_1db7
                } else {
                    0
                };
        }
    }
    Ok(crc)
}

pub fn write(
    rate: u32,
    channels: u16,
    pcm: &[u8],
    metadata: &Metadata,
    name: Option<&str>,
) -> Result<Vec<u8>> {
    if ![1, 2].contains(&channels)
        || rate == 0
        || rate > 384000
        || pcm.is_empty()
        || !pcm.len().is_multiple_of(usize::from(channels) * 2)
        || pcm.len() > MAX_BYTES
    {
        return Err(error("wave_pcm", "PCM16 采样率、声道数或数据长度无效"));
    }
    let size = (pcm.len() + 3) & !3;
    if size > MAX_BYTES {
        return Err(error("wave_length", "WAV 采样数据超过 64 MiB"));
    }
    let mut data = b"data".to_vec();
    data.extend((size as u32).to_le_bytes());
    data.extend(pcm);
    data.resize(8 + size, 0);
    // 原生从 data 块头起覆盖 size 字节，因而不覆盖最后八个 PCM 字节。
    let crc = checksum(&data[..size])?;
    let mut entries = metadata.clone();
    if let Some((_, value)) = entries.iter_mut().find(|(k, _)| k == b"crc\0") {
        *value = crc.to_le_bytes().to_vec();
    } else {
        entries.push((*b"crc\0", crc.to_le_bytes().to_vec()));
    }
    if !entries.iter().any(|(k, _)| k == b"name")
        && let Some(name) = name
    {
        entries.push((*b"name", name.as_bytes().to_vec()));
    }
    let mut ext = b"hmic".to_vec();
    ext.extend(504u32.to_le_bytes());
    for (key, value) in entries {
        ext.extend(key);
        ext.extend((value.len() as u32).to_le_bytes());
        ext.extend(value);
    }
    ext.extend([0; 8]);
    if ext.len() > 512 {
        return Err(error("wave_metadata", "WAV 扩展信息超过 512 字节容量"));
    }
    ext.resize(512, 0);
    let mut out = b"RIFF".to_vec();
    out.extend(((548 + size) as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(channels.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * u32::from(channels) * 2).to_le_bytes());
    out.extend((channels * 2).to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(ext);
    out.extend(data);
    inspect_native(&out)?;
    Ok(out)
}

pub fn inspect_native(bytes: &[u8]) -> Result<Wave<'_>> {
    let w = inspect(bytes)?;
    if w.format != 1
        || w.bits != 16
        || ![1, 2].contains(&w.channels)
        || w.data_header != 548
        || !w.data.len().is_multiple_of(4)
        || bytes.get(36..44) != Some(b"hmic\xf8\x01\0\0")
    {
        return Err(error("wave_native", "WAV 不符合 HMI PCM16 包装"));
    }
    let crc = w
        .metadata
        .iter()
        .find(|(k, _)| k == b"crc\0")
        .ok_or_else(|| error("wave_crc", "WAV 缺少 crc 扩展项"))?;
    if crc.1.as_slice()
        != checksum(&bytes[w.data_header..w.data_header + w.data.len()])?.to_le_bytes()
    {
        return Err(error("wave_crc", "WAV 采样数据校验不匹配"));
    }
    Ok(w)
}
