use hmi_artifact::asset::AssetObject;
use hmi_core::target::{Target, TargetConfig};
use image::ImageEncoder;
use std::{fs, path::Path, process::Command};
mod zi {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/zi.rs"
    ));
}
mod gif {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/gif.rs"
    ));
}
mod wave {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/wave.rs"
    ));
}
mod avi {
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
fn description(
    root: &Path,
    kind: &str,
    input: &str,
    source: &str,
    extra: &str,
) -> std::path::PathBuf {
    let p = root.join("asset.toml");
    fs::write(
        &p,
        format!("schema='hmi.asset/1'\nkind='{kind}'\ninput='{input}'\nsource='{source}'\n{extra}"),
    )
    .unwrap();
    p
}
fn seed(root: &Path, kind: &str, t: &Target) -> AssetObject {
    let source = match kind {
        "image" => {
            let mut bytes = Vec::new();
            image::codecs::png::PngEncoder::new(&mut bytes)
                .write_image(
                    &[220, 40, 20, 128].repeat(64 * 48),
                    64,
                    48,
                    image::ExtendedColorType::Rgba8,
                )
                .unwrap();
            fs::write(root.join("source.png"), bytes).unwrap();
            "source.png"
        }
        "font" => {
            fs::write(root.join("source.zi"), zi::font()).unwrap();
            "source.zi"
        }
        "animation" => {
            fs::write(
                root.join("source.gif"),
                gif::gif(
                    64,
                    48,
                    &[gif::full(64, 48, 1, 50), gif::full(64, 48, 2, 50)],
                ),
            )
            .unwrap();
            "source.gif"
        }
        "audio" => {
            fs::write(root.join("source.wav"), wave::wave(8000, 1, 8000)).unwrap();
            "source.wav"
        }
        _ => unreachable!(),
    };
    hmi_media::encode(&description(root, kind, "source", source, ""), t)
        .unwrap()
        .object
}
fn encoded_video(root: &Path, t: &Target) -> AssetObject {
    let mut jpeg = Vec::new();
    let (w, h) = if t.orientation.is_multiple_of(180) {
        (64, 48)
    } else {
        (48, 64)
    };
    let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg, 80);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
    encoder
        .encode(
            &[220, 40, 20].repeat(w as usize * h as usize),
            w,
            h,
            jpeg_encoder::ColorType::Rgb,
        )
        .unwrap();
    let audio = seed(root, "audio", t);
    let bytes = hmi_format::video::write_with_audio(
        64,
        48,
        t.orientation,
        80,
        4,
        &vec![jpeg; 4],
        Some(audio.member(0).unwrap()),
    )
    .unwrap();
    fs::write(root.join("clip.bin"), &bytes).unwrap();
    let result =
        hmi_media::encode(&description(root, "video", "encoded", "clip.bin", ""), t).unwrap();
    assert_eq!(result.object.member(0).unwrap(), bytes);
    result.object
}

#[test]
fn xi_in_both_deliveries_preserves_color_alpha_and_has_no_fabricated_editor_source() {
    let tmp = tempfile::tempdir().unwrap();
    for orientation in [0, 90, 180, 270] {
        let t = target(orientation);
        let image = seed(tmp.path(), "image", &t);
        let xi =
            hmi_format::picture::xi::from_runtime(image.member(0).unwrap(), 64, 48, orientation)
                .unwrap();
        fs::write(tmp.path().join("input.unrelated"), &xi).unwrap();
        for extra in ["", "[delivery]\nmode='external'\npath='sd0/image.xi'\n"] {
            let path = description(tmp.path(), "image", "encoded", "input.unrelated", extra);
            let p = hmi_media::probe(&path, &t).unwrap();
            let r = hmi_media::encode(&path, &t).unwrap();
            assert_eq!(p.info, r.object.metadata.info);
            assert_eq!(p.action, "encoded");
            assert_eq!(r.object.metadata.members.len(), 1);
            assert!(
                r.object
                    .metadata
                    .info
                    .image()
                    .unwrap()
                    .jpeg
                    .as_ref()
                    .unwrap()
                    .quality
                    .is_none()
            );
            if extra.is_empty() {
                assert_eq!(r.object.member(0).unwrap(), image.member(0).unwrap());
            } else {
                assert_eq!(r.object.member(0).unwrap(), xi);
            }
            assert!(hmi_media::encode(&path, &target((orientation + 90) % 360)).is_err());
        }
    }
}

