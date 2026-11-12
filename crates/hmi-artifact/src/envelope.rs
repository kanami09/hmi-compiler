//! 中间文件共用封装；长度来自实际文件，摘要覆盖描述和载荷。
use hmi_core::{Result, error, sha256};
use serde::{Serialize, de::DeserializeOwned};
const HEADER: usize = 80;
pub fn encode<T: Serialize>(
    magic: &[u8; 8],
    metadata: &T,
    payload: &[u8],
    kind: &str,
) -> Result<Vec<u8>> {
    let meta = serde_json::to_vec(metadata)
        .map_err(|e| error(&format!("{kind}_metadata"), e.to_string()))?;
    let meta_len = u32::try_from(meta.len())
        .map_err(|_| error(&format!("{kind}_length"), "目标文件描述超过 u32 范围"))?;
    let data_len = u32::try_from(payload.len())
        .map_err(|_| error(&format!("{kind}_length"), "目标文件载荷超过 u32 范围"))?;
    let mut bytes = vec![0; HEADER];
    bytes[..8].copy_from_slice(magic);
    bytes[8..12].copy_from_slice(&meta_len.to_le_bytes());
    bytes[12..16].copy_from_slice(&data_len.to_le_bytes());
    bytes.extend(meta);
    bytes.extend(payload);
    let digest = sha256(&bytes[HEADER..]);
    bytes[16..80].copy_from_slice(digest.as_bytes());
    Ok(bytes)
}
pub fn decode<T: DeserializeOwned>(
    magic: &[u8; 8],
    bytes: &[u8],
    kind: &str,
) -> Result<(T, Vec<u8>)> {
    if bytes.len() < HEADER || &bytes[..8] != magic {
        return Err(error(
            &format!("{kind}_format"),
            "目标文件格式或版本不受支持，请重新生成",
        ));
    }
    let meta = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let data = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let end = HEADER
        .checked_add(meta)
        .ok_or_else(|| error(&format!("{kind}_length"), "目标文件描述长度溢出"))?;
    if end.checked_add(data) != Some(bytes.len()) {
        return Err(error(
            &format!("{kind}_length"),
            "目标文件长度与头部记录不一致",
        ));
    }
    if sha256(&bytes[HEADER..]).as_bytes() != &bytes[16..80] {
        return Err(error(
            &format!("{kind}_hash"),
            "目标文件摘要不匹配，请重新生成",
        ));
    }
    let metadata = serde_json::from_slice(&bytes[HEADER..end]).map_err(|e| {
        error(
            &format!("{kind}_metadata"),
            format!("目标文件描述无效：{e}"),
        )
    })?;
    Ok((metadata, bytes[end..].to_vec()))
}
