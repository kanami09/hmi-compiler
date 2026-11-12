use hmi_core::{
    file,
    model::{PageSource, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, project::Project};
use std::{fs, path::Path, process::Command};

fn config(profile: &str, orientation: u16) -> TargetConfig {
    TargetConfig {
        profile: profile.into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation,
    }
}
fn write_page(root: &Path, relative: &str, name: &str, event: Option<&str>) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut page = hmi_page_compiler::empty_page(name);
    if let Some(code) = event {
        page.events.insert("load".into(), "load.s".into());
        fs::write(path.parent().unwrap().join("load.s"), code).unwrap();
    }
    fs::write(path, file::toml(&page).unwrap()).unwrap();
}

#[test]
fn blank_known_directions_match_native_vectors() {
    for (orientation, expected) in [
        (
            0,
            "4b6b217071905eb11b787fddd27bbe602c87c0363a0a0b291863e6cc64ee1f65",
        ),
        (
            90,
            "4fea9cac81d75a63d61cae10fd4a80d52c7b91c9b24b5357972e8f2a390ea300",
        ),
        (
            270,
            "978ff2117ea8159157fb9e137c9d5618501b1037b05722961cd744cf2bf7cf9a",
        ),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        hmi_cli::init(tmp.path(), &config("TJC4832K035_011", orientation)).unwrap();
        let linked = hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
        assert_eq!(hmi_core::sha256(&linked.bytes), expected);
        assert!(linked.diagnostics.is_empty());
    }
}

#[test]
fn four_directions_write_geometry_and_direction_without_changing_model_fields() {
    // 尺寸来自配置规则，型号字段来自原生对照；检查最终文件而不只检查 Target。
    let cases = [
        (
            "TJC4832K035_011",
            [(320, 480), (480, 320), (320, 480), (480, 320)],
            1,
            0x04096270u32,
        ),
        (
            "TJC4832T035_011",
            [(320, 480), (480, 320), (320, 480), (480, 320)],
            0,
            0xADE186D6,
        ),
        (
            "TJC3224T028_011",
            [(240, 320), (320, 240), (240, 320), (320, 240)],
            0,
            0x0B997EF5,
        ),
        (
            "TJC4827X543_011",
            [(480, 272), (272, 480), (480, 272), (272, 480)],
            3,
            0x51841CCD,
        ),
    ];
    for (profile, sizes, tag, model_code) in cases {
        for ((orientation, code), (width, height)) in
            [(0, 0), (90, 1), (180, 2), (270, 3)].into_iter().zip(sizes)
        {
            let tmp = tempfile::tempdir().unwrap();
            hmi_cli::init(tmp.path(), &config(profile, orientation)).unwrap();
            let linked = hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
            let container = Container::parse(linked.bytes).unwrap();
            let project = Project::parse(&container).unwrap();
            let main = container
                .data(container.active("main.HMI").unwrap())
                .unwrap();
            assert_eq!(main[12], code);
            assert_eq!(main[11], tag);
            assert_eq!(main[65], 3);
            assert_eq!(
                u32::from_le_bytes(main[16..20].try_into().unwrap()),
                model_code
            );
            let page = &project.pages[0].1.objects[0];
            assert_eq!(page.integer("w"), Some(width));
            assert_eq!(page.integer("h"), Some(height));
            assert_eq!(page.integer("endx"), Some(width - 1));
            assert_eq!(page.integer("endy"), Some(height - 1));
        }
    }
}

#[test]
fn all_initial_profiles_build_without_snapshot_or_media_backend() {
    for profile in hmi_core::target::PROFILES {
        let tmp = tempfile::tempdir().unwrap();
        hmi_cli::init(tmp.path(), &config(profile, 0)).unwrap();
        assert!(!tmp.path().join(".hmi").exists());
        let (path, linked) = hmi_cli::build(tmp.path(), None, true).unwrap();
        assert_eq!(fs::read(path).unwrap(), linked.bytes);
        let container = Container::parse(linked.bytes).unwrap();
        let project = Project::parse(&container).unwrap();
        assert_eq!(project.pages.len(), 1);
        assert_eq!(
            project.pages[0].1.objects[0].integer("w"),
            Some(
                hmi_core::target::Target::resolve(&config(profile, 0))
                    .unwrap()
                    .canvas
                    .width
                    .into()
            )
        );
        assert!(container.entries.iter().all(|e| !e.deleted));
    }
}

