use hmi_core::{
    asset::ImageSource,
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, page::Page, project::Project};
use std::{collections::BTreeMap, fs, path::Path};
mod zi_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/zi.rs"
    ));
}
fn bmp() -> Vec<u8> {
    let mut b = vec![0; 54 + 64 * 64 * 3];
    let n = b.len() as u32;
    b[..2].copy_from_slice(b"BM");
    b[2..6].copy_from_slice(&n.to_le_bytes());
    b[10..14].copy_from_slice(&54u32.to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());
    b[18..22].copy_from_slice(&64u32.to_le_bytes());
    b[22..26].copy_from_slice(&64u32.to_le_bytes());
    b[26..30].copy_from_slice(&[1, 0, 24, 0]);
    b
}
fn reference(name: &str) -> toml::Value {
    toml::Value::Table(toml::map::Map::from_iter([("ref".into(), name.into())]))
}
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
    fs::write(tmp.path().join("assets/ui.zi"), zi_fixture::font()).unwrap();
    fs::write(
        tmp.path().join("assets/ui.asset.toml"),
        "schema='hmi.asset/1'\nkind='font'\nsource='ui.zi'\n",
    )
    .unwrap();
    let mut p: ProjectSource = file::read_toml(&tmp.path().join("hmi.toml")).unwrap();
    p.assets.items.push(AssetSource {
        key: "ui".into(),
        source: "assets/ui.asset.toml".into(),
    });
    for name in ["folder", "file"] {
        fs::write(tmp.path().join(format!("assets/{name}.bmp")), bmp()).unwrap();
        fs::write(
            tmp.path().join(format!("assets/{name}.asset.toml")),
            file::toml(&ImageSource {
                source: format!("{name}.bmp"),
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
        p.assets.items.push(AssetSource {
            key: name.into(),
            source: format!("assets/{name}.asset.toml"),
        });
    }
    fs::write(tmp.path().join("hmi.toml"), file::toml(&p).unwrap()).unwrap();
    tmp
}
fn browser() -> ObjectFile {
    ObjectFile {
        name: "fbrowser0".into(),
        type_name: "file_browser".into(),
        properties: BTreeMap::from([
            ("x".into(), 20.into()),
            ("y".into(), 20.into()),
            ("w".into(), 440.into()),
            ("h".into(), 232.into()),
            ("font".into(), reference("ui")),
            ("pic1".into(), reference("folder")),
            ("pic2".into(), reference("file")),
            ("filter".into(), "*.txt".into()),
        ]),
        events: BTreeMap::new(),
    }
}
fn stream(name: &str) -> ObjectFile {
    ObjectFile {
        name: name.into(),
        type_name: "file_stream".into(),
        properties: BTreeMap::new(),
        events: BTreeMap::new(),
    }
}
fn save(root: &Path, name: &str, objects: Vec<ObjectFile>, load: Option<&str>) {
    let mut p = hmi_page_compiler::empty_page(name);
    p.objects = objects;
    if let Some(text) = load {
        p.events.insert("load".into(), "load.s".into());
        fs::write(root.join("pages/page0/load.s"), text).unwrap();
    }
    fs::write(root.join("pages/page0/page.toml"), file::toml(&p).unwrap()).unwrap();
}
fn link(root: &Path) -> Page {
    let p = hmi_cli::prepare(root).unwrap();
    p.check(true).unwrap();
    Project::parse(&Container::parse(p.link(true).unwrap().bytes).unwrap())
        .unwrap()
        .pages
        .remove(0)
        .1
}

#[test]
fn four_native_page_vectors_keep_cached_state_and_method_fields() {
    for (kind, variant, expected) in [
        (
            "browser",
            0,
            "01e23fb0c5081ba2da9d28fdf857686ad03707eeea38b7b3d3840b2e09f55807",
        ),
        (
            "browser",
            1,
            "615f130925e85337aeb3b72ed0fa032337a9a1c158ed283d50f0c6bb37298f21",
        ),
        (
            "stream",
            0,
            "3707ba5e76b3de07925a86d329341d4d17b13ffc825cff99a3d9b9468305fea2",
        ),
        (
            "stream",
            1,
            "cc2894ecb564e54c39145962afd6e36aca3e82cfcabe9b058b881fab8de86143",
        ),
    ] {
        let tmp = setup();
        let mut o = if kind == "browser" {
            browser()
        } else {
            stream(if variant == 0 { "fs0" } else { "fs1" })
        };
        if kind == "browser" && variant == 1 {
            o.properties.insert("filter".into(), "*.txt|*.data".into());
        }
        save(
            tmp.path(),
            if kind == "browser" {
                "p_browser"
            } else {
                "p_stream"
            },
            vec![o],
            None,
        );
        let p = link(tmp.path());
        assert_eq!(hmi_core::sha256(&p.raw), expected);
        if kind == "browser" {
            assert_eq!(p.objects[1].integer("val"), Some(u32::MAX));
            assert!(p.objects[1].attribute("txt").unwrap().value.is_empty());
        } else {
            assert!(p.objects[1].events.is_empty());
            assert!(p.objects[1].attribute("x").is_none());
            assert_eq!(p.objects[1].integer("molloc_s"), Some(10752));
            assert_eq!(p.objects[1].integer("read"), Some(869));
        }
    }
}

#[test]
fn icon_references_follow_final_image_ids_and_objects_link_without_inputs() {
    let tmp = setup();
    save(
        tmp.path(),
        "page0",
        vec![browser(), stream("fs0")],
        Some("sys0=fs0.qty\nfs0.val=0\n"),
    );
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    p.check(true).unwrap();
    assert!(!tmp.path().join(".build").exists());
    let first = link(tmp.path());
    assert_eq!(first.objects[1].integer("pic1"), Some(0));
    assert_eq!(first.objects[1].integer("pic2"), Some(1));
    let source = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&source).unwrap();
    config.assets.items.swap(1, 2);
    fs::write(&source, file::toml(&config).unwrap()).unwrap();
    let changed = link(tmp.path());
    assert_eq!(changed.objects[1].integer("pic1"), Some(1));
    assert_eq!(changed.objects[1].integer("pic2"), Some(0));
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    let expected = p.link(true).unwrap().bytes;
    let moved = tempfile::tempdir().unwrap();
    p.save_plan(moved.path(), true, None).unwrap();
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        expected
    );
}