#[test]
fn native_packages_preserve_every_member_and_reject_mode_and_kind_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let t = target(0);
    for kind in ["image", "font", "animation", "audio", "video"] {
        let object = if kind == "video" {
            encoded_video(tmp.path(), &t)
        } else {
            seed(tmp.path(), kind, &t)
        };
        object.write(&tmp.path().join("native.hma"), &[]).unwrap();
        let path = description(tmp.path(), kind, "native", "native.hma", "");
        let p = hmi_media::probe(&path, &t).unwrap();
        let r = hmi_media::encode(&path, &t).unwrap();
        assert_eq!(r.action, "native");
        assert_eq!(p.info, r.object.metadata.info);
        assert_eq!(r.object.payload, object.payload);
        assert_eq!(
            serde_json::to_value(r.object.metadata).unwrap(),
            serde_json::to_value(&object.metadata).unwrap()
        );
        assert!(
            hmi_media::encode(
                &description(
                    tmp.path(),
                    kind,
                    "native",
                    "native.hma",
                    "[encoding]\nquality=80\n"
                ),
                &t
            )
            .is_err()
        );
        assert!(
            hmi_media::encode(
                &description(
                    tmp.path(),
                    kind,
                    "native",
                    "native.hma",
                    "[delivery]\nmode='external'\npath='sd0/changed.video'\n"
                ),
                &t
            )
            .is_err()
        );
        let wrong = if kind == "audio" { "video" } else { "audio" };
        assert!(
            hmi_media::encode(
                &description(tmp.path(), wrong, "native", "native.hma", ""),
                &t
            )
            .is_err()
        );
    }
}

#[test]
fn encoded_media_keep_validated_bytes_and_never_confuse_runtime_gmov_with_editor_data() {
    let tmp = tempfile::tempdir().unwrap();
    let t = target(0);
    for kind in ["font", "audio", "video", "animation"] {
        let object = if kind == "video" {
            encoded_video(tmp.path(), &t)
        } else {
            seed(tmp.path(), kind, &t)
        };
        fs::write(tmp.path().join("encoded.data"), object.member(0).unwrap()).unwrap();
        let extra = if kind == "animation" {
            "[delivery]\nmode='external'\npath='sd0/movie.gmov'\n"
        } else {
            ""
        };
        let result = hmi_media::encode(
            &description(tmp.path(), kind, "encoded", "encoded.data", extra),
            &t,
        )
        .unwrap();
        assert_eq!(result.action, "encoded");
        assert_eq!(result.object.member(0).unwrap(), object.member(0).unwrap());
        if kind == "animation" {
            assert_eq!(result.object.metadata.members.len(), 1);
            assert!(
                hmi_media::encode(
                    &description(tmp.path(), kind, "encoded", "encoded.data", ""),
                    &t
                )
                .is_err()
            );
            fs::write(tmp.path().join("encoded.data"), object.member(1).unwrap()).unwrap();
            assert!(
                hmi_media::encode(
                    &description(tmp.path(), kind, "encoded", "encoded.data", extra),
                    &t
                )
                .is_err()
            );
        }
    }
    fs::write(tmp.path().join("encoded.data"), wave::wave(8000, 1, 8000)).unwrap();
    assert!(
        hmi_media::encode(
            &description(tmp.path(), "audio", "encoded", "encoded.data", ""),
            &t
        )
        .is_err()
    );
}

