//! UTF-8 ZI v6 字形检查与方向适配，保留描述、字符码和宽度指标。
//! 支持已验证的灰度 RLE 和透明空白字形；其他版本与编码明确拒绝。
//! RLE 操作码线索来自作者的 v5 笔记，再用本地 v6 字形核对：
//! https://github.com/hagronnestad/nextion-font-editor/blob/master/Docs/Nextion%20Font%20Format%20Specification%20ZI%20version%205.md
use crate::{
    Error, Result,
    binary::{checked_table, put_u32, slice, usize32},
};

fn invalid(message: &str) -> Error {
    hmi_core::error("font_format", format!("ZI 字库：{message}"))
}

fn append(pixels: &mut Vec<u8>, value: u8, count: usize, limit: usize) -> Result<()> {
    if count > limit.saturating_sub(pixels.len()) {
        return Err(invalid("压缩数据超过预期像素数"));
    }
    pixels.resize(pixels.len() + count, value);
    Ok(())
}

fn decode(data: &[u8], expected: usize) -> Result<Vec<u8>> {
    let (&encoding, stream) = data.split_first().ok_or_else(|| invalid("空字形数据"))?;
    if !matches!(encoding, 1 | 3) {
        return Err(invalid("未验证的字形像素编码"));
    }
    let mut pixels = Vec::with_capacity(expected);
    for &byte in stream {
        match byte >> 6 {
            0 => append(
                &mut pixels,
                if byte & 32 == 0 { 0 } else { 7 },
                usize::from(byte & 31),
                expected,
            )?,
            1 => {
                append(&mut pixels, 0, usize::from(byte & 31), expected)?;
                append(&mut pixels, 7, if byte & 32 == 0 { 1 } else { 2 }, expected)?;
            }
            2 => {
                append(&mut pixels, 0, usize::from((byte >> 3) & 7), expected)?;
                append(&mut pixels, byte & 7, 1, expected)?;
            }
            3 => {
                append(&mut pixels, (byte >> 3) & 7, 1, expected)?;
                append(&mut pixels, byte & 7, 1, expected)?;
            }
            _ => unreachable!(),
        }
    }
    if pixels.len() != expected {
        return Err(invalid("解码像素数与高度及三项宽度指标之和不符"));
    }
    // 当前样本中编码 1 只用于全透明的空格，不扩展为所有黑白字形的支持。
    if encoding == 1 && pixels.iter().any(|&pixel| pixel != 0) {
        return Err(invalid("非空白的单色字形尚未验证"));
    }
    Ok(pixels)
}

fn encode(pixels: &[u8], encoding: u8) -> Vec<u8> {
    let mut result = vec![encoding];
    let mut i = 0;
    while i < pixels.len() {
        let value = pixels[i];
        if matches!(value, 0 | 7) {
            let mut count = 1usize;
            while count < 31 && pixels.get(i + count) == Some(&value) {
                count += 1;
            }
            if value == 0
                && let Some(&next) = pixels.get(i + count)
            {
                // 两个相同灰度像素优先成对编码；此优先级与两份本地字体逐字节一致。
                if count <= 7 && !matches!(next, 0 | 7) && pixels.get(i + count + 1) != Some(&next)
                {
                    result.push(0x80 | ((count as u8) << 3) | next);
                    i += count + 1;
                    continue;
                }
                if next == 7 {
                    let mut opaque = 1;
                    while opaque < 3 && pixels.get(i + count + opaque) == Some(&7) {
                        opaque += 1;
                    }
                    if opaque <= 2 {
                        result.push(0x40 | if opaque == 2 { 0x20 } else { 0 } | count as u8);
                        i += count + opaque;
                        continue;
                    }
                }
            }
            if value == 7
                && count == 1
                && let Some(&next) = pixels.get(i + 1)
                && !matches!(next, 0 | 7)
            {
                result.push(0xC0 | (7 << 3) | next);
                i += 2;
                continue;
            }
            result.push(if value == 7 { 0x20 } else { 0 } | count as u8);
            i += count;
        } else if let Some(&next) = pixels.get(i + 1) {
            result.push(0xC0 | (value << 3) | next);
            i += 2;
        } else {
            result.push(0x80 | value);
            i += 1;
        }
    }
    result
}

