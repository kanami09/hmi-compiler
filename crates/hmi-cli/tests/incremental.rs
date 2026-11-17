use hmi_core::{
    file,
    model::{AssetSource, PageSource, ProjectSource},
    target::TargetConfig,
};
use image::ImageEncoder;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn setup() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    hmi_cli::init(
        tmp.path(),
        &TargetConfig {
            profile: "TJC4832K035_011".into(),
            format: "ver21234".into(),
            encoding: "utf-8".into(),
            orientation: 0,
        },
    )
    .unwrap();
    let root = tmp.path();
    let mut project = manifest(root);
    let page = hmi_page_compiler::empty_page("settings");
    fs::create_dir_all(root.join("pages/settings")).unwrap();
    fs::write(
        root.join("pages/settings/page.toml"),
        file::toml(&page).unwrap(),
    )
    .unwrap();
    project
        .pages
        .sources
        .push("pages/settings/page.toml".into());
    let page_path = root.join("pages/page0/page.toml");
    let mut page: PageSource = file::read_toml(&page_path).unwrap();
    page.events.insert("load".into(), "load.s".into());
    fs::write(page_path, file::toml(&page).unwrap()).unwrap();
    fs::write(root.join("pages/page0/load.s"), "sys0=1\n").unwrap();
    fs::create_dir_all(root.join("assets")).unwrap();
    for (name, color) in [("red", 230), ("dark", 25)] {
        png(&root.join(format!("assets/{name}.png")), color);
        fs::write(
            root.join(format!("assets/{name}.toml")),
            format!("schema='hmi.asset/1'\nkind='image'\nsource='{name}.png'\n"),
        )
        .unwrap();
        project.assets.items.push(AssetSource {
            key: name.into(),
            source: format!("assets/{name}.toml"),
        });
    }
    save_manifest(root, &project);
    tmp
}
fn manifest(root: &Path) -> ProjectSource {
    file::read_toml(&root.join("hmi.toml")).unwrap()
}
fn save_manifest(root: &Path, project: &ProjectSource) {
    fs::write(root.join("hmi.toml"), file::toml(project).unwrap()).unwrap();
}
fn png(path: &Path, color: u8) {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &[color, 0, 0, 255].repeat(17 * 19),
            17,
            19,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    fs::write(path, bytes).unwrap();
}
fn build(root: &Path, force: bool) -> (Vec<u8>, hmi_cli::BuildStats) {
    let (_, linked, stats) = hmi_cli::build_with_options(root, None, true, force).unwrap();
    (linked.bytes, stats)
}
fn counts(stats: &hmi_cli::BuildStats) -> (usize, usize, usize, usize) {
    (
        stats.compiled,
        stats.reused_objects,
        stats.encoded,
        stats.reused_assets,
    )
}
fn cache_files(root: &Path, extension: &str) -> Vec<PathBuf> {
    fs::read_dir(root.join(".build/hmi-cache"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == extension))
        .collect()
}

#[test]
fn edits_invalidate_only_their_units_and_rebuild_matches_incremental() {
    let tmp = setup();
    let root = tmp.path();
    let (cold, stats) = build(root, false);
    assert_eq!(counts(&stats), (3, 0, 2, 0));
    let (warm, stats) = build(root, false);
    assert_eq!(counts(&stats), (0, 3, 0, 2));
    assert_eq!(warm, cold);
    fs::write(root.join("pages/page0/load.s"), "sys0=2\n").unwrap();
    let (edited, stats) = build(root, false);
    assert_eq!(counts(&stats), (1, 2, 0, 2));
    assert_ne!(edited, cold);
    png(&root.join("assets/red.png"), 123);
    let (media, stats) = build(root, false);
    assert_eq!(counts(&stats), (0, 3, 1, 1));
    assert_ne!(media, edited);
    let (forced, stats) = build(root, true);
    assert_eq!(counts(&stats), (3, 0, 2, 0));
    assert_eq!(forced, media);
    assert_eq!(
        hmi_cli::prepare(root).unwrap().link(true).unwrap().bytes,
        media
    );
}

