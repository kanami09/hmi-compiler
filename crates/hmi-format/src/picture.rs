//! 图片运行资源与 BMP／PNG 编辑源的包装规则，不依赖媒体编码器。

use crate::binary::{put_u32, u32_at};
use hmi_core::{Result, error};

pub mod alpha7;
pub mod jpeg;

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
    editor_source_with_format(width, height, "bmp", bmp)
}

/// 生成未旋转、逻辑尺寸的编辑源封装；透明图使用 RGBA8 PNG。
pub fn editor_source_with_format(
    width: u16,
    height: u16,
    format: &str,
    data: &[u8],
) -> Result<Vec<u8>> {
    if !matches!(format, "bmp" | "png") {
        return Err(error("image_format", "编辑源仅支持 BMP 或 PNG"));
    }
    let mut output = header([10, 0x64, 1, 1], width, height, 27, data.len())?;
    output.extend(format.as_bytes());
    output.extend(data);
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
    validate_header(runtime, [direction, 0x60, 1, 0], width, height, 24)?;
    if runtime[20..24] != [0; 4]
        || (runtime.len() - 24) as u64 != u64::from(width) * u64::from(height) * 2
    {
        return Err(invalid());
    }
    validate_bmp_source(source, width, height)
}

fn validate_header(
    bytes: &[u8],
    prefix: [u8; 4],
    width: u16,
    height: u16,
    start: usize,
) -> Result<()> {
    if width == 0
        || height == 0
        || bytes.len() < start
        || bytes.len() < 24
        || bytes[..4] != prefix
        || bytes[4..8] != [0; 4]
        || bytes[12..14] != width.to_le_bytes()
        || bytes[14..16] != height.to_le_bytes()
        || u32_at(bytes, 8)? as usize != start
        || u32_at(bytes, 16)? as usize != bytes.len() - start
    {
        return Err(error("image_layout", "图片资源头、尺寸或载荷长度无效"));
    }
    Ok(())
}

pub fn validate_bmp_source(source: &[u8], width: u16, height: u16) -> Result<()> {
    let invalid = || error("image_layout", "BMP 编辑源的包装或像素布局无效");
    validate_header(source, [10, 0x64, 1, 1], width, height, 27)?;
    if source.len() < 81 || source[20..24] != [0; 4] || source[24..27] != *b"bmp" {
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

pub fn validate_source(source: &[u8], width: u16, height: u16, format: &str) -> Result<()> {
    match format {
        "bmp" => validate_bmp_source(source, width, height),
        "png" => {
            validate_header(source, [10, 0x64, 1, 1], width, height, 27)?;
            if source[20..24] != [0; 4] || source[24..27] != *b"png" {
                return Err(error("image_layout", "PNG 编辑源的包装无效"));
            }
            validate_png(&source[27..], width, height)
        }
        _ => Err(error("image_format", "未知的编辑源格式")),
    }
}

/// 只检查规范化 RGBA8 PNG 的尺寸、块边界和 CRC，不在链接器解压图像。
fn validate_png(bytes: &[u8], width: u16, height: u16) -> Result<()> {
    let invalid = || error("png_layout", "PNG 编辑源的尺寸、块布局或 CRC 无效");
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(invalid());
    }
    let mut pos = 8usize;
    let mut header_seen = false;
    let mut data_bytes = 0usize;
    loop {
        let prefix = bytes.get(pos..pos + 8).ok_or_else(invalid)?;
        let length = u32::from_be_bytes(prefix[..4].try_into().unwrap()) as usize;
        let end = pos
            .checked_add(12)
            .and_then(|v| v.checked_add(length))
            .ok_or_else(invalid)?;
        let chunk = bytes.get(pos..end).ok_or_else(invalid)?;
        if crc32fast::hash(&chunk[4..8 + length])
            != u32::from_be_bytes(chunk[8 + length..].try_into().unwrap())
        {
            return Err(invalid());
        }
        let tag = &prefix[4..8];
        if !header_seen {
            if tag != b"IHDR"
                || length != 13
                || chunk[8..12] != u32::from(width).to_be_bytes()
                || chunk[12..16] != u32::from(height).to_be_bytes()
                || chunk[16..21] != [8, 6, 0, 0, 0]
            {
                return Err(invalid());
            }
            header_seen = true;
        } else if tag == b"IHDR" {
            return Err(invalid());
        }
        if tag == b"IDAT" {
            data_bytes = data_bytes.checked_add(length).ok_or_else(invalid)?;
        }
        if tag == b"IEND" {
            return if length == 0 && end == bytes.len() && data_bytes > 0 {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        pos = end;
    }
}
