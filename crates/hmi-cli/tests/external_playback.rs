use hmi_artifact::{
    CompiledObject,
    asset::{AssetInfo, AssetObject, VideoInfo},
};
use hmi_core::{
    asset::{Delivery, FfmpegInfo},
    file,
    model::ObjectFile,
    target::{Target, TargetConfig},
};
use hmi_format::{container::Container, project::Project};
use std::{collections::BTreeMap, fs, path::Path};
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
const KINDS: [(&str, &str, &str); 3] = [
    ("animation", "gm0", "gmov"),
    ("video", "v0", "video"),
    ("audio", "wav0", "wav"),
];
fn target(orientation: u16) -> Target {
    Target::resolve(&TargetConfig {
        profile: "TJC4827X543_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation,
    })
    .unwrap()
}
fn reference(key: &str) -> toml::Value {
    toml::Value::Table(toml::map::Map::from_iter([("ref".into(), key.into())]))
}
fn assets(root: &Path, t: &Target) -> Vec<(String, AssetObject)> {
    fs::write(
        root.join("clip.gif"),
        gif::gif(
            64,
            48,
            &[gif::full(64, 48, 1, 50), gif::full(64, 48, 2, 50)],
        ),
    )
    .unwrap();
    fs::write(root.join("tone.wav"), wave::wave(8000, 1, 8000)).unwrap();
    // VIDEO 用已有图片编码器生成标准 JPEG 测试帧，默认测试不加载 FFmpeg DLL。
    let mut bmp = vec![0; 54 + 64 * 48 * 3];
    let size = bmp.len() as u32;
    bmp[..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&size.to_le_bytes());
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&64i32.to_le_bytes());
    bmp[22..26].copy_from_slice(&48i32.to_le_bytes());
    bmp[26..30].copy_from_slice(&[1, 0, 24, 0]);
    fs::write(root.join("frame.bmp"), bmp).unwrap();
    fs::write(
        root.join("frame.toml"),
        "schema='hmi.asset/1'\nkind='image'\nsource='frame.bmp'\n",
    )
    .unwrap();
    let image = hmi_media::encode(&root.join("frame.toml"), t)
        .unwrap()
        .object;
    let frame = hmi_format::picture::jpeg::parts(image.member(0).unwrap(), 64, 48, t.orientation)
        .unwrap()
        .jpeg
        .to_vec();
    let mut out = Vec::new();
    for (kind, _, extension) in KINDS {
        for external in [false, true] {
            let delivery = if external {
                Delivery {
                    mode: "external".into(),
                    path: Some(format!("sd0/{kind}.{extension}")),
                }
            } else {
                Delivery::default()
            };
            let object = if kind == "video" {
                AssetObject::video(
                    VideoInfo {
                        delivery,
                        width: 64,
                        height: 48,
                        orientation: t.orientation,
                        quality: 80,
                        fps: 4,
                        frame_count: 4,
                        duration_ms: 1000,
                        source_fps_num: 4,
                        source_fps_den: 1,
                        source_frames: 4,
                        clip_start_ms: 0,
                        source_audio_streams: 0,
                        audio_mode: "drop".into(),
                        audio_applied: false,
                        audio: None,
                        codec: "fixture".into(),
                        source_sha256: "0".repeat(64),
                        backend: Some(FfmpegInfo {
                            version: "fixture".into(),
                            libraries: BTreeMap::new(),
                            fingerprint: "0".repeat(64),
                        }),
                        encoder: "fixture".into(),
                    },
                    hmi_format::video::write(64, 48, t.orientation, 80, 4, &vec![frame.clone(); 4])
                        .unwrap(),
                )
                .unwrap()
            } else {
                let description = root.join(format!("{kind}.toml"));
                fs::write(
                    &description,
                    format!(
                        "schema='hmi.asset/1'\nkind='{kind}'\nsource='{}'\n{}",
                        if kind == "animation" {
                            "clip.gif"
                        } else {
                            "tone.wav"
                        },
                        if external {
                            format!("[delivery]\nmode='external'\npath='sd0/{kind}.{extension}'\n")
                        } else {
                            String::new()
                        }
                    ),
                )
                .unwrap();
                hmi_media::encode(&description, t).unwrap().object
            };
            object.validate_for(t).unwrap();
            let key = format!("{kind}_{}", if external { "out" } else { "in" });
            if !external {
                out.push((format!("{kind}_spare"), object.clone()));
            }
            out.push((key, object));
        }
    }
    out
}
fn widgets(from: i64, keep_vid: bool) -> Vec<ObjectFile> {
    KINDS
        .iter()
        .map(|(kind, name, _)| {
            let mut properties = BTreeMap::from([
                ("from".into(), from.into()),
                ("path".into(), reference(&format!("{kind}_out"))),
            ]);
            if keep_vid {
                properties.insert("vid".into(), reference(&format!("{kind}_in")));
            }
            if *kind != "audio" {
                properties.extend([
                    ("x".into(), 20.into()),
                    ("y".into(), 20.into()),
                    ("w".into(), 64.into()),
                    ("h".into(), 48.into()),
                ]);
            }
            ObjectFile {
                name: (*name).into(),
                type_name: (*kind).into(),
                properties,
                events: BTreeMap::new(),
            }
        })
        .collect()
}
fn compile(root: &Path, t: &Target, objects: Vec<ObjectFile>) -> (CompiledObject, CompiledObject) {
    fs::write(root.join("program.s"), hmi_core::target::DEFAULT_PROGRAM).unwrap();
    let mut page = hmi_page_compiler::empty_page("page0");
    page.objects = objects;
    fs::write(root.join("page.toml"), file::toml(&page).unwrap()).unwrap();
    (
        hmi_page_compiler::compile_program(&root.join("program.s"), t)
            .unwrap()
            .object,
        hmi_page_compiler::compile_page(&root.join("page.toml"), t)
            .unwrap()
            .object,
    )
}
fn project(bytes: Vec<u8>) -> Project {
    Project::parse(&Container::parse(bytes).unwrap()).unwrap()
}

