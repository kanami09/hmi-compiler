//! X543 的不透明 JPEG 图片：24 字节资源头、20 字节帧前缀、标准 JPEG。
use crate::binary::{put_u32, u32_at};
use hmi_core::{Result, error};

/// X5 将小尺寸至少补至 48，再按 16 对齐；逻辑尺寸保留在外层资源头。
pub fn padded_dimensions(width: u16, height: u16) -> Result<(u16, u16)> {
    let pad = |n: u16| -> Result<u16> {
        if n == 0 {
            return Err(error("image_dimensions", "图片尺寸必须大于零"));
        }
        u16::try_from(((u32::from(n) + 15) & !15).max(48))
            .map_err(|_| error("image_dimensions", "JPEG 补齐后尺寸超过 65535"))
    };
    Ok((pad(width)?, pad(height)?))
}

pub fn storage_dimensions(width: u16, height: u16, orientation: u16) -> Result<(u16, u16)> {
    let (w, h) = padded_dimensions(width, height)?;
    match orientation {
        0 | 180 => Ok((w, h)),
        90 | 270 => Ok((h, w)),
        _ => Err(error(
            "image_orientation",
            "图片方向必须为 0、90、180 或 270",
        )),
    }
}

pub fn runtime(width: u16, height: u16, orientation: u16, jpeg: &[u8]) -> Result<Vec<u8>> {
    let (w, h) = storage_dimensions(width, height, orientation)?;
    if dimensions(jpeg)? != (w, h) {
        return Err(error("jpeg_dimensions", "JPEG 帧尺寸与存储尺寸不一致"));
    }
    let mut out = super::header(
        [10 + (orientation / 90) as u8, 0x60, 1, 4],
        width,
        height,
        24,
        20 + jpeg.len(),
    )?;
    out.resize(44, 0);
    put_u32(&mut out, 28, usize::from(w))?;
    put_u32(&mut out, 32, usize::from(h))?;
    out.extend(jpeg);
    Ok(out)
}

/// 复核封装与 JPEG 标记，不解码熵数据，链接器无需图像编解码后端。
pub fn validate(
    runtime: &[u8],
    source: &[u8],
    width: u16,
    height: u16,
    orientation: u16,
) -> Result<()> {
    let (w, h) = storage_dimensions(width, height, orientation)?;
    super::validate_header(
        runtime,
        [10 + (orientation / 90) as u8, 0x60, 1, 4],
        width,
        height,
        24,
    )?;
    if runtime.len() < 44
        || runtime[24..28] != [0; 4]
        || runtime[36..44] != [0; 8]
        || u32_at(runtime, 28)? != u32::from(w)
        || u32_at(runtime, 32)? != u32::from(h)
        || dimensions(&runtime[44..])? != (w, h)
    {
        return Err(error("image_layout", "JPEG 图片帧前缀或尺寸无效"));
    }
    super::validate_bmp_source(source, width, height)
}

