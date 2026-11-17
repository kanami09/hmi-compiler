use hmi_core::{
    controls::{self, DefaultValue, Presentation, Storage},
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::{Target, TargetConfig},
};
use hmi_format::{container::Container, project::Project};
use std::{collections::BTreeMap, fs, path::Path};
mod zi {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/zi.rs"
    ));
}
fn target(profile: &str) -> Target {
    Target::resolve(&TargetConfig {
        profile: profile.into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation: 0,
    })
    .unwrap()
}
fn required(definition: &controls::Definition) -> BTreeMap<String, toml::Value> {
    let mut p = BTreeMap::new();
    for property in definition.all_properties() {
        if matches!(property.default, DefaultValue::Required) {
            let value = match property.name {
                "w" | "h" => 200.into(),
                "txt" => "ABC".into(),
                "path" => "sd0/records.data".into(),
                "lenth" => 32.into(),
                "maxval" => 16.into(),
                "dez" => 2.into(),
                _ => match property.storage {
                    Storage::Text | Storage::Lines | Storage::ExternalPath(_) => "".into(),
                    _ => 0.into(),
                },
            };
            p.insert(property.name.into(), value);
        }
    }
    if definition.name == "animation" || definition.name == "video" {
        p.insert("vid".into(), 0.into());
    }
    p
}
#[test]
fn all_visual_recipes_share_ranges_and_preserve_every_other_native_field() {
    let x = target("TJC4827X543_011");
    let k = target("TJC4832K035_011");
    let mut count = 0;
    for definition in controls::DEFINITIONS
        .iter()
        .filter(|d| matches!(d.presentation, Presentation::Control))
    {
        count += 1;
        let mut p = required(definition);
        let original = definition.attributes("o0", &p).unwrap();
        p.extend([
            ("drag".into(), 1.into()),
            ("aph".into(), 64.into()),
            ("effect".into(), 8.into()),
        ]);
        let attrs = definition.attributes("o0", &p).unwrap();
        x.check_control_properties(definition.code, &attrs).unwrap();
        assert!(k.check_control_properties(definition.code, &attrs).is_err());
        for ((name, old), (new_name, new)) in original.iter().zip(&attrs) {
            assert_eq!(name, new_name);
            if !matches!(name.as_str(), "drag" | "aph" | "effect") {
                assert_eq!(old, new, "{}.{}", definition.name, name);
            }
        }
        let decoded = definition.properties_from_record(&attrs).unwrap();
        assert_eq!(decoded["drag"].as_integer(), Some(1));
        assert_eq!(decoded["aph"].as_integer(), Some(64));
        definition.validate_record("o0", &attrs).unwrap();
        for (key, value) in [("drag", 2), ("aph", 128), ("effect", 9)] {
            let mut bad = p.clone();
            bad.insert(key.into(), value.into());
            assert!(definition.attributes("o0", &bad).is_err());
        }
    }
    assert_eq!(count, 24);
}
#[test]
fn hotspot_nonvisual_and_page_opt_in_are_explicit_not_inferred_from_record_slots() {
    for name in [
        "hotspot",
        "variable",
        "timer",
        "touch_cap",
        "file_stream",
        "audio",
    ] {
        let d = controls::by_name(name).unwrap();
        for property in ["drag", "aph", "effect"] {
            assert!(!d.all_properties().any(|p| p.name == property));
        }
    }
    assert!(controls::PAGE.all_properties().any(|p| p.name == "effect"));
    assert!(
        !controls::PAGE
            .all_properties()
            .any(|p| p.name == "aph" || p.name == "drag")
    );
    for profile in hmi_core::target::PROFILES {
        let t = target(profile);
        let p = BTreeMap::from([
            ("w".into(), i64::from(t.canvas.width).into()),
            ("h".into(), i64::from(t.canvas.height).into()),
            ("effect".into(), 1.into()),
        ]);
        let attrs = controls::PAGE.attributes("page0", &p).unwrap();
        assert_eq!(
            t.check_control_properties(121, &attrs).is_ok(),
            *profile == "TJC4827X543_011"
        );
    }
}
fn reference(key: &str) -> toml::Value {
    toml::Value::Table(toml::map::Map::from_iter([("ref".into(), key.into())]))
}
fn save(root: &Path, page_effect: i64, aph: i64) {
    let mut page = hmi_page_compiler::empty_page("page0");
    page.properties.insert("effect".into(), page_effect.into());
    page.objects.push(ObjectFile {
        name: "t0".into(),
        type_name: "text".into(),
        properties: BTreeMap::from([
            ("x".into(), 20.into()),
            ("y".into(), 20.into()),
            ("w".into(), 200.into()),
            ("h".into(), 100.into()),
            ("font".into(), reference("ui")),
            ("txt".into(), "ABC".into()),
            ("drag".into(), 1.into()),
            ("aph".into(), aph.into()),
            ("effect".into(), 2.into()),
        ]),
        events: BTreeMap::new(),
    });
    fs::write(
        root.join("pages/page0/page.toml"),
        file::toml(&page).unwrap(),
    )
    .unwrap();
}
#[test]
fn compilation_linking_and_moved_targets_keep_shared_values_and_final_event_context() {
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
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
    config.assets.items.push(AssetSource {
        key: "ui".into(),
        source: "assets/ui.toml".into(),
    });
    fs::write(&path, file::toml(&config).unwrap()).unwrap();
    save(tmp.path(), 1, 64);
    for orientation in [0, 90, 180, 270] {
        config.target.orientation = orientation;
        fs::write(&path, file::toml(&config).unwrap()).unwrap();
        let p = hmi_cli::prepare(tmp.path()).unwrap();
        p.check(true).unwrap();
        let linked = p.link(true).unwrap();
        let parsed = Project::parse(&Container::parse(linked.bytes).unwrap()).unwrap();
        assert_eq!(parsed.pages[0].1.objects[0].integer("effect"), Some(1));
        assert_eq!(parsed.pages[0].1.objects[1].integer("aph"), Some(64));
        assert!(!tmp.path().join(".build").exists());
    }
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    let expected = p.link(true).unwrap().bytes;
    let moved = tempfile::tempdir().unwrap();
    p.save_plan(moved.path(), true, None).unwrap();
    let previous = tmp.path().join("previous.HMI");
    fs::write(&previous, b"previous").unwrap();
    save(tmp.path(), 1, 128);
    assert!(hmi_cli::build(tmp.path(), Some(&previous), true).is_err());
    assert_eq!(fs::read(&previous).unwrap(), b"previous");
    // 独立目标也不能仅改型号描述就绕过公共属性的系列约束。
    let k = target("TJC4832K035_011");
    let mut program = p.program.object.clone();
    program.metadata.target = k.clone();
    let mut page = p.pages[0].object.clone();
    page.metadata.target = k.clone();
    let raw = hmi_format::page::Object::parse(page.record(0).unwrap())
        .unwrap()
        .rebuild(
            &BTreeMap::from([
                ("w".into(), k.canvas.width.to_le_bytes().to_vec()),
                ("h".into(), k.canvas.height.to_le_bytes().to_vec()),
                ("endx".into(), (k.canvas.width - 1).to_le_bytes().to_vec()),
                ("endy".into(), (k.canvas.height - 1).to_le_bytes().to_vec()),
            ]),
            &BTreeMap::new(),
        )
        .unwrap();
    page.payload[..raw.len()].copy_from_slice(&raw);
    assert_eq!(
        hmi_linker::check(&k, &program, &[page], &[], true)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "control_capability"
    );
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        expected
    );
}
