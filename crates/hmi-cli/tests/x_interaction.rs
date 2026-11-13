use hmi_core::{
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
    fs::write(tmp.path().join("assets/font.zi"), zi_fixture::font()).unwrap();
    fs::write(
        tmp.path().join("assets/font.asset.toml"),
        "schema='hmi.asset/1'\nkind='font'\nsource='font.zi'\n",
    )
    .unwrap();
    let mut p: ProjectSource = file::read_toml(&tmp.path().join("hmi.toml")).unwrap();
    p.assets.items.push(AssetSource {
        key: "ui".into(),
        source: "assets/font.asset.toml".into(),
    });
    fs::write(tmp.path().join("hmi.toml"), file::toml(&p).unwrap()).unwrap();
    tmp
}
fn widget(kind: &str, name: &str) -> ObjectFile {
    let (w, h) = match kind {
        "switch" => (320, 88),
        "combo_box" => (300, 84),
        "text_select" => (300, 232),
        _ => (440, 232),
    };
    let mut p = BTreeMap::from([
        ("x".into(), 20.into()),
        ("y".into(), 20.into()),
        ("w".into(), w.into()),
        ("h".into(), h.into()),
        (
            "font".into(),
            toml::Value::Table(toml::map::Map::from_iter([("ref".into(), "ui".into())])),
        ),
    ]);
    match kind {
        "switch" => {
            p.insert("txt".into(), "0/1".into());
        }
        "combo_box" | "text_select" => {
            p.insert("path".into(), "A\nB\nC".into());
            if kind == "combo_box" {
                p.insert("txt".into(), "40V".into());
            }
        }
        _ => {
            p.insert("txt".into(), "A\nB\nC\nD".into());
        }
    }
    ObjectFile {
        name: name.into(),
        type_name: kind.into(),
        properties: p,
        events: BTreeMap::new(),
    }
}
fn save(root: &Path, name: &str, objects: Vec<ObjectFile>) {
    let mut page = hmi_page_compiler::empty_page(name);
    page.objects = objects;
    fs::write(
        root.join("pages/page0/page.toml"),
        file::toml(&page).unwrap(),
    )
    .unwrap();
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
fn eight_native_pages_match_and_selection_does_not_rewrite_saved_text() {
    for (kind, name, page, variant, expected) in [
        (
            "switch",
            "sw0",
            "p_switch",
            0,
            "c59be31865064a3c0c40ff92903fc79a65ddfdea019e1a27b5b8c7ebe1fb06da",
        ),
        (
            "switch",
            "sw0",
            "p_switch",
            1,
            "0d45fca084b35194b60d2883799d7c2e427488899bd03f602d585cafb90bcb21",
        ),
        (
            "combo_box",
            "cb0",
            "p_combo",
            0,
            "f5e68e9999798de58937ea7d8aaee88096b793006407564174fdec03c547eb68",
        ),
        (
            "combo_box",
            "cb0",
            "p_combo",
            1,
            "5beabde33b4adbc9c8acac8026cb9fa3c1aab60ee4ea856db5bf4b9e96869b7f",
        ),
        (
            "text_select",
            "select0",
            "p_select",
            0,
            "01b9620fe05446aa985e24d1cf2345431fc66bf313e45feddf479b26599d1364",
        ),
        (
            "text_select",
            "select0",
            "p_select",
            1,
            "65e3bf2c41a72b813d16a725b3cb16623ab11e12cdbdee728190a30c72b7ce56",
        ),
        (
            "sliding_text",
            "slt0",
            "p_sltext",
            0,
            "c09194ae170d770ff1afbd00d96b695293fecfb3be6b942a386e12ee296ed328",
        ),
        (
            "sliding_text",
            "slt0",
            "p_sltext",
            1,
            "647d3cfa3b56c392a194789101dcc33910ac36bbaf920377678d2fdb0696b7dd",
        ),
    ] {
        let tmp = setup();
        let mut o = widget(kind, name);
        if kind == "sliding_text" {
            if variant == 1 {
                o.properties.insert("txt".into(), "A\nB\nC\nEE".into());
            }
        } else {
            o.properties.insert("val".into(), variant.into());
        }
        save(tmp.path(), page, vec![o]);
        let p = link(tmp.path());
        assert_eq!(hmi_core::sha256(&p.raw), expected, "{kind}:{variant}");
        if kind == "combo_box" {
            assert_eq!(p.objects[1].attribute("txt").unwrap().value, b"40V");
        }
        if kind == "text_select" {
            assert!(p.objects[1].attribute("txt").unwrap().value.is_empty());
            assert_eq!(p.objects[1].attribute("val").unwrap().value.len(), 4);
        }
    }
}

#[test]
fn multiline_input_normalizes_newlines_and_checks_encoded_byte_capacity() {
    let tmp = setup();
    for kind in ["combo_box", "text_select", "sliding_text"] {
        let field = if kind == "sliding_text" {
            "txt"
        } else {
            "path"
        };
        let capacity = if field == "txt" { "txt_maxl" } else { "path_m" };
        let mut o = widget(kind, "o0");
        o.properties.insert(field.into(), "测\nA\n".into());
        o.properties.insert(capacity.into(), 8.into());
        save(tmp.path(), "page0", vec![o.clone()]);
        let lf = link(tmp.path());
        assert_eq!(
            lf.objects[1].attribute(field).unwrap().value,
            "测\r\nA\r\n".as_bytes()
        );
        o.properties.insert(field.into(), "测\r\nA\r\n".into());
        save(tmp.path(), "page0", vec![o.clone()]);
        assert_eq!(link(tmp.path()).raw, lf.raw);
        o.properties.insert(capacity.into(), 7.into());
        save(tmp.path(), "page0", vec![o.clone()]);
        assert_eq!(
            hmi_cli::prepare(tmp.path())
                .err()
                .unwrap()
                .as_diagnostic()
                .unwrap()
                .code,
            "text_capacity"
        );
        o.properties.insert(capacity.into(), 64.into());
        o.properties.insert(field.into(), "A\rB".into());
        save(tmp.path(), "page0", vec![o]);
        assert!(hmi_cli::prepare(tmp.path()).is_err());
    }
    let mut o = widget("sliding_text", "slt0");
    o.properties.insert("path_m".into(), 6.into());
    save(tmp.path(), "page0", vec![o]);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .location
            .property
            .as_deref(),
        Some("path_m")
    );
}

