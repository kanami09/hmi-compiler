use hmi_core::{
    dependency::RuntimeFile,
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, project::Project};
use std::{collections::BTreeMap, fs, path::Path};
mod zi_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/zi.rs"
    ));
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
    let path = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
    config.assets.items.push(AssetSource {
        key: "ui".into(),
        source: "assets/ui.asset.toml".into(),
    });
    fs::write(path, file::toml(&config).unwrap()).unwrap();
    tmp
}

fn record(name: &str, path: &str) -> ObjectFile {
    ObjectFile {
        name: name.into(),
        type_name: "data_record".into(),
        properties: BTreeMap::from([
            ("x".into(), 20.into()),
            ("y".into(), 20.into()),
            ("w".into(), 440.into()),
            ("h".into(), 232.into()),
            (
                "font".into(),
                toml::Value::Table(toml::map::Map::from_iter([("ref".into(), "ui".into())])),
            ),
            ("path".into(), path.into()),
            ("lenth".into(), 32.into()),
            ("maxval".into(), 16.into()),
            ("dez".into(), 2.into()),
            ("format".into(), "200^200".into()),
            ("dir".into(), "T^V".into()),
        ]),
        events: BTreeMap::new(),
    }
}

fn save(root: &Path, name: &str, objects: Vec<ObjectFile>, script: Option<&str>) {
    let directory = root.join("pages").join(name);
    fs::create_dir_all(&directory).unwrap();
    let mut p = hmi_page_compiler::empty_page(name);
    p.objects = objects;
    if let Some(script) = script {
        p.events.insert("load".into(), "load.s".into());
        fs::write(directory.join("load.s"), script).unwrap();
    }
    fs::write(directory.join("page.toml"), file::toml(&p).unwrap()).unwrap();
}

fn page_sources(root: &Path, names: &[&str]) {
    let path = root.join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
    config.pages.sources = names
        .iter()
        .map(|name| format!("pages/{name}/page.toml"))
        .collect();
    fs::write(path, file::toml(&config).unwrap()).unwrap();
}

#[test]
fn native_vectors_preserve_structure_caches_and_method_dispatch() {
    let tmp = setup();
    page_sources(tmp.path(), &["p_record"]);
    for (order, expected) in [
        (
            0,
            "a063618ed7456b348146f5d88e99e3555d24c10ac193a746f4c669d29b4e3887",
        ),
        (
            1,
            "91c48133b4c324e2aa955d56c92833e856d28dbb43d4171ea7e2cc1ea85268ba",
        ),
    ] {
        let mut o = record("data0", "sd0/rev24_a.data");
        o.properties.insert("order".into(), order.into());
        save(tmp.path(), "p_record", vec![o], None);
        let p = hmi_cli::prepare(tmp.path()).unwrap();
        p.check(true).unwrap();
        assert!(!tmp.path().join(".build").exists());
        let linked = p.link(true).unwrap();
        let container = Container::parse(linked.bytes).unwrap();
        let parsed = Project::parse(&container).unwrap();
        let page = &parsed.pages[0].1;
        assert_eq!(hmi_core::sha256(&page.raw), expected);
        assert_eq!(page.objects[1].attributes.len(), 72);
        assert_eq!(page.objects[1].integer("vscope"), Some(1));
        assert_eq!(page.objects[1].integer("val"), Some(u32::MAX));
        assert_eq!(page.objects[1].integer("qty"), Some(0));
        assert_eq!(page.objects[1].integer("molloc"), Some(10752));
        assert_eq!(page.objects[1].integer("up"), Some(613));
        assert!(!container.entries.iter().any(|e| e.name.ends_with(".data")));
        let requirement = &linked.map["runtime_files"][0];
        assert_eq!(requirement["path"], "sd0/rev24_a.data");
        assert_eq!(requirement["generated"], false);
        assert_eq!(
            requirement["structure"],
            serde_json::json!({"record_bytes":32,"max_records":16,"columns":2})
        );
    }
}