#[test]
fn moved_targets_link_without_sources_and_page_reorder_uses_final_ids() {
    let source = tempfile::tempdir().unwrap();
    let outputs = tempfile::tempdir().unwrap();
    hmi_cli::init(source.path(), &config("TJC4827X543_011", 0)).unwrap();
    write_page(
        source.path(),
        "pages/home/page.toml",
        "home",
        Some("sys0=1\r\n\r\npage settings\r\n"),
    );
    write_page(
        source.path(),
        "pages/settings/page.toml",
        "settings",
        Some("page home\n"),
    );
    let path = source.path().join("hmi.toml");
    let mut manifest: ProjectSource = file::read_toml(&path).unwrap();
    manifest.pages.sources = vec![
        "pages/settings/page.toml".into(),
        "pages/home/page.toml".into(),
    ];
    fs::write(&path, file::toml(&manifest).unwrap()).unwrap();
    let prepared = hmi_cli::prepare(source.path()).unwrap();
    let expected = prepared.link(true).unwrap().bytes;
    prepared.save_plan(outputs.path(), true, None).unwrap();
    source.close().unwrap();
    let linked = hmi_linker::from_manifest(&outputs.path().join("link.toml"), true).unwrap();
    assert_eq!(linked.bytes, expected);
    let container = Container::parse(linked.bytes).unwrap();
    let project = Project::parse(&container).unwrap();
    assert_eq!(project.pages[0].1.name, "settings");
    assert_eq!(project.pages[1].1.name, "home");
    let event = project.pages[1].1.objects[0]
        .events
        .iter()
        .find(|e| e.kind == "load")
        .unwrap();
    assert_eq!(
        event.lines,
        [b"sys0=1".to_vec(), vec![], b"page settings".to_vec()]
    );
}

#[test]
fn invalid_inputs_and_werror_keep_existing_output() {
    let tmp = tempfile::tempdir().unwrap();
    hmi_cli::init(tmp.path(), &config("TJC4827X543_011", 0)).unwrap();
    let output = tmp.path().join("out.HMI");
    fs::write(&output, b"previous output").unwrap();
    fs::write(tmp.path().join("program.s"), "page 99\n").unwrap();
    assert_eq!(
        hmi_cli::build(tmp.path(), Some(&output), false)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "script_page"
    );
    assert_eq!(fs::read(&output).unwrap(), b"previous output");
    fs::write(tmp.path().join("program.s"), "int sys0=0\npage sys0\n").unwrap();
    assert!(hmi_cli::build(tmp.path(), Some(&output), true).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"previous output");
    let linked = hmi_cli::build(tmp.path(), Some(&output), false).unwrap().1;
    assert_eq!(linked.diagnostics[0].code, "script_page_target");
    let original = fs::read(tmp.path().join("program.s")).unwrap();
    assert!(hmi_cli::build(tmp.path(), Some(&tmp.path().join("program.s")), false).is_err());
    assert_eq!(fs::read(tmp.path().join("program.s")).unwrap(), original);
}

#[test]
fn duplicate_names_unsupported_properties_and_corrupt_targets_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    hmi_cli::init(tmp.path(), &config("TJC4827X543_011", 0)).unwrap();
    let path = tmp.path().join("hmi.toml");
    let mut manifest: ProjectSource = file::read_toml(&path).unwrap();
    write_page(tmp.path(), "pages/another/page.toml", "page0", None);
    manifest
        .pages
        .sources
        .push("pages/another/page.toml".into());
    fs::write(&path, file::toml(&manifest).unwrap()).unwrap();
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .unwrap()
            .link(false)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "page_name"
    );
    let page_path = tmp.path().join("pages/page0/page.toml");
    let mut page: PageSource = file::read_toml(&page_path).unwrap();
    page.properties
        .insert("bco".into(), toml::Value::Integer(65536));
    fs::write(&page_path, file::toml(&page).unwrap()).unwrap();
    assert!(hmi_cli::prepare(tmp.path()).is_err());
    page.properties.clear();
    fs::write(&page_path, file::toml(&page).unwrap()).unwrap();
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let artifact = prepared.program.object.encode().unwrap();
    let mut corrupt = artifact;
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(hmi_artifact::CompiledObject::decode(&corrupt).is_err());
}

#[test]
fn cli_json_check_has_no_build_side_effects_and_init_preserves_existing_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_hmi"))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        (output.status.code().unwrap(), json)
    };
    assert_eq!(
        run(&[
            "--json",
            "init",
            root.to_str().unwrap(),
            "--target",
            "TJC4827X543_011"
        ])
        .0,
        0
    );
    assert_eq!(run(&["check", root.to_str().unwrap(), "--json"]).0, 0);
    assert!(!root.join(".build").exists());
    let before = fs::read(root.join("hmi.toml")).unwrap();
    assert_eq!(
        run(&[
            "init",
            root.to_str().unwrap(),
            "--target",
            "TJC4827X543_011",
            "--json"
        ])
        .0,
        1
    );
    assert_eq!(fs::read(root.join("hmi.toml")).unwrap(), before);
    assert_eq!(
        run(&[
            "target",
            "export",
            "unknown",
            "-o",
            tmp.path().join("target.json").to_str().unwrap(),
            "--json"
        ])
        .0,
        1
    );
    assert_eq!(run(&["--json", "not-a-command"]).0, 2);
}