/// 当前输出固定为单次扫描、8 位、三个分量的 Baseline YUV420。
pub fn dimensions(bytes: &[u8]) -> Result<(u16, u16)> {
    let invalid = || error("jpeg_layout", "JPEG 标记、尺寸或 Baseline YUV420 格式无效");
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return Err(invalid());
    }
    let mut pos = 2usize;
    let mut frame = None;
    let mut ids = [0; 3];
    loop {
        if bytes.get(pos) != Some(&0xFF) {
            return Err(invalid());
        }
        while bytes.get(pos) == Some(&0xFF) {
            pos += 1;
        }
        let marker = *bytes.get(pos).ok_or_else(invalid)?;
        pos += 1;
        if matches!(marker, 0x00 | 0x01 | 0xD0..=0xD9) {
            return Err(invalid());
        }
        let length = bytes.get(pos..pos + 2).ok_or_else(invalid)?;
        let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
        if length < 2 {
            return Err(invalid());
        }
        let segment = bytes
            .get(pos..pos.checked_add(length).ok_or_else(invalid)?)
            .ok_or_else(invalid)?;
        pos += length;
        if marker == 0xC0 {
            if frame.is_some()
                || segment.len() != 17
                || segment[2] != 8
                || segment[7] != 3
                || segment[9] != 0x22
                || segment[12] != 0x11
                || segment[15] != 0x11
            {
                return Err(invalid());
            }
            let height = u16::from_be_bytes([segment[3], segment[4]]);
            let width = u16::from_be_bytes([segment[5], segment[6]]);
            if width == 0 || height == 0 {
                return Err(invalid());
            }
            ids = [segment[8], segment[11], segment[14]];
            if ids[0] == ids[1] || ids[0] == ids[2] || ids[1] == ids[2] {
                return Err(invalid());
            }
            frame = Some((width, height));
        } else if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            return Err(invalid());
        } else if marker == 0xDA {
            let frame = frame.ok_or_else(invalid)?;
            if segment.len() != 12
                || segment[2] != 3
                || segment[9..12] != [0, 63, 0]
                || [segment[3], segment[5], segment[7]] != ids
            {
                return Err(invalid());
            }
            let start = pos;
            // 熵流中的 FF00 是转义，FFD0～FFD7 是重启标记，FFD9 结束整帧。
            while pos < bytes.len() {
                if bytes[pos] != 0xFF {
                    pos += 1;
                    continue;
                }
                pos += 1;
                while bytes.get(pos) == Some(&0xFF) {
                    pos += 1;
                }
                match bytes.get(pos) {
                    Some(0x00 | 0xD0..=0xD7) => pos += 1,
                    Some(0xD9) if pos + 1 == bytes.len() && pos > start + 1 => return Ok(frame),
                    _ => return Err(invalid()),
                }
            }
            return Err(invalid());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn markers() -> Vec<u8> {
        // 仅用于标记解析的独立向量，熵字节不代表一张可解码的图片。
        vec![
            0xFF, 0xD8, 0xFF, 0xC0, 0, 17, 8, 0, 48, 0, 64, 3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1,
            0xFF, 0xDA, 0, 12, 3, 1, 0, 2, 0x11, 3, 0x11, 0, 63, 0, 0x12, 0xFF, 0, 0x34, 0xFF,
            0xD0, 0x56, 0xFF, 0xD9,
        ]
    }
    #[test]
    fn padding_and_wrapper_keep_logical_and_storage_dimensions_separate() {
        assert_eq!(padded_dimensions(1, 33).unwrap(), (48, 48));
        assert_eq!(padded_dimensions(34, 49).unwrap(), (48, 64));
        assert_eq!(storage_dimensions(61, 35, 90).unwrap(), (48, 64));
        assert!(padded_dimensions(65535, 1).is_err());
        let data = markers();
        assert_eq!(dimensions(&data).unwrap(), (64, 48));
        let wrapped = runtime(61, 35, 0, &data).unwrap();
        assert_eq!(&wrapped[..4], &[10, 0x60, 1, 4]);
        assert_eq!(&wrapped[12..16], &[61, 0, 35, 0]);
        assert_eq!(u32_at(&wrapped, 28).unwrap(), 64);
        assert_eq!(u32_at(&wrapped, 32).unwrap(), 48);
        assert_eq!(&wrapped[44..], data);
    }
    #[test]
    fn malformed_or_unsupported_jpeg_markers_are_rejected() {
        let good = markers();
        for end in 0..good.len() {
            assert!(dimensions(&good[..end]).is_err());
        }
        for (pos, value) in [(3, 0xC2), (4, 255), (6, 12), (8, 0), (13, 0x11)] {
            let mut bad = good.clone();
            bad[pos] = value;
            assert!(dimensions(&bad).is_err(), "{pos}");
        }
        let mut extra = good.clone();
        extra.push(0);
        assert!(dimensions(&extra).is_err());
    }
}