#[test]
fn runtime_directory_is_data_and_source_restrictions_preserve_previous_output() {
    let tmp = setup();
    let output = tmp.path().join("old.HMI");
    fs::write(&output, b"previous").unwrap();
    for (kind, key, value) in [
        ("browser", "psta", 2),
        ("browser", "ch", 33),
        ("browser", "fwarning", 2),
        ("browser", "qty", 0),
        ("browser", "val", 0),
        ("browser", "vvs2_m", 64),
        ("stream", "vscope", 1),
        ("stream", "x", 0),
        ("stream", "val", 0),
        ("stream", "open", 357),
    ] {
        let mut o = if kind == "browser" {
            browser()
        } else {
            stream("fs0")
        };
        o.properties.insert(key.into(), value.into());
        save(tmp.path(), "page0", vec![o], None);
        assert!(
            hmi_cli::build(tmp.path(), Some(&output), true).is_err(),
            "{kind}.{key}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }
    let mut o = browser();
    o.properties
        .insert("dir".into(), "sd0/not-on-build-machine/".into());
    save(tmp.path(), "page0", vec![o.clone()], None);
    hmi_cli::prepare(tmp.path()).unwrap().check(true).unwrap();
    o.properties.insert("dir".into(), "测".repeat(86).into());
    save(tmp.path(), "page0", vec![o.clone()], None);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "text_capacity"
    );
    o = browser();
    o.properties.insert(
        "pic2".into(),
        toml::Value::Table(toml::map::Map::from_iter([("none".into(), true.into())])),
    );
    save(tmp.path(), "page0", vec![o], None);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "resource_reference"
    );
    let mut o = stream("fs0");
    o.events.insert("up".into(), "missing.s".into());
    save(tmp.path(), "page0", vec![o], None);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "event_slot"
    );
}

#[test]
fn runtime_access_rules_distinguish_readonly_compile_only_and_page_local_fields() {
    let tmp = setup();
    for (code, expected) in [
        ("fs0.qty=1", "script_readonly"),
        ("fs0.en=1", "script_readonly"),
        ("fs0.open=1", "script_readonly"),
        ("fbrowser0.qty=1", "script_readonly"),
        ("fbrowser0.txt=\"x\"", "script_readonly"),
        ("sys0=fbrowser0.fwarning", "script_property_access"),
        ("fbrowser0.fwarning=0", "script_property_access"),
    ] {
        save(
            tmp.path(),
            "page0",
            vec![browser(), stream("fs0")],
            Some(code),
        );
        let e = hmi_cli::prepare(tmp.path())
            .unwrap()
            .check(false)
            .unwrap_err();
        assert_eq!(e.as_diagnostic().unwrap().code, expected, "{code}");
    }
    let mut fb = browser();
    fb.properties.insert("vscope".into(), 1.into());
    save(tmp.path(), "page0", vec![fb, stream("fs0")], None);
    fs::create_dir_all(tmp.path().join("pages/other")).unwrap();
    let mut other = hmi_page_compiler::empty_page("other");
    other.events.insert("load".into(), "load.s".into());
    fs::write(
        tmp.path().join("pages/other/page.toml"),
        file::toml(&other).unwrap(),
    )
    .unwrap();
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.pages.sources.push("pages/other/page.toml".into());
    fs::write(&path, file::toml(&p).unwrap()).unwrap();
    for (code, valid) in [
        ("sys0=page0.fbrowser0.qty", true),
        ("prints page0.fbrowser0.txt,0", false),
        ("page0.fs0.val=0", false),
        ("page0.fs0.close()", false),
    ] {
        fs::write(tmp.path().join("pages/other/load.s"), code).unwrap();
        let result = hmi_cli::prepare(tmp.path()).unwrap().check(true);
        if valid {
            result.unwrap();
        } else {
            assert_eq!(
                result.unwrap_err().as_diagnostic().unwrap().code,
                "script_scope"
            );
        }
    }
}

#[test]
fn file_types_obey_model_support_and_x543_directions() {
    let tmp = setup();
    save(tmp.path(), "page0", vec![browser(), stream("fs0")], None);
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    for profile in ["TJC4832K035_011", "TJC4832T035_011", "TJC3224T028_011"] {
        p.target.profile = profile.into();
        fs::write(&path, file::toml(&p).unwrap()).unwrap();
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
    p.target.profile = "TJC4827X543_011".into();
    for orientation in [0, 90, 180, 270] {
        p.target.orientation = orientation;
        fs::write(&path, file::toml(&p).unwrap()).unwrap();
        let page = link(tmp.path());
        assert_eq!(page.objects[2].integer("vscope"), Some(0));
    }
}
