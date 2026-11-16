use hmi_core::{
    asset::{AnimationEncoding, AnimationSource, Delivery, ImageSource},
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, project::Project};
use image::ImageEncoder;
use std::{collections::BTreeMap, fs, path::Path};
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/gif.rs"
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
    fs::write(
        tmp.path().join("assets/input.gif"),
        fixture::gif(
            160,
            80,
            &[fixture::full(160, 80, 1, 50), fixture::full(160, 80, 2, 50)],
        ),
    )
    .unwrap();
    let source = AnimationSource {
        schema: "hmi.asset/1".into(),
        kind: "animation".into(),
        input: "source".into(),
        source: "input.gif".into(),
        encoding: AnimationEncoding::default(),
        delivery: Delivery::default(),
    };
    fs::write(
        tmp.path().join("assets/clip.asset.toml"),
        file::toml(&source).unwrap(),
    )
    .unwrap();
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.assets.items.push(AssetSource {
        key: "clip".into(),
        source: "assets/clip.asset.toml".into(),
    });
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    tmp
}
fn reference(key: &str) -> toml::Value {
    toml::Value::Table(toml::map::Map::from_iter([("ref".into(), key.into())]))
}
fn animation(name: &str, key: &str) -> ObjectFile {
    ObjectFile {
        name: name.into(),
        type_name: "animation".into(),
        properties: BTreeMap::from([
            ("x".into(), 0.into()),
            ("y".into(), 0.into()),
            ("w".into(), 160.into()),
            ("h".into(), 80.into()),
            ("vid".into(), reference(key)),
        ]),
        events: BTreeMap::new(),
    }
}
fn save(root: &Path, name: &str, objects: Vec<ObjectFile>, script: Option<&str>) {
    let mut p = hmi_page_compiler::empty_page(name);
    p.objects = objects;
    if let Some(script) = script {
        p.events.insert("load".into(), "load.s".into());
        fs::write(root.join("pages/page0/load.s"), script).unwrap();
    }
    fs::write(root.join("pages/page0/page.toml"), file::toml(&p).unwrap()).unwrap();
}

#[test]
fn two_native_animation_pages_match_without_backfilling_runtime_state() {
    let tmp = setup();
    for (loop_value, sha) in [
        (
            0,
            "85c07b42a5e4473da8d7e6eba0263e35ee6eb584462eee4db4dced28ca9c33a9",
        ),
        (
            1,
            "310c3f02491d193a08344a0cbefc3161bf3ec8cf9bcb0e43d79ab2220b41e1d8",
        ),
    ] {
        let mut gm = animation("gm0", "clip");
        gm.properties.insert("loop".into(), loop_value.into());
        save(tmp.path(), "p_anim", vec![gm], None);
        let prepared = hmi_cli::prepare(tmp.path()).unwrap();
        prepared.check(true).unwrap();
        assert!(!tmp.path().join(".build").exists());
        let linked = prepared.link(true).unwrap();
        let c = Container::parse(linked.bytes).unwrap();
        let p = Project::parse(&c).unwrap();
        let page = &p.pages[0].1;
        assert_eq!(hmi_core::sha256(&page.raw), sha);
        assert_eq!(page.objects[1].attributes.len(), 33);
        assert_eq!(page.objects[1].integer("qty"), Some(0));
        assert_eq!(page.objects[1].integer("stim"), Some(0));
        assert_eq!(page.objects[1].integer("en"), Some(0));
        assert_eq!(page.objects[1].integer("molloc"), Some(12000));
        assert_eq!(
            page.objects[1]
                .events
                .iter()
                .map(|e| e.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["down", "up", "playend"]
        );
        assert_eq!(c.entries.len(), 5);
        assert!(
            p.references
                .iter()
                .any(|r| r.kind == "gmov" && r.resource == "0.gmov")
        );
    }
}

#[test]
fn animation_id_order_and_event_owners_survive_reordering_and_independent_linking() {
    let tmp = setup();
    let root = tmp.path();
    fs::copy(
        root.join("assets/clip.asset.toml"),
        root.join("assets/second.asset.toml"),
    )
    .unwrap();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &[220, 80, 30, 255].repeat(64 * 48),
            64,
            48,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    fs::write(root.join("assets/pic.png"), png).unwrap();
    fs::write(
        root.join("assets/pic.asset.toml"),
        file::toml(&ImageSource {
            source: "pic.png".into(),
            ..Default::default()
        })
        .unwrap(),
    )
    .unwrap();
    let path = root.join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
    config.assets.items.extend([
        AssetSource {
            key: "second".into(),
            source: "assets/second.asset.toml".into(),
        },
        AssetSource {
            key: "picture".into(),
            source: "assets/pic.asset.toml".into(),
        },
    ]);
    let mut gm = animation("gm0", "clip");
    gm.events.insert("playend".into(), "gm0.playend.s".into());
    fs::write(root.join("pages/page0/gm0.playend.s"), "sys0=gm0.qty").unwrap();
    for expected_id in [0, 1] {
        fs::write(&path, file::toml(&config).unwrap()).unwrap();
        save(
            root,
            "page0",
            vec![animation("gm1", "second"), gm.clone()],
            None,
        );
        let linked = hmi_cli::prepare(root).unwrap().link(true).unwrap();
        let c = Container::parse(linked.bytes).unwrap();
        let p = Project::parse(&c).unwrap();
        assert_eq!(p.pages[0].1.objects[2].integer("vid"), Some(expected_id));
        assert_eq!(
            p.pages[0].1.objects[2].events[2].lines,
            vec![b"sys0=gm0.qty".to_vec()]
        );
        assert!(c.entries.iter().any(|e| e.name == "0.i"));
        assert!(c.entries.iter().any(|e| e.name == "1.gmov"));
        config.assets.items.swap(0, 1);
    }
    let p = hmi_cli::prepare(root).unwrap();
    let expected = p.link(true).unwrap().bytes;
    let moved = tempfile::tempdir().unwrap();
    p.save_plan(moved.path(), true, None).unwrap();
    let mut bad = hmi_artifact::CompiledObject::read(&moved.path().join("obj/p00000.hmo")).unwrap();
    bad.metadata
        .relocations
        .iter_mut()
        .find(|r| r.kind == hmi_artifact::RelocationKind::AnimationId)
        .unwrap()
        .kind = hmi_artifact::RelocationKind::ImageId;
    assert!(bad.validate().is_err());
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        expected
    );
}

