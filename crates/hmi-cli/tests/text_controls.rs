use hmi_artifact::RelocationKind;
use hmi_core::{
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, project::Project};
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
            profile: "TJC4832T035_011".into(),
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
    p.assets.items = ["font_a", "font_b"]
        .map(|key| AssetSource {
            key: key.into(),
            source: "assets/font.asset.toml".into(),
        })
        .into();
    fs::write(tmp.path().join("hmi.toml"), file::toml(&p).unwrap()).unwrap();
    tmp
}
fn reference(field: &str, value: toml::Value) -> toml::Value {
    toml::Value::Table(toml::map::Map::from_iter([(field.into(), value)]))
}
fn widget(kind: &str, txt: &str) -> ObjectFile {
    ObjectFile {
        original_record: None,
        name: if kind == "text" { "t0" } else { "b0" }.into(),
        type_name: kind.into(),
        properties: BTreeMap::from([
            ("x".into(), 10.into()),
            ("y".into(), 10.into()),
            ("w".into(), 280.into()),
            ("h".into(), 100.into()),
            ("font".into(), reference("ref", "font_a".into())),
            ("txt".into(), txt.into()),
        ]),
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
fn linked_page(root: &Path) -> hmi_format::page::Page {
    let bytes = hmi_cli::prepare(root).unwrap().link(true).unwrap().bytes;
    Project::parse(&Container::parse(bytes).unwrap())
        .unwrap()
        .pages
        .remove(0)
        .1
}

#[test]
fn eight_native_page_vectors_cover_text_button_styles_geometry_and_events() {
    // 固定原生页面摘要；测试只使用独立小字库，不读取原生 HMI 或私有素材。
    let cases = [
        (
            "text",
            "ABC",
            280,
            0,
            false,
            "b4ac62aab184ee032317e9b992b9b169d756d561403a8e3e6fc44abb4840bc6c",
        ),
        (
            "text",
            "AB12",
            280,
            0,
            false,
            "ef34ac7f7f554620f3324065aab619aa706facc31a988f09363eb55e4f992931",
        ),
        (
            "text",
            "ABC",
            240,
            0,
            false,
            "15423e7a5b8a1127fe9f47642e246612959f6d922d74f125f8ceee5819843812",
        ),
        (
            "text",
            "ABC",
            280,
            1,
            false,
            "bc4dd7f35844ff055cf913e6281136837ee8df5d98b25bc064756ab502d5eb5e",
        ),
        (
            "button",
            "OK",
            280,
            4,
            false,
            "1b81ff6618304447332805d571d22affe412069b73da3ba2d055a3b479b94a4b",
        ),
        (
            "button",
            "GO1",
            280,
            4,
            false,
            "375e39f375b3e46111051d25239a5887c383144897d7724defade49f6b3bdf47",
        ),
        (
            "button",
            "OK",
            280,
            4,
            true,
            "d3e06b75d4e99176cd59eeff5ac8e2ad9cef775b7ad1b2fa5e949f7ae3d8bf5a",
        ),
        (
            "button",
            "OK",
            280,
            1,
            false,
            "51c6e93f81c989e731407e1c90eda12fc4dcfc9772526c18d7eb44c2663f5502",
        ),
    ];
    for (kind, txt, width, style, events, expected) in cases {
        let tmp = setup();
        let mut control = widget(kind, txt);
        control.properties.insert("w".into(), width.into());
        control.properties.insert("style".into(), style.into());
        if events {
            for (slot, code) in [("down", "sys0=1"), ("up", "sys0=2")] {
                let name = format!("{slot}.s");
                fs::write(tmp.path().join("pages/page0").join(&name), code).unwrap();
                control.events.insert(slot.into(), name);
            }
        }
        save(tmp.path(), vec![control]);
        let p = linked_page(tmp.path());
        assert_eq!(
            hmi_core::sha256(&p.raw),
            expected,
            "{kind}, txt={txt}, style={style}"
        );
    }
}

#[test]
fn symbolic_fonts_reorder_numeric_fonts_use_final_ids_and_targets_are_self_contained() {
    let tmp = setup();
    let mut button = widget("button", "A");
    button
        .properties
        .insert("font".into(), reference("id", 0.into()));
    save(tmp.path(), vec![widget("text", "A"), button]);
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let object = &prepared.pages[0].object;
    assert_eq!(
        object
            .metadata
            .relocations
            .iter()
            .filter(|r| r.kind == RelocationKind::FontId)
            .count(),
        2
    );
    let before = linked_page(tmp.path());
    assert_eq!(before.objects[1].integer("font"), Some(0));
    let source = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&source).unwrap();
    p.assets.items.swap(0, 1);
    fs::write(&source, file::toml(&p).unwrap()).unwrap();
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    prepared.check(true).unwrap();
    let linked = prepared.link(true).unwrap();
    let after = Project::parse(&Container::parse(linked.bytes.clone()).unwrap())
        .unwrap()
        .pages
        .remove(0)
        .1;
    assert_eq!(after.objects[1].integer("font"), Some(1));
    assert_eq!(after.objects[2].integer("font"), Some(0));
    assert_eq!(after.objects[1].attribute("font").unwrap().value.len(), 1);
    assert_eq!(before.objects[1].integer("pic"), Some(65535));
    let moved = tempfile::tempdir().unwrap();
    prepared.save_plan(moved.path(), true, None).unwrap();
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        linked.bytes
    );
}

