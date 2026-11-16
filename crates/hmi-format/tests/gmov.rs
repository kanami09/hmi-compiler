use hmi_format::gmov;
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/gif.rs"
    ));
}

#[test]
fn timing_vectors_match_native_short_frame_and_long_delay_rules() {
    for (input, expected) in [
        (
            vec![0, 10, 10, 10, 20, 70, 655350],
            vec![(1, 30), (4, 30), (5, 60), (6, 65000)],
        ),
        (vec![20, 50], vec![(0, 30), (1, 40)]),
        (vec![10, 20], vec![(0, 30)]),
        (vec![30, 20], vec![(0, 30), (1, 20)]),
        (vec![0, 0], vec![]),
        (vec![70000], vec![(0, 65000)]),
    ] {
        assert_eq!(gmov::normalize_timing(&input), expected);
    }
}

#[test]
fn source_wrapper_retains_gif_and_scanner_rejects_bad_boundaries() {
    let gif = fixture::gif(
        16,
        8,
        &[fixture::full(16, 8, 1, 50), fixture::full(16, 8, 2, 50)],
    );
    let source = gmov::write_source(&gif).unwrap();
    assert_eq!(
        &source[..24],
        b"GMOV\x0a\x64\x0a\x01\x4c\0\0\0\x02\0\0\0\x10\0\x08\0\0\0\0\0"
    );
    assert_eq!(&source[76..], gif);
    assert_eq!(
        gmov::inspect_source(&source).unwrap().delays,
        vec![500, 500]
    );
    for end in [0, 6, 12, gif.len() - 1] {
        assert!(gmov::inspect_gif(&gif[..end]).is_err());
    }
    let mut bad = gif.clone();
    bad.push(0);
    assert!(gmov::inspect_gif(&bad).is_err());
    let mut bad = source.clone();
    bad[12] = 3;
    assert!(gmov::inspect_source(&bad).is_err());
    let mut bad = source;
    bad[49] = 0;
    assert!(gmov::inspect_source(&bad).is_err());
    let mut frame = fixture::full(16, 8, 1, 50);
    frame.left = 1;
    assert!(gmov::inspect_gif(&fixture::gif(16, 8, &[frame])).is_err());
}
