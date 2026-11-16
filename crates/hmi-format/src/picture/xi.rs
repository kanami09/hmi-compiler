//! 外部 XI：hmipicxi 签名、24 字节头、JPEG 和可选七位透明度；无内部帧前缀。
use crate::binary::{put_u32, u32_at};
use hmi_core::{Result, error};

pub struct Header {
    pub width: u16,
    pub height: u16,
    pub orientation: u16,
    pub has_alpha: bool,
}

pub fn from_runtime(runtime: &[u8], width: u16, height: u16, orientation: u16) -> Result<Vec<u8>> {
    let parts = super::jpeg::parts(runtime, width, height, orientation)?;
    let mut out = vec![0; 32];
    out[..8].copy_from_slice(b"hmipicxi");
    out[8..12].copy_from_slice(&[10 + (orientation / 90) as u8, 0, 0, 1]);
    out[12..14].copy_from_slice(&[255, 255]);
    put_u32(&mut out, 16, 24)?;
    out[20..22].copy_from_slice(&width.to_le_bytes());
    out[22..24].copy_from_slice(&height.to_le_bytes());
    put_u32(
        &mut out,
        24,
        parts.jpeg.len() + parts.alpha.map_or(0, <[u8]>::len),
    )?;
    if parts.alpha.is_some() {
        put_u32(&mut out, 28, parts.jpeg.len())?;
    }
    out.extend(parts.jpeg);
    if let Some(alpha) = parts.alpha {
        out.extend(alpha);
    }
    Ok(out)
}

/// 核对标记、长度、方向、存储尺寸和透明像素数量；不在链接器中解码 JPEG 熵数据。
pub fn inspect(bytes: &[u8]) -> Result<Header> {
    if bytes.len() < 32
        || &bytes[..8] != b"hmipicxi"
        || !(10..=13).contains(&bytes[8])
        || bytes[9..16] != [0, 0, 1, 255, 255, 0, 0]
        || u32_at(bytes, 16)? != 24
        || u32_at(bytes, 24)? as usize != bytes.len() - 32
    {
        return Err(error("xi_layout", "XI 标识、头部或载荷长度无效"));
    }
    let width = u16::from_le_bytes(bytes[20..22].try_into().unwrap());
    let height = u16::from_le_bytes(bytes[22..24].try_into().unwrap());
    let orientation = u16::from(bytes[8] - 10) * 90;
    let (w, h) = super::jpeg::storage_dimensions(width, height, orientation)?;
    let body = &bytes[32..];
    let offset = u32_at(bytes, 28)? as usize;
    let split = if offset == 0 { body.len() } else { offset };
    let jpeg = body
        .get(..split)
        .ok_or_else(|| error("xi_layout", "XI 透明度偏移越界"))?;
    if super::jpeg::dimensions(jpeg)? != (w, h) {
        return Err(error("xi_dimensions", "XI 逻辑尺寸与 JPEG 存储尺寸不一致"));
    }
    if offset != 0 {
        super::alpha7::validate(&body[split..], usize::from(w) * usize::from(h))?;
    }
    Ok(Header {
        width,
        height,
        orientation,
        has_alpha: offset != 0,
    })
}
