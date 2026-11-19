use hmi_core::{
    progress::{Progress, ProgressValue},
    target::{Target, TargetConfig},
};
use std::{
    fs,
    sync::{Arc, Mutex},
};
mod gif {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/gif.rs"
    ));
}

#[test]
fn gif_progress_reports_real_decode_and_encode_counts_without_changing_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(
        root.join("input.gif"),
        gif::gif(
            8,
            8,
            &[
                gif::full(8, 8, 1, 10),
                gif::full(8, 8, 2, 10),
                gif::full(8, 8, 3, 10),
            ],
        ),
    )
    .unwrap();
    let path = root.join("animation.toml");
    fs::write(
        &path,
        "schema='hmi.asset/1'\nkind='animation'\nsource='input.gif'\n",
    )
    .unwrap();
    let target = Target::resolve(&TargetConfig {
        profile: "TJC4827X543_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation: 90,
    })
    .unwrap();
    let plain = hmi_media::encode(&path, &target)
        .unwrap()
        .object
        .encode()
        .unwrap();
    let events = Arc::new(Mutex::new(Vec::<ProgressValue>::new()));
    let captured = events.clone();
    let progress = Progress::new(move |value| captured.lock().unwrap().push(value));
    let reported = hmi_media::encode_with_progress(&path, &target, &progress)
        .unwrap()
        .object
        .encode()
        .unwrap();
    assert_eq!(reported, plain);
    let events = events.lock().unwrap();
    for phase in ["decode", "encode"] {
        let samples = events
            .iter()
            .filter(|e| e.phase == phase)
            .collect::<Vec<_>>();
        assert_eq!(samples.first().unwrap().current, 0);
        assert_eq!(samples.last().unwrap().current, 3);
        assert_eq!(samples.last().unwrap().total, Some(3));
        assert!(samples.windows(2).all(|v| v[0].current <= v[1].current));
        assert!(samples.iter().all(|p| p.unit == "frames"));
    }
}
