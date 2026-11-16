use hmi_core::{
    asset::{AudioEncoding, AudioSource, Clip, Delivery},
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, project::Project};
use std::{collections::BTreeMap, fs, path::Path};
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/wave.rs"
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
        tmp.path().join("assets/input.wav"),
        fixture::wave(22050, 1, 22050),
    )
    .unwrap();
    let c = AudioSource {
        schema: "hmi.asset/1".into(),
        kind: "audio".into(),
        input: "source".into(),
        source: "input.wav".into(),
        encoding: AudioEncoding::default(),
        clip: Clip::default(),
        delivery: Delivery::default(),
    };
    fs::write(
        tmp.path().join("assets/tone.asset.toml"),
        file::toml(&c).unwrap(),
    )
    .unwrap();
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.assets.items.push(AssetSource {
        key: "tone".into(),
        source: "assets/tone.asset.toml".into(),
    });
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    tmp
}
fn object() -> ObjectFile {
    ObjectFile {
        name: "wav0".into(),
        type_name: "audio".into(),
        properties: BTreeMap::from([(
            "vid".into(),
            toml::Value::Table(toml::map::Map::from_iter([("ref".into(), "tone".into())])),
        )]),
        events: BTreeMap::new(),
    }
}
fn save(root: &Path, objects: Vec<ObjectFile>, script: Option<&str>) {
    let mut p = hmi_page_compiler::empty_page("p_audio");
    p.objects = objects;
    if let Some(s) = script {
        p.events.insert("load".into(), "load.s".into());
        fs::write(root.join("pages/page0/load.s"), s).unwrap();
    }
    fs::write(root.join("pages/page0/page.toml"), file::toml(&p).unwrap()).unwrap();
}

#[test]
fn native_audio_pages_match_and_keep_nonvisual_fields() {
    let tmp = setup();
    for (value, sha) in [
        (
            0,
            "a2a4aadb176abde3d10e20871a529c1e1b885c446e30edfc5b5e9a319ebdfb36",
        ),
        (
            1,
            "11e51ccbfbef422fe7728ea404c635fe6fbf9b4414d3d8ee0f82bd806d75316a",
        ),
    ] {
        let mut w = object();
        w.properties.insert("loop".into(), value.into());
        save(tmp.path(), vec![w], None);
        let p = hmi_cli::prepare(tmp.path()).unwrap();
        p.check(true).unwrap();
        assert!(!tmp.path().join(".build").exists());
        let linked = p.link(true).unwrap();
        let c = Container::parse(linked.bytes).unwrap();
        let p = Project::parse(&c).unwrap();
        let page = &p.pages[0].1;
        assert_eq!(hmi_core::sha256(&page.raw), sha);
        assert_eq!(page.objects[1].attributes.len(), 20);
        assert!(page.objects[1].attribute("x").is_none());
        assert_eq!(page.objects[1].integer("molloc"), Some(64000));
        assert_eq!(page.objects[1].events[0].kind, "playend");
        assert!(
            p.references
                .iter()
                .any(|r| r.kind == "wav" && r.resource == "0.wav")
        );
    }
}
#[test]
fn audio_id_order_and_independent_linking_need_no_source_or_decoder() {
    let tmp = setup();
    fs::copy(
        tmp.path().join("assets/tone.asset.toml"),
        tmp.path().join("assets/second.asset.toml"),
    )
    .unwrap();
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.assets.items.insert(
        0,
        AssetSource {
            key: "other".into(),
            source: "assets/second.asset.toml".into(),
        },
    );
    fs::write(&path, file::toml(&p).unwrap()).unwrap();
    let mut o = object();
    o.events.insert("playend".into(), "end.s".into());
    fs::write(tmp.path().join("pages/page0/end.s"), "sys0=wav0.qty").unwrap();
    save(tmp.path(), vec![o], None);
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let expected = prepared.link(true).unwrap();
    let parsed = Project::parse(&Container::parse(expected.bytes.clone()).unwrap()).unwrap();
    assert_eq!(parsed.pages[0].1.objects[1].integer("vid"), Some(1));
    let moved = tempfile::tempdir().unwrap();
    prepared.save_plan(moved.path(), true, None).unwrap();
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        expected.bytes
    );
}
#[test]
fn unsupported_fields_readonly_writes_and_input_damage_preserve_hmi() {
    let tmp = setup();
    let output = tmp.path().join("old.HMI");
    fs::write(&output, b"previous").unwrap();
    for (key, value) in [
        ("x", 0),
        ("en", 3),
        ("dis", 9),
        ("loop", 2),
        ("from", 1),
        ("fps", 1),
        ("qty", 0),
    ] {
        let mut o = object();
        o.properties.insert(key.into(), value.into());
        save(tmp.path(), vec![o], None);
        assert!(
            hmi_cli::build(tmp.path(), Some(&output), false).is_err(),
            "{key}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }
    for key in ["stim", "qty", "fps"] {
        save(tmp.path(), vec![object()], Some(&format!("wav0.{key}=1")));
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
    save(tmp.path(), vec![object()], None);
    fs::write(tmp.path().join("assets/input.wav"), b"broken").unwrap();
    assert!(hmi_cli::build(tmp.path(), Some(&output), false).is_err());
    assert_eq!(fs::read(output).unwrap(), b"previous");
}