#[test]
fn dependencies_are_self_contained_and_tampering_is_rejected() {
    let tmp = setup();
    save(
        tmp.path(),
        "page0",
        vec![record("data0", "sd0/missing-on-host.data")],
        None,
    );
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let expected = prepared.link(true).unwrap();
    let moved = tempfile::tempdir().unwrap();
    prepared.save_plan(moved.path(), true, None).unwrap();
    let object = hmi_artifact::CompiledObject::read(&moved.path().join("obj/p00000.hmo")).unwrap();
    assert_eq!(object.metadata.runtime_dependencies.len(), 1);
    for variant in 0..4 {
        let mut changed = object.clone();
        match variant {
            0 => changed.metadata.runtime_dependencies.clear(),
            1 => changed.metadata.runtime_dependencies[0].object = 0,
            2 => {
                let RuntimeFile::DataRecord { path, .. } =
                    &mut changed.metadata.runtime_dependencies[0].file;
                *path = "sd0/forged.data".into();
            }
            _ => {
                let RuntimeFile::DataRecord { structure, .. } =
                    &mut changed.metadata.runtime_dependencies[0].file;
                structure.record_bytes = 31;
            }
        }
        assert_eq!(
            changed.encode().unwrap_err().as_diagnostic().unwrap().code,
            "runtime_dependency"
        );
    }
    tmp.close().unwrap();
    let actual = hmi_linker::from_manifest(&moved.path().join("link.toml"), true).unwrap();
    assert_eq!(actual.bytes, expected.bytes);
    assert_eq!(actual.map["runtime_files"], expected.map["runtime_files"]);
    assert!(!moved.path().join("sd0").exists());
}

#[test]
fn file_users_and_fonts_follow_current_object_and_asset_order() {
    let tmp = setup();
    let path = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
    config.assets.items.insert(
        0,
        AssetSource {
            key: "other".into(),
            source: "assets/ui.asset.toml".into(),
        },
    );
    fs::write(path, file::toml(&config).unwrap()).unwrap();
    let a = record("a", "sd0/a.data");
    let b = record("b", "sd0/b.data");
    for objects in [vec![a.clone(), b.clone()], vec![b, a]] {
        let first = objects[0].name.clone();
        save(tmp.path(), "page0", objects, None);
        let linked = hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
        let parsed = Project::parse(&Container::parse(linked.bytes).unwrap()).unwrap();
        assert_eq!(parsed.pages[0].1.objects[1].integer("font"), Some(1));
        assert_eq!(linked.map["runtime_files"][0]["users"][0]["object"], first);
        assert_eq!(linked.map["runtime_files"][0]["users"][0]["object_id"], 1);
    }
}

