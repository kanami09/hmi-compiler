//! 原生七位透明度游程编码。先比较原始八位值，再量化，最长游程为 255。
use hmi_core::{Result, error};

pub fn encode(alpha: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut pos = 0;
    while pos < alpha.len() {
        let value = alpha[pos];
        let mut count = 1;
        while count < 255 && alpha.get(pos + count) == Some(&value) {
            count += 1;
        }
        let q = (u16::from(value) * 127 / 255) as u8;
        if count == 1 {
            output.push(q | 128);
        } else {
            output.extend([q, count as u8]);
        }
        pos += count;
    }
    output
}

fn visit(data: &[u8], expected: usize, mut run: impl FnMut(u8, usize)) -> Result<()> {
    let invalid = || error("alpha_layout", "透明度流截断、游程为零或像素数不符");
    let mut pos = 0;
    let mut total = 0usize;
    while pos < data.len() {
        let code = data[pos];
        pos += 1;
        let count = if code & 128 != 0 {
            1
        } else {
            let count = usize::from(*data.get(pos).ok_or_else(invalid)?);
            pos += 1;
            count
        };
        total = total
            .checked_add(count)
            .filter(|&n| count != 0 && n <= expected)
            .ok_or_else(invalid)?;
        run(((u16::from(code & 127) * 255) / 127) as u8, count);
    }
    if total != expected {
        return Err(invalid());
    }
    Ok(())
}

/// 校验无需展开像素，链接器可以按压缩流大小使用内存。
pub fn validate(data: &[u8], expected: usize) -> Result<()> {
    visit(data, expected, |_, _| {})
}

pub fn decode(data: &[u8], expected: usize) -> Result<Vec<u8>> {
    if expected > 64 * 1024 * 1024 {
        return Err(error("alpha_length", "透明度解码结果超过 64 MiB"));
    }
    validate(data, expected)?;
    let mut output = Vec::with_capacity(expected);
    visit(data, expected, |value, count| {
        output.resize(output.len() + count, value)
    })?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_vectors_preserve_raw_runs_before_quantization() {
        for (input, bytes) in [
            (vec![0, 0, 1, 2], vec![0, 2, 128, 128]),
            (vec![255; 255], vec![127, 255]),
            (vec![255; 256], vec![127, 255, 255]),
            (vec![255; 510], vec![127, 255, 127, 255]),
            (vec![0; 257], vec![0, 255, 0, 2]),
        ] {
            assert_eq!(encode(&input), bytes);
            assert_eq!(
                decode(&bytes, input.len()).unwrap(),
                input
                    .iter()
                    .map(|&v| ((u16::from(v) * 127 / 255) * 255 / 127) as u8)
                    .collect::<Vec<_>>()
            );
        }
        let input = (0..=255).collect::<Vec<u8>>();
        assert_eq!(
            encode(&input),
            input
                .iter()
                .map(|&v| (u16::from(v) * 127 / 255) as u8 | 128)
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn malformed_runs_and_dimension_mismatches_are_rejected() {
        for (data, n) in [
            (vec![0], 1),
            (vec![0, 0], 0),
            (vec![127, 255], 254),
            (vec![128], 2),
            (vec![128, 128], 1),
        ] {
            assert!(validate(&data, n).is_err());
        }
        assert!(decode(&[], usize::MAX).is_err());
    }
}
