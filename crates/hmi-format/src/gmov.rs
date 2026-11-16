//! GMOV 运行帧与原 GIF 编辑源。偏移相对于 76 字节头之后，源副本不做对象反序列化。
use crate::{
    binary::{put_u32, u32_at},
    picture::{alpha7, jpeg},
};
use hmi_core::{Result, error};
pub const HEADER: usize = 76;
pub const FRAME: usize = 40;
pub const MAX_FRAMES: usize = 1000;
pub const MAX_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug)]
pub struct GifInfo {
    pub width: u16,
    pub height: u16,
    pub delays: Vec<u32>,
}

/// 原生时序：丢弃零延时；先补足前一短帧至 30ms，单帧最长 65000ms。
pub fn normalize_timing(delays: &[u32]) -> Vec<(u32, u16)> {
    let mut kept: Vec<(u32, u16)> = Vec::new();
    for (i, delay) in delays.iter().enumerate() {
        let mut delay = (*delay).min(65000);
        if delay == 0 {
            continue;
        }
        if let Some(last) = kept.last_mut()
            && last.1 < 30
        {
            let combined = u32::from(last.1) + delay;
            if combined <= 30 {
                last.1 = combined as u16;
                continue;
            }
            last.1 = 30;
            delay = combined - 30;
        }
        kept.push((i as u32, delay as u16));
    }
    kept
}
fn u16_at(bytes: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(|| error("gmov_layout", "动画字段超出范围"))?
            .try_into()
            .unwrap(),
    ))
}

/// 只扫描 GIF 容器、调色板和子块；LZW 解码由媒体编码器的标准库负责。
pub fn inspect_gif(bytes: &[u8]) -> Result<GifInfo> {
    let invalid = || error("gif_layout", "GIF 头部、帧范围或数据子块无效");
    if bytes.len() < 14 || !matches!(&bytes[..6], b"GIF87a" | b"GIF89a") {
        return Err(invalid());
    }
    let width = u16_at(bytes, 6)?;
    let height = u16_at(bytes, 8)?;
    if width == 0 || height == 0 {
        return Err(invalid());
    }
    let palette = |flags: u8| {
        if flags & 128 != 0 {
            3 * (1usize << ((flags & 7) + 1))
        } else {
            0
        }
    };
    let mut p = 13 + palette(bytes[10]);
    let mut delay = 0;
    let mut delays = Vec::new();
    let subblocks = |p: &mut usize| -> Result<()> {
        loop {
            let len = usize::from(*bytes.get(*p).ok_or_else(invalid)?);
            *p += 1;
            if len == 0 {
                return Ok(());
            }
            *p = p
                .checked_add(len)
                .filter(|n| *n <= bytes.len())
                .ok_or_else(invalid)?;
        }
    };
    loop {
        match bytes.get(p).copied().ok_or_else(invalid)? {
            0x3b => {
                if p + 1 != bytes.len() || delays.is_empty() {
                    return Err(invalid());
                }
                break;
            }
            0x21 => {
                let label = *bytes.get(p + 1).ok_or_else(invalid)?;
                p += 2;
                if label == 0xf9 {
                    let block = bytes.get(p..p + 6).ok_or_else(invalid)?;
                    if block[0] != 4 || block[5] != 0 {
                        return Err(invalid());
                    }
                    delay = u32::from(u16_at(block, 2)?) * 10;
                    p += 6;
                } else if label == 1 {
                    return Err(error("gif_feature", "当前不支持包含纯文本扩展的 GIF"));
                } else {
                    subblocks(&mut p)?;
                }
            }
            0x2c => {
                let descriptor = bytes.get(p + 1..p + 10).ok_or_else(invalid)?;
                let x = u16_at(descriptor, 0)?;
                let y = u16_at(descriptor, 2)?;
                let w = u16_at(descriptor, 4)?;
                let h = u16_at(descriptor, 6)?;
                if w == 0
                    || h == 0
                    || u32::from(x) + u32::from(w) > u32::from(width)
                    || u32::from(y) + u32::from(h) > u32::from(height)
                {
                    return Err(invalid());
                }
                p += 10 + palette(descriptor[8]);
                if !bytes.get(p).is_some_and(|n| (2..=8).contains(n)) {
                    return Err(invalid());
                }
                p += 1;
                subblocks(&mut p)?;
                delays.push(delay);
                delay = 0;
                if delays.len() > MAX_FRAMES {
                    return Err(error("animation_frames", "当前 GIF 最多支持 1000 帧"));
                }
            }
            _ => return Err(invalid()),
        }
    }
    Ok(GifInfo {
        width,
        height,
        delays,
    })
}