#[test]
fn manifest_order_names_and_output_do_not_recompile_units() {
    let tmp = setup();
    let root = tmp.path();
    build(root, false);
    let mut project = manifest(root);
    project.pages.sources.reverse();
    project.assets.items.reverse();
    project.assets.items[0].key = "renamed".into();
    project.build.output = Some("elsewhere/out.HMI".into());
    save_manifest(root, &project);
    let (bytes, stats) = build(root, false);
    assert_eq!(counts(&stats), (0, 3, 0, 2));
    assert_eq!(
        hmi_cli::prepare(root).unwrap().link(true).unwrap().bytes,
        bytes
    );
    assert!(root.join("elsewhere/out.HMI").is_file());
}

#[test]
fn final_name_binding_is_checked_again_when_all_objects_are_reused() {
    let tmp = setup();
    let root = tmp.path();
    fs::write(root.join("pages/page0/load.s"), "page settings\n").unwrap();
    let (before, _) = build(root, false);
    let mut project = manifest(root);
    project.pages.sources.pop();
    save_manifest(root, &project);
    let error = hmi_cli::build(root, None, true).unwrap_err();
    assert_eq!(error.as_diagnostic().unwrap().code, "script_page_target");
    assert_eq!(fs::read(root.join("dist/Panel/Panel.HMI")).unwrap(), before);
}

#[test]
fn cached_warnings_are_replayed_and_werror_keeps_previous_output() {
    let tmp = setup();
    let root = tmp.path();
    fs::write(root.join("pages/page0/load.s"), "page sys0\n").unwrap();
    let (output, linked, _) = hmi_cli::build_with_options(root, None, false, false).unwrap();
    assert!(
        linked
            .diagnostics
            .iter()
            .any(|d| d.code == "script_page_target")
    );
    let error = hmi_cli::build(root, None, true).unwrap_err();
    assert_eq!(error.as_diagnostic().unwrap().code, "script_page_target");
    assert_eq!(
        error.as_diagnostic().unwrap().severity,
        hmi_core::diagnostic::Severity::Error
    );
    assert_eq!(fs::read(output).unwrap(), linked.bytes);
    let (_, cached, stats) = hmi_cli::build_with_options(root, None, false, false).unwrap();
    assert_eq!(counts(&stats), (0, 3, 0, 2));
    assert_eq!(cached.diagnostics, linked.diagnostics);
}

#[test]
fn corrupt_or_missing_artifacts_and_state_are_rebuilt() {
    let tmp = setup();
    let root = tmp.path();
    let (expected, _) = build(root, false);
    let object = cache_files(root, "hmo").pop().unwrap();
    fs::write(object, b"broken hmo").unwrap();
    let (bytes, stats) = build(root, false);
    assert_eq!(counts(&stats), (1, 2, 0, 2));
    assert_eq!(bytes, expected);
    let asset = cache_files(root, "hma").pop().unwrap();
    fs::remove_file(asset).unwrap();
    let (bytes, stats) = build(root, false);
    assert_eq!(counts(&stats), (0, 3, 1, 1));
    assert_eq!(bytes, expected);
    for state in cache_files(root, "json") {
        fs::write(state, b"bad json").unwrap();
    }
    let (bytes, stats) = build(root, false);
    assert_eq!(counts(&stats), (3, 0, 2, 0));
    assert_eq!(bytes, expected);
}

#[test]
fn missing_source_or_changed_descriptor_never_falls_back_to_cache() {
    let tmp = setup();
    let root = tmp.path();
    let (before, _) = build(root, false);
    let source = root.join("assets/red.png");
    fs::remove_file(&source).unwrap();
    assert!(hmi_cli::build(root, None, true).is_err());
    assert_eq!(fs::read(root.join("dist/Panel/Panel.HMI")).unwrap(), before);
    png(&source, 230);
    fs::write(
        root.join("assets/red.toml"),
        "schema='hmi.asset/1'\nkind='image'\nsource='absent.png'\n",
    )
    .unwrap();
    assert!(hmi_cli::build(root, None, true).is_err());
    assert_eq!(fs::read(root.join("dist/Panel/Panel.HMI")).unwrap(), before);
}