fn rotate_clockwise(pixels: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut rotated = Vec::with_capacity(pixels.len());
    for y in 0..width {
        for x in 0..height {
            rotated.push(pixels[(height - 1 - x) * width + y]);
        }
    }
    rotated
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub orientation: u16,
    pub height: u8,
    pub glyph_count: u32,
}

struct Glyph<'a> {
    entry: usize,
    width: usize,
    data: &'a [u8],
}

struct Parsed<'a> {
    header: Header,
    map_start: usize,
    glyph_start: usize,
    glyphs: Vec<Glyph<'a>>,
}

fn parse(input: &[u8]) -> Result<Parsed<'_>> {
    slice(input, 0, 44)?;
    if input[..3] != [4, 255, 0]
        || !(10..=13).contains(&input[3])
        || input[4] != 24
        || !matches!(input[5], 0 | 2)
        || input[16] != 6
        || usize32(input, 24)? != 44
    {
        return Err(invalid("当前支持 UTF-8、v6、方向标记 10～13 的字库"));
    }
    if usize32(input, 20)? != input.len() - 44 {
        return Err(invalid("载荷长度与文件长度不符"));
    }
    let height = usize::from(input[7]);
    let count = usize32(input, 12)?;
    if height == 0 || count == 0 || count > 65536 || input.len() > 64 * 1024 * 1024 {
        return Err(invalid("高度、字形数量或文件大小超出支持范围"));
    }
    let map_start = 44 + usize::from(input[17]);
    let table_end = checked_table(input, map_start, count, 10)?;
    let first = slice(input, map_start + 5, 3)?;
    let first_offset =
        usize::from(first[0]) | (usize::from(first[1]) << 8) | (usize::from(first[2]) << 16);
    let glyph_start = map_start
        .checked_add(first_offset)
        .ok_or_else(|| invalid("字形偏移溢出"))?;
    if glyph_start < table_end {
        return Err(invalid("字形数据与索引表重叠"));
    }
    slice(input, 0, glyph_start)?;
    let mut glyphs = Vec::with_capacity(count);
    let mut codes = std::collections::BTreeSet::new();
    let mut total_pixels = 0usize;
    let mut cursor = glyph_start;
    for i in 0..count {
        let entry = map_start + i * 10;
        let row = slice(input, entry, 10)?;
        if !codes.insert(u16::from_le_bytes([row[0], row[1]])) {
            return Err(invalid("字形索引包含重复字符码"));
        }
        let raster_width = usize::from(row[2]) + usize::from(row[3]) + usize::from(row[4]);
        if raster_width == 0 {
            return Err(invalid("字形宽度指标之和为零"));
        }
        let offset = usize::from(row[5]) | (usize::from(row[6]) << 8) | (usize::from(row[7]) << 16);
        let length = usize::from(u16::from_le_bytes([row[8], row[9]]));
        let position = map_start
            .checked_add(offset)
            .ok_or_else(|| invalid("字形偏移溢出"))?;
        if position != cursor {
            return Err(invalid("非连续字形布局尚未验证"));
        }
        let original = slice(input, position, length)?;
        total_pixels += height * raster_width;
        if total_pixels > 128 * 1024 * 1024 {
            return Err(invalid("字形解码总像素数超过 128 MiB 限制"));
        }
        decode(original, height * raster_width)?;
        glyphs.push(Glyph {
            entry,
            width: raster_width,
            data: original,
        });
        cursor = position + length;
    }
    if cursor != input.len() {
        return Err(invalid("字库尾部包含未解释数据"));
    }
    Ok(Parsed {
        header: Header {
            orientation: u16::from(input[3] - 10) * 90,
            height: input[7],
            glyph_count: count as u32,
        },
        map_start,
        glyph_start,
        glyphs,
    })
}

/// 完整检查索引和压缩流，不只读取头部声明。
pub fn inspect(input: &[u8]) -> Result<Header> {
    Ok(parse(input)?.header)
}

