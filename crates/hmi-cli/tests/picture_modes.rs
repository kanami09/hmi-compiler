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
fn none() -> toml::Value {
    toml::Value::Table(toml::map::Map::from_iter([("none".into(), true.into())]))
}
fn bmp(w: u32, h: u32) -> Vec<u8> {
    let stride = (w * 3 + 3) & !3;
    let mut b = vec![0; 54 + (stride * h) as usize];
    let size = b.len() as u32;
    b[..2].copy_from_slice(b"BM");
    b[2..6].copy_from_slice(&size.to_le_bytes());
    b[10..14].copy_from_slice(&54u32.to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());
    b[18..22].copy_from_slice(&w.to_le_bytes());
    b[22..26].copy_from_slice(&h.to_le_bytes());
    b[26..30].copy_from_slice(&[1, 0, 24, 0]);
    b
}
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
    fs::create_dir(tmp.path().join("assets")).unwrap();
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    for (key, w, h) in [
        ("full", 320, 480),
        ("panel", 280, 100),
        ("pressed", 280, 100),
        ("bar", 280, 40),
        ("green", 280, 40),
        ("cursor", 20, 20),
    ] {
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
fn widget(kind: &str, state: u8) -> ObjectFile {
    let text = matches!(kind, "text" | "button");
    let mut properties = BTreeMap::from([
        ("x".into(), (if text { 10 } else { 20 }).into()),
        ("y".into(), (if text { 10 } else { 40 }).into()),
        ("w".into(), 280.into()),
        ("h".into(), (if text { 100 } else { 40 }).into()),
    ]);
    if text {
        properties.insert("font".into(), reference("ui"));
        properties.insert(
            "txt".into(),
            (if kind == "text" { "ABC" } else { "OK" }).into(),
        );
    } else {
        properties.insert("val".into(), 40.into());
    }
    properties.insert(
        "sta".into(),
        (if kind == "progress_bar" {
            if state == 2 { 0 } else { 1 }
        } else {
            [2, 0, 1][state as usize]
        })
        .into(),
    );
    let names = match kind {
        "text" => vec![("pic", "panel")],
        "button" => vec![("pic", "panel"), ("pic2", "pressed")],
        "slider" => vec![("pic", "bar"), ("pic1", "green"), ("pic2", "cursor")],
        _ => vec![("bpic", "bar"), ("ppic", "green")],
    };
    for (field, key) in names {
        properties.insert(field.into(), reference(key));
    }
    if state > 0 && kind != "progress_bar" {
        properties.insert("picc".into(), reference("full"));
        if kind == "button" {
            properties.insert("picc2".into(), reference("full"));
        }
        if kind == "slider" {
            properties.insert("picc1".into(), reference("full"));
        }
    }
    if kind == "slider" {
        properties.insert("psta".into(), (if state == 2 { 0 } else { 1 }).into());
    }
    ObjectFile {
        name: match kind {
            "text" => "t0",
            "button" => "b0",
            "slider" => "h0",
            _ => "j0",
        }
        .into(),
        type_name: kind.into(),
        properties,
        events: BTreeMap::new(),
    }
}
fn save(root: &Path, kind: &str, mut o: ObjectFile, events: bool) {
    let mut page = hmi_page_compiler::empty_page(match kind {
        "text" => "p_text",
        "button" => "p_button",
        "slider" => "p_slider",
        _ => "p_progress",
    });
    if events && kind != "text" {
        for (i, event) in ["down", "up", "slide"].iter().enumerate() {
            if *event == "slide" && kind != "slider" {
                continue;
            }
            let name = format!("{event}.s");
            fs::write(
                root.join("pages/page0").join(&name),
                format!("sys0={}\n", i + 1),
            )
            .unwrap();
            o.events.insert((*event).into(), name);
        }
    }
    page.objects.push(o);
    fs::write(
        root.join("pages/page0/page.toml"),
        file::toml(&page).unwrap(),
    )
    .unwrap();
}

#[test]
fn twelve_native_picture_crop_and_retained_single_color_pages_match() {
    let tmp = setup();
    let vectors = [
        (
            "text",
            [
                "93dfe46d7e33a744190eae4d94f1b797cee30084104bc7cc2d7a0ada436a6ba9",
                "3c50c80746e4c35057a4e365f41bc0b263ff4af30aed4c91911f054be775757c",
                "336e56d8b4e131e6dbf767ce9d0729ef02868feaf81d3161a30f6d6bc73680a2",
            ],
        ),
        (
            "button",
            [
                "22dd22a4a1a888ddf2a28882530c6ca68448f3f2ac5604cddcfea7868f9441a2",
                "b882c002210db0e74280fcef68562e51bddbd9cfea2c0fe6eb40867e34d734ad",
                "274820911c33c1f8cfda12097f03bd99e0ef979e885c0aeb444c8630387852a7",
            ],
        ),
        (
            "slider",
            [
                "44d7c14e95c76296b6d81d482f84498301b30481eb27a672ca726819359853ab",
                "f107a92faa4df979ee0f12b926ca060ea4600c7e80f935251ef110856802e923",
                "1e8f7c756538cdc909dc7924d7ed7fb0d92b324bb6534f0ccdd0e9c71b567ed1",
            ],
        ),
        (
            "progress_bar",
            [
                "89b33dbbb4ab15b01af1bd351832f61e606ad238b890c2b3c7c15d3f6e3659b8",
                "89b33dbbb4ab15b01af1bd351832f61e606ad238b890c2b3c7c15d3f6e3659b8",
                "edf7f255f5d48ec07bbc442aa14a4b1d356a5993e2215a98c414c001ac969ec2",
            ],
        ),
    ];
    for (kind, hashes) in vectors {
        for state in 0..3 {
            save(tmp.path(), kind, widget(kind, state), true);
            let p = hmi_cli::prepare(tmp.path()).unwrap();
            p.check(true).unwrap();
            let linked = p.link(true).unwrap();
            let project = Project::parse(&Container::parse(linked.bytes).unwrap()).unwrap();
            assert_eq!(
                hmi_core::sha256(&project.pages[0].1.raw),
                hashes[state as usize],
                "{kind} state {state}"
            );
        }
    }
}

#[test]
fn inactive_images_follow_reordering_and_do_not_impose_active_crop_geometry() {
    let tmp = setup();
    let mut o = widget("text", 2);
    o.properties.insert("picc".into(), reference("cursor"));
    save(tmp.path(), "text", o, false);
    let path = tmp.path().join("hmi.toml");
    let mut c: ProjectSource = file::read_toml(&path).unwrap();
    c.assets.items.swap(0, 5);
    fs::write(&path, file::toml(&c).unwrap()).unwrap();
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let expected = prepared.link(true).unwrap().bytes;
    let p = Project::parse(&Container::parse(expected.clone()).unwrap()).unwrap();
    assert_eq!(p.pages[0].1.objects[1].integer("picc"), Some(0));
    assert_eq!(p.pages[0].1.objects[1].integer("pic"), Some(1));
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
fn missing_active_images_none_and_crop_errors_preserve_old_output() {
    let tmp = setup();
    let out = tmp.path().join("old.HMI");
    fs::write(&out, b"previous").unwrap();
    for (kind, field) in [
        ("text", "pic"),
        ("button", "pic2"),
        ("slider", "pic1"),
        ("slider", "pic2"),
        ("progress_bar", "ppic"),
    ] {
        let mut o = widget(kind, 0);
        o.properties.insert(field.into(), none());
        save(tmp.path(), kind, o, false);
        assert_eq!(
            hmi_cli::build(tmp.path(), Some(&out), false)
                .err()
                .unwrap()
                .as_diagnostic()
                .unwrap()
                .code,
            "property_required"
        );
        assert_eq!(fs::read(&out).unwrap(), b"previous");
    }
    let mut o = widget("text", 1);
    o.properties.insert("picc".into(), reference("panel"));
    save(tmp.path(), "text", o, false);
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
    let mut o = widget("text", 1);
    o.properties.insert("x".into(), 100.into());
    save(tmp.path(), "text", o, false);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .unwrap()
            .check(true)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "crop_geometry"
    );
}

#[test]
fn optional_image_relocations_cannot_be_removed_or_forged() {
    let tmp = setup();
    let mut o = widget("text", 2);
    o.properties.remove("picc");
    save(tmp.path(), "text", o, false);
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let moved = tempfile::tempdir().unwrap();
    prepared.save_plan(moved.path(), true, None).unwrap();
    let original =
        hmi_artifact::CompiledObject::read(&moved.path().join("obj/p00000.hmo")).unwrap();
    let mut missing = original.clone();
    missing
        .metadata
        .relocations
        .retain(|r| r.property != "picc");
    assert!(missing.validate().is_err());
    let mut forged = original;
    forged
        .metadata
        .relocations
        .iter_mut()
        .find(|r| r.property == "pic")
        .unwrap()
        .reference = Some(hmi_core::asset::ResourceReference {
        name: None,
        id: None,
        none: Some(true),
    });
    forged
        .write(&moved.path().join("obj/p00000.hmo"), &[])
        .unwrap();
    assert!(hmi_linker::from_manifest(&moved.path().join("link.toml"), true).is_err());
}

#[test]
fn transparent_mode_is_not_enabled_implicitly_for_other_controls() {
    let tmp = setup();
    for kind in ["number", "xfloat", "dual_state_button", "scrolling_text"] {
        let mut o = additional_widget(kind, 0);
        o.properties.insert("sta".into(), 3.into());
        save_additional(tmp.path(), o);
        assert_eq!(
            hmi_cli::prepare(tmp.path())
                .err()
                .unwrap()
                .as_diagnostic()
                .unwrap()
                .code,
            "property_range",
            "{kind}"
        );
    }
}

fn additional_widget(kind: &str, state: u8) -> ObjectFile {
    let mut o = ObjectFile {
        name: "w0".into(),
        type_name: kind.into(),
        properties: BTreeMap::from([
            ("x".into(), 10.into()),
            ("y".into(), 10.into()),
            ("w".into(), 280.into()),
            ("h".into(), 100.into()),
            ("font".into(), reference("ui")),
            ("sta".into(), [2_i64, 0, 1][state as usize].into()),
            ("pic".into(), reference("panel")),
            ("picc".into(), reference("full")),
        ]),
        events: BTreeMap::new(),
    };
    if matches!(kind, "number" | "xfloat") {
        o.properties.insert("val".into(), (-12345).into());
    } else {
        o.properties.insert("txt".into(), "ABC".into());
    }
    if kind == "dual_state_button" {
        o.properties.insert("val".into(), 1.into());
        o.properties.insert("pic2".into(), reference("pressed"));
        o.properties.insert("picc2".into(), reference("full"));
    }
    o
}
fn save_additional(root: &Path, o: ObjectFile) {
    let mut page = hmi_page_compiler::empty_page("page0");
    page.objects.push(o);
    fs::write(
        root.join("pages/page0/page.toml"),
        file::toml(&page).unwrap(),
    )
    .unwrap();
}

#[test]
fn additional_picture_modes_only_change_the_mode_and_keep_numeric_fields() {
    let tmp = setup();
    for kind in ["number", "xfloat", "dual_state_button", "scrolling_text"] {
        let mut previous: Option<hmi_format::page::Object> = None;
        for state in 0..3 {
            save_additional(tmp.path(), additional_widget(kind, state));
            let p = hmi_cli::prepare(tmp.path()).unwrap();
            p.check(true).unwrap();
            let parsed =
                Project::parse(&Container::parse(p.link(true).unwrap().bytes).unwrap()).unwrap();
            let o = &parsed.pages[0].1.objects[1];
            assert_eq!(o.integer("pic"), Some(1));
            assert_eq!(o.integer("picc"), Some(0));
            if matches!(kind, "number" | "xfloat") {
                assert_eq!(
                    i32::from_le_bytes(
                        o.attribute("val")
                            .unwrap()
                            .value
                            .as_slice()
                            .try_into()
                            .unwrap()
                    ),
                    -12345
                );
                assert!(o.attribute("txt").is_none());
            }
            if let Some(old) = &previous {
                assert_eq!(
                    old.rebuild(
                        &BTreeMap::from([("sta".into(), vec![[2, 0, 1][state as usize]])]),
                        &BTreeMap::new()
                    )
                    .unwrap(),
                    o.raw
                );
            }
            previous = Some(hmi_format::page::Object::parse(&o.raw).unwrap());
        }
    }
}

#[test]
fn additional_active_requirements_and_hidden_references_use_the_common_rules() {
    let tmp = setup();
    let output = tmp.path().join("old.HMI");
    fs::write(&output, b"previous").unwrap();
    for kind in ["number", "xfloat", "dual_state_button", "scrolling_text"] {
        let mut o = additional_widget(kind, 0);
        o.properties.insert("pic".into(), none());
        save_additional(tmp.path(), o);
        assert_eq!(
            hmi_cli::build(tmp.path(), Some(&output), true)
                .err()
                .unwrap()
                .as_diagnostic()
                .unwrap()
                .code,
            "property_required"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");
        let mut o = additional_widget(kind, 1);
        o.properties.insert("picc".into(), reference("cursor"));
        save_additional(tmp.path(), o);
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
        let mut o = additional_widget(kind, 2);
        o.properties.insert("picc".into(), reference("cursor"));
        save_additional(tmp.path(), o);
        hmi_cli::prepare(tmp.path()).unwrap().check(true).unwrap();
    }
    let mut o = additional_widget("dual_state_button", 0);
    o.properties.insert("pic2".into(), none());
    save_additional(tmp.path(), o);
    assert!(hmi_cli::prepare(tmp.path()).is_err());
}

#[test]
fn additional_references_follow_final_ids_and_link_without_source() {
    let tmp = setup();
    save_additional(tmp.path(), additional_widget("dual_state_button", 2));
    let path = tmp.path().join("hmi.toml");
    let mut c: ProjectSource = file::read_toml(&path).unwrap();
    c.assets.items.swap(1, 2);
    fs::write(&path, file::toml(&c).unwrap()).unwrap();
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    let expected = p.link(true).unwrap().bytes;
    let parsed = Project::parse(&Container::parse(expected.clone()).unwrap()).unwrap();
    assert_eq!(parsed.pages[0].1.objects[1].integer("pic"), Some(2));
    assert_eq!(parsed.pages[0].1.objects[1].integer("pic2"), Some(1));
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
fn additional_modes_support_all_known_model_directions() {
    let tmp = setup();
    let path = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
    for profile in hmi_core::target::PROFILES {
        for direction in [0, 90, 180, 270] {
            config.target.profile = (*profile).into();
            config.target.orientation = direction;
            fs::write(&path, file::toml(&config).unwrap()).unwrap();
            let t = hmi_core::target::Target::resolve(&config.target).unwrap();
            fs::write(
                tmp.path().join("assets/full.bmp"),
                bmp(u32::from(t.canvas.width), u32::from(t.canvas.height)),
            )
            .unwrap();
            for kind in ["number", "xfloat", "dual_state_button", "scrolling_text"] {
                let mut o = additional_widget(kind, 1);
                o.properties
                    .insert("w".into(), (i64::from(t.canvas.width) - 20).into());
                save_additional(tmp.path(), o);
                let p = hmi_cli::prepare(tmp.path()).unwrap();
                p.check(true).unwrap();
                if kind == "scrolling_text" {
                    p.link(true).unwrap();
                }
            }
        }
    }
}

#[test]
fn common_image_modes_work_for_all_builtin_models_and_directions() {
    let tmp = setup();
    let path = tmp.path().join("hmi.toml");
    let mut c: ProjectSource = file::read_toml(&path).unwrap();
    for profile in hmi_core::target::PROFILES {
        for orientation in [0, 90, 180, 270] {
            c.target.profile = (*profile).into();
            c.target.orientation = orientation;
            fs::write(&path, file::toml(&c).unwrap()).unwrap();
            let target = hmi_core::target::Target::resolve(&c.target).unwrap();
            fs::write(
                tmp.path().join("assets/full.bmp"),
                bmp(
                    u32::from(target.canvas.width),
                    u32::from(target.canvas.height),
                ),
            )
            .unwrap();
            for kind in ["text", "button", "slider", "progress_bar"] {
                let mut o = widget(kind, if kind == "progress_bar" { 0 } else { 1 });
                o.properties
                    .insert("w".into(), (i64::from(target.canvas.width) - 40).into());
                save(tmp.path(), kind, o, false);
                let prepared = hmi_cli::prepare(tmp.path()).unwrap();
                prepared.check(true).unwrap();
                if kind == "slider" {
                    prepared.link(true).unwrap();
                }
            }
        }
    }
}
