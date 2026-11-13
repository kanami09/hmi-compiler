use hmi_core::{
    file,
    model::{AssetSource, ObjectFile, PageSource, ProjectSource},
    target::TargetConfig,
};
use hmi_format::{container::Container, project::Project};
use std::{collections::BTreeMap, fs, path::Path};

fn bmp(width: u32, height: u32, red: u8) -> Vec<u8> {
    let stride = (width * 3 + 3) & !3;
    let size = 54 + stride * height;
    let mut b = vec![0; size as usize];
    b[..2].copy_from_slice(b"BM");
    b[2..6].copy_from_slice(&size.to_le_bytes());
    b[10..14].copy_from_slice(&54u32.to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());
    b[18..22].copy_from_slice(&width.to_le_bytes());
    b[22..26].copy_from_slice(&height.to_le_bytes());
    b[26..30].copy_from_slice(&[1, 0, 24, 0]);
    for y in 0..height {
        for x in 0..width {
            b[(54 + y * stride + x * 3 + 2) as usize] = red;
        }
    }
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
    for (name, w, h, color) in [("background", 320, 480, 0), ("small", 17, 19, 255)] {
        fs::write(
            tmp.path().join(format!("assets/{name}.bmp")),
            bmp(w, h, color),
        )
        .unwrap();
        fs::write(
            tmp.path().join(format!("assets/{name}.asset.toml")),
            format!("schema='hmi.asset/1'\nkind='image'\nsource='{name}.bmp'\n"),
        )
        .unwrap();
    }
    let mut project: ProjectSource = file::read_toml(&tmp.path().join("hmi.toml")).unwrap();
    project.assets.items = vec![
        AssetSource {
            key: "background".into(),
            source: "assets/background.asset.toml".into(),
        },
        AssetSource {
            key: "small".into(),
            source: "assets/small.asset.toml".into(),
        },
    ];
    fs::write(tmp.path().join("hmi.toml"), file::toml(&project).unwrap()).unwrap();
    let mut page = hmi_page_compiler::empty_page("page0");
    page.objects = vec![
        widget("p0", "picture", (20, 20, 17, 19), "pic", "small"),
        widget("q0", "crop", (140, 200, 96, 96), "picc", "background"),
    ];
    save_page(tmp.path(), &page);
    tmp
}
fn widget(
    name: &str,
    kind: &str,
    geometry: (i64, i64, i64, i64),
    property: &str,
    key: &str,
) -> ObjectFile {
    let (x, y, w, h) = geometry;
    ObjectFile {
        name: name.into(),
        type_name: kind.into(),
        properties: BTreeMap::from([
            ("x".into(), x.into()),
            ("y".into(), y.into()),
            ("w".into(), w.into()),
            ("h".into(), h.into()),
            (
                property.into(),
                toml::Value::Table(toml::map::Map::from_iter([("ref".into(), key.into())])),
            ),
        ]),
        events: BTreeMap::new(),
    }
}
fn save_page(root: &Path, page: &PageSource) {
    fs::write(
        root.join("pages/page0/page.toml"),
        file::toml(page).unwrap(),
    )
    .unwrap();
}
fn parse(bytes: Vec<u8>) -> (Container, Project) {
    let container = Container::parse(bytes).unwrap();
    let project = Project::parse(&container).unwrap();
    (container, project)
}

#[test]
fn source_assets_link_references_reorder_and_retain_editor_sources() {
    let tmp = setup();
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    prepared.check(true).unwrap();
    assert!(!tmp.path().join(".build").exists());
    let (_, project) = parse(prepared.link(true).unwrap().bytes);
    assert_eq!(project.pages[0].1.objects[1].integer("pic"), Some(1));
    assert_eq!(project.pages[0].1.objects[2].integer("picc"), Some(0));
    let source = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&source).unwrap();
    config.assets.items.swap(0, 1);
    fs::write(&source, file::toml(&config).unwrap()).unwrap();
    let (container, project) = parse(
        hmi_cli::prepare(tmp.path())
            .unwrap()
            .link(true)
            .unwrap()
            .bytes,
    );
    assert_eq!(project.pages[0].1.objects[1].integer("pic"), Some(0));
    assert_eq!(project.pages[0].1.objects[2].integer("picc"), Some(1));
    assert_eq!(
        project.references.iter().filter(|r| r.kind == "i").count(),
        2
    );
    assert!(container.active("0.is").is_some() && container.active("1.is").is_some());
    let mut page: PageSource = file::read_toml(&tmp.path().join("pages/page0/page.toml")).unwrap();
    page.objects[0].properties.insert(
        "pic".into(),
        toml::Value::Table(toml::map::Map::from_iter([("id".into(), 1.into())])),
    );
    save_page(tmp.path(), &page);
    let (_, project) = parse(
        hmi_cli::prepare(tmp.path())
            .unwrap()
            .link(true)
            .unwrap()
            .bytes,
    );
    assert_eq!(project.pages[0].1.objects[1].integer("pic"), Some(1));
}

#[test]
fn encoded_assets_and_objects_link_without_source_files() {
    let tmp = setup();
    let moved = tempfile::tempdir().unwrap();
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let expected = prepared.link(true).unwrap().bytes;
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
fn missing_duplicate_or_wrong_sized_resources_do_not_update_output() {
    let tmp = setup();
    let output = tmp.path().join("output.HMI");
    fs::write(&output, b"old output").unwrap();
    let source = tmp.path().join("hmi.toml");
    let mut config: ProjectSource = file::read_toml(&source).unwrap();
    config.assets.items.pop();
    fs::write(&source, file::toml(&config).unwrap()).unwrap();
    assert_eq!(
        hmi_cli::build(tmp.path(), Some(&output), false)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "resource_symbol"
    );
    assert_eq!(fs::read(&output).unwrap(), b"old output");
    config.assets.items.push(config.assets.items[0].clone());
    fs::write(&source, file::toml(&config).unwrap()).unwrap();
    assert!(hmi_cli::prepare(tmp.path()).is_err());
    let fresh = setup();
    fs::write(fresh.path().join("assets/background.bmp"), bmp(2, 3, 50)).unwrap();
    assert_eq!(
        hmi_cli::prepare(fresh.path())
            .unwrap()
            .link(false)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "crop_image"
    );
    let mut page: PageSource =
        file::read_toml(&fresh.path().join("pages/page0/page.toml")).unwrap();
    page.objects[0].properties.insert("pic".into(), 0.into());
    save_page(fresh.path(), &page);
    assert!(hmi_cli::prepare(fresh.path()).is_err());
}

#[test]
fn changed_input_and_output_aliases_are_rejected_before_publication() {
    let tmp = setup();
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    fs::write(tmp.path().join("assets/small.bmp"), bmp(17, 19, 0)).unwrap();
    assert_eq!(
        prepared
            .encode_assets()
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "input_changed"
    );
    let input = tmp.path().join("assets/small.bmp");
    let bytes = fs::read(&input).unwrap();
    assert!(hmi_cli::build(tmp.path(), Some(&input), false).is_err());
    assert_eq!(fs::read(input).unwrap(), bytes);
}
