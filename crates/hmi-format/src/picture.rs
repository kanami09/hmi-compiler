//! 已验证 RGB565 图片及 BMP 编辑源的包装规则，不解码常规图片。

use crate::binary::{put_u32, u32_at};
use hmi_core::{Result, error};

fn header(prefix: [u8; 4], width: u16, height: u16, start: usize, size: usize) -> Result<Vec<u8>> {
    if width == 0 || height == 0 {
        return Err(error("image_dimensions", "图片宽高必须大于零"));
    }
    let mut bytes = vec![0; 24];
    bytes[..4].copy_from_slice(&prefix);
    put_u32(&mut bytes, 8, start)?;
    bytes[12..14].copy_from_slice(&width.to_le_bytes());
    bytes[14..16].copy_from_slice(&height.to_le_bytes());
    put_u32(&mut bytes, 16, size)?;
    Ok(bytes)
}

pub fn runtime(width: u16, height: u16, orientation: u16, pixels: &[u8]) -> Result<Vec<u8>> {
    let direction = match orientation {
        0 => 10,
        90 => 11,
        180 => 12,
        270 => 13,
        _ => {
            return Err(error(
                "image_orientation",
                "图片方向必须为 0、90、180 或 270",
            ));
        }
    };
    if u64::from(width) * u64::from(height) * 2 != pixels.len() as u64 {
        return Err(error("image_length", "RGB565 像素长度与逻辑尺寸不一致"));
    }
    let mut output = header([direction, 0x60, 1, 0], width, height, 24, pixels.len())?;
    output.extend(pixels);
    Ok(output)
}

/// 编辑源不随运行像素旋转，保留 BMP 原始或归一化后的完整字节。
pub fn editor_source(width: u16, height: u16, bmp: &[u8]) -> Result<Vec<u8>> {
    let mut output = header([10, 0x64, 1, 1], width, height, 27, bmp.len())?;
    output.extend(b"bmp");
    output.extend(bmp);
    Ok(output)
}

pub fn validate(
    runtime: &[u8],
    source: &[u8],
    width: u16,
    height: u16,
    orientation: u16,
) -> Result<()> {
    let invalid = || error("image_layout", "图片资源头、尺寸或载荷长度无效");
    if width == 0 || height == 0 {
        return Err(invalid());
    }
    let direction = match orientation {
        0 => 10,
        90 => 11,
        180 => 12,
        270 => 13,
        _ => return Err(invalid()),
    };
    if runtime.len() < 24
        || source.len() < 81
        || runtime[..4] != [direction, 0x60, 1, 0]
        || source[..4] != [10, 0x64, 1, 1]
        || source[24..27] != *b"bmp"
    {
        return Err(invalid());
    }
    for bytes in [runtime, source] {
        if bytes[12..14] != width.to_le_bytes()
            || bytes[14..16] != height.to_le_bytes()
            || bytes[4..8] != [0; 4]
            || bytes[20..24] != [0; 4]
        {
            return Err(invalid());
        }
    }
    if u32_at(runtime, 8)? != 24
        || u32_at(source, 8)? != 27
        || u32_at(runtime, 16)? as usize != runtime.len() - 24
        || u32_at(source, 16)? as usize != source.len() - 27
        || (runtime.len() - 24) as u64 != u64::from(width) * u64::from(height) * 2
    {
        return Err(invalid());
    }
    let bmp = &source[27..];
    if &bmp[..2] != b"BM"
        || u32_at(bmp, 2)? as usize != bmp.len()
        || u32_at(bmp, 14)? != 40
        || u32_at(bmp, 18)? != u32::from(width)
        || i32::from_le_bytes(bmp[22..26].try_into().unwrap()).unsigned_abs() != u32::from(height)
        || bmp[26..30] != [1, 0, 24, 0]
        || u32_at(bmp, 30)? != 0
    {
        return Err(invalid());
    }
    let stride = (usize::from(width) * 3 + 3) & !3;
    let start = u32_at(bmp, 10)? as usize;
    if start < 54
        || stride
            .checked_mul(usize::from(height))
            .and_then(|size| start.checked_add(size))
            != Some(bmp.len())
    {
        return Err(invalid());
    }
    Ok(())
}
