use hmi_core::{
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::{Target, TargetConfig},
};
use hmi_format::{container::Container, project::Project};
use std::{collections::BTreeMap, fs, path::Path};
mod zi {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/zi.rs"
    ));
}

const KINDS: [&str; 5] = [
    "combo_box",
    "text_select",
    "sliding_text",
    "file_browser",
    "data_record",
];
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
    fs::write(tmp.path().join("assets/ui.zi"), zi::font()).unwrap();
    fs::write(
        tmp.path().join("assets/ui.asset.toml"),
        "schema='hmi.asset/1'\nkind='font'\nsource='ui.zi'\n",
    )
    .unwrap();
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.assets.items.push(AssetSource {
        key: "ui".into(),
        source: "assets/ui.asset.toml".into(),
    });
    for (key, w, h) in [("full", 480, 272), ("panel", 200, 160), ("icon", 16, 16)] {
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
fn widget(kind: &str, mode: i64) -> ObjectFile {
    let mut p = BTreeMap::from([
        ("x".into(), 10.into()),
        ("y".into(), 20.into()),
        ("w".into(), 200.into()),
        ("h".into(), 160.into()),
        ("sta".into(), mode.into()),
        ("font".into(), reference("ui")),
        ("pic".into(), reference("panel")),
        ("picc".into(), reference("full")),
    ]);
    match kind {
        "combo_box" | "text_select" => {
            p.insert("path".into(), "A\nB\nC".into());
            p.insert("val".into(), 1.into());
            if kind == "combo_box" {
                p.insert("txt".into(), "INIT".into());
            }
        }
        "sliding_text" => {
            p.insert("txt".into(), "A\nB\nC".into());
        }
        "file_browser" => {
            p.insert("pic1".into(), reference("icon"));
            p.insert("pic2".into(), reference("icon"));
        }
        _ => {
            p.extend([
                ("path".into(), format!("sd0/records{mode}.data").into()),
                ("lenth".into(), 32.into()),
                ("maxval".into(), 16.into()),
                ("dez".into(), 2.into()),
                ("format".into(), "90^90".into()),
                ("dir".into(), "A^B".into()),
            ]);
        }
    }
    ObjectFile {
        name: format!("o{}", KINDS.iter().position(|k| *k == kind).unwrap()),
        type_name: kind.into(),
        properties: p,
        events: BTreeMap::new(),
    }
}
fn save(root: &Path, name: &str, objects: Vec<ObjectFile>) {
    let directory = root.join("pages").join(name);
    fs::create_dir_all(&directory).unwrap();
    let mut page = hmi_page_compiler::empty_page(name);
    page.objects = objects;
    fs::write(directory.join("page.toml"), file::toml(&page).unwrap()).unwrap();
}
fn sources(root: &Path, names: &[&str]) {
    let path = root.join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.pages.sources = names
        .iter()
        .map(|name| format!("pages/{name}/page.toml"))
        .collect();
    fs::write(path, file::toml(&p).unwrap()).unwrap();
}
fn batch(root: &Path) {
    sources(root, &["crop", "solid", "picture", "transparent"]);
    for (mode, name) in ["crop", "solid", "picture", "transparent"]
        .iter()
        .enumerate()
    {
        save(
            root,
            name,
            KINDS.iter().map(|kind| widget(kind, mode as i64)).collect(),
        );
    }
}

#[test]
fn four_source_modes_only_change_sta_and_preserve_cached_fields_and_dependencies() {
    let tmp = setup();
    let p: ProjectSource = file::read_toml(&tmp.path().join("hmi.toml")).unwrap();
    let target = Target::resolve(&p.target).unwrap();
    // 这里只核对编译记录，不重复编码相同图片；最终资源绑定由批量链接测试覆盖。
    for (kind, count) in KINDS.into_iter().zip([55, 41, 44, 58, 72]) {
        let mut previous: Option<hmi_artifact::CompiledObject> = None;
        for mode in [2, 0, 1, 3] {
            let mut o = widget(kind, mode);
            if kind == "data_record" {
                o.properties
                    .insert("path".into(), "sd0/records.data".into());
            }
            save(tmp.path(), "page0", vec![o]);
            let current =
                hmi_page_compiler::compile_page(&tmp.path().join("pages/page0/page.toml"), &target)
                    .unwrap()
                    .object;
            let object = hmi_format::page::Object::parse(current.record(1).unwrap()).unwrap();
            assert_eq!(object.attributes.len(), count);
            if let Some(old) = previous {
                let raw = hmi_format::page::Object::parse(old.record(1).unwrap()).unwrap();
                assert_eq!(
                    raw.rebuild(
                        &BTreeMap::from([("sta".into(), vec![mode as u8])]),
                        &BTreeMap::new()
                    )
                    .unwrap(),
                    object.raw,
                    "{kind}/{mode}"
                );
                assert_eq!(
                    serde_json::to_value(old.metadata.relocations).unwrap(),
                    serde_json::to_value(&current.metadata.relocations).unwrap()
                );
                assert_eq!(
                    serde_json::to_value(old.metadata.runtime_dependencies).unwrap(),
                    serde_json::to_value(&current.metadata.runtime_dependencies).unwrap()
                );
            }
            previous = Some(current);
        }
    }
}

#[test]
fn twenty_combinations_link_in_all_directions_with_inactive_small_crop_references() {
    let tmp = setup();
    batch(tmp.path());
    let path = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&path).unwrap();
    for orientation in [0, 90, 180, 270] {
        config.target.orientation = orientation;
        fs::write(&path, file::toml(&config).unwrap()).unwrap();
        let target = Target::resolve(&config.target).unwrap();
        fs::write(
            tmp.path().join("assets/full.bmp"),
            bmp(target.canvas.width, target.canvas.height),
        )
        .unwrap();
        for (mode, name) in ["crop", "solid", "picture", "transparent"]
            .iter()
            .enumerate()
        {
            let objects = KINDS
                .iter()
                .map(|kind| {
                    let mut o = widget(kind, mode as i64);
                    if mode != 0 {
                        o.properties.insert("picc".into(), reference("panel"));
                    }
                    o
                })
                .collect();
            save(tmp.path(), name, objects);
        }
        let prepared = hmi_cli::prepare(tmp.path()).unwrap();
        prepared.check(true).unwrap();
        assert!(!tmp.path().join(".build").exists());
        let linked = prepared.link(true).unwrap();
        let project = Project::parse(&Container::parse(linked.bytes).unwrap()).unwrap();
        assert_eq!(linked.map["runtime_files"].as_array().unwrap().len(), 4);
        for (mode, (_, page)) in project.pages.iter().enumerate() {
            for o in &page.objects[1..] {
                assert_eq!(o.integer("sta"), Some(mode as u32));
                assert_eq!(o.integer("pic"), Some(1));
                assert_eq!(o.integer("picc"), Some(if mode == 0 { 0 } else { 1 }));
            }
            assert_eq!(page.objects[4].integer("pic1"), Some(2));
            assert_eq!(page.objects[4].integer("pic2"), Some(2));
        }
    }
}

