use hmi_core::{
    asset::{Delivery, ImageSource},
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, project::Project};
use image::ImageEncoder;
use std::{collections::BTreeMap, fs, path::Path};

fn setup() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    hmi_cli::init(
        tmp.path(),
        &TargetConfig {
            profile: "TJC4827X543_011".into(),
            format: "ver21234".into(),
            encoding: "utf-8".into(),
            orientation: 0,
        },
    )
    .unwrap();
    fs::create_dir(tmp.path().join("assets")).unwrap();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &[220, 80, 30, 128].repeat(64 * 48),
            64,
            48,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    fs::write(tmp.path().join("assets/input.png"), png).unwrap();
    asset(tmp.path(), "logo", "sd0/pictures/logo.xi");
    tmp
}
fn asset(root: &Path, key: &str, path: &str) {
    let c = ImageSource {
        source: "input.png".into(),
        delivery: Delivery {
            mode: "external".into(),
            path: Some(path.into()),
        },
        ..Default::default()
    };
    fs::write(
        root.join(format!("assets/{key}.asset.toml")),
        file::toml(&c).unwrap(),
    )
    .unwrap();
    let p = root.join("hmi.toml");
    let mut project: ProjectSource = file::read_toml(&p).unwrap();
    project.assets.items.push(AssetSource {
        key: key.into(),
        source: format!("assets/{key}.asset.toml"),
    });
    fs::write(p, file::toml(&project).unwrap()).unwrap();
}
fn object(name: &str, path: toml::Value) -> ObjectFile {
    ObjectFile {
        original_record: None,
        name: name.into(),
        type_name: "external_picture".into(),
        properties: BTreeMap::from([
            ("x".into(), 20.into()),
            ("y".into(), 20.into()),
            ("w".into(), 320.into()),
            ("h".into(), 160.into()),
            ("path".into(), path),
        ]),
        events: BTreeMap::new(),
    }
}
fn reference(key: &str) -> toml::Value {
    toml::Value::Table(toml::map::Map::from_iter([("ref".into(), key.into())]))
}
fn save(root: &Path, name: &str, objects: Vec<ObjectFile>) {
    let mut p = hmi_page_compiler::empty_page(name);
    p.objects = objects;
    fs::write(root.join("pages/page0/page.toml"), file::toml(&p).unwrap()).unwrap();
}

#[test]
fn three_literal_path_pages_match_native_vectors_without_embedding_images() {
    let tmp = setup();
    let p = tmp.path().join("hmi.toml");
    let mut c: ProjectSource = file::read_toml(&p).unwrap();
    c.assets.items.clear();
    fs::write(p, file::toml(&c).unwrap()).unwrap();
    for (path, sha) in [
        (
            "sd0/rev25_a.jpg",
            "5d80284d02d164fd2a2b5300a79160ae01926042b08f7a9ee8a078aaaa4b130c",
        ),
        (
            "sd0/rev25_b.jpg",
            "68390693e1755e8c621a5c110d7f1c3adc99e9a72b4ebf9d005be8fa1638afd2",
        ),
        (
            "sd0/rev25_c.xi",
            "b0231c02a29deaef1b7adc443c055264be64e373970ad124d9c2052c4d4ef84a",
        ),
    ] {
        save(tmp.path(), "page0", vec![object("exp0", path.into())]);
        let p = hmi_cli::prepare(tmp.path()).unwrap();
        p.check(true).unwrap();
        let linked = p.link(true).unwrap();
        let container = Container::parse(linked.bytes).unwrap();
        let project = Project::parse(&container).unwrap();
        assert_eq!(hmi_core::sha256(&project.pages[0].1.raw), sha);
        assert_eq!(container.entries.len(), 3);
        assert!(linked.external_files.is_empty());
        assert_eq!(linked.map["runtime_files"][0]["generated"], false);
    }
}