#[test]
fn all_three_sources_switch_without_rewriting_inactive_configuration_in_four_directions() {
    let tmp = tempfile::tempdir().unwrap();
    for orientation in [0, 90, 180, 270] {
        let t = target(orientation);
        let mut media = assets(tmp.path(), &t);
        media.reverse();
        let mut previous: Option<Project> = None;
        for from in [0, 1] {
            let (program, page) = compile(tmp.path(), &t, widgets(from, true));
            let linked = hmi_linker::link_with_assets(&t, &program, &[page], &media, true).unwrap();
            assert_eq!(linked.external_files.len(), 3);
            let parsed = project(linked.bytes);
            for (index, (kind, _, extension)) in KINDS.iter().enumerate() {
                let o = &parsed.pages[0].1.objects[index + 1];
                assert_eq!(o.integer("vid"), Some(0));
                assert_eq!(o.integer("from"), Some(from as u32));
                assert_eq!(o.integer("en"), Some(0));
                assert_eq!(
                    o.attribute("path").unwrap().value,
                    format!("sd0/{kind}.{extension}").as_bytes()
                );
                if let Some(old) = &previous {
                    assert_eq!(
                        old.pages[0].1.objects[index + 1]
                            .rebuild(
                                &BTreeMap::from([("from".into(), vec![1])]),
                                &BTreeMap::new()
                            )
                            .unwrap(),
                        o.raw
                    );
                }
            }
            for f in linked.map["runtime_files"].as_array().unwrap() {
                assert_eq!(f["users"].as_array().unwrap().len(), from as usize);
                assert_eq!(f["generated"], true);
            }
            previous = Some(parsed);
        }
        for (kind, _, _) in KINDS {
            let inside = &media
                .iter()
                .find(|(key, _)| key == &format!("{kind}_in"))
                .unwrap()
                .1;
            let outside = &media
                .iter()
                .find(|(key, _)| key == &format!("{kind}_out"))
                .unwrap()
                .1;
            assert_eq!(inside.member(0).unwrap(), outside.member(0).unwrap());
        }
    }
}

#[test]
fn external_only_project_has_no_embedded_media_ids_and_keeps_files_self_contained() {
    let tmp = tempfile::tempdir().unwrap();
    let t = target(0);
    let media = assets(tmp.path(), &t)
        .into_iter()
        .filter(|(_, a)| a.metadata.info.is_external())
        .collect::<Vec<_>>();
    let (program, page) = compile(tmp.path(), &t, widgets(1, false));
    let linked =
        hmi_linker::link_with_assets(&t, &program, std::slice::from_ref(&page), &media, true)
            .unwrap();
    assert!(
        project(linked.bytes.clone())
            .references
            .iter()
            .all(|r| r.kind == "pa")
    );
    assert!(
        linked.map["assets"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["id"].is_null())
    );
    for o in &project(linked.bytes.clone()).pages[0].1.objects[1..] {
        assert_eq!(o.integer("vid"), Some(65535));
    }
    let moved = tempfile::tempdir().unwrap();
    let root = moved.path();
    fs::write(root.join("target.json"), serde_json::to_vec(&t).unwrap()).unwrap();
    program.write(&root.join("program.hmo"), &[]).unwrap();
    page.write(&root.join("page.hmo"), &[]).unwrap();
    let mut entries = Vec::new();
    for (index, (key, asset)) in media.iter().enumerate() {
        asset
            .write(&root.join(format!("a{index}.hma")), &[])
            .unwrap();
        entries.push(format!("{{key='{key}',object='a{index}.hma'}}"));
    }
    fs::write(root.join("link.toml"),format!("schema='hmi.link/1'\nname='Outside'\ntarget='target.json'\nprogram='program.hmo'\n[pages]\nobjects=['page.hmo']\n[assets]\nitems=[{}]\n",entries.join(","))).unwrap();
    tmp.close().unwrap();
    let independent = hmi_linker::from_manifest(&root.join("link.toml"), true).unwrap();
    assert_eq!(independent.bytes, linked.bytes);
    for (a, b) in independent
        .external_files
        .iter()
        .zip(&linked.external_files)
    {
        assert_eq!(a.path, b.path);
        assert_eq!(a.bytes, b.bytes);
    }
}

