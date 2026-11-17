use hmi_artifact::{
    asset::{AssetInfo, AssetObject},
    original::OriginalBundle,
};
use hmi_core::{
    file,
    model::{ObjectFile, ProjectSource},
    target::{Target, TargetConfig},
};
use hmi_format::{container::Container, project::Project, write};
use image::ImageEncoder;
use std::{collections::BTreeMap, fs, path::Path};
mod zi {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/zi.rs"
    ));
}
mod gif {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/gif.rs"
    ));
}
mod wave {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/wave.rs"
    ));
}
fn reference(key: &str) -> toml::Value {
    toml::Value::Table(toml::map::Map::from_iter([("ref".into(), key.into())]))
}
fn asset(root: &Path, t: &Target, kind: &str, source: &str) -> AssetObject {
    let p = root.join("asset.toml");
    fs::write(
        &p,
        format!("schema='hmi.asset/1'\nkind='{kind}'\nsource='{source}'\n"),
    )
    .unwrap();
    hmi_media::encode(&p, t).unwrap().object
}
fn fixture(path: &Path, orientation: u16) -> Vec<u8> {
    let root = path.parent().unwrap();
    let t = Target::resolve(&TargetConfig {
        profile: "TJC4827X543_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation,
    })
    .unwrap();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &[220, 40, 20, 128].repeat(64 * 48),
            64,
            48,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    fs::write(root.join("image.png"), png).unwrap();
    fs::write(root.join("font.zi"), zi::font()).unwrap();
    fs::write(
        root.join("anim.gif"),
        gif::gif(
            64,
            48,
            &[gif::full(64, 48, 1, 50), gif::full(64, 48, 2, 50)],
        ),
    )
    .unwrap();
    fs::write(root.join("tone.wav"), wave::wave(8000, 1, 8000)).unwrap();
    let image = asset(root, &t, "image", "image.png");
    let font = asset(root, &t, "font", "font.zi");
    let animation = asset(root, &t, "animation", "anim.gif");
    let audio = asset(root, &t, "audio", "tone.wav");
    let jpeg = hmi_format::picture::jpeg::parts(image.member(0).unwrap(), 64, 48, orientation)
        .unwrap()
        .jpeg
        .to_vec();
    let video = hmi_artifact::native::video(
        &hmi_format::video::write_with_audio(
            64,
            48,
            orientation,
            80,
            4,
            &vec![jpeg; 4],
            Some(audio.member(0).unwrap()),
        )
        .unwrap(),
        Default::default(),
    )
    .unwrap();
    let media = vec![
        ("font".into(), font),
        ("red".into(), image.clone()),
        ("video".into(), video),
        ("green".into(), image),
        ("anim".into(), animation),
        ("audio".into(), audio),
    ];
    let mut page = hmi_page_compiler::empty_page("home");
    for (name, kind, resource, key) in [
        ("p0", "picture", "red", "pic"),
        ("p1", "picture", "green", "pic"),
        ("t0", "text", "font", "font"),
        ("gm0", "animation", "anim", "vid"),
        ("v0", "video", "video", "vid"),
        ("wav0", "audio", "audio", "vid"),
        ("fb0", "file_browser", "font", "font"),
        ("data0", "data_record", "font", "font"),
    ] {
        let mut properties = BTreeMap::from([(key.into(), reference(resource))]);
        if kind != "audio" {
            properties.extend([
                ("x".into(), 10.into()),
                ("y".into(), 20.into()),
                ("w".into(), 64.into()),
                ("h".into(), 48.into()),
            ]);
        }
        if kind == "text" {
            properties.insert("txt".into(), "ABC".into());
        }
        if kind == "file_browser" {
            properties.insert("pic1".into(), reference("red"));
            properties.insert("pic2".into(), reference("green"));
            properties.insert("vvs2".into(), "*.txt/0*.log/1".into());
        }
        if kind == "data_record" {
            properties.extend([
                ("path".into(), "sd0/records.data".into()),
                ("lenth".into(), 32.into()),
                ("maxval".into(), 16.into()),
                ("dez".into(), 2.into()),
            ]);
        }
        page.objects.push(ObjectFile {
            original_record: None,
            name: name.into(),
            type_name: kind.into(),
            properties,
            events: BTreeMap::new(),
        });
    }
    fs::write(root.join("page.toml"), file::toml(&page).unwrap()).unwrap();
    fs::write(root.join("program.s"), hmi_core::target::DEFAULT_PROGRAM).unwrap();
    let program = hmi_page_compiler::compile_program(&root.join("program.s"), &t)
        .unwrap()
        .object;
    let page = hmi_page_compiler::compile_page(&root.join("page.toml"), &t)
        .unwrap()
        .object;
    let linked = hmi_linker::link_with_assets(&t, &program, &[page], &media, true).unwrap();
    let c = Container::parse(linked.bytes).unwrap();
    let p = Project::parse(&c).unwrap();
    // 物理成员名故意不等于逻辑ID，防止把文件名前缀误当引用表下标。
    let rename = |name: &str| {
        if name == "0.i" {
            "7.i".into()
        } else if name == "0.is" {
            "7.is".into()
        } else {
            name.to_string()
        }
    };
    let refs = p
        .references
        .iter()
        .map(|r| (r.kind.clone(), rename(&r.resource)))
        .collect::<Vec<_>>();
    let borrowed = refs
        .iter()
        .map(|(k, n)| (k.as_str(), n.as_str()))
        .collect::<Vec<_>>();
    let main =
        write::main_resource(&t.profile, t.header_tag().unwrap(), orientation, &borrowed).unwrap();
    let resources = c
        .entries
        .iter()
        .filter(|e| !e.deleted)
        .map(|e| {
            (
                rename(&e.name),
                if e.name == "main.HMI" {
                    main.clone()
                } else {
                    c.data(e).unwrap().to_vec()
                },
            )
        })
        .collect::<Vec<_>>();
    let unknown = resources
        .iter()
        .enumerate()
        .map(|(i, (n, _))| (n.clone(), [i as u8, 2, 3]))
        .collect();
    let bytes = write::container_preserved(&resources, &unknown).unwrap();
    fs::write(path, &bytes).unwrap();
    bytes
}
fn parsed(bytes: Vec<u8>) -> (Container, Project) {
    let c = Container::parse(bytes).unwrap();
    let p = Project::parse(&c).unwrap();
    (c, p)
}
#[test]
fn rgb565_import_keeps_native_bmp_members_in_all_t_k_profiles_and_directions() {
    for profile in ["TJC4832K035_011", "TJC4832T035_011", "TJC3224T028_011"] {
        for orientation in [0, 90, 180, 270] {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path();
            let t = Target::resolve(&TargetConfig {
                profile: profile.into(),
                format: "ver21234".into(),
                encoding: "utf-8".into(),
                orientation,
            })
            .unwrap();
            let mut png = Vec::new();
            image::codecs::png::PngEncoder::new(&mut png)
                .write_image(
                    &[220, 40, 20, 255].repeat(32 * 32),
                    32,
                    32,
                    image::ExtendedColorType::Rgba8,
                )
                .unwrap();
            fs::write(root.join("image.png"), png).unwrap();
            let image = asset(root, &t, "image", "image.png");
            let mut page = hmi_page_compiler::empty_page("home");
            page.objects.push(ObjectFile {
                original_record: None,
                name: "p0".into(),
                type_name: "picture".into(),
                properties: BTreeMap::from([
                    ("x".into(), 20.into()),
                    ("y".into(), 20.into()),
                    ("w".into(), 32.into()),
                    ("h".into(), 32.into()),
                    ("pic".into(), reference("image")),
                ]),
                events: BTreeMap::new(),
            });
            fs::write(root.join("page.toml"), file::toml(&page).unwrap()).unwrap();
            fs::write(root.join("program.s"), hmi_core::target::DEFAULT_PROGRAM).unwrap();
            let page = hmi_page_compiler::compile_page(&root.join("page.toml"), &t)
                .unwrap()
                .object;
            let program = hmi_page_compiler::compile_program(&root.join("program.s"), &t)
                .unwrap()
                .object;
            let old = hmi_linker::link_with_assets(
                &t,
                &program,
                &[page],
                &[("image".into(), image)],
                true,
            )
            .unwrap()
            .bytes;
            let input = root.join("source.HMI");
            fs::write(&input, &old).unwrap();
            let output = root.join("project");
            hmi_parser::export(&input, &output).unwrap();
            let linked = hmi_cli::prepare(&output).unwrap().link(true).unwrap();
            let (a, pa) = parsed(old);
            let (b, pb) = parsed(linked.bytes);
            assert_eq!(logical(&a, &pa), logical(&b, &pb));
            assert_eq!(
                a.data(a.active("0.is").unwrap()).unwrap(),
                b.data(b.active("0.is").unwrap()).unwrap()
            );
        }
    }
}
fn logical(c: &Container, p: &Project) -> BTreeMap<(String, usize), Vec<u8>> {
    let mut counters = BTreeMap::new();
    let mut result = BTreeMap::new();
    for r in p.references.iter().filter(|r| r.kind != "pa") {
        let id = counters.entry(r.kind.clone()).or_insert(0);
        result.insert(
            (r.kind.clone(), *id),
            c.data(c.active(&r.resource).unwrap()).unwrap().to_vec(),
        );
        *id += 1;
    }
    result
}
#[test]
fn all_media_import_without_reencoding_and_keep_source_members_directory_fields_and_bindings() {
    for orientation in [0, 90, 180, 270] {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("source.HMI");
        let old = fixture(&input, orientation);
        let root = tmp.path().join("project");
        hmi_parser::export(&input, &root).unwrap();
        let manifest: ProjectSource = file::read_toml(&root.join("hmi.toml")).unwrap();
        assert_eq!(manifest.assets.items.len(), 6);
        let bundle = OriginalBundle::read(&root.join(".preserve/origin.hpo")).unwrap();
        assert!(bundle.metadata.extras.is_empty());
        assert_eq!(bundle.metadata.managed_resources.len(), 9);
        let linked = hmi_cli::prepare(&root).unwrap().link(true).unwrap();
        let (a, pa) = parsed(old);
        let (b, pb) = parsed(linked.bytes);
        assert_eq!(logical(&a, &pa), logical(&b, &pb));
        assert_eq!(pa.pages[0].1.raw, pb.pages[0].1.raw);
        for ext in ["is", "gmovs"] {
            let old_name = if ext == "is" {
                "7.is".to_string()
            } else {
                format!("0.{ext}")
            };
            let new_name = format!("0.{ext}");
            assert_eq!(
                a.data(a.active(&old_name).unwrap()).unwrap(),
                b.data(b.active(&new_name).unwrap()).unwrap()
            );
            assert_eq!(
                a.active(&old_name).unwrap().unknown,
                b.active(&new_name).unwrap().unknown
            );
        }
        assert!(root.join("assets/view/image_00000.png").exists());
        assert!(root.join("assets/view/animation_00000.gif").exists());
    }
}
#[test]
fn preview_is_not_authority_and_symbol_reuse_follows_current_names_instead_of_original_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("source.HMI");
    fixture(&input, 0);
    let root = tmp.path().join("project");
    hmi_parser::export(&input, &root).unwrap();
    let expected = hmi_cli::prepare(&root).unwrap().link(true).unwrap().bytes;
    fs::write(
        root.join("assets/view/image_00000.png"),
        b"changed view only",
    )
    .unwrap();
    assert_eq!(
        hmi_cli::prepare(&root).unwrap().link(true).unwrap().bytes,
        expected
    );
    let path = root.join("hmi.toml");
    let mut source: ProjectSource = file::read_toml(&path).unwrap();
    source.assets.items[0].key = "image_00001".into();
    source.assets.items[1].key = "image_00000".into();
    fs::write(path, file::toml(&source).unwrap()).unwrap();
    let (_, p) = parsed(hmi_cli::prepare(&root).unwrap().link(true).unwrap().bytes);
    assert_eq!(p.pages[0].1.objects[1].integer("pic"), Some(1));
    assert_eq!(
        p.pages[0].1.objects[7].attribute("vvs2").unwrap().value,
        b"*.txt/1*.log/0"
    );
}
#[test]
fn independent_imported_targets_keep_all_media_and_do_not_need_original_hmi_or_source_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("source.HMI");
    fixture(&input, 0);
    let root = tmp.path().join("project");
    hmi_parser::export(&input, &root).unwrap();
    let p = hmi_cli::prepare(&root).unwrap();
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
#[test]
fn native_runtime_only_animation_preserves_the_actual_absence_of_an_editor_source() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("source.HMI");
    fixture(&input, 0);
    let c = Container::parse(fs::read(&input).unwrap()).unwrap();
    let resources = c
        .entries
        .iter()
        .filter(|e| !e.deleted && e.name != "0.gmovs")
        .map(|e| (e.name.clone(), c.data(e).unwrap().to_vec()))
        .collect::<Vec<_>>();
    fs::write(&input, write::container(&resources).unwrap()).unwrap();
    let root = tmp.path().join("project");
    hmi_parser::export(&input, &root).unwrap();
    let asset = AssetObject::read(&root.join(".preserve/assets/animation_00000.hma")).unwrap();
    assert_eq!(asset.metadata.members.len(), 1);
    let AssetInfo::Animation { animation } = &asset.metadata.info else {
        panic!()
    };
    assert_eq!(animation.source_frames, 0);
    let (c, _) = parsed(hmi_cli::prepare(&root).unwrap().link(true).unwrap().bytes);
    assert!(c.active("0.gmovs").is_none());
}
#[test]
fn missing_or_corrupt_native_package_is_not_replaced_by_a_view_and_old_hmi_is_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("source.HMI");
    fixture(&input, 0);
    let root = tmp.path().join("project");
    hmi_parser::export(&input, &root).unwrap();
    fs::write(
        root.join(".preserve/assets/image_00000.hma"),
        b"bad package",
    )
    .unwrap();
    let output = tmp.path().join("previous.HMI");
    fs::write(&output, b"previous").unwrap();
    assert!(hmi_cli::build(&root, Some(&output), true).is_err());
    assert_eq!(fs::read(output).unwrap(), b"previous");
}
#[test]
fn removing_an_asset_and_its_control_does_not_restore_it_from_the_original_bundle() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("source.HMI");
    fixture(&input, 0);
    let root = tmp.path().join("project");
    hmi_parser::export(&input, &root).unwrap();
    let manifest = root.join("hmi.toml");
    let mut project: ProjectSource = file::read_toml(&manifest).unwrap();
    project.assets.items.retain(|a| a.key != "audio_00000");
    fs::write(manifest, file::toml(&project).unwrap()).unwrap();
    let path = root.join("pages/p00000/page.toml");
    let mut page: hmi_core::model::PageSource = file::read_toml(&path).unwrap();
    page.objects.retain(|o| o.name != "wav0");
    fs::write(path, file::toml(&page).unwrap()).unwrap();
    let (c, p) = parsed(hmi_cli::prepare(&root).unwrap().link(true).unwrap().bytes);
    assert!(c.active("0.wav").is_none());
    assert!(!p.references.iter().any(|r| r.kind == "wav"));
    let video = c.data(c.active("0.v").unwrap()).unwrap();
    assert!(hmi_format::video::inspect(video).unwrap().audio.is_some());
}
#[test]
fn native_standard_rgb_png_and_jpeg_editor_sources_are_preserved_without_normalization() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("source.HMI");
    fixture(&input, 0);
    let c = Container::parse(fs::read(&input).unwrap()).unwrap();
    let runtime = c.data(c.active("7.i").unwrap()).unwrap();
    let jpeg = hmi_format::picture::jpeg::parts(runtime, 64, 48, 0)
        .unwrap()
        .jpeg;
    let opaque = hmi_format::picture::jpeg::runtime(64, 48, 0, jpeg).unwrap();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &[220, 40, 20].repeat(64 * 48),
            64,
            48,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    for (format, bytes) in [("png", png.as_slice()), ("jpg", jpeg)] {
        let mut source =
            hmi_format::picture::editor_source_with_format(64, 48, "png", bytes).unwrap();
        source[24..27].copy_from_slice(format.as_bytes());
        let package = hmi_artifact::native::image(&opaque, Some(&source)).unwrap();
        assert_eq!(package.member(0).unwrap(), opaque);
        assert_eq!(package.member(1).unwrap(), source);
        package
            .validate_for(
                &Target::resolve(&TargetConfig {
                    profile: "TJC4827X543_011".into(),
                    format: "ver21234".into(),
                    encoding: "utf-8".into(),
                    orientation: 0,
                })
                .unwrap(),
            )
            .unwrap();
    }
}
