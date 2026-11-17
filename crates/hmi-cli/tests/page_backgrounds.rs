use hmi_core::{
    asset::ImageSource,
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, project::Project};
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
fn bmp(w: u16, h: u16) -> Vec<u8> {
    let stride = (usize::from(w) * 3 + 3) & !3;
    let mut b = vec![0; 54 + stride * usize::from(h)];
    let size = b.len() as u32;
    b[..2].copy_from_slice(b"BM");
    b[2..6].copy_from_slice(&size.to_le_bytes());
    b[10..14].copy_from_slice(&54u32.to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());
    b[18..22].copy_from_slice(&i32::from(w).to_le_bytes());
    b[22..26].copy_from_slice(&i32::from(h).to_le_bytes());
    b[26..30].copy_from_slice(&[1, 0, 24, 0]);
    b
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
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    for (key, w, h) in [("background", 480, 272), ("small", 80, 40)] {
        fs::write(tmp.path().join(format!("assets/{key}.bmp")), bmp(w, h)).unwrap();
        fs::write(
            tmp.path().join(format!("assets/{key}.asset.toml")),
            file::toml(&ImageSource {
                source: format!("{key}.bmp"),
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
        p.assets.items.push(AssetSource {
            key: key.into(),
            source: format!("assets/{key}.asset.toml"),
        });
    }
    fs::write(tmp.path().join("assets/ui.zi"), zi::font()).unwrap();
    fs::write(
        tmp.path().join("assets/ui.asset.toml"),
        "schema='hmi.asset/1'\nkind='font'\nsource='ui.zi'\n",
    )
    .unwrap();
    p.assets.items.push(AssetSource {
        key: "ui".into(),
        source: "assets/ui.asset.toml".into(),
    });
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    tmp
}
fn save(root: &Path, properties: BTreeMap<String, toml::Value>, objects: Vec<ObjectFile>) {
    let mut p = hmi_page_compiler::empty_page("page0");
    p.properties = properties;
    p.objects = objects;
    fs::write(root.join("pages/page0/page.toml"), file::toml(&p).unwrap()).unwrap();
}
fn background() -> BTreeMap<String, toml::Value> {
    BTreeMap::from([
        ("sta".into(), 2.into()),
        ("pic".into(), reference("background")),
    ])
}
fn parsed(root: &Path) -> hmi_format::page::Page {
    let linked = hmi_cli::prepare(root).unwrap().link(true).unwrap();
    Project::parse(&Container::parse(linked.bytes).unwrap())
        .unwrap()
        .pages
        .remove(0)
        .1
}

#[test]
fn picture_pages_bind_record_zero_in_all_models_and_directions() {
    let tmp = setup();
    let path = tmp.path().join("hmi.toml");
    let mut c: ProjectSource = file::read_toml(&path).unwrap();
    for profile in hmi_core::target::PROFILES {
        for orientation in [0, 90, 180, 270] {
            c.target.profile = (*profile).into();
            c.target.orientation = orientation;
            fs::write(&path, file::toml(&c).unwrap()).unwrap();
            let t = hmi_core::target::Target::resolve(&c.target).unwrap();
            fs::write(
                tmp.path().join("assets/background.bmp"),
                bmp(t.canvas.width, t.canvas.height),
            )
            .unwrap();
            save(tmp.path(), background(), vec![]);
            let prepared = hmi_cli::prepare(tmp.path()).unwrap();
            prepared.check(true).unwrap();
            assert!(!tmp.path().join(".build").exists());
            let p = parsed(tmp.path());
            assert_eq!(p.objects[0].attributes.len(), 28);
            assert_eq!(p.objects[0].integer("sta"), Some(2));
            assert_eq!(p.objects[0].integer("pic"), Some(0));
            assert_eq!(p.objects[0].integer("w"), Some(u32::from(t.canvas.width)));
            assert_eq!(p.objects[0].id, 0);
        }
    }
}

#[test]
fn page_mode_changes_preserve_picture_and_colour_fields() {
    let tmp = setup();
    let mut previous: Option<hmi_format::page::Object> = None;
    for mode in [2, 1, 0] {
        let mut props = background();
        props.insert("sta".into(), mode.into());
        props.insert("bco".into(), 123.into());
        save(tmp.path(), props, vec![]);
        let p = parsed(tmp.path());
        let o = &p.objects[0];
        assert_eq!(o.integer("pic"), Some(0));
        assert_eq!(o.integer("bco"), Some(123));
        if let Some(old) = previous {
            assert_eq!(
                old.rebuild(
                    &BTreeMap::from([("sta".into(), vec![mode as u8])]),
                    &BTreeMap::new()
                )
                .unwrap(),
                o.raw
            );
        }
        previous = Some(hmi_format::page::Object::parse(&o.raw).unwrap());
    }
    save(tmp.path(), BTreeMap::new(), vec![]);
    assert_eq!(parsed(tmp.path()).objects[0].integer("pic"), Some(65535));
}

#[test]
fn page_and_control_relocations_reorder_and_link_without_sources() {
    let tmp = setup();
    let picture = ObjectFile {
        original_record: None,
        name: "p0".into(),
        type_name: "picture".into(),
        properties: BTreeMap::from([
            ("x".into(), 0.into()),
            ("y".into(), 0.into()),
            ("w".into(), 80.into()),
            ("h".into(), 40.into()),
            ("pic".into(), reference("small")),
        ]),
        events: BTreeMap::new(),
    };
    save(tmp.path(), background(), vec![picture]);
    let path = tmp.path().join("hmi.toml");
    let mut c: ProjectSource = file::read_toml(&path).unwrap();
    c.assets.items.swap(0, 1);
    fs::write(&path, file::toml(&c).unwrap()).unwrap();
    let p = parsed(tmp.path());
    assert_eq!(p.objects[0].integer("pic"), Some(1));
    assert_eq!(p.objects[1].integer("pic"), Some(0));
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let expected = prepared.link(true).unwrap().bytes;
    let moved = tempfile::tempdir().unwrap();
    prepared.save_plan(moved.path(), true, None).unwrap();
    let original =
        hmi_artifact::CompiledObject::read(&moved.path().join("obj/p00000.hmo")).unwrap();
    assert!(
        original
            .metadata
            .relocations
            .iter()
            .any(|r| r.object == 0 && r.kind == hmi_artifact::RelocationKind::ImageId)
    );
    let mut invalid = original;
    invalid
        .metadata
        .relocations
        .iter_mut()
        .find(|r| r.object == 0)
        .unwrap()
        .kind = hmi_artifact::RelocationKind::ObjectId;
    assert!(invalid.validate().is_err());
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        expected
    );
}

#[test]
fn page_background_requirements_and_fixed_geometry_preserve_previous_hmi() {
    let tmp = setup();
    let output = tmp.path().join("previous.HMI");
    fs::write(&output, b"previous").unwrap();
    for props in [
        BTreeMap::from([("sta".into(), 2.into())]),
        BTreeMap::from([("x".into(), 1.into())]),
        BTreeMap::from([("sta".into(), 3.into())]),
        BTreeMap::from([("pic".into(), 0.into())]),
    ] {
        save(tmp.path(), props, vec![]);
        assert!(hmi_cli::build(tmp.path(), Some(&output), false).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }
    let mut props = background();
    props.insert("pic".into(), reference("small"));
    save(tmp.path(), props.clone(), vec![]);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .unwrap()
            .check(true)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "crop_image"
    );
    props.insert("sta".into(), 1.into());
    save(tmp.path(), props, vec![]);
    hmi_cli::prepare(tmp.path()).unwrap().check(true).unwrap();
}

#[test]
fn transparent_controls_are_allowed_on_x_and_rejected_by_known_t_k_rules() {
    let tmp = setup();
    let path = tmp.path().join("hmi.toml");
    let mut c: ProjectSource = file::read_toml(&path).unwrap();
    for kind in [
        "text",
        "button",
        "number",
        "xfloat",
        "dual_state_button",
        "scrolling_text",
    ] {
        let mut props = BTreeMap::from([
            ("x".into(), 10.into()),
            ("y".into(), 20.into()),
            ("w".into(), 220.into()),
            ("h".into(), 100.into()),
            ("sta".into(), 3.into()),
            ("font".into(), reference("ui")),
            ("pic".into(), reference("small")),
            ("picc".into(), reference("small")),
        ]);
        if matches!(kind, "number" | "xfloat") {
            props.insert("val".into(), 1234.into());
        } else {
            props.insert("txt".into(), "ABC".into());
        }
        let o = ObjectFile {
            original_record: None,
            name: "w0".into(),
            type_name: kind.into(),
            properties: props,
            events: BTreeMap::new(),
        };
        for profile in hmi_core::target::PROFILES {
            c.target.profile = (*profile).into();
            fs::write(&path, file::toml(&c).unwrap()).unwrap();
            save(tmp.path(), BTreeMap::new(), vec![o.clone()]);
            let prepared = hmi_cli::prepare(tmp.path());
            if *profile == "TJC4827X543_011" {
                let prepared = prepared.unwrap();
                prepared.check(true).unwrap();
                let p = parsed(tmp.path());
                assert_eq!(p.objects[1].integer("sta"), Some(3));
                assert_eq!(p.objects[1].integer("picc"), Some(1));
            } else {
                let e = prepared.err().unwrap();
                let d = e.as_diagnostic().unwrap();
                assert_eq!(d.code, "control_capability");
                assert_eq!(d.location.property.as_deref(), Some("sta"));
            }
        }
    }
}
