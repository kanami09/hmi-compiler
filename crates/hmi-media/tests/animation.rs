use hmi_artifact::asset::AssetInfo;
use hmi_core::{
    asset::{Alpha, AnimationEncoding, AnimationSource, Delivery},
    file,
    target::{Target, TargetConfig},
};
use hmi_format::{
    gmov,
    picture::{alpha7, jpeg},
};
use std::{fs, path::Path};
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/gif.rs"
    ));
}
fn target(orientation: u16) -> Target {
    Target::resolve(&TargetConfig {
        profile: "TJC4827X543_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation,
    })
    .unwrap()
}
fn config(root: &Path, gif: &[u8], encoding: AnimationEncoding) -> std::path::PathBuf {
    fs::write(root.join("input.gif"), gif).unwrap();
    let path = root.join("animation.asset.toml");
    fs::write(
        &path,
        file::toml(&AnimationSource {
            schema: "hmi.asset/1".into(),
            kind: "animation".into(),
            input: "source".into(),
            source: "input.gif".into(),
            original: None,
            encoding,
            delivery: Delivery::default(),
        })
        .unwrap(),
    )
    .unwrap();
    path
}

#[test]
fn disposal_local_frames_timing_and_transparency_are_preserved_in_all_directions() {
    let mut a = fixture::full(37, 23, 1, 0);
    a.pixels[0] = 0;
    a.transparent = true;
    let patch = |color, delay, disposal| fixture::GifFrame {
        left: 4,
        top: 3,
        width: 7,
        height: 5,
        delay_cs: delay,
        disposal,
        transparent: true,
        pixels: vec![color; 35],
    };
    let gif = fixture::gif(
        37,
        23,
        &[
            a,
            patch(2, 1, 1),
            patch(3, 1, 2),
            patch(2, 1, 3),
            patch(3, 2, 1),
            patch(2, 7, 1),
            patch(3, 65535, 1),
        ],
    );
    let tmp = tempfile::tempdir().unwrap();
    let path = config(
        tmp.path(),
        &gif,
        AnimationEncoding {
            quality: 96,
            ..Default::default()
        },
    );
    for orientation in [0, 90, 180, 270] {
        let p = hmi_media::probe(&path, &target(orientation)).unwrap();
        let encoded = hmi_media::encode(&path, &target(orientation))
            .unwrap()
            .object;
        assert_eq!(p.info, encoded.metadata.info);
        let AssetInfo::Animation { animation: info } = &encoded.metadata.info else {
            panic!()
        };
        assert_eq!(info.frame_indices, vec![1, 4, 5, 6]);
        assert_eq!(info.delays_ms, vec![30, 30, 60, 65000]);
        assert_eq!(info.duration_ms, 65120);
        assert!(info.timing_adjusted);
        assert!(info.has_alpha);
        assert_eq!(&encoded.member(1).unwrap()[76..], gif);
        let runtime = gmov::inspect_runtime(encoded.member(0).unwrap()).unwrap();
        let (w, h) = gmov::storage_dimensions(37, 23, orientation).unwrap();
        for (index, frame) in runtime.frames.iter().enumerate() {
            let split = frame.alpha_offset as usize;
            let pixels = image::load_from_memory(&frame.data[20..split])
                .unwrap()
                .to_rgb8();
            assert_eq!(pixels.dimensions(), (u32::from(w), u32::from(h)));
            let (x, y) = match orientation {
                0 => (6, 5),
                90 => (26, 6),
                180 => (41, 26),
                _ => (5, 41),
            };
            let color = if index % 2 == 0 {
                [30, 210, 60]
            } else {
                [30, 70, 230]
            };
            assert!(
                pixels
                    .get_pixel(x, y)
                    .0
                    .iter()
                    .zip(color)
                    .all(|(a, b)| a.abs_diff(b) < 35)
            );
            let alpha =
                alpha7::decode(&frame.data[split..], usize::from(w) * usize::from(h)).unwrap();
            for y in 0..32 {
                for x in 0..48 {
                    let (tx, ty) = match orientation {
                        0 => (x, y),
                        90 => (31 - y, x),
                        180 => (47 - x, 31 - y),
                        _ => (y, 47 - x),
                    };
                    let expected = if x < 37 && y < 23 && (x, y) != (0, 0) {
                        255
                    } else {
                        0
                    };
                    assert_eq!(alpha[(ty * u32::from(w) + tx) as usize], expected);
                }
            }
        }
        encoded.validate_for(&target(orientation)).unwrap();
    }
}

#[test]
fn flatten_quality_100_and_corrupt_animation_members_are_checked() {
    let mut f = fixture::full(32, 16, 0, 50);
    f.transparent = true;
    let gif = fixture::gif(32, 16, &[f]);
    let tmp = tempfile::tempdir().unwrap();
    let path = config(
        tmp.path(),
        &gif,
        AnimationEncoding {
            quality: 100,
            alpha: Alpha::Flatten,
            background: Some("#FFFFFF".into()),
            ..Default::default()
        },
    );
    let encoded = hmi_media::encode(&path, &target(0)).unwrap().object;
    let runtime = gmov::inspect_runtime(encoded.member(0).unwrap()).unwrap();
    assert!(!runtime.alpha_enabled);
    assert_eq!(runtime.frames[0].alpha_offset, 0);
    let color = image::load_from_memory(&runtime.frames[0].data[20..])
        .unwrap()
        .to_rgb8();
    assert!(color.get_pixel(0, 0).0.iter().all(|c| *c > 245));
    assert_eq!(
        jpeg::dimensions(&runtime.frames[0].data[20..]).unwrap(),
        (32, 16)
    );
    for offset in [
        0, 4, 5, 6, 8, 12, 16, 24, 36, 40, 44, 48, 49, 76, 84, 92, 100, 104,
    ] {
        let mut bad = encoded.member(0).unwrap().to_vec();
        bad[offset] ^= 0x40;
        if offset == 5 {
            bad[offset] = 0;
        }
        assert!(gmov::inspect_runtime(&bad).is_err(), "{offset}");
    }
    let mut bad = encoded.clone();
    if let AssetInfo::Animation { animation } = &mut bad.metadata.info {
        animation.delays_ms[0] = 30;
    }
    assert!(bad.validate().is_err());
    let zero = fixture::gif(2, 2, &[fixture::full(2, 2, 1, 0)]);
    let path = config(tmp.path(), &zero, AnimationEncoding::default());
    assert!(hmi_media::probe(&path, &target(0)).is_err());
}