#[test]
fn literal_paths_need_no_asset_or_filesystem_but_active_sources_are_required() {
    let tmp = tempfile::tempdir().unwrap();
    let t = target(0);
    let mut objects = widgets(1, false);
    for (o, (kind, _, extension)) in objects.iter_mut().zip(KINDS) {
        o.properties
            .insert("path".into(), format!("ram/{kind}.{extension}").into());
    }
    let (program, page) = compile(tmp.path(), &t, objects.clone());
    let linked = hmi_linker::link(&t, &program, &[page], true).unwrap();
    assert!(linked.external_files.is_empty());
    assert_eq!(linked.map["runtime_files"].as_array().unwrap().len(), 3);
    assert!(
        linked.map["runtime_files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["generated"] == false)
    );
    for o in &objects {
        for (field, value) in [
            ("from", 0.into()),
            ("from", 2.into()),
            ("path", "".into()),
            ("path", "x".repeat(256).into()),
        ] {
            let mut bad = o.clone();
            bad.properties.insert(field.into(), value);
            let mut source = hmi_page_compiler::empty_page("page0");
            source.objects = vec![bad];
            fs::write(tmp.path().join("bad.toml"), file::toml(&source).unwrap()).unwrap();
            assert!(
                hmi_page_compiler::compile_page(&tmp.path().join("bad.toml"), &t).is_err(),
                "{}.{}",
                o.type_name,
                field
            );
        }
    }
}

#[test]
fn wrong_asset_types_hidden_references_and_output_path_conflicts_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let t = target(0);
    let media = assets(tmp.path(), &t);
    for (from, field, key) in [
        (1, "path", "audio_out"),
        (0, "path", "video_in"),
        (1, "vid", "video_out"),
    ] {
        let mut objects = widgets(from, true);
        objects[1].properties.insert(field.into(), reference(key));
        let (program, page) = compile(tmp.path(), &t, objects);
        assert!(hmi_linker::link_with_assets(&t, &program, &[page], &media, true).is_err());
    }
    let mut conflict = media.clone();
    let (_, asset) = conflict.iter_mut().find(|(k, _)| k == "video_out").unwrap();
    if let AssetInfo::Video { video } = &mut asset.metadata.info {
        video.delivery.path = Some("sd0/audio.wav/clip.video".into());
    }
    let (program, page) = compile(tmp.path(), &t, widgets(1, true));
    assert_eq!(
        hmi_linker::link_with_assets(&t, &program, &[page], &conflict, true)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "deployment_conflict"
    );
    let mut objects = widgets(1, true);
    objects[1]
        .properties
        .insert("path".into(), "sd0/audio.wav".into());
    let (program, page) = compile(tmp.path(), &t, objects);
    assert_eq!(
        hmi_linker::link_with_assets(&t, &program, &[page], &media, true)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "runtime_file_conflict"
    );
}

#[test]
fn tampered_empty_media_or_missing_path_relocations_do_not_bypass_validation() {
    let tmp = tempfile::tempdir().unwrap();
    let t = target(0);
    let media = assets(tmp.path(), &t);
    let (program, page) = compile(tmp.path(), &t, widgets(1, false));
    let mut invalid = page.clone();
    invalid
        .metadata
        .relocations
        .retain(|r| r.kind != hmi_artifact::RelocationKind::ExternalPath);
    assert!(hmi_linker::link_with_assets(&t, &program, &[invalid], &media, true).is_err());
    let mut invalid = page;
    let record = invalid.metadata.layout.as_ref().unwrap().objects[1].clone();
    let raw = hmi_format::page::Object::parse(invalid.record(1).unwrap())
        .unwrap()
        .rebuild(
            &BTreeMap::from([("vid".into(), vec![0, 0])]),
            &BTreeMap::new(),
        )
        .unwrap();
    invalid.payload[record.offset as usize..(record.offset + record.length) as usize]
        .copy_from_slice(&raw);
    assert!(hmi_linker::link_with_assets(&t, &program, &[invalid], &media, true).is_err());
    for (_, a) in &media {
        if a.metadata.info.is_external() {
            assert!(a.metadata.members[0].reference_kind.is_none());
        }
    }
}
