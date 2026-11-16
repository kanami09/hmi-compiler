use hmi_core::{
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, page::Page, project::Project};
use std::{collections::BTreeMap, fs, path::Path};

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
    for (key, w, h) in [("full", 480, 272), ("panel", 160, 160), ("logo", 24, 24)] {
        fs::write(tmp.path().join(format!("assets/{key}.bmp")), bmp(w, h)).unwrap();
        fs::write(
            tmp.path().join(format!("assets/{key}.asset.toml")),
            format!("schema='hmi.asset/1'\nkind='image'\nsource='{key}.bmp'\n"),
        )
        .unwrap();
        p.assets.items.push(AssetSource {
            key: key.into(),
            source: format!("assets/{key}.asset.toml"),
        });
    }
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    tmp
}
fn widget(kind: &str, sta: i64) -> ObjectFile {
    let mut properties = BTreeMap::from([
        ("x".into(), 20.into()),
        ("y".into(), 40.into()),
        ("w".into(), 160.into()),
        ("h".into(), 160.into()),
        ("sta".into(), sta.into()),
        (
            "pic".into(),
            reference(if kind == "qrcode" { "logo" } else { "panel" }),
        ),
    ]);
    match kind {
        "gauge" => {
            properties.insert("picc".into(), reference("full"));
            properties.insert("val".into(), 135.into());
            properties.insert("down".into(), (-10).into());
        }
        "waveform" => {
            properties.insert("picc".into(), reference("full"));
            properties.insert("ch".into(), 2.into());
            properties.insert("dir".into(), 1.into());
            properties.insert("dis".into(), 200.into());
        }
        _ => {
            properties.insert("txt".into(), "测试A".into());
        }
    }
    ObjectFile {
        name: match kind {
            "gauge" => "z0",
            "waveform" => "s0",
            _ => "qr0",
        }
        .into(),
        type_name: kind.into(),
        properties,
        events: BTreeMap::new(),
    }
}
fn save(root: &Path, objects: Vec<ObjectFile>) {
    let mut page = hmi_page_compiler::empty_page("page0");
    page.objects = objects;
    fs::write(
        root.join("pages/page0/page.toml"),
        file::toml(&page).unwrap(),
    )
    .unwrap();
}
fn linked(root: &Path) -> Page {
    let prepared = hmi_cli::prepare(root).unwrap();
    prepared.check(true).unwrap();
    Project::parse(&Container::parse(prepared.link(true).unwrap().bytes).unwrap())
        .unwrap()
        .pages
        .remove(0)
        .1
}

#[test]
fn mode_changes_preserve_hidden_images_shapes_channels_and_qr_text() {
    let tmp = setup();
    for (kind, modes, count) in [
        ("gauge", vec![2, 0, 1, 3], 37),
        ("waveform", vec![2, 0, 1, 3], 40),
        ("qrcode", vec![1, 0], 28),
    ] {
        let mut previous: Option<hmi_format::page::Object> = None;
        for mode in modes {
            save(tmp.path(), vec![widget(kind, mode)]);
            let page = linked(tmp.path());
            let object = &page.objects[1];
            assert_eq!(object.attributes.len(), count);
            assert_eq!(
                object.integer("pic"),
                Some(if kind == "qrcode" { 2 } else { 1 })
            );
            if let Some(old) = previous {
                assert_eq!(
                    old.rebuild(
                        &BTreeMap::from([("sta".into(), vec![mode as u8])]),
                        &BTreeMap::new()
                    )
                    .unwrap(),
                    object.raw,
                    "{kind}/{mode}"
                );
            }
            previous = Some(hmi_format::page::Object::parse(&object.raw).unwrap());
        }
    }
}

#[test]
fn image_modes_and_logo_link_in_all_known_models_and_directions() {
    let tmp = setup();
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    save(
        tmp.path(),
        vec![
            widget("gauge", 0),
            widget("waveform", 2),
            widget("qrcode", 1),
        ],
    );
    for profile in hmi_core::target::PROFILES {
        for orientation in [0, 90, 180, 270] {
            p.target.profile = (*profile).into();
            p.target.orientation = orientation;
            fs::write(&path, file::toml(&p).unwrap()).unwrap();
            let target = hmi_core::target::Target::resolve(&p.target).unwrap();
            fs::write(
                tmp.path().join("assets/full.bmp"),
                bmp(target.canvas.width, target.canvas.height),
            )
            .unwrap();
            let page = linked(tmp.path());
            assert_eq!(page.objects[1].integer("picc"), Some(0));
            assert_eq!(page.objects[2].integer("pic"), Some(1));
            assert_eq!(page.objects[3].integer("pic"), Some(2));
            assert_eq!(
                page.objects[3].attribute("txt").unwrap().value,
                "测试A".as_bytes()
            );
            assert_eq!(page.objects[3].integer("w"), Some(160));
            assert!(!tmp.path().join(".build").exists());
        }
    }
}