#[test]
fn bad_font_references_and_utf8_capacity_preserve_existing_output() {
    for value in [
        0.into(),
        reference("none", true.into()),
        reference("id", 255.into()),
        reference("id", 2.into()),
        reference("ref", "missing".into()),
    ] {
        let tmp = setup();
        let mut w = widget("text", "A");
        w.properties.insert("font".into(), value);
        save(tmp.path(), vec![w]);
        let output = tmp.path().join("old.HMI");
        fs::write(&output, b"previous").unwrap();
        assert!(hmi_cli::build(tmp.path(), Some(&output), false).is_err());
        assert_eq!(fs::read(output).unwrap(), b"previous");
    }
    let tmp = setup();
    let mut w = widget("text", "测试A");
    w.properties.insert("txt_maxl".into(), 7.into());
    save(tmp.path(), vec![w.clone()]);
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    p.check(true).unwrap();
    let mut broken = p.pages[0].object.clone();
    broken
        .metadata
        .relocations
        .retain(|r| r.kind != RelocationKind::FontId);
    assert!(broken.validate().is_err());
    let mut broken = p.pages[0].object.clone();
    let current = hmi_format::page::Object::parse(broken.record(1).unwrap()).unwrap();
    let bytes = current
        .rebuild(
            &BTreeMap::from([("font".into(), vec![1])]),
            &BTreeMap::new(),
        )
        .unwrap();
    let offset = broken.metadata.layout.as_ref().unwrap().objects[1].offset as usize;
    broken.payload[offset..offset + bytes.len()].copy_from_slice(&bytes);
    let infos = p
        .assets
        .iter()
        .map(|a| (a.key.clone(), a.probe.info.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        hmi_linker::check(&p.target, &p.program.object, &[broken], &infos, true)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "object_relocation"
    );
    let mut broken = p.pages[0].object.clone();
    broken
        .metadata
        .relocations
        .iter_mut()
        .find(|r| r.kind == RelocationKind::FontId)
        .unwrap()
        .kind = RelocationKind::ImageId;
    assert!(broken.validate().is_err());
    w.properties.insert("txt_maxl".into(), 6.into());
    save(tmp.path(), vec![w]);
    let e = hmi_cli::prepare(tmp.path()).err().unwrap();
    let d = e.as_diagnostic().unwrap();
    assert_eq!(d.code, "text_capacity");
    assert_eq!(d.location.property.as_deref(), Some("txt"));
    // 字库只是像素和索引，UTF-8 标签不代表已验证所有字符的覆盖范围。
}

#[test]
fn unsupported_background_styles_and_font_id_capacity_are_rejected() {
    let tmp = setup();
    for (kind, property, value) in [
        ("text", "sta", 0),
        ("text", "sta", 2),
        ("text", "style", 4),
        ("button", "style", 2),
        ("button", "val", 2),
    ] {
        let mut w = widget(kind, "A");
        w.properties.insert(property.into(), value.into());
        save(tmp.path(), vec![w]);
        assert!(hmi_cli::prepare(tmp.path()).is_err());
    }
    save(tmp.path(), vec![widget("text", "A")]);
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    let infos = (0..256)
        .map(|i| (format!("f{i}"), p.assets[0].probe.info.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        hmi_linker::check(
            &p.target,
            &p.program.object,
            &[p.pages[0].object.clone()],
            &infos,
            false
        )
        .unwrap_err()
        .as_diagnostic()
        .unwrap()
        .code,
        "resource_count"
    );
    let mut w = widget("text", "A");
    w.properties
        .insert("font".into(), reference("id", 254.into()));
    save(tmp.path(), vec![w]);
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    hmi_linker::check(
        &p.target,
        &p.program.object,
        &[p.pages[0].object.clone()],
        &infos[..255],
        true,
    )
    .unwrap();
}
