use hmi_core::{
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, project::Project};
use image::ImageEncoder;
use std::{collections::BTreeMap, fs, path::Path};
mod zi {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/zi.rs"
    ));
}
fn reference(key: &str) -> toml::Value {
    toml::Value::Table(toml::map::Map::from_iter([("ref".into(), key.into())]))
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
    fs::write(tmp.path().join("assets/ui.zi"), zi::font()).unwrap();
    fs::write(
        tmp.path().join("assets/ui.toml"),
        "schema='hmi.asset/1'\nkind='font'\nsource='ui.zi'\n",
    )
    .unwrap();
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.assets.items.push(AssetSource {
        key: "ui".into(),
        source: "assets/ui.toml".into(),
    });
    for (name, color) in [("red", [230, 20, 40, 255]), ("green", [20, 210, 40, 255])] {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                &color.repeat(16 * 16),
                16,
                16,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        fs::write(tmp.path().join(format!("assets/{name}.png")), bytes).unwrap();
        fs::write(
            tmp.path().join(format!("assets/{name}.toml")),
            format!("schema='hmi.asset/1'\nkind='image'\nsource='{name}.png'\n"),
        )
        .unwrap();
        p.assets.items.push(AssetSource {
            key: name.into(),
            source: format!("assets/{name}.toml"),
        });
    }
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    tmp
}
fn browser() -> ObjectFile {
    ObjectFile {
        name: "fb0".into(),
        type_name: "file_browser".into(),
        properties: BTreeMap::from([
            ("x".into(), 20.into()),
            ("y".into(), 20.into()),
            ("w".into(), 200.into()),
            ("h".into(), 200.into()),
            ("font".into(), reference("ui")),
        ]),
        events: BTreeMap::new(),
    }
}
fn mapping() -> toml::Value {
    toml::Value::Array(
        [("*.txt", "red"), ("*.log", "green")]
            .map(|(pattern, key)| {
                toml::Value::Table(toml::map::Map::from_iter([
                    ("pattern".into(), pattern.into()),
                    ("pic".into(), reference(key)),
                ]))
            })
            .to_vec(),
    )
}
fn save(root: &Path, name: &str, object: ObjectFile) {
    let dir = root.join("pages").join(name);
    fs::create_dir_all(&dir).unwrap();
    let mut p = hmi_page_compiler::empty_page(name);
    p.objects = vec![object];
    fs::write(dir.join("page.toml"), file::toml(&p).unwrap()).unwrap();
}
fn sources(root: &Path, names: &[&str]) {
    let path = root.join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.pages.sources = names
        .iter()
        .map(|n| format!("pages/{n}/page.toml"))
        .collect();
    fs::write(path, file::toml(&p).unwrap()).unwrap();
}
fn parsed(bytes: Vec<u8>) -> Project {
    Project::parse(&Container::parse(bytes).unwrap()).unwrap()
}

#[test]
fn implicit_icons_are_shared_appended_after_explicit_assets_and_check_does_not_write() {
    let tmp = setup();
    sources(tmp.path(), &["one", "two"]);
    save(tmp.path(), "one", browser());
    save(tmp.path(), "two", browser());
    let path = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
    for orientation in [0, 90, 180, 270] {
        config.target.orientation = orientation;
        fs::write(&path, file::toml(&config).unwrap()).unwrap();
        let p = hmi_cli::prepare(tmp.path()).unwrap();
        p.check(true).unwrap();
        assert!(!tmp.path().join(".build").exists());
        assert_eq!(p.assets.len(), 5);
        assert_eq!(p.assets[3].key, "__hmi_browser_folder");
        assert_eq!(p.assets[4].key, "__hmi_browser_file");
        let linked = p.link(true).unwrap();
        let project = parsed(linked.bytes);
        for (_, page) in project.pages {
            assert_eq!(page.objects[1].integer("pic1"), Some(2));
            assert_eq!(page.objects[1].integer("pic2"), Some(3));
        }
        assert_eq!(
            linked.map["assets"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|a| a["implicit"] == true)
                .count(),
            2
        );
        let plan = tempfile::tempdir().unwrap();
        p.save_plan(plan.path(), false, None).unwrap();
        let jobs: serde_json::Value =
            serde_json::from_slice(&fs::read(plan.path().join("jobs.json")).unwrap()).unwrap();
        let builtins = jobs["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|j| j["command"] == "hmi-media" && j["args"][0] == "builtin")
            .collect::<Vec<_>>();
        assert_eq!(builtins.len(), 2);
        assert_eq!(builtins[0]["args"][1], "browser-folder");
        assert_eq!(builtins[1]["args"][1], "browser-file");
    }
}

#[test]
fn symbolic_maps_rebind_after_reordering_while_numeric_maps_keep_current_ids() {
    let tmp = setup();
    sources(tmp.path(), &["typed", "numeric"]);
    let mut o = browser();
    o.properties.insert("vvs2".into(), mapping());
    save(tmp.path(), "typed", o);
    let mut o = browser();
    o.properties.insert("vvs2".into(), "*.txt/0*.log/1".into());
    o.properties.insert("pic1".into(), reference("red"));
    save(tmp.path(), "numeric", o);
    let before = parsed(
        hmi_cli::prepare(tmp.path())
            .unwrap()
            .link(true)
            .unwrap()
            .bytes,
    );
    assert_eq!(
        before.pages[0].1.objects[1]
            .attribute("vvs2")
            .unwrap()
            .value,
        b"*.txt/0*.log/1"
    );
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.assets.items.reverse();
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    let after = parsed(
        hmi_cli::prepare(tmp.path())
            .unwrap()
            .link(true)
            .unwrap()
            .bytes,
    );
    assert_eq!(
        after.pages[0].1.objects[1].attribute("vvs2").unwrap().value,
        b"*.txt/1*.log/0"
    );
    assert_eq!(
        after.pages[1].1.objects[1].attribute("vvs2").unwrap().value,
        b"*.txt/0*.log/1"
    );
    assert_eq!(after.pages[1].1.objects[1].integer("pic1"), Some(1));
}

#[test]
fn explicit_icons_suppress_defaults_and_reserved_symbols_cannot_be_user_assets() {
    let tmp = setup();
    let mut o = browser();
    o.properties.insert("pic1".into(), reference("red"));
    o.properties.insert("pic2".into(), reference("green"));
    save(tmp.path(), "page0", o.clone());
    assert_eq!(hmi_cli::prepare(tmp.path()).unwrap().assets.len(), 3);
    o.properties.remove("pic2");
    save(tmp.path(), "page0", o.clone());
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    assert_eq!(p.assets.len(), 4);
    assert_eq!(p.assets[3].key, "__hmi_browser_file");
    o.properties
        .insert("pic1".into(), reference("__hmi_browser_folder"));
    save(tmp.path(), "page0", o);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "asset_key"
    );
    save(tmp.path(), "page0", browser());
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.assets.items[1].key = "__hmi_browser_folder".into();
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    assert!(hmi_cli::prepare(tmp.path()).is_err());
}