#[test]
fn images_and_event_owners_follow_reordering_and_link_without_sources() {
    let tmp = setup();
    let mut objects = vec![
        widget("gauge", 0),
        widget("waveform", 2),
        widget("qrcode", 1),
    ];
    objects[0].events.insert("up".into(), "up.s".into());
    fs::write(tmp.path().join("pages/page0/up.s"), "z0.val=45\n").unwrap();
    objects.reverse();
    save(tmp.path(), objects);
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.assets.items.reverse();
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    let page = linked(tmp.path());
    assert_eq!(page.objects[1].name, "qr0");
    assert_eq!(page.objects[1].integer("pic"), Some(0));
    assert_eq!(page.objects[3].integer("picc"), Some(2));
    assert_eq!(page.objects[3].events[1].lines[0], b"z0.val=45");
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let expected = prepared.link(true).unwrap().bytes;
    let moved = tempfile::tempdir().unwrap();
    prepared.save_plan(moved.path(), true, None).unwrap();
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        expected
    );
}

#[test]
fn invalid_active_images_and_crop_geometry_preserve_previous_output() {
    let tmp = setup();
    let output = tmp.path().join("previous.HMI");
    fs::write(&output, b"previous").unwrap();
    for (kind, mode, field, value) in [
        ("gauge", 2, "pic", None),
        ("waveform", 2, "pic", None),
        ("qrcode", 1, "pic", None),
        ("gauge", 0, "picc", Some(reference("panel"))),
        ("waveform", 0, "x", Some(450.into())),
        ("qrcode", 1, "pic", Some(reference("missing"))),
    ] {
        let mut o = widget(kind, mode);
        if let Some(value) = value {
            o.properties.insert(field.into(), value);
        } else {
            o.properties.remove(field);
        }
        save(tmp.path(), vec![o]);
        assert!(
            hmi_cli::build(tmp.path(), Some(&output), true).is_err(),
            "{kind}.{field}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }
    for kind in ["gauge", "waveform", "qrcode"] {
        let mut o = widget(kind, if kind == "qrcode" { 0 } else { 1 });
        o.properties.remove("pic");
        o.properties.remove("picc");
        save(tmp.path(), vec![o]);
        assert_eq!(linked(tmp.path()).objects[1].integer("pic"), Some(65535));
    }
}

#[test]
fn transparent_ranges_use_known_series_exceptions_and_defensive_link_checks() {
    let tmp = setup();
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    for profile in hmi_core::target::PROFILES {
        p.target.profile = (*profile).into();
        fs::write(&path, file::toml(&p).unwrap()).unwrap();
        save(tmp.path(), vec![widget("gauge", 3)]);
        if *profile == "TJC4827X543_011" {
            linked(tmp.path());
        } else {
            let e = hmi_cli::prepare(tmp.path()).err().unwrap();
            assert_eq!(e.as_diagnostic().unwrap().code, "control_capability");
        }
        save(tmp.path(), vec![widget("waveform", 3)]);
        assert_eq!(linked(tmp.path()).objects[1].integer("sta"), Some(3));
    }
    p.target.profile = "TJC4832K035_011".into();
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    let mut o = widget("gauge", 1);
    o.properties.remove("pic");
    o.properties.remove("picc");
    save(tmp.path(), vec![o]);
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let original = &prepared.pages[0].object;
    let raw = hmi_format::page::Object::parse(original.record(1).unwrap())
        .unwrap()
        .rebuild(&BTreeMap::from([("sta".into(), vec![3])]), &BTreeMap::new())
        .unwrap();
    let forged = hmi_artifact::CompiledObject::page(
        "page0".into(),
        &prepared.target,
        vec![],
        vec![
            (121, "page0".into(), original.record(0).unwrap().to_vec()),
            (122, "z0".into(), raw),
        ],
        original
            .metadata
            .relocations
            .iter()
            .filter(|r| r.kind != hmi_artifact::RelocationKind::ObjectId)
            .cloned()
            .collect(),
    )
    .unwrap();
    assert_eq!(
        hmi_linker::check(
            &prepared.target,
            &prepared.program.object,
            &[forged],
            &[],
            true
        )
        .unwrap_err()
        .as_diagnostic()
        .unwrap()
        .code,
        "control_capability"
    );
}
