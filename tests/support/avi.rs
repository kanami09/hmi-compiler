// 标准 AVI 1.0 测试素材：原始 BGR24 视频，可选单声道 PCM 音轨，不调用任何 EXE。
fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = tag.to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    if !body.len().is_multiple_of(2) {
        out.push(0);
    }
    out
}
fn list(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    chunk(b"LIST", &[tag.as_slice(), body].concat())
}
fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|n| n.to_le_bytes()).collect()
}
fn stream(
    kind: &[u8; 4],
    rate: u32,
    length: u32,
    size: u32,
    sample: u32,
    width: u16,
    height: u16,
) -> Vec<u8> {
    let mut out = kind.to_vec();
    out.extend([0; 4]);
    out.extend(words(&[
        0,
        0,
        0,
        1,
        rate,
        0,
        length,
        size,
        u32::MAX,
        sample,
    ]));
    out.extend(0i16.to_le_bytes());
    out.extend(0i16.to_le_bytes());
    out.extend(width.to_le_bytes());
    out.extend(height.to_le_bytes());
    assert_eq!(out.len(), 56);
    out
}
pub fn avi(width: u16, height: u16, fps: u32, count: u32, audio: bool) -> Vec<u8> {
    let stride = (u32::from(width) * 3 + 3) & !3;
    let size = stride * u32::from(height);
    let avih = words(&[
        1_000_000 / fps,
        size * fps,
        0,
        16,
        count,
        0,
        if audio { 2 } else { 1 },
        size,
        u32::from(width),
        u32::from(height),
        0,
        0,
        0,
        0,
    ]);
    let mut strf = words(&[40, u32::from(width), u32::from(height)]);
    strf.extend([1, 0, 24, 0]);
    strf.extend(words(&[0, size, 0, 0, 0, 0]));
    let video = list(
        b"strl",
        &[
            chunk(
                b"strh",
                &stream(b"vids", fps, count, size, 0, width, height),
            ),
            chunk(b"strf", &strf),
        ]
        .concat(),
    );
    let mut hdrl = [chunk(b"avih", &avih), video].concat();
    if audio {
        let mut fmt = vec![1, 0, 1, 0];
        fmt.extend(words(&[8000, 16000]));
        fmt.extend([2, 0, 16, 0]);
        hdrl.extend(list(
            b"strl",
            &[
                chunk(
                    b"strh",
                    &stream(b"auds", 8000, count * 8000 / fps, 16000, 2, 0, 0),
                ),
                chunk(b"strf", &fmt),
            ]
            .concat(),
        ));
    }
    let mut movi = Vec::new();
    let mut index = Vec::new();
    for frame in 0..count {
        let mut pixels = vec![0; size as usize];
        let color =
            [[220, 40, 20], [30, 210, 60], [30, 70, 230], [220, 190, 20]][frame as usize % 4];
        for y in 0..height {
            for x in 0..width {
                let c = if x < 4 && y < 4 {
                    [255, 255, 255]
                } else {
                    color
                };
                let p = (u32::from(height - 1 - y) * stride + u32::from(x) * 3) as usize;
                pixels[p..p + 3].copy_from_slice(&[c[2], c[1], c[0]]);
            }
        }
        index.extend(b"00db");
        index.extend(words(&[16, 4 + movi.len() as u32, size]));
        movi.extend(chunk(b"00db", &pixels));
        if audio {
            let samples = (frame + 1) * 8000 / fps - frame * 8000 / fps;
            let pcm = vec![0; samples as usize * 2];
            index.extend(b"01wb");
            index.extend(words(&[0, 4 + movi.len() as u32, pcm.len() as u32]));
            movi.extend(chunk(b"01wb", &pcm));
        }
    }
    chunk(
        b"RIFF",
        &[
            b"AVI ".as_slice(),
            &list(b"hdrl", &hdrl),
            &list(b"movi", &movi),
            &chunk(b"idx1", &index),
        ]
        .concat(),
    )
}
