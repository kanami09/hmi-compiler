use hmi_artifact::asset::{AssetInfo, AssetObject};
use hmi_core::{
    asset::{AudioEncoding, AudioSource, Clip, Delivery},
    file,
    target::{Target, TargetConfig},
};
use std::{fs, path::Path};
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/wave.rs"
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
fn config() -> AudioSource {
    AudioSource {
        schema: "hmi.asset/1".into(),
        kind: "audio".into(),
        input: "source".into(),
        source: "input.wav".into(),
        original: None,
        encoding: AudioEncoding::default(),
        clip: Clip::default(),
        delivery: Delivery::default(),
    }
}
fn save(root: &Path, c: &AudioSource) -> std::path::PathBuf {
    let p = root.join("audio.asset.toml");
    fs::write(&p, file::toml(c).unwrap()).unwrap();
    p
}
#[test]
fn pcm16_encoding_needs_no_dll_and_is_independent_of_screen_direction() {
    let tmp = tempfile::tempdir().unwrap();
    let source = fixture::wave(22050, 1, 22050);
    fs::write(tmp.path().join("input.wav"), &source).unwrap();
    let path = save(tmp.path(), &config());
    let mut previous = None;
    for direction in [0, 90, 180, 270] {
        let p = hmi_media::probe(&path, &target(direction)).unwrap();
        let result = hmi_media::encode(&path, &target(direction)).unwrap();
        assert_eq!(p.info, result.object.metadata.info);
        let AssetInfo::Audio { audio } = &p.info else {
            panic!()
        };
        assert!(audio.backend.is_none());
        assert_eq!(audio.duration_ms, 1000);
        assert_eq!(audio.sample_frames, 22050);
        let wave = hmi_format::wave::inspect_native(result.object.member(0).unwrap()).unwrap();
        assert_eq!(wave.data, &source[44..]);
        if let Some(bytes) = previous {
            assert_eq!(bytes, result.object.encode().unwrap());
        }
        previous = Some(result.object.encode().unwrap());
        AssetObject::decode(&result.object.encode().unwrap())
            .unwrap()
            .validate_for(&target(direction))
            .unwrap();
    }
}
#[test]
fn direct_clip_padding_and_metadata_mismatch_are_checked() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("input.wav"), fixture::wave(11025, 1, 11025)).unwrap();
    let mut c = config();
    let path = save(tmp.path(), &c);
    let object = hmi_media::encode(&path, &target(0)).unwrap().object;
    let AssetInfo::Audio { audio } = &object.metadata.info else {
        panic!()
    };
    assert_eq!(audio.padding_samples, 1);
    let mut bad = object.clone();
    if let AssetInfo::Audio { audio } = &mut bad.metadata.info {
        audio.sample_frames -= 1;
    }
    assert!(bad.validate().is_err());
    c.clip = Clip {
        start_ms: 200,
        duration_ms: Some(400),
    };
    save(tmp.path(), &c);
    let clip = hmi_media::encode(&path, &target(0)).unwrap().object;
    let w = hmi_format::wave::inspect_native(clip.member(0).unwrap()).unwrap();
    assert_eq!(w.samples, 4410);
    c.clip.start_ms = 1;
    save(tmp.path(), &c);
    assert!(hmi_media::probe(&path, &target(0)).is_err());
    c = config();
    c.encoding.sample_format = "f32".into();
    save(tmp.path(), &c);
    assert!(hmi_media::probe(&path, &target(0)).is_err());
}
#[test]
#[ignore = "需要 HMI_FFMPEG_DIR 指向 FFmpeg 9 shared DLL"]
fn resampling_and_stereo_to_mono_follow_requested_parameters() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("input.wav"), fixture::wave(48000, 2, 48000)).unwrap();
    let mut c = config();
    c.encoding.sample_rate = Some(8000);
    c.encoding.channels = Some(1);
    c.clip = Clip {
        start_ms: 250,
        duration_ms: Some(500),
    };
    let p = save(tmp.path(), &c);
    let encoded = hmi_media::encode(&p, &target(0)).unwrap().object;
    let w = hmi_format::wave::inspect_native(encoded.member(0).unwrap()).unwrap();
    assert_eq!((w.sample_rate, w.channels, w.samples), (8000, 1, 4000));
    assert!(
        w.data
            .as_chunks::<2>()
            .0
            .iter()
            .all(|b| i16::from_le_bytes(*b).abs() <= 1)
    );
    let AssetInfo::Audio { audio } = &encoded.metadata.info else {
        panic!()
    };
    assert!(audio.backend.is_some());
    assert_eq!(audio.source_channels, 2);
}