#[test]
fn malformed_maps_and_final_id_capacity_overflow_preserve_old_output() {
    let tmp = setup();
    let output = tmp.path().join("previous.HMI");
    fs::write(&output, b"previous").unwrap();
    for value in [
        "*.txt/65535",
        "*.txt/0*.TXT/1",
        "*.txt/x",
        "txt/0",
        "*.txt/0 junk",
        "*.txt/999",
    ] {
        let mut o = browser();
        o.properties.insert("vvs2".into(), value.into());
        save(tmp.path(), "page0", o);
        assert!(
            hmi_cli::build(tmp.path(), Some(&output), true).is_err(),
            "{value}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    for n in 0..11 {
        p.assets.items.push(AssetSource {
            key: format!("fill{n}"),
            source: "assets/red.toml".into(),
        });
    }
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    let entries = (0..5)
        .map(|n| {
            toml::Value::Table(toml::map::Map::from_iter([
                ("pattern".into(), format!("*.p{n:07}").into()),
                ("pic".into(), reference("fill10")),
            ]))
        })
        .collect();
    let mut o = browser();
    o.properties
        .insert("vvs2".into(), toml::Value::Array(entries));
    save(tmp.path(), "page0", o);
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    assert_eq!(
        prepared
            .check(true)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "icon_map_capacity"
    );
    assert!(hmi_cli::build(tmp.path(), Some(&output), true).is_err());
    assert_eq!(fs::read(output).unwrap(), b"previous");
}

#[test]
fn independent_targets_keep_map_relocations_and_defaults_without_any_sources() {
    let tmp = setup();
    let mut o = browser();
    o.properties.insert("vvs2".into(), mapping());
    save(tmp.path(), "page0", o);
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    let expected = p.link(true).unwrap().bytes;
    let moved = tempfile::tempdir().unwrap();
    p.save_plan(moved.path(), true, None).unwrap();
    let original =
        hmi_artifact::CompiledObject::read(&moved.path().join("obj/p00000.hmo")).unwrap();
    let mut bad = original.clone();
    bad.metadata
        .relocations
        .retain(|r| r.kind != hmi_artifact::RelocationKind::ImageMap);
    assert!(bad.validate().is_err());
    let mut bad = original;
    let record = bad.metadata.layout.as_ref().unwrap().objects[1].clone();
    let raw = hmi_format::page::Object::parse(bad.record(1).unwrap())
        .unwrap()
        .rebuild(
            &BTreeMap::from([("vvs2".into(), b"*.txt/9*.log/0".to_vec())]),
            &BTreeMap::new(),
        )
        .unwrap();
    bad.payload[record.offset as usize..(record.offset + record.length) as usize]
        .copy_from_slice(&raw);
    bad.write(&moved.path().join("bad.hmo"), &[]).unwrap();
    assert!(
        hmi_linker::check(
            &p.target,
            &p.program.object,
            &[bad],
            &p.assets
                .iter()
                .map(|a| (a.key.clone(), a.probe.info.clone()))
                .collect::<Vec<_>>(),
            true
        )
        .is_err()
    );
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        expected
    );
}
