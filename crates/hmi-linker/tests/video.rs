use hmi_artifact::asset::{AssetObject, VideoInfo};
use hmi_core::{
    asset::FfmpegInfo,
    file,
    model::ObjectFile,
    target::{Target, TargetConfig},
};
use hmi_format::{container::Container, project::Project};
use std::{collections::BTreeMap, fs, path::Path, process::Command};
fn target() -> Target {
    Target::resolve(&TargetConfig {
        profile: "TJC4827X543_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation: 0,
    })
    .unwrap()
}
fn asset() -> AssetObject {
    let mut frame = Vec::new();
    let mut e = jpeg_encoder::Encoder::new(&mut frame, 80);
    e.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
    e.encode(
        &[220, 40, 20].repeat(320 * 160),
        320,
        160,
        jpeg_encoder::ColorType::Rgb,
    )
    .unwrap();
    AssetObject::video(
        VideoInfo {
            width: 320,
            height: 160,
            orientation: 0,
            quality: 80,
            fps: 4,
            frame_count: 4,
            duration_ms: 1000,
            source_fps_num: 4,
            source_fps_den: 1,
            source_frames: 4,
            clip_start_ms: 0,
            source_audio_streams: 0,
            audio_mode: "if-present".into(),
            audio_applied: false,
            audio: None,
            codec: "rawvideo".into(),
            source_sha256: "0".repeat(64),
            backend: FfmpegInfo {
                version: "fixture".into(),
                libraries: BTreeMap::new(),
                fingerprint: "0".repeat(64),
            },
            encoder: "fixture".into(),
        },
        hmi_format::video::write(320, 160, 0, 80, 4, &vec![frame; 4]).unwrap(),
    )
    .unwrap()
}
fn compile(
    root: &Path,
    loop_value: i64,
    script: Option<&str>,
) -> (hmi_artifact::CompiledObject, hmi_artifact::CompiledObject) {
    let t = target();
    fs::write(root.join("program.s"), hmi_core::target::DEFAULT_PROGRAM).unwrap();
    let program = hmi_page_compiler::compile_program(&root.join("program.s"), &t)
        .unwrap()
        .object;
    let mut p = hmi_page_compiler::empty_page("p_video");
    p.objects.push(ObjectFile {
        name: "v0".into(),
        type_name: "video".into(),
        properties: BTreeMap::from([
            ("x".into(), 0.into()),
            ("y".into(), 0.into()),
            ("w".into(), 320.into()),
            ("h".into(), 160.into()),
            ("loop".into(), loop_value.into()),
            (
                "vid".into(),
                serde_json::from_value(serde_json::json!({"ref":"clip"})).unwrap(),
            ),
        ]),
        events: BTreeMap::new(),
    });
    if let Some(script) = script {
        p.objects[0].events.insert("playend".into(), "end.s".into());
        fs::write(root.join("end.s"), script).unwrap();
    }
    fs::write(root.join("page.toml"), file::toml(&p).unwrap()).unwrap();
    let page = hmi_page_compiler::compile_page(&root.join("page.toml"), &t)
        .unwrap()
        .object;
    (program, page)
}
#[test]
fn two_native_video_pages_match_and_reserved_fps_is_not_asset_rate() {
    let tmp = tempfile::tempdir().unwrap();
    let a = asset();
    for (loop_value, sha) in [
        (
            0,
            "a97cf09b7d9b6cf681210293da838367dfc15820c7f02788182db91014bb5d04",
        ),
        (
            1,
            "4195333b7a32bd80ec1607ed00b3288b0902ad30801984dcc98f57a3abc4f479",
        ),
    ] {
        let (program, page) = compile(tmp.path(), loop_value, None);
        let linked = hmi_linker::link_with_assets(
            &target(),
            &program,
            &[page],
            &[("clip".into(), a.clone())],
            true,
        )
        .unwrap();
        let c = Container::parse(linked.bytes).unwrap();
        let p = Project::parse(&c).unwrap();
        let page = &p.pages[0].1;
        assert_eq!(hmi_core::sha256(&page.raw), sha);
        assert_eq!(page.objects[1].integer("fps"), Some(1));
        assert_eq!(page.objects[1].integer("molloc"), Some(64000));
        assert_eq!(page.objects[1].integer("qty"), Some(0));
        assert!(
            p.references
                .iter()
                .any(|r| r.kind == "v" && r.resource == "0.v")
        );
    }
}
#[test]
fn self_contained_video_targets_link_without_decoder_and_keep_event_owners() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (program, page) = compile(root, 1, Some("sys0=v0.qty"));
    let a = asset();
    let reordered = hmi_linker::link_with_assets(
        &target(),
        &program,
        std::slice::from_ref(&page),
        &[("second".into(), a.clone()), ("clip".into(), a.clone())],
        true,
    )
    .unwrap();
    let parsed = Project::parse(&Container::parse(reordered.bytes).unwrap()).unwrap();
    assert_eq!(parsed.pages[0].1.objects[1].integer("vid"), Some(1));
    let expected = hmi_linker::link_with_assets(
        &target(),
        &program,
        std::slice::from_ref(&page),
        &[("clip".into(), a.clone())],
        true,
    )
    .unwrap();
    fs::write(root.join("target.json"), file::json(&target()).unwrap()).unwrap();
    program.write(&root.join("program.hmo"), &[]).unwrap();
    page.write(&root.join("page.hmo"), &[]).unwrap();
    a.write(&root.join("clip.hma"), &[]).unwrap();
    fs::write(root.join("link.toml"),"schema='hmi.link/1'\nname='Video'\ntarget='target.json'\nprogram='program.hmo'\n[pages]\nobjects=['page.hmo']\n[assets]\nitems=[{key='clip',object='clip.hma'}]\n").unwrap();
    for source in ["page.toml", "program.s", "end.s"] {
        fs::remove_file(root.join(source)).unwrap();
    }
    let output = root.join("video.HMI");
    let result = Command::new(env!("CARGO_BIN_EXE_hmi-link"))
        .arg(root.join("link.toml"))
        .args(["-o".as_ref(), output.as_os_str(), "--json".as_ref()])
        .env_remove("HMI_FFMPEG_DIR")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    assert!(result.stderr.is_empty());
    assert_eq!(fs::read(output).unwrap(), expected.bytes);
    let (program, page) = compile(root, 0, Some("v0.fps=4"));
    assert_eq!(
        hmi_linker::link_with_assets(&target(), &program, &[page], &[("clip".into(), a)], false)
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "script_readonly"
    );
}
