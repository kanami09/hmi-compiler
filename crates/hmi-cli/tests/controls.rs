use hmi_core::{
    file,
    model::{ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{
    container::Container,
    page::{Object, Page},
    project::Project,
};
use std::{collections::BTreeMap, fs, path::Path};

fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    hmi_cli::init(
        tmp.path(),
        &TargetConfig {
            profile: "TJC4832T035_011".into(),
            format: "ver21234".into(),
            encoding: "utf-8".into(),
            orientation: 0,
        },
    )
    .unwrap();
    tmp
}
fn control(name: &str, kind: &str, properties: &[(&str, toml::Value)]) -> ObjectFile {
    ObjectFile {
        name: name.into(),
        type_name: kind.into(),
        properties: properties
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        events: BTreeMap::new(),
    }
}
fn n(value: i64) -> toml::Value {
    toml::Value::Integer(value)
}
fn hotspot(name: &str, x: i64, y: i64) -> ObjectFile {
    control(
        name,
        "hotspot",
        &[("x", n(x)), ("y", n(y)), ("w", n(60)), ("h", n(60))],
    )
}
fn page(root: &Path, objects: Vec<ObjectFile>, load: Option<&str>) {
    let mut source = hmi_page_compiler::empty_page("page0");
    source.objects = objects;
    let directory = root.join("pages/page0");
    if let Some(text) = load {
        source.events.insert("load".into(), "load.s".into());
        fs::write(directory.join("load.s"), text).unwrap();
    }
    fs::write(directory.join("page.toml"), file::toml(&source).unwrap()).unwrap();
}
fn parsed(bytes: Vec<u8>) -> Page {
    Project::parse(&Container::parse(bytes).unwrap())
        .unwrap()
        .pages
        .remove(0)
        .1
}
fn code(error: &hmi_core::Error) -> &str {
    &error.as_diagnostic().unwrap().code
}

#[test]
fn plain_diagnostic_identifies_page_control_property_and_constraint() {
    let tmp = fixture();
    page(
        tmp.path(),
        vec![control("tm0", "timer", &[("tim", n(49))])],
        None,
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hmi"))
        .arg("check")
        .arg(tmp.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    for expected in [
        "错误 [property_range]",
        "页面 page0",
        "控件 tm0",
        "属性 tim",
        "50～65534",
    ] {
        assert!(diagnostic.contains(expected), "{diagnostic}");
    }
    assert!(!diagnostic.contains("[]"));
}

#[test]
fn known_control_recipes_match_twelve_native_vectors() {
    let original = vec![
        hotspot("m0", 0, 0),
        hotspot("m1", 80, 0),
        hotspot("m2", 185, 52),
    ];
    let mut renamed = original.clone();
    renamed[1].name = "middle".into();
    let mut cases = vec![
        (
            original.clone(),
            "31f467163a3411267b9deb51e127a3fbc86b97950aa03933c5cf5b04c4d79435",
        ),
        (
            renamed,
            "8a3d39bec0ca143224e6ad5514aef6aa500e78ca1afb8932b8a1064e4e348c8f",
        ),
        (
            vec![
                original[0].clone(),
                original[2].clone(),
                original[1].clone(),
            ],
            "5cf2750082d21c3c12d09d01c28d8f6980a4c09683fbffb81b79c956e774e11b",
        ),
        (
            vec![original[0].clone(), original[2].clone()],
            "2a29459f39db24009f886881e01471b72845aecf614e0e2e10d3e86b1c0b3335",
        ),
        (
            vec![control("va0", "variable", &[])],
            "a93814575ab670a51619adef87dbbf47369a3116c30b5f63868f0ea8ba10f50a",
        ),
        (
            vec![control("va0", "variable", &[("sta", n(1))])],
            "4307e36ad7f51fbc9aaaa7fc931193ca5f907284e06b6f51ad08c034e9176b22",
        ),
        (
            vec![control("va0", "variable", &[("val", n(-123456789))])],
            "76bfa30881b8247b208e9956937c0ca6014cd16cc253f0fd56127e38a1ca18db",
        ),
        (
            vec![control(
                "va0",
                "variable",
                &[
                    ("sta", n(1)),
                    ("val", n(-123456789)),
                    ("txt", toml::Value::String("测试A".into())),
                ],
            )],
            "c5abfcdf6952d6e18e51c0b2ae2ad745aa5bdf88692a9ed0ae4dfb4f19879d8b",
        ),
        (
            vec![control(
                "va0",
                "variable",
                &[
                    ("val", n(-123456789)),
                    ("txt", toml::Value::String("测试A".into())),
                ],
            )],
            "68c8965cd506802459bcd46b41989478bdbb0e979559b9aa310ae8779d9bbd23",
        ),
        (
            vec![control("tm0", "timer", &[])],
            "1fabbf52f8cf357686446a046e4f72b950e3a63bc11c1f6027b9d9ef6ca82797",
        ),
        (
            vec![control("tm0", "timer", &[("tim", n(12345))])],
            "652003b0365f2dc9f5b3220d6b42310b0052946dbe181616ee084390dcdda882",
        ),
    ];
    let mut timer = control("tm0", "timer", &[("tim", n(12345)), ("en", n(0))]);
    timer.events.insert("timer".into(), "timer.s".into());
    cases.push((
        vec![timer],
        "c32792b0834f0c097c945f14ddf5ee6fe20e6745126052dcc95623e3283c82d1",
    ));
    for (index, (objects, expected)) in cases.into_iter().enumerate() {
        let tmp = fixture();
        fs::write(tmp.path().join("pages/page0/timer.s"), "sys0=sys0+1\n").unwrap();
        page(tmp.path(), objects, None);
        let linked = hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
        assert_eq!(
            hmi_core::sha256(&linked.bytes),
            expected,
            "原生向量 {index}"
        );
    }
}

#[test]
fn linker_assigns_ids_and_keeps_event_owners_after_reordering() {
    let tmp = fixture();
    let mut area = hotspot("m0", 0, 0);
    area.events.insert("up".into(), "up.s".into());
    let variable = control("va0", "variable", &[("val", n(17))]);
    let mut timer = control("tm0", "timer", &[("en", n(0))]);
    timer.events.insert("timer".into(), "timer.s".into());
    fs::write(tmp.path().join("pages/page0/up.s"), "sys0=va0.val\n").unwrap();
    fs::write(
        tmp.path().join("pages/page0/timer.s"),
        "va0.val=va0.val+1\n",
    )
    .unwrap();
    page(
        tmp.path(),
        vec![area.clone(), variable.clone(), timer.clone()],
        None,
    );
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let object = &prepared.pages[0].object;
    assert_eq!(
        object
            .metadata
            .relocations
            .iter()
            .map(|r| r.object)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    for index in 0..4 {
        assert_eq!(Object::parse(object.record(index).unwrap()).unwrap().id, 0);
    }
    let first = parsed(prepared.link(true).unwrap().bytes);
    assert_eq!(
        first.objects.iter().map(|o| o.id).collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    page(tmp.path(), vec![timer.clone(), variable, area], None);
    let reordered = hmi_cli::prepare(tmp.path()).unwrap();
    let event = reordered.pages[0]
        .object
        .metadata
        .scripts
        .iter()
        .find(|f| f.source.event.as_deref() == Some("timer"))
        .unwrap();
    assert_eq!(event.source.object, Some(1));
    let output = parsed(reordered.link(true).unwrap().bytes);
    assert_eq!(
        output
            .objects
            .iter()
            .map(|o| o.name.as_str())
            .collect::<Vec<_>>(),
        ["page0", "tm0", "va0", "m0"]
    );
    assert_eq!(
        output.objects[1].events[0].lines,
        [b"va0.val=va0.val+1".to_vec()]
    );
    assert_eq!(
        output.objects[3].events[1].lines,
        [b"sys0=va0.val".to_vec()]
    );
    let mut invalid = reordered.pages[0].object.clone();
    invalid.metadata.relocations.pop();
    assert_eq!(code(&invalid.encode().unwrap_err()), "object_relocation");
    let mut invalid = reordered.pages[0].object.clone();
    invalid.metadata.scripts[0].source.object = Some(2); // 变量没有 timer 事件。
    assert_eq!(
        code(
            &hmi_linker::link(
                &reordered.target,
                &reordered.program.object,
                &[invalid],
                false
            )
            .unwrap_err()
        ),
        "script_facts"
    );
    page(tmp.path(), vec![timer], None);
    assert_eq!(
        code(
            &hmi_cli::prepare(tmp.path())
                .unwrap()
                .link(false)
                .unwrap_err()
        ),
        "script_object"
    );
}

#[test]
fn references_follow_current_names_and_final_ids_without_script_rewriting() {
    let tmp = fixture();
    page(
        tmp.path(),
        vec![hotspot("confirm", 10, 0), hotspot("m0", 100, 0)],
        Some("sys0=m0.x\nsys1=b[1].x\n"),
    );
    let first = parsed(
        hmi_cli::prepare(tmp.path())
            .unwrap()
            .link(true)
            .unwrap()
            .bytes,
    );
    assert_eq!(first.objects[1].name, "confirm");
    assert_eq!(first.objects[2].integer("x"), Some(100));
    assert_eq!(
        first.objects[0].events[0].lines,
        [b"sys0=m0.x".to_vec(), b"sys1=b[1].x".to_vec()]
    );
    page(
        tmp.path(),
        vec![hotspot("m0", 100, 0), hotspot("confirm", 10, 0)],
        Some("sys0=m0.x\nsys1=b[1].x\n"),
    );
    let second = parsed(
        hmi_cli::prepare(tmp.path())
            .unwrap()
            .link(true)
            .unwrap()
            .bytes,
    );
    assert_eq!(second.objects[1].name, "m0");
    assert_eq!(
        second.objects[0].events[0].lines,
        first.objects[0].events[0].lines
    );
    page(
        tmp.path(),
        vec![control("va0", "variable", &[]), hotspot("m0", 100, 0)],
        Some("sys0=b[1].x\n"),
    );
    assert_eq!(
        code(
            &hmi_cli::prepare(tmp.path())
                .unwrap()
                .link(false)
                .unwrap_err()
        ),
        "script_property"
    );
}

#[test]
fn scope_mode_and_diagnostic_owner_use_the_final_project() {
    let tmp = fixture();
    let source = tmp.path().join("hmi.toml");
    let mut project: ProjectSource = file::read_toml(&source).unwrap();
    project.pages.sources.push("pages/other/page.toml".into());
    fs::write(source, file::toml(&project).unwrap()).unwrap();
    fs::create_dir_all(tmp.path().join("pages/other")).unwrap();
    let mut other = hmi_page_compiler::empty_page("other");
    let mut timer = control("tm0", "timer", &[("en", n(0))]);
    timer.events.insert("timer".into(), "timer.s".into());
    other.objects.push(timer);
    fs::write(
        tmp.path().join("pages/other/page.toml"),
        file::toml(&other).unwrap(),
    )
    .unwrap();
    let script = tmp.path().join("pages/other/timer.s");
    fs::write(&script, "page0.shared.val=7\n").unwrap();
    page(tmp.path(), vec![control("shared", "variable", &[])], None);
    let error = hmi_cli::prepare(tmp.path())
        .unwrap()
        .link(false)
        .unwrap_err();
    let diagnostic = error.as_diagnostic().unwrap();
    assert_eq!(diagnostic.code, "script_scope");
    assert_eq!(diagnostic.location.page.as_deref(), Some("other"));
    assert_eq!(diagnostic.location.object.as_deref(), Some("tm0"));
    assert_eq!(diagnostic.location.event.as_deref(), Some("timer"));
    assert_eq!(diagnostic.location.line, Some(1));
    page(
        tmp.path(),
        vec![control("shared", "variable", &[("vscope", n(1))])],
        None,
    );
    hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
    page(
        tmp.path(),
        vec![control(
            "shared",
            "variable",
            &[("vscope", n(1)), ("sta", n(1))],
        )],
        None,
    );
    assert_eq!(
        code(
            &hmi_cli::prepare(tmp.path())
                .unwrap()
                .link(false)
                .unwrap_err()
        ),
        "script_property_mode"
    );
    fs::write(&script, "page0.shared.txt=\"A\"\n").unwrap();
    hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
}

#[test]
fn source_limits_and_semantic_errors_preserve_the_previous_output() {
    let tmp = fixture();
    let output = tmp.path().join("out.HMI");
    fs::write(&output, b"previous output").unwrap();
    let mut timer = control("tm0", "timer", &[]);
    timer.events.insert("up".into(), "missing.s".into());
    page(tmp.path(), vec![timer], None);
    assert_eq!(
        code(&hmi_cli::build(tmp.path(), Some(&output), false).unwrap_err()),
        "event_slot"
    );
    assert_eq!(fs::read(&output).unwrap(), b"previous output");
    page(
        tmp.path(),
        vec![control("tm0", "timer", &[("tim", n(49))])],
        None,
    );
    let error = hmi_cli::prepare(tmp.path()).err().unwrap();
    let location = &error.as_diagnostic().unwrap().location;
    assert_eq!(location.object.as_deref(), Some("tm0"));
    assert_eq!(location.property.as_deref(), Some("tim"));
    page(
        tmp.path(),
        vec![hotspot("m0", 0, 0), hotspot("m0", 80, 0)],
        None,
    );
    assert_eq!(
        code(&hmi_cli::prepare(tmp.path()).err().unwrap()),
        "object_name"
    );
    let controls = (0..250)
        .map(|i| control(&format!("v{i}"), "variable", &[]))
        .collect::<Vec<_>>();
    page(tmp.path(), controls.clone(), None);
    assert_eq!(
        parsed(
            hmi_cli::prepare(tmp.path())
                .unwrap()
                .link(true)
                .unwrap()
                .bytes
        )
        .objects
        .len(),
        251
    );
    let mut too_many = controls;
    too_many.push(control("extra", "variable", &[]));
    page(tmp.path(), too_many, None);
    assert_eq!(
        code(&hmi_cli::prepare(tmp.path()).err().unwrap()),
        "object_count"
    );
}

#[test]
fn structured_targets_are_self_contained_and_reject_record_overlaps() {
    let tmp = fixture();
    let moved = tempfile::tempdir().unwrap();
    page(
        tmp.path(),
        vec![
            hotspot("m0", 0, 0),
            control("va0", "variable", &[("val", n(-7))]),
        ],
        None,
    );
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let expected = prepared.link(true).unwrap().bytes;
    prepared.save_plan(moved.path(), true, None).unwrap();
    let mut invalid = prepared.pages[0].object.clone();
    invalid.metadata.layout.as_mut().unwrap().objects[1].offset = 0;
    assert_eq!(code(&invalid.encode().unwrap_err()), "object_layout");
    let mut invalid = prepared.pages[0].object.clone();
    invalid.metadata.layout.as_mut().unwrap().objects[1].length = u32::MAX;
    assert_eq!(code(&invalid.encode().unwrap_err()), "object_length");
    let mut invalid = prepared.pages[0].object.clone();
    invalid
        .metadata
        .relocations
        .push(invalid.metadata.relocations[0].clone());
    assert_eq!(code(&invalid.encode().unwrap_err()), "object_relocation");
    let mut invalid = prepared.pages[0].object.clone();
    let layout = invalid.metadata.layout.as_ref().unwrap();
    let current = Object::parse(invalid.record(1).unwrap()).unwrap();
    let changed = current
        .rebuild(&BTreeMap::from([("id".into(), vec![9])]), &BTreeMap::new())
        .unwrap();
    let start = layout.objects[1].offset as usize;
    invalid.payload[start..start + changed.len()].copy_from_slice(&changed);
    assert_eq!(
        code(
            &hmi_linker::link(
                &prepared.target,
                &prepared.program.object,
                &[invalid],
                false
            )
            .unwrap_err()
        ),
        "object_record"
    );
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        expected
    );
}
