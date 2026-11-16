use hmi_format::video;
fn jpeg(w: u16, h: u16) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut bytes, 80);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
    encoder
        .encode(
            &[220, 40, 20].repeat(usize::from(w) * usize::from(h)),
            w,
            h,
            jpeg_encoder::ColorType::Rgb,
        )
        .unwrap();
    bytes
}
#[test]
fn frame_table_offsets_and_four_directions_follow_video_format() {
    for orientation in [0, 90, 180, 270] {
        let (w, h) = if orientation % 180 == 0 {
            (64, 32)
        } else {
            (32, 64)
        };
        let frame = jpeg(w, h);
        let bytes = video::write(
            64,
            32,
            orientation,
            80,
            4,
            &[frame.clone(), frame.clone(), frame.clone(), frame.clone()],
        )
        .unwrap();
        assert_eq!(
            &bytes[..24],
            &[
                b'V',
                b'I',
                b'D',
                b'E',
                10 + (orientation / 90) as u8,
                80,
                4,
                0,
                76,
                0,
                0,
                0,
                4,
                0,
                0,
                0,
                64,
                0,
                32,
                0,
                0,
                0,
                0,
                0
            ]
        );
        assert_eq!(u32::from_le_bytes(bytes[84..88].try_into().unwrap()), 160);
        assert_eq!(&bytes[96..116], &[0; 20]);
        let parsed = video::inspect(&bytes).unwrap();
        assert_eq!(parsed.duration_ms, 1000);
        assert_eq!(parsed.frames, vec![frame.as_slice(); 4]);
    }
}
#[test]
fn damaged_video_records_and_unsupported_parameters_are_rejected() {
    let frame = jpeg(64, 32);
    let bytes = video::write(64, 32, 0, 80, 4, &vec![frame.clone(); 4]).unwrap();
    for position in [
        0, 4, 5, 6, 8, 12, 16, 24, 28, 32, 36, 40, 44, 48, 76, 84, 92, 96, 100,
    ] {
        let mut bad = bytes.clone();
        bad[position] ^= 0x40;
        if position == 5 {
            bad[position] = 0;
        }
        assert!(video::inspect(&bad).is_err(), "{position}");
    }
    for end in [0, 4, 75, 76, bytes.len() - 1] {
        assert!(video::inspect(&bytes[..end]).is_err());
    }
    assert!(video::write(64, 32, 45, 80, 4, std::slice::from_ref(&frame)).is_err());
    assert!(video::write(64, 32, 0, 80, 0, std::slice::from_ref(&frame)).is_err());
    assert!(video::write(64, 32, 0, 80, 30, &[frame]).is_err());
}