#[test]
fn imported_originals_remain_inputs_across_cache_rebuild_and_clean() {
    let seed = setup();
    let (bytes, _) = build(seed.path(), false);
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("input.HMI");
    fs::write(&input, &bytes).unwrap();
    let root = tmp.path().join("project");
    hmi_parser::export(&input, &root).unwrap();
    let (before, _) = build(&root, false);
    let (_, warm) = build(&root, false);
    assert_eq!(counts(&warm), (0, 3, 0, 2));
    let original = root.join(manifest(&root).original.unwrap().bundle);
    let mut bundle = hmi_artifact::original::OriginalBundle::read(&original).unwrap();
    bundle.metadata.pages[0].directory_unknown = [1, 2, 3];
    bundle.write(&original, &[]).unwrap();
    let (changed, stats) = build(&root, false);
    assert_eq!(counts(&stats), (2, 1, 0, 2));
    assert_ne!(changed, before);
    let project = manifest(&root);
    let preserved: Vec<_> = project
        .assets
        .items
        .iter()
        .map(|a| {
            let descriptor: toml::Value = file::read_toml(&root.join(&a.source)).unwrap();
            let path = file::absolute(
                &root
                    .join(&a.source)
                    .parent()
                    .unwrap()
                    .join(descriptor["source"].as_str().unwrap()),
            )
            .unwrap();
            (path.clone(), fs::read(path).unwrap())
        })
        .collect();
    assert!(hmi_cli::clean(&root).unwrap() > 0);
    assert!(original.is_file());
    for (path, bytes) in preserved {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    let (fresh, stats) = build(&root, true);
    assert_eq!(counts(&stats), (3, 0, 2, 0));
    assert_eq!(fresh, changed);
    fs::remove_file(original).unwrap();
    assert!(hmi_cli::build(&root, None, true).is_err());
}

#[test]
fn clean_preserves_outputs_unknown_files_and_modified_derived_files() {
    let tmp = setup();
    let root = tmp.path();
    let (expected, _) = build(root, false);
    let unknown = root.join(".build/hmi-toolchain/notes.txt");
    fs::write(&unknown, b"notes").unwrap();
    let modified = cache_files(root, "hmo").pop().unwrap();
    fs::write(&modified, b"user edited").unwrap();
    fs::create_dir_all(root.join(".preserve")).unwrap();
    fs::write(root.join(".preserve/keep"), b"original").unwrap();
    assert!(hmi_cli::clean(root).unwrap() > 0);
    assert_eq!(fs::read(&unknown).unwrap(), b"notes");
    assert_eq!(fs::read(&modified).unwrap(), b"user edited");
    assert_eq!(
        fs::read(root.join("dist/Panel/Panel.HMI")).unwrap(),
        expected
    );
    assert_eq!(fs::read(root.join(".preserve/keep")).unwrap(), b"original");
    assert_eq!(hmi_cli::clean(root).unwrap(), 0);
    assert_eq!(build(root, false).0, expected);
}

#[test]
fn clean_checks_all_paths_before_deleting_when_an_artifact_becomes_an_input() {
    let tmp = setup();
    let root = tmp.path();
    build(root, false);
    let source = cache_files(root, "hmo").pop().unwrap();
    fs::write(&source, "sys0=2\n").unwrap();
    let mut project = manifest(root);
    project.program.source = source
        .strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    save_manifest(root, &project);
    let count = cache_files(root, "hma").len();
    let error = hmi_cli::clean(root).unwrap_err();
    assert_eq!(error.as_diagnostic().unwrap().code, "output_input");
    assert_eq!(cache_files(root, "hma").len(), count);
    assert_eq!(fs::read(source).unwrap(), b"sys0=2\n");
}

#[test]
fn malformed_cleanup_paths_are_rejected_without_deleting_anything() {
    let tmp = setup();
    let root = tmp.path();
    build(root, false);
    let state = root.join(".build/hmi-owned.json");
    let mut json: serde_json::Value = file::read_json(&state).unwrap();
    json["files"][".build/hmi-cache/../../program.s"] =
        hmi_core::sha256(&fs::read(root.join("program.s")).unwrap()).into();
    fs::write(state, file::json(&json).unwrap()).unwrap();
    let before = cache_files(root, "hmo").len();
    let error = hmi_cli::clean(root).unwrap_err();
    assert_eq!(error.as_diagnostic().unwrap().code, "clean_path");
    assert_eq!(cache_files(root, "hmo").len(), before);
    assert!(root.join("program.s").is_file());
}

#[test]
fn process_lock_blocks_build_and_clean_then_recovers_without_deleting_lock() {
    let tmp = setup();
    let root = tmp.path();
    build(root, false);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join(".build/hmi.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    for action in ["build", "rebuild", "clean"] {
        let output = Command::new(env!("CARGO_BIN_EXE_hmi"))
            .args(["--json", action])
            .arg(root)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(json.to_string().contains("project_locked"), "{json}");
    }
    drop(lock);
    let output = Command::new(env!("CARGO_BIN_EXE_hmi"))
        .arg("build")
        .arg(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(root.join(".build/hmi.lock").is_file());
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

#[test]
fn changing_event_dependencies_does_not_require_the_old_script() {
    let tmp = setup();
    let root = tmp.path();
    build(root, false);
    let path = root.join("pages/page0/page.toml");
    let mut page: PageSource = file::read_toml(&path).unwrap();
    page.events.insert("load".into(), "other.s".into());
    fs::write(path, file::toml(&page).unwrap()).unwrap();
    fs::remove_file(root.join("pages/page0/load.s")).unwrap();
    fs::write(root.join("pages/page0/other.s"), "sys0=3\n").unwrap();
    let (_, stats) = build(root, false);
    assert_eq!(counts(&stats), (1, 2, 0, 2));
}

#[test]
fn audio_is_reused_across_screen_directions() {
    let tmp = setup();
    let root = tmp.path();
    let mut project = manifest(root);
    project.target.profile = "TJC4827X543_011".into();
    fs::write(root.join("assets/ui.wav"), wave::wave(22050, 1, 2205)).unwrap();
    fs::write(
        root.join("assets/ui.toml"),
        "schema='hmi.asset/1'\nkind='audio'\nsource='ui.wav'\n",
    )
    .unwrap();
    project.assets.items.push(AssetSource {
        key: "sound".into(),
        source: "assets/ui.toml".into(),
    });
    save_manifest(root, &project);
    build(root, false);
    project.target.orientation = 90;
    save_manifest(root, &project);
    let (bytes, stats) = build(root, false);
    assert_eq!(counts(&stats), (3, 0, 2, 1));
    assert_eq!(
        hmi_cli::prepare(root).unwrap().link(true).unwrap().bytes,
        bytes
    );
}

#[test]
#[ignore = "需要 HMI_FFMPEG_DIR 指向 FFmpeg 9 shared DLL"]
fn dll_media_dependencies_are_recorded_and_reused_across_processes() {
    let tmp = setup();
    let root = tmp.path();
    let mut project = manifest(root);
    project.target.profile = "TJC4827X543_011".into();
    fs::write(root.join("assets/video.avi"), avi::avi(64, 32, 4, 4, false)).unwrap();
    fs::write(
        root.join("assets/video.toml"),
        "schema='hmi.asset/1'\nkind='video'\nsource='video.avi'\n[encoding]\nquality=80\nfps=4\n",
    )
    .unwrap();
    project.assets.items.push(AssetSource {
        key: "movie".into(),
        source: "assets/video.toml".into(),
    });
    fs::write(root.join("assets/audio.wav"), wave::wave(48000, 2, 4800)).unwrap();
    fs::write(
        root.join("assets/audio.toml"),
        "schema='hmi.asset/1'\nkind='audio'\nsource='audio.wav'\n[encoding]\nsample_rate=22050\nchannels=1\n",
    )
    .unwrap();
    project.assets.items.push(AssetSource {
        key: "sound".into(),
        source: "assets/audio.toml".into(),
    });
    save_manifest(root, &project);
    let run = |action: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_hmi"))
            .args(["--json", action])
            .arg(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let cold = run("build");
    let warm = run("build");
    assert_eq!(cold["result"]["build"]["encoded"], 4);
    assert_eq!(warm["result"]["build"]["encoded"], 0);
    assert_eq!(warm["result"]["build"]["reused_assets"], 4);
    assert_eq!(warm["result"]["sha256"], cold["result"]["sha256"]);
    let mut backend_entries = 0;
    for entry in cache_files(root, "json") {
        let json: serde_json::Value = file::read_json(&entry).unwrap();
        if !json["backend"].is_null() {
            assert_eq!(
                json["inputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|d| d["path"].as_str().unwrap().ends_with(".dll"))
                    .count(),
                5
            );
            backend_entries += 1;
        }
    }
    assert_eq!(backend_entries, 2);
    assert_eq!(run("rebuild")["result"]["sha256"], cold["result"]["sha256"]);
}