/// 按工程方向旋转存储矩阵，再重建索引；同方向输入保持原字节。
pub fn adapt(input: &[u8], orientation: u16) -> Result<Vec<u8>> {
    if !matches!(orientation, 0 | 90 | 180 | 270) {
        return Err(invalid("工程方向必须为 0、90、180 或 270"));
    }
    let parsed = parse(input)?;
    let turns = (orientation + 360 - parsed.header.orientation) % 360 / 90;
    if turns == 0 {
        return Ok(input.to_vec());
    }
    let mut output = input[..parsed.glyph_start].to_vec();
    for glyph in &parsed.glyphs {
        let height = usize::from(parsed.header.height);
        let (mut w, mut h) = if matches!(parsed.header.orientation, 90 | 270) {
            (height, glyph.width)
        } else {
            (glyph.width, height)
        };
        let mut pixels = decode(glyph.data, w * h)?;
        for _ in 0..turns {
            pixels = rotate_clockwise(&pixels, w, h);
            std::mem::swap(&mut w, &mut h);
        }
        let encoded = encode(&pixels, glyph.data[0]);
        let offset = output.len() - parsed.map_start;
        if offset > 0xFF_FFFF {
            return Err(invalid("字形偏移超过 u24"));
        }
        let length = u16::try_from(encoded.len()).map_err(|_| invalid("字形长度超过 u16"))?;
        output[glyph.entry + 5..glyph.entry + 8]
            .copy_from_slice(&(offset as u32).to_le_bytes()[..3]);
        output[glyph.entry + 8..glyph.entry + 10].copy_from_slice(&length.to_le_bytes());
        output.extend(encoded);
    }
    output[3] = 10 + (orientation / 90) as u8;
    let payload_length = output.len() - 44;
    put_u32(&mut output, 20, payload_length)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/support/zi.rs"
        ));
    }

    #[test]
    fn opcode_reference_vector() {
        let expected = [0, 0, 7, 7, 7, 0, 7, 0, 7, 7, 0, 0, 3, 2, 5];
        assert_eq!(
            decode(&[3, 0x02, 0x23, 0x41, 0x61, 0x93, 0xD5], 15).unwrap(),
            expected
        );
        assert!(decode(&[3, 0x1F], 15).is_err());
    }

    #[test]
    fn rectangular_raster_rotation() {
        assert_eq!(
            rotate_clockwise(&[0, 1, 2, 3, 4, 5], 2, 3),
            [4, 2, 0, 5, 3, 1]
        );
        assert!(adapt(&[4, 255, 0, 13], 0).is_err());
    }

    #[test]
    fn all_direction_pairs_preserve_metrics_and_expected_pixels() {
        let original = fixture::font();
        let expected = [
            [0, 1, 2, 3, 4, 7],
            [4, 2, 0, 7, 3, 1],
            [7, 4, 3, 2, 1, 0],
            [1, 3, 7, 0, 2, 4],
        ];
        for source in [0, 90, 180, 270] {
            let input = adapt(&original, source).unwrap();
            assert_eq!(adapt(&input, source).unwrap(), input);
            for target in [0, 90, 180, 270] {
                let output = adapt(&input, target).unwrap();
                let p = parse(&output).unwrap();
                assert_eq!(p.header.orientation, target);
                assert_eq!(
                    decode(p.glyphs[0].data, 6).unwrap(),
                    expected[usize::from(target / 90)]
                );
                assert_eq!(&output[p.map_start..p.map_start + 5], &[65, 0, 2, 0, 0]);
                let restored = adapt(&output, 0).unwrap();
                assert_eq!(
                    decode(parse(&restored).unwrap().glyphs[0].data, 6).unwrap(),
                    expected[0]
                );
            }
        }
    }

    #[test]
    fn rejects_bad_header_layout_stream_and_unverified_encoding() {
        let good = fixture::font();
        for (position, value) in [
            (3, 14),
            (16, 5),
            (24, 43),
            (57, 11),
            (good.len() - 4, 2),
            (good.len() - 4, 1),
            (good.len() - 1, 31),
        ] {
            let mut bad = good.clone();
            bad[position] = value;
            assert!(inspect(&bad).is_err(), "position={position}, value={value}");
        }
        for size in 0..good.len() {
            assert!(inspect(&good[..size]).is_err());
        }
    }
}
