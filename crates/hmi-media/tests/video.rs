use hmi_artifact::asset::AssetInfo;
use hmi_core::{
    asset::{Clip, Delivery, VideoAudio, VideoEncoding, VideoSource},
    file,
    target::{Target, TargetConfig},
};
use hmi_format::video;
use std::{fs, path::Path};
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/avi.rs"
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
fn description(root: &Path, source: &VideoSource) -> std::path::PathBuf {
    let p = root.join("video.asset.toml");
    fs::write(&p, file::toml(source).unwrap()).unwrap();
    p
}
fn config() -> VideoSource {
    VideoSource {
        schema: "hmi.asset/1".into(),
        kind: "video".into(),
        input: "source".into(),
        source: "源视频.avi".into(),
        original: None,
        encoding: VideoEncoding {
            quality: 80,
            fps: 4,
        },
        clip: Clip::default(),
        audio: VideoAudio::default(),
        delivery: Delivery::default(),
    }
}

#[test]
#[ignore = "需要 HMI_FFMPEG_DIR 指向 FFmpeg 9 shared DLL"]
fn dll_decode_precise_clip_all_orientations_and_complete_video_payload() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("源视频.avi"),
        fixture::avi(64, 32, 4, 4, false),
    )
    .unwrap();
    let mut c = config();
    c.clip = Clip {
        start_ms: 250,
        duration_ms: Some(500),
    };
    let path = description(tmp.path(), &c);
    for orientation in [0, 90, 180, 270] {
        let encoded = hmi_media::encode(&path, &target(orientation)).unwrap();
        let AssetInfo::Video { video: info } = &encoded.object.metadata.info else {
            panic!()
        };
        assert_eq!(
            (
                info.source_frames,
                info.frame_count,
                info.duration_ms,
                info.clip_start_ms
            ),
            (4, 2, 500, 250)
        );
        assert!(!info.audio_applied);
        assert_eq!(info.source_audio_streams, 0);
        assert_eq!(encoded.object.metadata.members[0].extension, "v");
        assert_eq!(encoded.object.metadata.members.len(), 1);
        let v = video::inspect(encoded.object.member(0).unwrap()).unwrap();
        let pixels = image::load_from_memory(v.frames[0]).unwrap().to_rgb8();
        assert_eq!(
            pixels.dimensions(),
            if orientation % 180 == 0 {
                (64, 32)
            } else {
                (32, 64)
            }
        );
        assert!(
            pixels
                .get_pixel(pixels.width() / 2, pixels.height() / 2)
                .0
                .iter()
                .zip([30, 210, 60])
                .all(|(a, b)| a.abs_diff(b) < 20)
        );
        let (x, y) = match orientation {
            0 => (1, 1),
            90 => (30, 1),
            180 => (62, 30),
            _ => (1, 62),
        };
        assert!(pixels.get_pixel(x, y).0.iter().all(|c| *c > 205));
        encoded.object.validate_for(&target(orientation)).unwrap();
        let mut corrupt = encoded.object.member(0).unwrap().to_vec();
        corrupt[84] ^= 16;
        assert!(video::inspect(&corrupt).is_err());
        if orientation == 0 {
            assert_eq!(
                hmi_media::probe(&path, &target(orientation)).unwrap().info,
                encoded.object.metadata.info
            );
            let mut outside = c.clone();
            outside.delivery = Delivery {
                mode: "external".into(),
                path: Some("sd0/clip.video".into()),
            };
            let external = hmi_media::encode(&description(tmp.path(), &outside), &target(0))
                .unwrap()
                .object;
            assert_eq!(
                external.member(0).unwrap(),
                encoded.object.member(0).unwrap()
            );
            assert!(external.metadata.info.is_external());
            assert!(external.metadata.members[0].reference_kind.is_none());
        }
    }
}

#[test]
#[ignore = "需要 HMI_FFMPEG_DIR 指向 FFmpeg 9 shared DLL"]
fn audio_is_never_silently_dropped_and_bad_ranges_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("源视频.avi");
    fs::write(&input, fixture::avi(64, 32, 4, 4, true)).unwrap();
    let mut c = config();
    let path = description(tmp.path(), &c);
    let info = hmi_media::probe(&path, &target(0)).unwrap().info;
    let AssetInfo::Video { video } = &info else {
        panic!()
    };
    assert!(video.audio_applied);
    assert_eq!(video.audio.as_ref().unwrap().sample_rate, 8000);
    c.audio.mode = "drop".into();
    let p = description(tmp.path(), &c);
    let good = hmi_media::encode(&p, &target(0)).unwrap();
    let AssetInfo::Video { video: info } = &good.object.metadata.info else {
        panic!()
    };
    assert_eq!(info.source_audio_streams, 1);
    assert!(!info.audio_applied);
    for (fps, start, duration) in [
        (6, 0, 1000),
        (4, 1, 500),
        (4, 0, 100),
        (4, 500, 1000),
        (4, 0, 0),
    ] {
        c.encoding.fps = fps;
        c.clip = Clip {
            start_ms: start,
            duration_ms: Some(duration),
        };
        description(tmp.path(), &c);
        assert!(
            hmi_media::probe(&path, &target(0)).is_err(),
            "{fps}/{start}/{duration}"
        );
    }
    c = config();
    c.audio.mode = "require".into();
    fs::write(&input, fixture::avi(64, 32, 4, 4, false)).unwrap();
    description(tmp.path(), &c);
    assert!(hmi_media::probe(&path, &target(0)).is_err());
    c = config();
    fs::write(&input, b"bad video").unwrap();
    description(tmp.path(), &c);
    assert!(hmi_media::probe(&path, &target(0)).is_err());
}

#[test]
#[ignore = "需要 HMI_FFMPEG_DIR 指向 FFmpeg 9 shared DLL"]
fn exact_downsampling_and_asset_corruption_checks() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("源视频.avi"),
        fixture::avi(64, 32, 24, 24, false),
    )
    .unwrap();
    let mut c = config();
    c.clip = Clip {
        start_ms: 250,
        duration_ms: Some(500),
    };
    let p = description(tmp.path(), &c);
    let encoded = hmi_media::encode(&p, &target(0)).unwrap().object;
    let v = video::inspect(encoded.member(0).unwrap()).unwrap();
    assert_eq!(v.frames.len(), 2);
    let rgb = image::load_from_memory(v.frames[0]).unwrap().to_rgb8();
    assert!(
        rgb.get_pixel(32, 16)
            .0
            .iter()
            .zip([30, 70, 230])
            .all(|(a, b)| a.abs_diff(b) < 20)
    );
    let mut changed = encoded.clone();
    if let AssetInfo::Video { video } = &mut changed.metadata.info {
        video.clip_start_ms = 251;
    }
    assert!(changed.validate().is_err());
    for offset in [0, 4, 5, 6, 8, 12, 16, 24, 36, 40, 44, 48, 76, 84, 92] {
        let mut bytes = encoded.member(0).unwrap().to_vec();
        bytes[offset] ^= 0x40;
        if offset == 5 {
            bytes[offset] = 0;
        }
        assert!(video::inspect(&bytes).is_err(), "{offset}");
    }
    c.encoding.fps = 10;
    description(tmp.path(), &c);
    assert_eq!(
        hmi_media::probe(&p, &target(0))
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "video_timing"
    );
}