#[test]
fn required_images_and_geometry_fail_before_overwriting_hmi() {
    let tmp = setup();
    let output = tmp.path().join("previous.HMI");
    fs::write(&output, b"previous").unwrap();
    for kind in KINDS {
        for (mode, field, value, code) in [
            (2, "pic", None, "property_required"),
            (0, "picc", None, "property_required"),
            (
                2,
                "pic",
                Some(toml::Value::Table(toml::map::Map::from_iter([(
                    "none".into(),
                    true.into(),
                )]))),
                "property_required",
            ),
            (0, "picc", Some(reference("panel")), "crop_image"),
            (0, "x", Some(470.into()), "crop_geometry"),
            (4, "sta", Some(4.into()), "property_range"),
        ] {
            let mut o = widget(kind, mode);
            if let Some(v) = value {
                o.properties.insert(field.into(), v);
            } else {
                o.properties.remove(field);
            }
            save(tmp.path(), "page0", vec![o]);
            let e = hmi_cli::build(tmp.path(), Some(&output), true)
                .err()
                .unwrap();
            assert_eq!(
                e.as_diagnostic().unwrap().code,
                code,
                "{kind}.{field}/{mode}"
            );
            assert_eq!(fs::read(&output).unwrap(), b"previous");
        }
        // 透明和单色可以没有背景资源，但文件浏览器的两个图标仍是必填项。
        for mode in [1, 3] {
            let mut o = widget(kind, mode);
            o.properties.remove("pic");
            o.properties.remove("picc");
            save(tmp.path(), "page0", vec![o]);
            hmi_cli::prepare(tmp.path()).unwrap().check(true).unwrap();
        }
    }
    let mut o = widget("file_browser", 3);
    o.properties.insert(
        "pic2".into(),
        toml::Value::Table(toml::map::Map::from_iter([("none".into(), true.into())])),
    );
    save(tmp.path(), "page0", vec![o]);
    assert_eq!(
        hmi_cli::prepare(tmp.path())
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "resource_reference"
    );
}

#[test]
fn final_ids_runtime_dependencies_and_independent_linking_survive_reordering() {
    let tmp = setup();
    batch(tmp.path());
    sources(tmp.path(), &["transparent", "picture", "solid", "crop"]);
    for (mode, name) in ["crop", "solid", "picture", "transparent"]
        .iter()
        .enumerate()
    {
        let mut objects = KINDS
            .iter()
            .map(|kind| widget(kind, mode as i64))
            .collect::<Vec<_>>();
        objects.reverse();
        objects[4].events.insert("up".into(), "up.s".into());
        save(tmp.path(), name, objects);
        fs::write(
            tmp.path().join(format!("pages/{name}/up.s")),
            "sys0=o0.val\n",
        )
        .unwrap();
    }
    let path = tmp.path().join("hmi.toml");
    let mut p: ProjectSource = file::read_toml(&path).unwrap();
    p.assets.items.reverse();
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let linked = prepared.link(true).unwrap();
    let project = Project::parse(&Container::parse(linked.bytes.clone()).unwrap()).unwrap();
    for (_, page) in project.pages {
        assert_eq!(page.objects[1].type_code, 66);
        assert_eq!(page.objects[1].integer("pic"), Some(1));
        assert_eq!(page.objects[2].integer("pic1"), Some(0));
        assert_eq!(page.objects[5].events[1].lines[0], b"sys0=o0.val");
    }
    for dependency in linked.map["runtime_files"].as_array().unwrap() {
        assert_eq!(dependency["users"][0]["object_id"], 1);
        assert_eq!(dependency["generated"], false);
    }
    let moved = tempfile::tempdir().unwrap();
    prepared.save_plan(moved.path(), true, None).unwrap();
    let mut object =
        hmi_artifact::CompiledObject::read(&moved.path().join("obj/p00000.hmo")).unwrap();
    object
        .metadata
        .relocations
        .retain(|r| !(r.object == 1 && r.property == "pic"));
    assert!(object.validate().is_err());
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        linked.bytes
    );
}