#[test]
fn original_reuse_requires_live_source_matching_request_and_compatible_target() {
    let tmp = tempfile::tempdir().unwrap();
    let t = target(0);
    for (kind, source, changed) in [
        ("image", "source.png", "[encoding]\nquality=81\n"),
        ("font", "source.zi", ""),
        ("animation", "source.gif", "[encoding]\nquality=81\n"),
        (
            "audio",
            "source.wav",
            "[clip]\nstart_ms=250\nduration_ms=500\n",
        ),
    ] {
        let object = seed(tmp.path(), kind, &t);
        object.write(&tmp.path().join("original.hma"), &[]).unwrap();
        let extra = "original='original.hma'\n";
        let path = description(tmp.path(), kind, "source", source, extra);
        let p = hmi_media::probe(&path, &t).unwrap();
        let r = hmi_media::encode(&path, &t).unwrap();
        assert_eq!(p.action, "original");
        assert_eq!(r.action, "original");
        assert_eq!(p.inputs, r.inputs);
        assert_eq!(r.object.payload, object.payload);
        fs::copy(tmp.path().join(source), tmp.path().join("renamed.source")).unwrap();
        let result = hmi_media::encode(
            &description(tmp.path(), kind, "source", "renamed.source", extra),
            &t,
        )
        .unwrap();
        assert_eq!(result.action, "original");
        if kind == "image" {
            let mut changed_source = Vec::new();
            image::codecs::png::PngEncoder::new(&mut changed_source)
                .write_image(
                    &[20, 210, 40, 128].repeat(64 * 48),
                    64,
                    48,
                    image::ExtendedColorType::Rgba8,
                )
                .unwrap();
            fs::write(tmp.path().join(source), changed_source).unwrap();
            let changed_input =
                hmi_media::encode(&description(tmp.path(), kind, "source", source, extra), &t)
                    .unwrap();
            assert_eq!(changed_input.action, "source");
            assert_ne!(
                changed_input.object.metadata.info.source_sha256(),
                object.metadata.info.source_sha256()
            );
            fs::copy(tmp.path().join("renamed.source"), tmp.path().join(source)).unwrap();
        }
        let other = if kind == "font" {
            target(90)
        } else {
            t.clone()
        };
        let result = hmi_media::encode(
            &description(
                tmp.path(),
                kind,
                "source",
                source,
                &format!("{extra}{changed}"),
            ),
            &other,
        )
        .unwrap();
        assert_eq!(result.action, "source");
        let mut legacy = object.clone();
        legacy.metadata.source_request = None;
        legacy.write(&tmp.path().join("original.hma"), &[]).unwrap();
        assert_eq!(
            hmi_media::encode(&description(tmp.path(), kind, "source", source, extra), &t)
                .unwrap()
                .action,
            "source"
        );
        object.write(&tmp.path().join("original.hma"), &[]).unwrap();
        fs::remove_file(tmp.path().join(source)).unwrap();
        assert!(hmi_media::encode(&path, &t).is_err());
    }
}

#[test]
fn original_corruption_missing_attachments_and_inapplicable_encoded_parameters_are_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let t = target(0);
    let object = seed(tmp.path(), "image", &t);
    let path = description(
        tmp.path(),
        "image",
        "source",
        "source.png",
        "original='missing.hma'\n",
    );
    assert!(hmi_media::encode(&path, &t).is_err());
    fs::write(tmp.path().join("missing.hma"), b"not an asset").unwrap();
    assert!(hmi_media::encode(&path, &t).is_err());
    let xi = hmi_format::picture::xi::from_runtime(object.member(0).unwrap(), 64, 48, 0).unwrap();
    fs::write(tmp.path().join("input.xi"), xi).unwrap();
    for extra in [
        "[encoding]\nquality=80\n",
        "[resize]\nwidth=64\nheight=48\nmode='stretch'\n",
        "original='missing.hma'\n",
    ] {
        assert_eq!(
            hmi_media::encode(
                &description(tmp.path(), "image", "encoded", "input.xi", extra),
                &t
            )
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
            "asset_input_parameter"
        );
    }
}

#[test]
#[ignore = "需要 HMI_FFMPEG_DIR 指向 FFmpeg 9 shared DLL"]
fn video_original_is_reused_in_child_process_without_available_dlls() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let t = target(0);
    fs::write(root.join("input.avi"), avi::avi(64, 48, 4, 4, true)).unwrap();
    let path = description(root, "video", "source", "input.avi", "[encoding]\nfps=4\n");
    let object = hmi_media::encode(&path, &t).unwrap().object;
    object.write(&root.join("original.hma"), &[]).unwrap();
    fs::write(root.join("target.json"), serde_json::to_vec(&t).unwrap()).unwrap();
    let run = |fps| {
        description(
            root,
            "video",
            "source",
            "input.avi",
            &format!("original='original.hma'\n[encoding]\nfps={fps}\n"),
        );
        Command::new(env!("CARGO_BIN_EXE_hmi-media"))
            .args(["encode"])
            .arg(&path)
            .arg("--target")
            .arg(root.join("target.json"))
            .arg("-o")
            .arg(root.join("reused.hma"))
            .arg("--json")
            .env("HMI_FFMPEG_DIR", root.join("missing-backend"))
            .output()
            .unwrap()
    };
    let output = run(4);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["result"]["input_action"], "original");
    assert_eq!(
        AssetObject::read(&root.join("reused.hma")).unwrap().payload,
        object.payload
    );
    assert!(!run(8).status.success());
    fs::remove_file(root.join("input.avi")).unwrap();
    assert!(!run(4).status.success());
}
