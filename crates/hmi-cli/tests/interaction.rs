use hmi_core::{
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{
    container::Container,
    page::{Object, Page},
    project::Project,
};
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
fn widget(kind: &str, name: &str, variant: i64) -> ObjectFile {
    let mut p = BTreeMap::new();
    if kind != "touch_cap" {
        let (x, y, w, h) = match kind {
            "dual_state_button" => (20, 40 + variant * 140, 280, 100),
            "scrolling_text" => (20, 40, 280, 100),
            _ => (20, 40, 280, 40),
        };
        for (key, value) in [("x", x), ("y", y), ("w", w), ("h", h)] {
            p.insert(key.into(), value.into());
        }
    }
    match kind {
        "slider" => {
            p.insert("val".into(), 40.into());
        }
        "dual_state_button" | "scrolling_text" => {
            p.insert(
                "font".into(),
                toml::Value::Table(toml::map::Map::from_iter([("ref".into(), "ui".into())])),
            );
            p.insert(
                "txt".into(),
                if kind == "scrolling_text" {
                    "SCROLL ABC 123"
                } else {
                    "DUAL"
                }
                .into(),
            );
            if kind == "dual_state_button" {
                p.insert("val".into(), variant.into());
            }
        }
        _ => {}
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
fn event(root: &Path, o: &mut ObjectFile, slot: &str, text: &str) {
    let name = format!("{}.{slot}.s", o.name);
    fs::write(root.join("pages/page0").join(&name), text).unwrap();
    o.events.insert(slot.into(), name);
}

#[test]
fn four_pages_match_previously_verified_creation_recipes() {
    for (kind, prefix, page, count, expected) in [
        (
            "slider",
            "h",
            "p_slider",
            1,
            "40a03ff394d00a2195e663fd54af3803fe414e173440c38989217801c8000e38",
        ),
        (
            "touch_cap",
            "tc",
            "p_touch",
            1,
            "a7b7c2c1b759ac205f568b30d0507a2f8d9e366e808cc159a335680eae4c9e94",
        ),
        (
            "dual_state_button",
            "bt",
            "p_dual",
            2,
            "845b2e8fedfbaa93de0d4dcc3d311c61c58eff47532b6aa2b713369795315f90",
        ),
        (
            "scrolling_text",
            "g",
            "p_scroll",
            1,
            "795a8da7f2c56a25a67e90a05bfbc64b5de8b8e2692b0dd0910a608169788a28",
        ),
    ] {
        let tmp = setup(matches!(kind, "dual_state_button" | "scrolling_text"));
        let objects = (0..count)
            .map(|i| {
                let mut o = widget(kind, &format!("{prefix}{i}"), i);
                event(
                    tmp.path(),
                    &mut o,
                    "down",
                    if kind == "touch_cap" {
                        "sys1=1"
                    } else {
                        "sys0=1"
                    },
                );
                event(
                    tmp.path(),
                    &mut o,
                    "up",
                    if kind == "touch_cap" {
                        "sys1=2"
                    } else {
                        "sys0=2"
                    },
                );
                if kind == "slider" {
                    event(tmp.path(), &mut o, "slide", "sys0=3");
                }
                o
            })
            .collect();
        save(tmp.path(), page, objects);
        let p = link(tmp.path());
        assert_eq!(hmi_core::sha256(&p.raw), expected, "{kind}");
    }
}

#[test]
fn slider_range_limits_sliding_but_allows_explicit_initial_values_outside_it() {
    let tmp = setup(false);
    let mut o = widget("slider", "h0", 0);
    for (min, max, val) in [(20, 30, 0), (20, 30, 65535), (65535, 65535, 0)] {
        for (k, v) in [("minval", min), ("maxval", max), ("val", val)] {
            o.properties.insert(k.into(), v.into());
        }
        save(tmp.path(), "page0", vec![o.clone()]);
        assert_eq!(link(tmp.path()).objects[1].integer("val"), Some(val as u32));
    }
    o.properties.insert("minval".into(), 31.into());
    o.properties.insert("maxval".into(), 30.into());
    save(tmp.path(), "page0", vec![o]);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "slider_range"
    );
}

#[test]
fn slider_cursor_modes_follow_the_target_in_both_compilation_and_linking() {
    let tmp = setup(false);
    let source = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&source).unwrap();
    let mut slider = widget("slider", "h0", 0);
    slider.properties.insert("psta".into(), 2.into());
    save(tmp.path(), "page0", vec![slider]);
    for profile in hmi_core::target::PROFILES {
        config.target.profile = (*profile).into();
        fs::write(&source, file::toml(&config).unwrap()).unwrap();
        if *profile != "TJC4832K035_011" {
            let p = hmi_cli::prepare(tmp.path()).unwrap();
            p.check(true).unwrap();
            assert!(
                p.target
                    .capabilities
                    .control_property_constraints
                    .iter()
                    .all(|r| r.control != "slider" || r.property != "psta")
            );
            assert_eq!(link(tmp.path()).objects[1].integer("psta"), Some(2));
        } else {
            let e = hmi_cli::prepare(tmp.path()).err().unwrap();
            let d = e.as_diagnostic().unwrap();
            assert_eq!(d.code, "control_capability");
            assert_eq!(d.location.property.as_deref(), Some("psta"));
            assert_eq!(d.location.object.as_deref(), Some("h0"));
        }
    }
    // 独立目标即使被修改为 psta=2，也不能绕过 K035 的型号检查。
    config.target.profile = "TJC4832K035_011".into();
    fs::write(&source, file::toml(&config).unwrap()).unwrap();
    save(tmp.path(), "page0", vec![widget("slider", "h0", 0)]);
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    let mut object = p.pages[0].object.clone();
    let original = Object::parse(object.record(1).unwrap()).unwrap();
    let bytes = original
        .rebuild(
            &BTreeMap::from([("psta".into(), vec![2])]),
            &BTreeMap::new(),
        )
        .unwrap();
    let offset = object.metadata.layout.as_ref().unwrap().objects[1].offset as usize;
    object.payload[offset..offset + bytes.len()].copy_from_slice(&bytes);
    let e = hmi_linker::check(&p.target, &p.program.object, &[object], &[], false).unwrap_err();
    let d = e.as_diagnostic().unwrap();
    assert_eq!(d.code, "control_capability");
    assert_eq!(d.location.page.as_deref(), Some("page0"));
    assert_eq!(d.location.object.as_deref(), Some("h0"));
}

#[test]
fn control_specific_ranges_text_capacity_and_readonly_source_fields_are_checked() {
    let tmp = setup(true);
    let output = tmp.path().join("old.HMI");
    fs::write(&output, b"previous").unwrap();
    for (kind, field, value) in [
        ("slider", "sta", 2),
        ("slider", "psta", 1),
        ("slider", "psta", 2),
        ("slider", "mode", 2),
        ("slider", "val", 65536),
        ("slider", "ch", 101),
        ("slider", "wid", 256),
        ("slider", "hig", -1),
        ("slider", "dis", 101),
        ("touch_cap", "val", 0),
        ("touch_cap", "x", 10),
        ("dual_state_button", "val", 2),
        ("dual_state_button", "sta", 0),
        ("scrolling_text", "dir", 4),
        ("scrolling_text", "dis", 1),
        ("scrolling_text", "dis", 51),
        ("scrolling_text", "tim", 79),
        ("scrolling_text", "tim", 65535),
        ("scrolling_text", "en", 2),
        ("scrolling_text", "vvs0", 0),
    ] {
        let mut o = widget(kind, "o0", 0);
        o.properties.insert(field.into(), value.into());
        save(tmp.path(), "page0", vec![o]);
        assert!(
            hmi_cli::build(tmp.path(), Some(&output), true).is_err(),
            "{kind}.{field}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }
    for kind in ["dual_state_button", "scrolling_text"] {
        let mut o = widget(kind, "o0", 0);
        o.properties.insert("txt".into(), "测试A".into());
        o.properties.insert("txt_maxl".into(), 7.into());
        save(tmp.path(), "page0", vec![o.clone()]);
        hmi_cli::prepare(tmp.path()).unwrap().check(true).unwrap();
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
}

#[test]
fn touch_capture_is_singleton_and_linker_checks_target_records_again() {
    let tmp = setup(false);
    let a = widget("touch_cap", "tc0", 0);
    let b = widget("touch_cap", "tc1", 0);
    save(tmp.path(), "page0", vec![a.clone(), b]);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "control_count"
    );
    save(tmp.path(), "page0", vec![a]);
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    let compiled = &p.pages[0].object;
    let raw = compiled.record(1).unwrap();
    let touch = Object::parse(raw).unwrap();
    assert_eq!(touch.attributes.len(), 8);
    assert_eq!(touch.integer("val"), Some(0));
    assert!(touch.attribute("x").is_none());
    let second = touch
        .rebuild(
            &BTreeMap::from([("objname".into(), b"tc1".to_vec())]),
            &BTreeMap::new(),
        )
        .unwrap();
    let object = hmi_artifact::CompiledObject::page(
        "page0".into(),
        &p.target,
        vec![],
        vec![
            (121, "page0".into(), compiled.record(0).unwrap().to_vec()),
            (5, "tc0".into(), raw.to_vec()),
            (5, "tc1".into(), second),
        ],
        compiled
            .metadata
            .relocations
            .iter()
            .filter(|r| r.object == 0)
            .cloned()
            .collect(),
    )
    .unwrap();
    assert_eq!(
        hmi_linker::check(&p.target, &p.program.object, &[object], &[], false)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "control_count"
    );
}

#[test]
fn readonly_capture_values_reject_static_writes_but_allow_reads_and_comparisons() {
    let tmp = setup(false);
    for name in ["tc0", "_tc"] {
        for target in [
            name.to_string(),
            format!("page0.{name}"),
            "b[1]".into(),
            "page0.b[1]".into(),
            "p[0].b[1]".into(),
        ] {
            for op in ["=1", "+=1", "-=1", "++", "--"] {
                let mut o = widget("touch_cap", name, 0);
                event(tmp.path(), &mut o, "up", &format!("{target}.val{op}\n"));
                save(tmp.path(), "page0", vec![o]);
                let e = hmi_cli::prepare(tmp.path())
                    .unwrap()
                    .check(false)
                    .unwrap_err();
                let d = e.as_diagnostic().unwrap();
                assert_eq!(d.code, "script_readonly", "{target}.val{op}");
                assert_eq!(d.location.line, Some(1));
                assert_eq!(d.location.event.as_deref(), Some("up"));
            }
        }
        let mut o = widget("touch_cap", name, 0);
        event(
            tmp.path(),
            &mut o,
            "up",
            &format!(
                "sys0={name}.val\nif({name}.val==1){{sys1=1}}\nif({name}.val!=0){{sys1=2}}\nif({name}.val>=0){{sys1=3}}\nsys2={name}.val+1\n// {name}.val=1\n"
            ),
        );
        save(tmp.path(), "page0", vec![o]);
        hmi_cli::prepare(tmp.path()).unwrap().check(true).unwrap();
    }
}

#[test]
fn mixed_interaction_objects_reorder_and_link_without_sources_for_all_profiles() {
    let tmp = setup(true);
    let mut controls = Vec::new();
    for (i, kind) in ["slider", "touch_cap", "dual_state_button", "scrolling_text"]
        .iter()
        .enumerate()
    {
        let mut o = widget(kind, &format!("o{i}"), 0);
        if *kind != "touch_cap" {
            o.properties.insert("w".into(), 200.into());
            o.properties.insert("h".into(), 40.into());
            o.properties.insert("y".into(), (10 + i as i64 * 45).into());
        }
        if *kind == "slider" {
            event(
                tmp.path(),
                &mut o,
                "slide",
                "sys0=o1.val\no2.val=1\no3.en=0\n",
            );
        }
        controls.push(o);
    }
    controls.reverse();
    save(tmp.path(), "page0", controls);
    let source = tmp.path().join("hmi.toml");
    let mut cfg: ProjectSource = file::read_toml(&source).unwrap();
    for profile in hmi_core::target::PROFILES {
        for orientation in [0, 90, 180, 270] {
            cfg.target.profile = (*profile).into();
            cfg.target.orientation = orientation;
            fs::write(&source, file::toml(&cfg).unwrap()).unwrap();
            let page = link(tmp.path());
            assert_eq!(page.objects[4].name, "o0");
            assert_eq!(page.objects[4].events[2].kind, "slide");
            assert_eq!(page.objects[4].events[2].lines.len(), 3);
        }
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
