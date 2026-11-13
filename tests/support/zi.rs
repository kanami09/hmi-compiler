// 独立的小字形向量，不依赖私有字库或制字工具。
pub fn font() -> Vec<u8> {
    let description = b"fixture\0";
    let mut b = vec![0; 44];
    b[..6].copy_from_slice(&[4, 255, 0, 10, 24, 0]);
    b[7] = 3;
    b[12..16].copy_from_slice(&1u32.to_le_bytes());
    b[16] = 6;
    b[17] = description.len() as u8;
    b[24..28].copy_from_slice(&44u32.to_le_bytes());
    b.extend(description);
    // 字符 A：三项宽度为 2/0/0，数据位于索引起点 +12，长 4。
    b.extend([65, 0, 2, 0, 0, 12, 0, 0, 4, 0]);
    b.extend([0, 0]);
    // 三行、两列灰度矩阵：0/1、2/3、4/7。
    b.extend([3, 0xC1, 0xD3, 0xE7]);
    let size = (b.len() - 44) as u32;
    b[20..24].copy_from_slice(&size.to_le_bytes());
    b
}
