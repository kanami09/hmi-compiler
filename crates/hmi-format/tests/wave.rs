use hmi_format::wave;
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/wave.rs"
    ));
}
#[test]
fn native_wave_layout_metadata_and_four_byte_padding_are_correct() {
    let source = fixture::wave(11025, 1, 11025);
    let input = wave::inspect(&source).unwrap();
    let metadata = vec![
        (*b"test", b"keep".to_vec()),
        (*b"name", b"original".to_vec()),
    ];
    let out = wave::write(
        input.sample_rate,
        input.channels,
        input.data,
        &metadata,
        Some("new-name"),
    )
    .unwrap();
    let native = wave::inspect_native(&out).unwrap();
    assert_eq!(native.data_header, 548);
    assert_eq!(native.samples, 11026);
    assert_eq!(&native.data[..input.data.len()], input.data);
    assert_eq!(&native.data[input.data.len()..], &[0, 0]);
    assert!(native.metadata.contains(&(*b"test", b"keep".to_vec())));
    assert!(native.metadata.contains(&(*b"name", b"original".to_vec())));
    let roundtrip = wave::write(
        native.sample_rate,
        native.channels,
        native.data,
        &native.metadata,
        None,
    )
    .unwrap();
    assert_eq!(roundtrip, out);
}
#[test]
fn crc_uses_native_data_header_range_and_bad_riff_chunks_are_rejected() {
    let source = fixture::wave(8000, 1, 100);
    let w = wave::inspect(&source).unwrap();
    let out = wave::write(8000, 1, w.data, &vec![], None).unwrap();
    let mut corrupt = out.clone();
    corrupt[556] ^= 1;
    assert!(wave::inspect_native(&corrupt).is_err());
    // 原生 CRC 不覆盖最后八个 PCM 字节；兼容此规则，由 .hma 的完整摘要保护所有字节。
    let mut tail = out.clone();
    let last = tail.len() - 1;
    tail[last] ^= 1;
    wave::inspect_native(&tail).unwrap();
    for end in [0, 11, 35, 43, source.len() - 1] {
        assert!(wave::inspect(&source[..end]).is_err());
    }
    for index in [0, 4, 8, 16, 22, 24, 28, 32, 34, 40] {
        let mut bad = source.clone();
        bad[index] ^= 0x40;
        assert!(wave::inspect(&bad).is_err(), "{index}");
    }
}
