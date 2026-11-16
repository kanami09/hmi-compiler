// 标准 PCM16 WAV 测试素材，不依赖 HMI 或第三方音频工具。
pub fn wave(rate: u32, channels: u16, samples: u32) -> Vec<u8> {
    let mut pcm = Vec::new();
    for i in 0..samples {
        let value = ((i % 80) as i16 - 40) * 100;
        for ch in 0..channels {
            pcm.extend((if ch == 0 { value } else { -value }).to_le_bytes());
        }
    }
    let mut out = b"RIFF".to_vec();
    out.extend((36 + pcm.len() as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(channels.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * u32::from(channels) * 2).to_le_bytes());
    out.extend((channels * 2).to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend((pcm.len() as u32).to_le_bytes());
    out.extend(pcm);
    out
}