#[test]
fn symbolic_paths_and_external_files_link_without_sources_and_publish_equally() {
    let tmp = setup();
    save(
        tmp.path(),
        "page0",
        vec![
            object("exp0", reference("logo")),
            object("exp1", reference("logo")),
        ],
    );
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    p.check(true).unwrap();
    assert!(!tmp.path().join(".build").exists());
    let expected = p.link(true).unwrap();
    let container = Container::parse(expected.bytes.clone()).unwrap();
    assert_eq!(container.entries.len(), 3);
    assert_eq!(expected.map["assets"][0]["id"], serde_json::Value::Null);
    assert_eq!(
        expected.map["runtime_files"][0]["users"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(expected.map["runtime_files"][0]["generated"], true);
    let moved = tempfile::tempdir().unwrap();
    p.save_plan(moved.path(), true, None).unwrap();
    tmp.close().unwrap();
    let linked = hmi_linker::from_manifest(&moved.path().join("link.toml"), true).unwrap();
    assert_eq!(linked.bytes, expected.bytes);
    assert_eq!(
        linked.external_files[0].bytes,
        expected.external_files[0].bytes
    );
    let output = moved.path().join("release/panel.HMI");
    let inputs = linked
        .inputs
        .iter()
        .map(|p| p.as_path())
        .collect::<Vec<_>>();
    hmi_linker::publish::Publication::prepare(
        &linked,
        &output,
        Some(&moved.path().join("release/map.json")),
        &inputs,
    )
    .unwrap()
    .write()
    .unwrap();
    assert_eq!(fs::read(output).unwrap(), expected.bytes);
    assert_eq!(
        fs::read(moved.path().join("release/sd0/pictures/logo.xi")).unwrap(),
        expected.external_files[0].bytes
    );
}

#[test]
fn external_assets_do_not_shift_embedded_picture_ids() {
    let tmp = setup();
    let c = ImageSource {
        source: "input.png".into(),
        ..Default::default()
    };
    fs::write(
        tmp.path().join("assets/internal.asset.toml"),
        file::toml(&c).unwrap(),
    )
    .unwrap();
    let path = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
    config.assets.items.push(AssetSource {
        key: "inside".into(),
        source: "assets/internal.asset.toml".into(),
    });
    fs::write(&path, file::toml(&config).unwrap()).unwrap();
    let mut pic = object("p0", reference("logo"));
    pic.type_name = "picture".into();
    pic.properties.remove("path");
    pic.properties.insert("pic".into(), reference("inside"));
    save(
        tmp.path(),
        "page0",
        vec![pic.clone(), object("exp0", reference("logo"))],
    );
    for _ in 0..2 {
        let linked = hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
        let c = Container::parse(linked.bytes).unwrap();
        let p = Project::parse(&c).unwrap();
        assert_eq!(p.pages[0].1.objects[1].integer("pic"), Some(0));
        assert!(c.entries.iter().any(|e| e.name == "0.i"));
        assert!(!c.entries.iter().any(|e| e.name == "1.i"));
        config.assets.items.reverse();
        fs::write(&path, file::toml(&config).unwrap()).unwrap();
    }
    pic.properties.insert("pic".into(), reference("logo"));
    save(tmp.path(), "page0", vec![pic]);
    assert!(hmi_cli::prepare(tmp.path()).unwrap().check(false).is_err());
    save(
        tmp.path(),
        "page0",
        vec![object("exp0", reference("inside"))],
    );
    assert!(hmi_cli::prepare(tmp.path()).unwrap().check(false).is_err());
}

#[test]
fn invalid_paths_references_and_conflicts_preserve_existing_outputs() {
    let tmp = setup();
    let out = tmp.path().join("release/old.HMI");
    fs::create_dir_all(out.parent().unwrap()).unwrap();
    fs::write(&out, b"previous").unwrap();
    for path in [
        "sd0/../outside.xi",
        "sd0/CON.xi",
        "sd0/x:stream.xi",
        "sd0//x.xi",
        "sd0/a\\b.xi",
    ] {
        let c = ImageSource {
            source: "input.png".into(),
            delivery: Delivery {
                mode: "external".into(),
                path: Some(path.into()),
            },
            ..Default::default()
        };
        fs::write(
            tmp.path().join("assets/logo.asset.toml"),
            file::toml(&c).unwrap(),
        )
        .unwrap();
        assert!(hmi_cli::build(tmp.path(), Some(&out), false).is_err());
        assert_eq!(fs::read(&out).unwrap(), b"previous");
    }
    let c = ImageSource {
        source: "input.png".into(),
        delivery: Delivery {
            mode: "external".into(),
            path: Some("sd0/logo.xi".into()),
        },
        ..Default::default()
    };
    fs::write(
        tmp.path().join("assets/logo.asset.toml"),
        file::toml(&c).unwrap(),
    )
    .unwrap();
    for path in [
        "".into(),
        "测".repeat(86).into(),
        toml::Value::Table(toml::map::Map::from_iter([("id".into(), 0.into())])),
    ] {
        save(tmp.path(), "page0", vec![object("exp0", path)]);
        assert!(hmi_cli::build(tmp.path(), Some(&out), false).is_err());
    }
    save(tmp.path(), "page0", vec![object("exp0", reference("logo"))]);
    asset(tmp.path(), "duplicate", "sd0/LOGO.XI");
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .unwrap()
            .check(false)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "deployment_conflict"
    );
    assert_eq!(fs::read(&out).unwrap(), b"previous");
}

#[test]
fn publication_checks_all_paths_before_writing_any_file() {
    let tmp = setup();
    save(tmp.path(), "page0", vec![object("exp0", reference("logo"))]);
    let linked = hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
    let root = tmp.path().join("release");
    fs::create_dir_all(root.join("sd0/pictures")).unwrap();
    let output = root.join("old.HMI");
    let xi = root.join("sd0/pictures/logo.xi");
    fs::write(&output, b"previous").unwrap();
    fs::write(&xi, b"existing image").unwrap();
    assert!(
        hmi_linker::publish::Publication::prepare(&linked, &output, None, &[xi.as_path()]).is_err()
    );
    assert!(hmi_linker::publish::Publication::prepare(&linked, &output, Some(&xi), &[]).is_err());
    assert!(
        hmi_linker::publish::Publication::prepare(&linked, &output, Some(&root.join("sd0")), &[])
            .is_err()
    );
    assert_eq!(fs::read(&output).unwrap(), b"previous");
    assert_eq!(fs::read(&xi).unwrap(), b"existing image");
    let mut changed = linked;
    changed.external_files.push(hmi_linker::ExternalOutput {
        path: "sd0/pictures/second.xi".into(),
        bytes: b"second".to_vec(),
    });
    fs::create_dir(root.join("sd0/pictures/second.xi")).unwrap();
    assert!(hmi_linker::publish::Publication::prepare(&changed, &output, None, &[]).is_err());
    assert_eq!(fs::read(&xi).unwrap(), b"existing image");
}

#[test]
fn staging_failure_preserves_all_old_outputs_and_partial_publication_keeps_hmi_last() {
    for fail_during_staging in [true, false] {
        let tmp = setup();
        save(tmp.path(), "page0", vec![object("exp0", reference("logo"))]);
        let mut linked = hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
        linked.external_files.push(hmi_linker::ExternalOutput {
            path: "sd0/next/second.xi".into(),
            bytes: b"second".to_vec(),
        });
        let root = tmp.path().join("release");
        fs::create_dir_all(root.join("sd0/pictures")).unwrap();
        let output = root.join("old.HMI");
        let xi = root.join("sd0/pictures/logo.xi");
        fs::write(&output, b"previous").unwrap();
        fs::write(&xi, b"old image").unwrap();
        let publication =
            hmi_linker::publish::Publication::prepare(&linked, &output, None, &[]).unwrap();
        if fail_during_staging {
            fs::write(root.join("sd0/next"), b"blocks parent").unwrap();
        } else {
            fs::create_dir_all(root.join("sd0/next/second.xi")).unwrap();
        }
        let error = publication.write().unwrap_err();
        assert_eq!(fs::read(&output).unwrap(), b"previous");
        if fail_during_staging {
            assert_eq!(fs::read(&xi).unwrap(), b"old image");
        } else {
            assert_eq!(error.as_diagnostic().unwrap().code, "publish_partial");
            assert!(error.to_string().contains("logo.xi"));
            assert_eq!(fs::read(&xi).unwrap(), linked.external_files[0].bytes);
        }
    }
}

#[test]
fn external_control_obeys_model_rules_and_all_four_orientations() {
    let tmp = setup();
    save(tmp.path(), "page0", vec![object("exp0", reference("logo"))]);
    let path = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
    for orientation in [0, 90, 180, 270] {
        config.target.orientation = orientation;
        fs::write(&path, file::toml(&config).unwrap()).unwrap();
        hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
    }
    config.assets.items.clear();
    save(
        tmp.path(),
        "page0",
        vec![object("exp0", "sd0/available.jpg".into())],
    );
    for profile in ["TJC4832K035_011", "TJC4832T035_011", "TJC3224T028_011"] {
        config.target.profile = profile.into();
        fs::write(&path, file::toml(&config).unwrap()).unwrap();
        assert_eq!(
            hmi_cli::prepare(tmp.path())
                .err()
                .unwrap()
                .as_diagnostic()
                .unwrap()
                .code,
            "control_capability"
        );
    }
}
