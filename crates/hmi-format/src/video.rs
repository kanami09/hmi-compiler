//! 无声音轨的 VIDEO：76 字节头、40 字节帧表、直接 JPEG 帧与空 hmic 扩展区。
use crate::{
    binary::{put_u32, u32_at},
    picture::jpeg,
};
use hmi_core::{Result, error};
pub const MAX_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_FRAMES: usize = 10000;
pub const FPS: &[u8] = &[4, 6, 8, 10, 12, 14, 18, 24];
#[derive(Debug)]
pub struct Video<'a> {
    pub width: u16,
    pub height: u16,
    pub orientation: u16,
    pub quality: u8,
    pub fps: u8,
    pub duration_ms: u32,
    pub frames: Vec<&'a [u8]>,
    pub audio: Option<&'a [u8]>,
}

pub fn write(
    width: u16,
    height: u16,
    orientation: u16,
    quality: u8,
    fps: u8,
    frames: &[Vec<u8>],
) -> Result<Vec<u8>> {
    write_with_audio(width, height, orientation, quality, fps, frames, None)
}

pub fn write_with_audio(
    width: u16,
    height: u16,
    orientation: u16,
    quality: u8,
    fps: u8,
    frames: &[Vec<u8>],
    audio: Option<&[u8]>,
) -> Result<Vec<u8>> {
    if let Some(audio) = audio {
        crate::wave::inspect_native(audio)?;
    }
    if !matches!(orientation, 0 | 90 | 180 | 270) {
        return Err(error(
            "video_orientation",
            "视频方向必须为 0、90、180 或 270",
        ));
    }
    if frames.is_empty() || frames.len() > MAX_FRAMES {
        return Err(error("video_frames", "视频须包含 1～10000 个输出帧"));
    }
    let table = frames.len() * 40;
    let frame_end = frames.iter().try_fold(table, |n, f| {
        n.checked_add(f.len())
            .filter(|n| *n <= MAX_BYTES - 76 - 4096)
            .ok_or_else(|| error("video_length", "VIDEO 运行资源超过 128 MiB"))
    })?;
    let ext = frame_end
        .checked_add(audio.map_or(0, <[u8]>::len))
        .filter(|n| *n <= MAX_BYTES - 76 - 4096)
        .ok_or_else(|| error("video_length", "有声 VIDEO 超过 128 MiB"))?;
    let mut out = vec![0; 76 + table];
    out[..4].copy_from_slice(b"VIDE");
    out[4..8].copy_from_slice(&[10 + (orientation / 90) as u8, quality, fps, 0]);
    put_u32(&mut out, 8, 76)?;
    put_u32(&mut out, 12, frames.len())?;
    out[16..18].copy_from_slice(&width.to_le_bytes());
    out[18..20].copy_from_slice(&height.to_le_bytes());
    if fps == 0 {
        return Err(error("video_fps", "视频 fps 必须为正整数"));
    }
    put_u32(&mut out, 24, ext + 4096)?;
    if let Some(audio) = audio {
        put_u32(&mut out, 28, frame_end)?;
        put_u32(&mut out, 32, audio.len())?;
    }
    put_u32(&mut out, 36, frames.len() * 1000 / usize::from(fps))?;
    put_u32(&mut out, 40, ext)?;
    put_u32(&mut out, 44, 4096)?;
    for (i, frame) in frames.iter().enumerate() {
        let row = 76 + i * 40;
        let direction = out[4];
        out[row..row + 4].copy_from_slice(&[direction, 0, 0, 1]);
        let start = out.len() - 76;
        put_u32(&mut out, row + 8, start)?;
        out[row + 12..row + 14].copy_from_slice(&width.to_le_bytes());
        out[row + 14..row + 16].copy_from_slice(&height.to_le_bytes());
        put_u32(&mut out, row + 16, frame.len())?;
        out.extend(frame);
    }
    if let Some(audio) = audio {
        out.extend(audio);
    }
    out.extend(b"hmic");
    out.extend(4088u32.to_le_bytes());
    out.resize(out.len() + 4088, 0);
    inspect(&out)?;
    Ok(out)
}

pub fn inspect(bytes: &[u8]) -> Result<Video<'_>> {
    let invalid = || error("video_layout", "VIDEO 头部、帧表、帧尺寸或扩展信息无效");
    if bytes.len() < 76
        || bytes.len() > MAX_BYTES
        || &bytes[..4] != b"VIDE"
        || !(10..=13).contains(&bytes[4])
        || !(1..=100).contains(&bytes[5])
        || !FPS.contains(&bytes[6])
        || bytes[7] != 0
        || u32_at(bytes, 8)? != 76
        || bytes[20..24] != [0; 4]
        || u32_at(bytes, 24)? as usize != bytes.len() - 76
        || bytes[48..76] != [0; 28]
    {
        return Err(invalid());
    }
    let count = u32_at(bytes, 12)? as usize;
    if count == 0 || count > MAX_FRAMES {
        return Err(invalid());
    }
    let width = u16::from_le_bytes(bytes[16..18].try_into().unwrap());
    let height = u16::from_le_bytes(bytes[18..20].try_into().unwrap());
    if width == 0 || height == 0 {
        return Err(invalid());
    }
    let orientation = u16::from(bytes[4] - 10) * 90;
    let storage = if matches!(orientation, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    };
    let ext = u32_at(bytes, 40)? as usize;
    let wav_start = u32_at(bytes, 28)? as usize;
    let wav_size = u32_at(bytes, 32)? as usize;
    let frame_end = if wav_start == 0 && wav_size == 0 {
        ext
    } else {
        if wav_start == 0 || wav_size == 0 || wav_start.checked_add(wav_size) != Some(ext) {
            return Err(invalid());
        }
        wav_start
    };
    if u32_at(bytes, 44)? != 4096
        || ext.checked_add(76 + 4096) != Some(bytes.len())
        || u32_at(bytes, 36)? as usize != count * 1000 / usize::from(bytes[6])
    {
        return Err(invalid());
    }
    let footer = &bytes[76 + ext..];
    if footer[..8] != [b'h', b'm', b'i', b'c', 248, 15, 0, 0] || footer[8..].iter().any(|b| *b != 0)
    {
        return Err(invalid());
    }
    let mut cursor = count * 40;
    let mut frames = Vec::new();
    for i in 0..count {
        let row = bytes
            .get(76 + i * 40..76 + (i + 1) * 40)
            .ok_or_else(invalid)?;
        if row[..4] != [bytes[4], 0, 0, 1]
            || row[4..8] != [0; 4]
            || u32_at(row, 8)? as usize != cursor
            || row[12..16] != bytes[16..20]
            || row[20..40] != [0; 20]
        {
            return Err(invalid());
        }
        let end = cursor
            .checked_add(u32_at(row, 16)? as usize)
            .filter(|n| *n <= frame_end)
            .ok_or_else(invalid)?;
        let frame = bytes.get(76 + cursor..76 + end).ok_or_else(invalid)?;
        if jpeg::dimensions(frame)? != storage {
            return Err(invalid());
        }
        frames.push(frame);
        cursor = end;
    }
    if cursor != frame_end {
        return Err(invalid());
    }
    let audio = if wav_size > 0 {
        let data = &bytes[76 + wav_start..76 + ext];
        crate::wave::inspect_native(data)?;
        Some(data)
    } else {
        None
    };
    Ok(Video {
        width,
        height,
        orientation,
        quality: bytes[5],
        fps: bytes[6],
        duration_ms: u32_at(bytes, 36)?,
        frames,
        audio,
    })
}