#[test]
fn documented_ranges_and_readonly_source_fields_preserve_existing_output() {
    let tmp = setup();
    let output = tmp.path().join("old.HMI");
    fs::write(&output, b"previous").unwrap();
    for (kind, key, value) in [
        ("switch", "val", 2),
        ("switch", "dez", 2),
        ("switch", "txt_maxl", 25),
        ("combo_box", "qty", 0),
        ("combo_box", "qty", 255),
        ("combo_box", "hig", 15),
        ("combo_box", "wid", 7),
        ("combo_box", "ch", 33),
        ("combo_box", "val", 256),
        ("text_select", "txt_maxl", 1),
        ("text_select", "style", 2),
        ("text_select", "hig", 15),
        ("text_select", "val", 4294967296i64),
        ("sliding_text", "left", 3),
        ("sliding_text", "ch", 33),
        ("sliding_text", "maxval_y", 0),
        ("sliding_text", "val_y", 0),
    ] {
        let mut o = widget(kind, "o0");
        o.properties.insert(key.into(), value.into());
        save(tmp.path(), "page0", vec![o]);
        assert!(
            hmi_cli::build(tmp.path(), Some(&output), true).is_err(),
            "{kind}.{key}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }
    let mut o = widget("text_select", "select0");
    o.properties.insert("val".into(), 4294967295i64.into());
    save(tmp.path(), "page0", vec![o]);
    assert_eq!(link(tmp.path()).objects[1].integer("val"), Some(u32::MAX));
}

#[test]
fn model_support_is_checked_in_source_and_independent_objects() {
    let tmp = setup();
    let source = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&source).unwrap();
    for kind in ["switch", "combo_box", "text_select", "sliding_text"] {
        save(tmp.path(), "page0", vec![widget(kind, "o0")]);
        for profile in ["TJC4832K035_011", "TJC4832T035_011", "TJC3224T028_011"] {
            config.target.profile = profile.into();
            fs::write(&source, file::toml(&config).unwrap()).unwrap();
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
        fs::write(&source, file::toml(&config).unwrap()).unwrap();
        let p = hmi_cli::prepare(tmp.path()).unwrap();
        let mut object = p.pages[0].object.clone();
        config.target.profile = "TJC4832K035_011".into();
        let target = hmi_core::target::Target::resolve(&config.target).unwrap();
        object.metadata.target = target.clone();
        let root = hmi_format::page::Object::parse(object.record(0).unwrap()).unwrap();
        let geometry = BTreeMap::from([
            ("w".into(), target.canvas.width.to_le_bytes().to_vec()),
            ("h".into(), target.canvas.height.to_le_bytes().to_vec()),
            (
                "endx".into(),
                (target.canvas.width - 1).to_le_bytes().to_vec(),
            ),
            (
                "endy".into(),
                (target.canvas.height - 1).to_le_bytes().to_vec(),
            ),
        ]);
        let raw = root.rebuild(&geometry, &BTreeMap::new()).unwrap();
        object.payload[..raw.len()].copy_from_slice(&raw);
        let mut program = p.program.object.clone();
        program.metadata.target = target.clone();
        assert_eq!(
            hmi_linker::check(&target, &program, &[object], &[], false)
                .unwrap_err()
                .as_diagnostic()
                .unwrap()
                .code,
            "control_capability"
        );
    }
}

#[test]
fn readonly_properties_are_shared_with_script_analysis_and_accept_reads() {
    let tmp = setup();
    for (kind, field) in [("text_select", "txt"), ("sliding_text", "maxval_y")] {
        for target in [
            "o0".to_string(),
            "page0.o0".into(),
            "b[1]".into(),
            "p[0].b[1]".into(),
        ] {
            let mut o = widget(kind, "o0");
            o.events.insert("up".into(), "up.s".into());
            fs::write(
                tmp.path().join("pages/page0/up.s"),
                format!("{target}.{field}=1\n"),
            )
            .unwrap();
            save(tmp.path(), "page0", vec![o]);
            assert_eq!(
                hmi_cli::prepare(tmp.path())
                    .unwrap()
                    .check(false)
                    .unwrap_err()
                    .as_diagnostic()
                    .unwrap()
                    .code,
                "script_readonly"
            );
        }
        let mut o = widget(kind, "o0");
        o.events.insert("up".into(), "up.s".into());
        let code = if field == "txt" {
            "prints o0.txt,0\n"
        } else {
            "sys0=o0.maxval_y\no0.val_y=0\n"
        };
        fs::write(tmp.path().join("pages/page0/up.s"), code).unwrap();
        save(tmp.path(), "page0", vec![o]);
        hmi_cli::prepare(tmp.path()).unwrap().check(true).unwrap();
    }
}

#[test]
fn reordered_objects_and_font_targets_are_self_contained_in_all_directions() {
    let tmp = setup();
    let mut objects = Vec::new();
    for (i, kind) in ["switch", "combo_box", "text_select", "sliding_text"]
        .iter()
        .enumerate()
    {
        let mut o = widget(kind, &format!("o{i}"));
        if i == 0 {
            o.events.insert("up".into(), "up.s".into());
        }
        objects.push(o);
    }
    fs::write(
        tmp.path().join("pages/page0/up.s"),
        "o1.val=1\nsys0=o2.val\no3.val_y=0\n",
    )
    .unwrap();
    objects.reverse();
    save(tmp.path(), "page0", objects);
    let source = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&source).unwrap();
    for orientation in [0, 90, 180, 270] {
        config.target.orientation = orientation;
        fs::write(&source, file::toml(&config).unwrap()).unwrap();
        let p = link(tmp.path());
        assert_eq!(p.objects[4].name, "o0");
        assert_eq!(p.objects[4].events[1].lines.len(), 3);
    }
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