#[test]
fn source_validation_checks_utf8_capacity_and_preserves_old_output() {
    let tmp = setup();
    let output = tmp.path().join("old.HMI");
    fs::write(&output, b"previous").unwrap();
    for (key, value) in [
        ("vscope", 0),
        ("sta", 0),
        ("lenth", 256),
        ("lenth", -1),
        ("maxval", -1),
        ("dez", 0),
        ("dez", 13),
        ("mode", 2),
        ("order", 2),
        ("hig", 0),
        ("ch", 33),
        ("qty", 0),
        ("maxval_x", 0),
        ("init", 0),
        ("insert", 357),
    ] {
        let mut o = record("data0", "sd0/test.data");
        o.properties.insert(key.into(), value.into());
        save(tmp.path(), "page0", vec![o], None);
        assert!(
            hmi_cli::build(tmp.path(), Some(&output), true).is_err(),
            "{key}={value}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }
    for (key, text, expected) in [
        ("path", "", "runtime_file_path"),
        ("path", &"测".repeat(86), "text_capacity"),
        ("dir", &"测".repeat(86), "text_capacity"),
        ("format", &"1".repeat(257), "text_capacity"),
    ] {
        let mut o = record("data0", "sd0/test.data");
        o.properties.insert(key.into(), text.into());
        save(tmp.path(), "page0", vec![o], None);
        assert_eq!(
            hmi_cli::prepare(tmp.path())
                .err()
                .unwrap()
                .as_diagnostic()
                .unwrap()
                .code,
            expected
        );
    }
    // 不按字段数量猜测表头文字，不根据选中 ID 生成 txt；这些是运行时行为。
    let mut o = record("data0", "ram/test.data");
    o.properties.insert("format".into(), "".into());
    o.properties.insert("dir".into(), "".into());
    o.properties.insert("val".into(), 7.into());
    save(tmp.path(), "page0", vec![o], None);
    let linked = hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
    let parsed = Project::parse(&Container::parse(linked.bytes).unwrap()).unwrap();
    assert!(
        parsed.pages[0].1.objects[1]
            .attribute("txt")
            .unwrap()
            .value
            .is_empty()
    );
}

#[test]
fn structural_fields_are_runtime_readonly_and_global_access_remains_valid() {
    let tmp = setup();
    for key in [
        "lenth", "maxval", "dez", "qty", "txt", "maxval_y", "maxval_x", "clear",
    ] {
        save(
            tmp.path(),
            "page0",
            vec![record("data0", "sd0/test.data")],
            Some(&format!("data0.{key}=0")),
        );
        assert_eq!(
            hmi_cli::prepare(tmp.path())
                .unwrap()
                .check(false)
                .unwrap_err()
                .as_diagnostic()
                .unwrap()
                .code,
            "script_readonly",
            "{key}"
        );
    }
    save(
        tmp.path(),
        "page0",
        vec![record("data0", "sd0/test.data")],
        None,
    );
    save(
        tmp.path(),
        "other",
        vec![],
        Some(
            "sys0=page0.data0.qty\npage0.data0.val=0\npage0.data0.val_y=0\npage0.data0.path=\"sd0/next.data\"\npage0.data0.clear()\n",
        ),
    );
    page_sources(tmp.path(), &["page0", "other"]);
    hmi_cli::prepare(tmp.path()).unwrap().check(true).unwrap();
}

#[test]
fn project_count_and_file_conflicts_are_checked_across_pages() {
    let tmp = setup();
    for (name, start) in [("page0", 0), ("other", 4)] {
        save(
            tmp.path(),
            name,
            (start..start + 4)
                .map(|n| record(&format!("data{n}"), &format!("sd0/{n}.data")))
                .collect(),
            None,
        );
    }
    page_sources(tmp.path(), &["page0", "other"]);
    hmi_cli::prepare(tmp.path()).unwrap().check(true).unwrap();
    save(
        tmp.path(),
        "third",
        vec![record("data8", "sd0/8.data")],
        None,
    );
    page_sources(tmp.path(), &["page0", "other", "third"]);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .unwrap()
            .check(false)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "control_count"
    );
    save(
        tmp.path(),
        "page0",
        (0..9)
            .map(|n| record(&format!("data{n}"), &format!("sd0/{n}.data")))
            .collect(),
        None,
    );
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "control_count"
    );

    save(
        tmp.path(),
        "page0",
        vec![record("data0", "sd0/shared.data")],
        None,
    );
    let mut second = record("data1", "SD0/SHARED.data");
    second.properties.insert("mode".into(), 0.into());
    save(tmp.path(), "other", vec![second.clone()], None);
    page_sources(tmp.path(), &["page0", "other"]);
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    let warnings = p.check(false).unwrap();
    assert_eq!(warnings[0].code, "runtime_file_shared");
    assert_eq!(
        p.check(true).unwrap_err().as_diagnostic().unwrap().code,
        "runtime_file_shared"
    );
    let linked = p.link(false).unwrap();
    assert_eq!(linked.map["runtime_files"].as_array().unwrap().len(), 1);
    assert_eq!(
        linked.map["runtime_files"][0]["users"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        linked.map["runtime_files"][0]["users"][1]["create_if_missing"],
        false
    );
    second.properties.insert("dez".into(), 3.into());
    save(tmp.path(), "other", vec![second], None);
    let output = tmp.path().join("old.HMI");
    fs::write(&output, b"previous").unwrap();
    assert_eq!(
        hmi_cli::build(tmp.path(), Some(&output), false)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "runtime_file_conflict"
    );
    assert_eq!(fs::read(&output).unwrap(), b"previous");
}

#[test]
fn model_restrictions_and_four_directions_share_the_same_record_recipe() {
    let tmp = setup();
    save(
        tmp.path(),
        "page0",
        vec![record("data0", "sd0/test.data")],
        None,
    );
    let path = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
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
    config.target.profile = "TJC4827X543_011".into();
    for orientation in [0, 90, 180, 270] {
        config.target.orientation = orientation;
        fs::write(&path, file::toml(&config).unwrap()).unwrap();
        hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
    }
}