pub fn storage_dimensions(width: u16, height: u16, orientation: u16) -> Result<(u16, u16)> {
    let pad = |v: u16| {
        u16::try_from((u32::from(v) + 15) & !15)
            .ok()
            .filter(|v| *v > 0)
            .ok_or_else(|| error("animation_dimensions", "动画补齐尺寸无效或超过 65535"))
    };
    let (w, h) = (pad(width)?, pad(height)?);
    match orientation {
        0 | 180 => Ok((w, h)),
        90 | 270 => Ok((h, w)),
        _ => Err(error("animation_orientation", "动画方向无效")),
    }
}

pub struct EncodedFrame {
    pub data: Vec<u8>,
    pub delay_ms: u16,
    pub alpha_offset: u32,
}
#[derive(Debug)]
pub struct Frame<'a> {
    pub data: &'a [u8],
    pub delay_ms: u16,
    pub alpha_offset: u32,
}
#[derive(Debug)]
pub struct Runtime<'a> {
    pub width: u16,
    pub height: u16,
    pub orientation: u16,
    pub quality: u8,
    pub alpha_enabled: bool,
    pub duration_ms: u32,
    pub frames: Vec<Frame<'a>>,
}

/// 运行资源使用类型 4 的 JPEG 分支，保留 20 字节前缀和独立透明尾流。
pub fn write_runtime(
    width: u16,
    height: u16,
    orientation: u16,
    quality: u8,
    alpha_enabled: bool,
    frames: &[EncodedFrame],
) -> Result<Vec<u8>> {
    if frames.is_empty() || frames.len() > MAX_FRAMES {
        return Err(error("animation_frames", "动画须包含 1～1000 帧"));
    }
    storage_dimensions(width, height, orientation)?;
    let table = frames.len() * FRAME;
    let payload = frames.iter().try_fold(table, |n, f| {
        n.checked_add(f.data.len())
            .filter(|n| *n <= MAX_BYTES - HEADER - 4096)
            .ok_or_else(|| error("animation_length", "动画运行资源超过 128 MiB"))
    })?;
    let mut out = vec![0; HEADER + table];
    out[..4].copy_from_slice(b"GMOV");
    out[4..8].copy_from_slice(&[10 + (orientation / 90) as u8, quality, 10, 0]);
    put_u32(&mut out, 8, HEADER)?;
    put_u32(&mut out, 12, frames.len())?;
    out[16..18].copy_from_slice(&width.to_le_bytes());
    out[18..20].copy_from_slice(&height.to_le_bytes());
    put_u32(&mut out, 24, payload + 4096)?;
    put_u32(&mut out, 40, payload)?;
    put_u32(&mut out, 44, 4096)?;
    out[48] = u8::from(frames.iter().any(|f| f.alpha_offset != 0));
    out[49] = u8::from(alpha_enabled);
    let mut time = 0usize;
    for (i, f) in frames.iter().enumerate() {
        let row = HEADER + i * FRAME;
        let direction = out[4];
        out[row..row + 4].copy_from_slice(&[direction, 0, 0, 4]);
        let offset = out.len() - HEADER;
        put_u32(&mut out, row + 8, offset)?;
        out[row + 12..row + 16]
            .copy_from_slice(&[width.to_le_bytes(), height.to_le_bytes()].concat());
        put_u32(&mut out, row + 16, f.data.len())?;
        put_u32(&mut out, row + 20, f.alpha_offset as usize)?;
        out[row + 24..row + 26].copy_from_slice(&f.delay_ms.to_le_bytes());
        put_u32(&mut out, row + 28, time)?;
        time += usize::from(f.delay_ms);
        out.extend(&f.data);
    }
    put_u32(&mut out, 36, time)?;
    out.extend(b"hmic");
    out.extend(4088u32.to_le_bytes());
    out.resize(out.len() + 4088, 0);
    inspect_runtime(&out)?;
    Ok(out)
}