#[test]
fn invalid_source_properties_and_wrong_media_classes_keep_previous_output() {
    let tmp = setup();
    let output = tmp.path().join("old.HMI");
    fs::write(&output, b"previous").unwrap();
    for (property, value) in [
        ("en", 3),
        ("from", 2),
        ("dis", 9),
        ("dis", 1001),
        ("loop", 2),
        ("stim", 0),
        ("qty", 0),
        ("tim", -1),
    ] {
        let mut gm = animation("gm0", "clip");
        gm.properties.insert(property.into(), value.into());
        save(tmp.path(), "page0", vec![gm], None);
        assert!(
            hmi_cli::build(tmp.path(), Some(&output), false).is_err(),
            "{property}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }
    let mut gm = animation("gm0", "clip");
    gm.properties.insert(
        "vid".into(),
        toml::Value::Table(toml::map::Map::from_iter([("none".into(), true.into())])),
    );
    save(tmp.path(), "page0", vec![gm], None);
    assert!(hmi_cli::prepare(tmp.path()).is_err());
    let mut pic = animation("p0", "clip");
    pic.type_name = "picture".into();
    pic.properties.remove("vid");
    pic.properties.insert("pic".into(), reference("clip"));
    save(tmp.path(), "page0", vec![pic], None);
    assert!(hmi_cli::prepare(tmp.path()).unwrap().check(false).is_err());
    save(tmp.path(), "page0", vec![animation("gm0", "clip")], None);
    fs::write(
        tmp.path().join("assets/input.gif"),
        fixture::gif(2, 2, &[fixture::full(2, 2, 1, 0)]),
    )
    .unwrap();
    assert!(hmi_cli::build(tmp.path(), Some(&output), false).is_err());
    assert_eq!(fs::read(output).unwrap(), b"previous");
}

#[test]
fn timing_adjustments_are_reported_and_werror_preserves_old_hmi() {
    let tmp = setup();
    save(tmp.path(), "page0", vec![animation("gm0", "clip")], None);
    fs::write(
        tmp.path().join("assets/input.gif"),
        fixture::gif(
            16,
            8,
            &[fixture::full(16, 8, 1, 2), fixture::full(16, 8, 2, 5)],
        ),
    )
    .unwrap();
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    assert_eq!(p.check(false).unwrap()[0].code, "animation_timing");
    let output = tmp.path().join("old.HMI");
    fs::write(&output, b"previous").unwrap();
    assert_eq!(
        hmi_cli::build(tmp.path(), Some(&output), true)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "animation_timing"
    );
    assert_eq!(fs::read(output).unwrap(), b"previous");
    let linked = p.link(false).unwrap();
    assert_eq!(
        linked.map["assets"][0]["info"]["animation"]["delays_ms"],
        serde_json::json!([30, 40])
    );
}

#[test]
fn readonly_results_playend_scripts_and_four_directions_use_the_same_rules() {
    let tmp = setup();
    for key in ["qty", "stim"] {
        save(
            tmp.path(),
            "page0",
            vec![animation("gm0", "clip")],
            Some(&format!("gm0.{key}=1")),
        );
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
    save(
        tmp.path(),
        "page0",
        vec![animation("gm0", "clip")],
        Some("gm0.en=2\nsys0=gm0.stim"),
    );
    let path = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
    for orientation in [0, 90, 180, 270] {
        config.target.orientation = orientation;
        fs::write(&path, file::toml(&config).unwrap()).unwrap();
        hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
    }
    for profile in ["TJC4832K035_011", "TJC4832T035_011", "TJC3224T028_011"] {
        config.target.profile = profile.into();
        let target = hmi_core::target::Target::resolve(&config.target).unwrap();
        assert_eq!(
            hmi_page_compiler::compile_page(&tmp.path().join("pages/page0/page.toml"), &target)
                .err()
                .unwrap()
                .as_diagnostic()
                .unwrap()
                .code,
            "control_capability"
        );
    }
}
