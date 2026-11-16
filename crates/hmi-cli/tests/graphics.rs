use hmi_core::{
    file,
    model::{ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, page::Page, project::Project};
use std::{collections::BTreeMap, fs, path::Path};

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
    tmp
}
fn widget(kind: &str, name: &str) -> ObjectFile {
    let (x, y, w, h) = if kind == "waveform" {
        (20, 40, 280, 160)
    } else {
        (60, 40, 200, 200)
    };
    let mut p = BTreeMap::from([
        ("x".into(), x.into()),
        ("y".into(), y.into()),
        ("w".into(), w.into()),
        ("h".into(), h.into()),
    ]);
    if kind == "qrcode" {
        p.insert("txt".into(), "HMI-A".into());
    }
    ObjectFile {
        name: name.into(),
        type_name: kind.into(),
        properties: p,
        events: BTreeMap::new(),
    }
}
fn save(root: &Path, name: &str, objects: Vec<ObjectFile>) {
    let mut p = hmi_page_compiler::empty_page(name);
    p.objects = objects;
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
fn six_pages_match_native_vectors_without_fonts_or_images() {
    for (kind, name, page, field, value, expected) in [
        (
            "gauge",
            "z0",
            "p_gauge",
            "val",
            45.into(),
            "c2a66514e60d243c1372d5bc86760d4c6bc2acf3c224cfddc550a315d6c55428",
        ),
        (
            "gauge",
            "z0",
            "p_gauge",
            "val",
            135.into(),
            "9748e9db43b10384a2b1dcae80529e6dd24c60f8ca4142a8d672a27840cf3007",
        ),
        (
            "waveform",
            "s0",
            "p_wave",
            "ch",
            1.into(),
            "9b0478c6c808288510c3c3fb2911b6c0a9bd28d30939f2ce25d4e1ce442cc933",
        ),
        (
            "waveform",
            "s0",
            "p_wave",
            "ch",
            2.into(),
            "37542a06a1bb6a6c79e969635f5f922232ce1ad40cef978a579ada2c936e9786",
        ),
        (
            "qrcode",
            "qr0",
            "p_qrcode",
            "txt",
            "HMI-A".into(),
            "d9eee7a2e5cc94ee517914c1896108c049136154ffd25cc7e47cfa1525c0fc61",
        ),
        (
            "qrcode",
            "qr0",
            "p_qrcode",
            "txt",
            "HMI-BB".into(),
            "e007ccd4bb4cc12b4cd22cea93d03911f872d5bbd6807c3854651458c6b102fa",
        ),
    ] {
        let tmp = setup();
        let mut o = widget(kind, name);
        o.properties.insert(field.into(), value);
        save(tmp.path(), page, vec![o]);
        assert_eq!(hmi_core::sha256(&link(tmp.path()).raw), expected, "{kind}");
        assert!(!tmp.path().join("assets").exists());
    }
}

#[test]
fn gauge_signed_lengths_roundtrip_and_wave_channels_retain_inactive_colors() {
    let tmp = setup();
    for (up, down) in [(-32768, -1), (-5, -10), (32767, 32767)] {
        let mut o = widget("gauge", "z0");
        o.properties.insert("up".into(), up.into());
        o.properties.insert("down".into(), down.into());
        save(tmp.path(), "page0", vec![o]);
        let p = link(tmp.path());
        let o = &p.objects[1];
        assert_eq!(o.attribute("up").unwrap().value, (up as i16).to_le_bytes());
        let attrs = o
            .attributes
            .iter()
            .map(|a| (a.name.clone(), a.value.clone()))
            .collect::<Vec<_>>();
        let properties = hmi_core::controls::by_name("gauge")
            .unwrap()
            .properties_from_record(&attrs)
            .unwrap();
        assert_eq!(properties["up"].as_integer(), Some(up));
        assert_eq!(properties["down"].as_integer(), Some(down));
    }
    let mut first = None;
    for ch in 1..=4 {
        let mut o = widget("waveform", "s0");
        o.properties.insert("ch".into(), ch.into());
        save(tmp.path(), "page0", vec![o]);
        let p = link(tmp.path());
        for color in ["pco0", "pco1", "pco2", "pco3"] {
            assert_eq!(p.objects[1].attribute(color).unwrap().value.len(), 2);
        }
        let attrs = p.objects[1]
            .attributes
            .iter()
            .filter(|a| a.name != "ch")
            .map(|a| (a.name.clone(), a.value.clone()))
            .collect::<Vec<_>>();
        if let Some(original) = &first {
            assert_eq!(&attrs, original);
        } else {
            first = Some(attrs);
        }
    }
}

#[test]
fn type_ranges_reserved_fields_and_qr_byte_capacity_preserve_existing_output() {
    let tmp = setup();
    let output = tmp.path().join("old.HMI");
    fs::write(&output, b"previous").unwrap();
    for (kind, field, value) in [
        ("gauge", "val", -1),
        ("gauge", "val", 361),
        ("gauge", "up", -32769),
        ("gauge", "down", 32768),
        ("gauge", "hig", 101),
        ("gauge", "vvs0", 129),
        ("gauge", "wid", 2),
        ("gauge", "sta", 4),
        ("waveform", "ch", 0),
        ("waveform", "ch", 5),
        ("waveform", "dir", 3),
        ("waveform", "dis", 9),
        ("waveform", "dis", 1001),
        ("waveform", "gdw", 256),
        ("waveform", "objWid", 0),
        ("waveform", "molloc", 0),
        ("qrcode", "sta", 2),
        ("qrcode", "txt_maxl", 193),
        ("qrcode", "dis", 8),
        ("qrcode", "font", 0),
    ] {
        let mut o = widget(kind, "o0");
        o.properties.insert(field.into(), value.into());
        save(tmp.path(), "page0", vec![o]);
        assert!(
            hmi_cli::build(tmp.path(), Some(&output), true).is_err(),
            "{kind}.{field}={value}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }
    let mut o = widget("qrcode", "qr0");
    o.properties.insert("txt".into(), "测试A".into());
    o.properties.insert("txt_maxl".into(), 7.into());
    save(tmp.path(), "page0", vec![o.clone()]);
    assert_eq!(
        link(tmp.path()).objects[1].attribute("txt").unwrap().value,
        "测试A".as_bytes()
    );
    o.properties.insert("txt_maxl".into(), 6.into());
    save(tmp.path(), "page0", vec![o]);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "text_capacity"
    );
}

#[test]
fn mixed_graphics_reorder_and_link_without_sources_across_all_profiles() {
    let tmp = setup();
    let mut objects = Vec::new();
    for (i, kind) in ["gauge", "waveform", "qrcode"].iter().enumerate() {
        let mut o = widget(kind, ["z0", "s0", "qr0"][i]);
        for (key, value) in [("x", 10), ("y", 10 + i as i64 * 70), ("w", 60), ("h", 60)] {
            o.properties.insert(key.into(), value.into());
        }
        if *kind == "qrcode" {
            o.events.insert("up".into(), "qr.up.s".into());
            fs::write(
                tmp.path().join("pages/page0/qr.up.s"),
                "z0.val=135\ns0.dis=200\nqr0.txt=\"HMI-BB\"\n",
            )
            .unwrap();
        }
        objects.push(o);
    }
    objects.reverse();
    save(tmp.path(), "page0", objects);
    let source = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&source).unwrap();
    for profile in hmi_core::target::PROFILES {
        for orientation in [0, 90, 180, 270] {
            config.target.profile = (*profile).into();
            config.target.orientation = orientation;
            fs::write(&source, file::toml(&config).unwrap()).unwrap();
            let p = link(tmp.path());
            assert_eq!(p.objects[1].name, "qr0");
            assert_eq!(p.objects[1].events[1].lines.len(), 3);
            assert_eq!(p.objects[3].name, "z0");
        }
    }
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    assert!(p.assets.is_empty());
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