pub fn inspect_runtime(bytes: &[u8]) -> Result<Runtime<'_>> {
    let invalid = || error("gmov_layout", "GMOV 头部、帧表、偏移或扩展信息无效");
    if bytes.len() < HEADER
        || bytes.len() > MAX_BYTES
        || &bytes[..4] != b"GMOV"
        || !(10..=13).contains(&bytes[4])
        || !(1..=100).contains(&bytes[5])
        || bytes[6..8] != [10, 0]
        || u32_at(bytes, 8)? != 76
        || bytes[20..24] != [0; 4]
        || u32_at(bytes, 24)? as usize != bytes.len() - HEADER
        || bytes[28..36] != [0; 8]
        || bytes[48] > 1
        || bytes[49] > 1
        || bytes[50..76] != [0; 26]
    {
        return Err(invalid());
    }
    let count = u32_at(bytes, 12)? as usize;
    if count == 0 || count > MAX_FRAMES {
        return Err(invalid());
    }
    let width = u16_at(bytes, 16)?;
    let height = u16_at(bytes, 18)?;
    let orientation = u16::from(bytes[4] - 10) * 90;
    let (sw, sh) = storage_dimensions(width, height, orientation)?;
    let ext = u32_at(bytes, 40)? as usize;
    if u32_at(bytes, 44)? != 4096 || ext.checked_add(HEADER + 4096) != Some(bytes.len()) {
        return Err(invalid());
    }
    let footer = bytes.get(HEADER + ext..).ok_or_else(invalid)?;
    if footer[..8] != [b'h', b'm', b'i', b'c', 248, 15, 0, 0] || footer[8..].iter().any(|b| *b != 0)
    {
        return Err(invalid());
    }
    let mut cursor = count * FRAME;
    let mut time = 0u32;
    let mut any_alpha = false;
    let mut frames = Vec::new();
    for i in 0..count {
        let row = bytes
            .get(HEADER + i * FRAME..HEADER + (i + 1) * FRAME)
            .ok_or_else(invalid)?;
        if row[..4] != [bytes[4], 0, 0, 4]
            || row[4..8] != [0; 4]
            || u32_at(row, 8)? as usize != cursor
            || u16_at(row, 12)? != width
            || u16_at(row, 14)? != height
            || row[26..28] != [0; 2]
            || u32_at(row, 28)? != time
            || row[32..40] != [0; 8]
        {
            return Err(invalid());
        }
        let size = u32_at(row, 16)? as usize;
        let end = cursor
            .checked_add(size)
            .filter(|v| *v <= ext)
            .ok_or_else(invalid)?;
        let data = bytes
            .get(HEADER + cursor..HEADER + end)
            .ok_or_else(invalid)?;
        cursor = end;
        if data.len() < 20
            || data[..4] != [0; 4]
            || u32_at(data, 4)? != u32::from(sw)
            || u32_at(data, 8)? != u32::from(sh)
            || data[12..20] != [0; 8]
        {
            return Err(invalid());
        }
        let alpha_offset = u32_at(row, 20)?;
        let split = if alpha_offset == 0 {
            data.len()
        } else {
            alpha_offset as usize
        };
        let jpg = data.get(20..split).ok_or_else(invalid)?;
        if jpeg::dimensions(jpg)? != (sw, sh) {
            return Err(invalid());
        }
        if alpha_offset != 0 {
            alpha7::validate(&data[split..], usize::from(sw) * usize::from(sh))?;
            any_alpha = true;
        }
        let delay_ms = u16_at(row, 24)?;
        if delay_ms == 0 || delay_ms > 65000 {
            return Err(invalid());
        }
        time = time.checked_add(u32::from(delay_ms)).ok_or_else(invalid)?;
        frames.push(Frame {
            data,
            delay_ms,
            alpha_offset,
        });
    }
    if cursor != ext
        || time != u32_at(bytes, 36)?
        || u8::from(any_alpha) != bytes[48]
        || (any_alpha && bytes[49] == 0)
    {
        return Err(invalid());
    }
    Ok(Runtime {
        width,
        height,
        orientation,
        quality: bytes[5],
        alpha_enabled: bytes[49] == 1,
        duration_ms: time,
        frames,
    })
}

pub fn write_source(gif: &[u8]) -> Result<Vec<u8>> {
    let info = inspect_gif(gif)?;
    if gif.len() > 64 * 1024 * 1024 {
        return Err(error("animation_length", "GIF 源文件超过 64 MiB"));
    }
    let mut out = vec![0; HEADER];
    // 模式 1 包装原 GIF；100／10 为编辑源固定字段，不是运行帧质量或 GIF 帧率。
    out[..8].copy_from_slice(b"GMOV\x0a\x64\x0a\x01");
    put_u32(&mut out, 8, 76)?;
    put_u32(&mut out, 12, info.delays.len())?;
    out[16..18].copy_from_slice(&info.width.to_le_bytes());
    out[18..20].copy_from_slice(&info.height.to_le_bytes());
    put_u32(&mut out, 24, gif.len())?;
    out[49] = 1;
    out.extend(gif);
    Ok(out)
}

pub fn inspect_source(bytes: &[u8]) -> Result<GifInfo> {
    let gif = bytes
        .get(HEADER..)
        .ok_or_else(|| error("gmov_source", "动画编辑源头部不完整"))?;
    let info = inspect_gif(gif)?;
    if bytes.len() > 64 * 1024 * 1024 + HEADER
        || bytes[..8] != *b"GMOV\x0a\x64\x0a\x01"
        || u32_at(bytes, 8)? != 76
        || u32_at(bytes, 12)? as usize != info.delays.len()
        || u16_at(bytes, 16)? != info.width
        || u16_at(bytes, 18)? != info.height
        || bytes[20..24] != [0; 4]
        || u32_at(bytes, 24)? as usize != gif.len()
        || bytes[28..49] != [0; 21]
        || bytes[49] != 1
        || bytes[50..76] != [0; 26]
    {
        return Err(error("gmov_source", "动画编辑源与 GIF 不一致"));
    }
    Ok(info)
}
