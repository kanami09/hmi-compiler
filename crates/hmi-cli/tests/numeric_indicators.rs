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

fn setup(font: bool) -> tempfile::TempDir {
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
    if font {
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
    }
    tmp
}
fn widget(kind: &str, name: &str, x: i64, y: i64, w: i64, h: i64) -> ObjectFile {
    let mut p = BTreeMap::from([
        ("x".into(), x.into()),
        ("y".into(), y.into()),
        ("w".into(), w.into()),
        ("h".into(), h.into()),
    ]);
    if matches!(kind, "number" | "xfloat") {
        p.insert(
            "font".into(),
            toml::Value::Table(toml::map::Map::from_iter([("ref".into(), "ui".into())])),
        );
    }
    ObjectFile {
        original_record: None,
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
fn numeric_pages_match_four_native_vectors_and_decode_signed_values() {
    for (kind, name, page, val, fractional, expected) in [
        (
            "number",
            "n0",
            "p_number",
            -12,
            0,
            "e91d92e7cc32cacfae9ed6bb351902aec9934f9123bb6165481dbe724042b118",
        ),
        (
            "number",
            "n0",
            "p_number",
            34,
            0,
            "5c5fc5d1a533535ce0a5b6cfaed5107b5333b59a5473c98482980310ed3a73ce",
        ),
        (
            "xfloat",
            "x0",
            "p_xfloat",
            -1234,
            2,
            "9651dd1549eac1a79d2d8bae1d66b7e40d64c2dd8d1560348aaaab96d74beee9",
        ),
        (
            "xfloat",
            "x0",
            "p_xfloat",
            -1234,
            3,
            "6ee28df319f90f0e70b8d02747f4372dea4e224ea619d97316a531b4a2937e81",
        ),
    ] {
        let tmp = setup(true);
        let mut o = widget(kind, name, 20, 40, 280, 100);
        o.properties.insert("val".into(), val.into());
        if kind == "xfloat" {
            o.properties.insert("vvs1".into(), fractional.into());
        }
        save(tmp.path(), page, vec![o]);
        let p = link(tmp.path());
        assert_eq!(hmi_core::sha256(&p.raw), expected);
        let attributes = p.objects[1]
            .attributes
            .iter()
            .map(|a| (a.name.clone(), a.value.clone()))
            .collect::<Vec<_>>();
        let properties = hmi_core::controls::by_name(kind)
            .unwrap()
            .properties_from_record(&attributes)
            .unwrap();
        assert_eq!(properties["val"].as_integer(), Some(val));
        assert_eq!(p.objects[1].attributes.len(), 39);
        assert!(p.objects[1].attribute("txt").is_none());
    }
}

#[test]
fn selection_and_progress_pages_match_previously_verified_recipes_without_media() {
    // 固定的是此前已验收配方的页面摘要，不把业务来源模板称为出厂默认值。
    for (kind, prefix, page, count, expected) in [
        (
            "checkbox",
            "c",
            "p_check",
            2,
            "6101a8aad71aa554134275f1f3975dddaa6a6558127c286393f80450ac1f3251",
        ),
        (
            "radio",
            "r",
            "p_radio",
            2,
            "75f9a68eb868dcbca72655eab304a0014d6f53a2a173cadf28127259af6e050b",
        ),
        (
            "progress_bar",
            "j",
            "p_progress",
            1,
            "f3916fd3075bba90988560f136f8e0dc3bbc2b287328098fd2cc64090c78df77",
        ),
    ] {
        let tmp = setup(false);
        let objects = (0..count)
            .map(|i| {
                let mut o = if kind == "progress_bar" {
                    widget(kind, "j0", 20, 40, 280, 40)
                } else {
                    widget(kind, &format!("{prefix}{i}"), 20 + i * 100, 40, 48, 48)
                };
                o.properties.insert(
                    "val".into(),
                    (if kind == "progress_bar" { 40 } else { i }).into(),
                );
                for (slot, code) in [("down", "sys0=1"), ("up", "sys0=2")] {
                    let name = format!("{}.{slot}.s", o.name);
                    fs::write(tmp.path().join("pages/page0").join(&name), code).unwrap();
                    o.events.insert(slot.into(), name);
                }
                o
            })
            .collect();
        save(tmp.path(), page, objects);
        let p = link(tmp.path());
        assert_eq!(hmi_core::sha256(&p.raw), expected, "{kind}");
        assert!(p.objects.iter().all(|o| o.attribute("font").is_none()));
    }
}

#[test]
fn numeric_bounds_spelling_modes_and_selection_ranges_preserve_previous_output() {
    let tmp = setup(true);
    let output = tmp.path().join("previous.HMI");
    fs::write(&output, b"previous").unwrap();
    for (kind, field, value) in [
        ("number", "val", i64::from(i32::MIN) - 1),
        ("number", "val", i64::from(i32::MAX) + 1),
        ("number", "length", 5),
        ("number", "lenth", 16),
        ("number", "format", 3),
        ("number", "sta", 0),
        ("xfloat", "vvs0", 11),
        ("xfloat", "vvs1", 9),
        ("xfloat", "format", 0),
        ("checkbox", "val", 2),
        ("checkbox", "style", 5),
        ("radio", "style", 1),
        ("progress_bar", "val", 101),
        ("progress_bar", "sta", 1),
        ("progress_bar", "dez", 2),
        ("progress_bar", "dis", 101),
    ] {
        let mut o = widget(kind, "o0", 10, 10, 100, 50);
        o.properties.insert(field.into(), value.into());
        save(tmp.path(), "page0", vec![o]);
        assert!(
            hmi_cli::build(tmp.path(), Some(&output), true).is_err(),
            "{kind}.{field}={value}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }
    let mut o = widget("xfloat", "x0", 10, 10, 100, 50);
    o.properties
        .insert("val".into(), toml::Value::Float(-12.34));
    save(tmp.path(), "page0", vec![o]);
    assert!(hmi_cli::prepare(tmp.path()).is_err());
    for value in [i32::MIN, i32::MAX] {
        let mut o = widget("number", "n0", 10, 10, 100, 50);
        o.properties.insert("val".into(), i64::from(value).into());
        save(tmp.path(), "page0", vec![o]);
        hmi_cli::prepare(tmp.path()).unwrap().check(true).unwrap();
    }
}

#[test]
fn mixed_controls_bind_events_and_reorder_without_adding_radio_logic() {
    let tmp = setup(true);
    let mut p = widget("progress_bar", "j0", 10, 10, 40, 160);
    p.properties.insert("dez".into(), 1.into());
    p.properties.insert("val".into(), 75.into());
    let mut a = widget("radio", "r0", 100, 10, 48, 48);
    a.properties.insert("val".into(), 1.into());
    let mut b = widget("radio", "r1", 160, 10, 48, 48);
    b.properties.insert("val".into(), 1.into());
    let mut x = widget("xfloat", "x0", 10, 200, 280, 80);
    x.events.insert("up".into(), "x.up.s".into());
    fs::write(
        tmp.path().join("pages/page0/x.up.s"),
        "x0.val=-1234\nj0.val=25\nr0.val=0\nr1.val=1\n",
    )
    .unwrap();
    save(tmp.path(), "page0", vec![p, a, b, x]);
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let before = prepared.link(true).unwrap().bytes;
    let moved = tempfile::tempdir().unwrap();
    prepared.save_plan(moved.path(), true, None).unwrap();
    let mut page: hmi_core::model::PageSource =
        file::read_toml(&tmp.path().join("pages/page0/page.toml")).unwrap();
    page.objects.reverse();
    save(tmp.path(), "page0", page.objects);
    let after = link(tmp.path());
    assert_eq!(after.objects[1].name, "x0");
    assert_eq!(after.objects[1].events[1].lines.len(), 4);
    assert_eq!(after.objects[2].integer("val"), Some(1));
    assert_eq!(after.objects[3].integer("val"), Some(1));
    assert!(after.objects[2].events.iter().all(|e| e.lines.is_empty()));
    assert_eq!(after.objects[4].integer("dez"), Some(1));
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        before
    );
}

#[test]
fn all_profiles_and_orientations_link_each_new_control() {
    let tmp = setup(true);
    let mut objects = Vec::new();
    for (i, kind) in ["number", "xfloat", "checkbox", "radio", "progress_bar"]
        .iter()
        .enumerate()
    {
        objects.push(widget(
            kind,
            &format!("o{i}"),
            10,
            10 + i as i64 * 45,
            100,
            40,
        ));
    }
    save(tmp.path(), "page0", objects);
    let source = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&source).unwrap();
    for profile in hmi_core::target::PROFILES {
        for orientation in [0, 90, 180, 270] {
            config.target.profile = (*profile).into();
            config.target.orientation = orientation;
            fs::write(&source, file::toml(&config).unwrap()).unwrap();
            let page = link(tmp.path());
            assert_eq!(page.objects.len(), 6);
            for (id, o) in page.objects.iter().enumerate() {
                assert_eq!(usize::from(o.id), id);
            }
        }
    }
}
