use hmi_core::{
    asset::ImageSource,
    file,
    model::{AssetSource, ObjectFile, ProjectSource},
    target::{Target, TargetConfig},
};
use hmi_format::{container::Container, project::Project};
use std::{collections::BTreeMap, fs};
fn bmp(w: u32, h: u32) -> Vec<u8> {
    let stride = (w * 3 + 3) & !3;
    let mut b = vec![0; (54 + stride * h) as usize];
    let size = b.len() as u32;
    b[..2].copy_from_slice(b"BM");
    b[2..6].copy_from_slice(&size.to_le_bytes());
    b[10..14].copy_from_slice(&54u32.to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());
    b[18..22].copy_from_slice(&w.to_le_bytes());
    b[22..26].copy_from_slice(&h.to_le_bytes());
    b[26..30].copy_from_slice(&[1, 0, 24, 0]);
    b
}
fn setup(orientation: u16) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let config = TargetConfig {
        profile: "TJC4827X543_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation,
    };
    let target = Target::resolve(&config).unwrap();
    hmi_cli::init(tmp.path(), &config).unwrap();
    fs::create_dir(tmp.path().join("assets")).unwrap();
    let mut project: ProjectSource = file::read_toml(&tmp.path().join("hmi.toml")).unwrap();
    for (key, w, h) in [
        ("background", target.canvas.width, target.canvas.height),
        ("icon", 61, 35),
    ] {
        fs::write(
            tmp.path().join(format!("assets/{key}.bmp")),
            bmp(u32::from(w), u32::from(h)),
        )
        .unwrap();
        fs::write(
            tmp.path().join(format!("assets/{key}.asset.toml")),
            file::toml(&ImageSource {
                source: format!("{key}.bmp"),
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
        project.assets.items.push(AssetSource {
            key: key.into(),
            source: format!("assets/{key}.asset.toml"),
        });
    }
    fs::write(tmp.path().join("hmi.toml"), file::toml(&project).unwrap()).unwrap();
    let mut page = hmi_page_compiler::empty_page("page0");
    for (name, kind, property, key, w, h) in [
        ("p0", "picture", "pic", "icon", 61, 35),
        ("q0", "crop", "picc", "background", 96, 96),
    ] {
        page.objects.push(ObjectFile {
            name: name.into(),
            type_name: kind.into(),
            properties: BTreeMap::from([
                ("x".into(), 80.into()),
                ("y".into(), 100.into()),
                ("w".into(), w.into()),
                ("h".into(), h.into()),
                (
                    property.into(),
                    toml::Value::Table(toml::map::Map::from_iter([("ref".into(), key.into())])),
                ),
            ]),
            events: BTreeMap::new(),
        });
    }
    fs::write(
        tmp.path().join("pages/page0/page.toml"),
        file::toml(&page).unwrap(),
    )
    .unwrap();
    tmp
}

#[test]
fn jpeg_picture_and_crop_link_in_all_directions_without_media_backend_at_link_time() {
    for orientation in [0, 90, 180, 270] {
        let tmp = setup(orientation);
        let p = hmi_cli::prepare(tmp.path()).unwrap();
        p.check(true).unwrap();
        assert!(!tmp.path().join(".build").exists());
        let linked = p.link(true).unwrap();
        let c = Container::parse(linked.bytes.clone()).unwrap();
        let project = Project::parse(&c).unwrap();
        assert_eq!(project.pages[0].1.objects[1].integer("pic"), Some(1));
        assert_eq!(project.pages[0].1.objects[2].integer("picc"), Some(0));
        assert_eq!(
            &c.data(c.active("0.i").unwrap()).unwrap()[..4],
            &[10 + (orientation / 90) as u8, 0x60, 1, 4]
        );
        assert!(c.active("0.is").is_some());
        let moved = tempfile::tempdir().unwrap();
        p.save_plan(moved.path(), true, None).unwrap();
        tmp.close().unwrap();
        assert_eq!(
            hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
                .unwrap()
                .bytes,
            linked.bytes
        );
    }
}

#[test]
fn changed_jpeg_quality_is_detected_and_failed_encoding_preserves_output() {
    let tmp = setup(0);
    let p = hmi_cli::prepare(tmp.path()).unwrap();
    let desc = tmp.path().join("assets/icon.asset.toml");
    let mut config: ImageSource = file::read_toml(&desc).unwrap();
    config.encoding.quality = Some(100);
    fs::write(&desc, file::toml(&config).unwrap()).unwrap();
    assert_eq!(
        p.encode_assets()
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "input_changed"
    );
    let output = tmp.path().join("old.HMI");
    fs::write(&output, b"previous").unwrap();
    config.encoding.quality = Some(0);
    fs::write(&desc, file::toml(&config).unwrap()).unwrap();
    assert!(hmi_cli::build(tmp.path(), Some(&output), true).is_err());
    assert_eq!(fs::read(output).unwrap(), b"previous");
}
