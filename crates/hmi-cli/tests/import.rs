use hmi_core::{
    controls, file,
    model::PageSource,
    target::{Target, TargetConfig},
};
use hmi_format::{
    binary,
    container::{BACKUP_START, Container},
    project::Project,
    write,
};
use std::{collections::BTreeMap, fs, path::Path};
fn fixture(path: &Path, profile: &str, orientation: u16, history: bool) -> Vec<u8> {
    let t = Target::resolve(&TargetConfig {
        profile: profile.into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation,
    })
    .unwrap();
    let own = controls::PAGE
        .attributes(
            "home",
            &BTreeMap::from([
                ("w".into(), i64::from(t.canvas.width).into()),
                ("h".into(), i64::from(t.canvas.height).into()),
            ]),
        )
        .unwrap();
    let mut objects = vec![
        write::object(
            &own,
            &controls::PAGE_EVENTS
                .iter()
                .map(|s| ((*s).into(), Vec::new()))
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    ];
    for (index, (kind, name)) in [
        ("variable", "n0"),
        ("gauge", "z0"),
        ("waveform", "s0"),
        ("qrcode", "qr0"),
        ("checkbox", "c0"),
        ("radio", "r0"),
        ("timer", "tm0"),
        ("hotspot", "m0"),
        ("touch_cap", "tc0"),
    ]
    .iter()
    .enumerate()
    {
        let d = controls::by_name(kind).unwrap();
        let mut p = BTreeMap::new();
        if !matches!(*kind, "variable" | "timer" | "touch_cap") {
            p.extend([
                ("x".into(), 20.into()),
                ("y".into(), 20.into()),
                ("w".into(), 100.into()),
                ("h".into(), 100.into()),
            ]);
        }
        if *kind == "variable" {
            p.insert("val".into(), 7.into());
        }
        if *kind == "qrcode" {
            p.insert("txt".into(), "HMI-A".into());
        }
        let mut attrs = d.attributes(name, &p).unwrap();
        attrs.iter_mut().find(|(k, _)| k == "id").unwrap().1 = vec![(index + 1) as u8];
        if *kind == "gauge" {
            attrs.iter_mut().find(|(k, _)| k == "format").unwrap().1 = 17u16.to_le_bytes().to_vec();
        }
        if *kind == "variable" {
            attrs.push(("x_hidden".into(), vec![222, 173, 0, 190, 239]));
        }
        let events = d
            .events
            .iter()
            .map(|s| {
                (
                    (*s).into(),
                    if *s == "up" {
                        vec![b"sys0=1".to_vec(), Vec::new()]
                    } else {
                        Vec::new()
                    },
                )
            })
            .collect::<Vec<_>>();
        let mut bytes = write::object(&attrs, &events).unwrap();
        if *kind == "variable" {
            bytes.extend(11u32.to_le_bytes());
            bytes.extend(b"opaque-tail");
        }
        objects.push(bytes);
    }
    let mut header = write::page("home", &objects).unwrap()[..56].to_vec();
    header[44] = 0xB2;
    let page = write::page_preserved(
        "home",
        &objects,
        Some(&header),
        &(0..objects.len())
            .map(|n| 0xA500 + n as u32)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let mut main = write::main_resource(
        profile,
        t.header_tag().unwrap(),
        orientation,
        &[("pa", "10.pa"), ("bin", "odd.bin")],
    )
    .unwrap();
    main[70] = 0x7E;
    let crc = binary::main_crc(&main).unwrap();
    main[..4].copy_from_slice(&crc.to_le_bytes());
    let mut resources = vec![
        (
            "Program.s".into(),
            hmi_core::target::DEFAULT_PROGRAM.as_bytes().to_vec(),
        ),
        ("10.pa".into(), page),
        ("odd.bin".into(), b"opaque resource\0\xff".to_vec()),
        ("main.HMI".into(), main),
    ];
    if history {
        resources.push(("old.tmp".into(), b"deleted history".to_vec()));
    }
    let unknown = BTreeMap::from([
        ("Program.s".into(), [10, 11, 12]),
        ("main.HMI".into(), [20, 21, 22]),
        ("10.pa".into(), [30, 31, 32]),
        ("odd.bin".into(), [40, 41, 42]),
    ]);
    let mut bytes = write::container_preserved(&resources, &unknown).unwrap();
    if history {
        for base in [0, BACKUP_START] {
            bytes[base + 4 + 4 * 28 + 24] = 1;
            let end = base + 4 + resources.len() * 28;
            let crc = binary::directory_crc(&bytes[base..end]).unwrap();
            bytes[end..end + 4].copy_from_slice(&crc.to_le_bytes());
        }
        bytes.extend(b"unused allocation tail");
    }
    fs::write(path, &bytes).unwrap();
    bytes
}
fn parsed(bytes: Vec<u8>) -> (Container, Project) {
    let c = Container::parse(bytes).unwrap();
    let p = Project::parse(&c).unwrap();
    (c, p)
}
#[test]
fn import_roundtrips_all_profiles_and_directions_preserving_records_headers_and_active_extras() {
    for profile in hmi_core::target::PROFILES {
        for orientation in [0, 90, 180, 270] {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("native.HMI");
            let input = fixture(&path, profile, orientation, false);
            let root = tmp.path().join("project");
            let report = hmi_parser::export(&path, &root).unwrap();
            assert_eq!(report.objects, 9);
            assert_eq!(report.extra_resources, 1);
            let p = hmi_cli::prepare(&root).unwrap();
            p.check(true).unwrap();
            assert!(!root.join(".build").exists());
            let linked = p.link(true).unwrap();
            let (old_c, old_p) = parsed(input);
            let (new_c, new_p) = parsed(linked.bytes);
            assert_eq!(old_p.pages[0].1.raw, new_p.pages[0].1.raw);
            assert_eq!(
                new_c.data(new_c.active("odd.bin").unwrap()).unwrap(),
                old_c.data(old_c.active("odd.bin").unwrap()).unwrap()
            );
            for (old, new) in [
                ("Program.s", "Program.s"),
                ("main.HMI", "main.HMI"),
                ("10.pa", "0.pa"),
                ("odd.bin", "odd.bin"),
            ] {
                assert_eq!(
                    old_c.active(old).unwrap().unknown,
                    new_c.active(new).unwrap().unknown
                );
            }
            assert_eq!(
                new_c.data(new_c.active("main.HMI").unwrap()).unwrap()[70],
                0x7E
            );
            assert!(
                new_p
                    .references
                    .iter()
                    .any(|r| r.kind == "bin" && r.resource == "odd.bin")
            );
        }
    }
}
#[test]
fn source_edits_defaults_names_and_order_override_original_without_losing_private_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("native.HMI");
    fixture(&input, "TJC4827X543_011", 0, false);
    let root = tmp.path().join("project");
    hmi_parser::export(&input, &root).unwrap();
    let path = root.join("pages/p00000/page.toml");
    let mut p: PageSource = file::read_toml(&path).unwrap();
    p.name = "renamed".into();
    p.objects[0].name = "value".into();
    p.objects[0].properties.remove("val");
    p.objects[1].properties.insert("val".into(), 135.into());
    p.objects.swap(0, 1);
    fs::write(path, file::toml(&p).unwrap()).unwrap();
    let linked = hmi_cli::prepare(&root).unwrap().link(true).unwrap();
    let (_, parsed) = parsed(linked.bytes);
    let page = &parsed.pages[0].1;
    assert_eq!(page.name, "renamed");
    assert_eq!(page.objects[1].name, "z0");
    assert_eq!(page.objects[1].integer("format"), Some(17));
    assert_eq!(page.objects[1].integer("val"), Some(135));
    assert_eq!(page.objects[2].name, "value");
    assert_eq!(page.objects[2].integer("val"), Some(0));
    assert_eq!(
        page.objects[2].attribute("x_hidden").unwrap().value,
        [222, 173, 0, 190, 239]
    );
    assert!(page.objects[2].raw.ends_with(b"opaque-tail"));
    assert_eq!(page.indices[2].unknown, 0xA501);
    assert_eq!(page.indices[1].unknown, 0xA502);
}
#[test]
fn deleted_records_and_unused_history_are_not_import_dependencies() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("native.HMI");
    let original = fixture(&input, "TJC4832K035_011", 0, true);
    let root = tmp.path().join("project");
    hmi_parser::export(&input, &root).unwrap();
    let linked = hmi_cli::prepare(&root).unwrap().link(true).unwrap();
    assert!(linked.bytes.len() < original.len());
    let (c, _) = parsed(linked.bytes);
    assert!(c.active("old.tmp").is_none());
    assert!(c.contiguous);
    c.strict().unwrap();
}
#[test]
fn standalone_plan_needs_only_declared_original_bundle_and_targets_after_sources_are_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("native.HMI");
    fixture(&input, "TJC4827X543_011", 0, false);
    let root = tmp.path().join("project");
    hmi_parser::export(&input, &root).unwrap();
    let p = hmi_cli::prepare(&root).unwrap();
    let expected = p.link(true).unwrap().bytes;
    let moved = tempfile::tempdir().unwrap();
    p.save_plan(moved.path(), true, None).unwrap();
    assert!(
        hmi_linker::link(
            &p.target,
            &p.program.object,
            &p.pages.iter().map(|p| p.object.clone()).collect::<Vec<_>>(),
            true
        )
        .is_err()
    );
    tmp.close().unwrap();
    assert_eq!(
        hmi_linker::from_manifest(&moved.path().join("link.toml"), true)
            .unwrap()
            .bytes,
        expected
    );
    fs::remove_file(moved.path().join("original.hpo")).unwrap();
    assert!(hmi_linker::from_manifest(&moved.path().join("link.toml"), true).is_err());
}
#[test]
fn missing_corrupt_or_wrong_record_attachments_and_unsupported_media_do_not_replace_files() {
    let tmp = tempfile::tempdir().unwrap();
    let input = tmp.path().join("native.HMI");
    fixture(&input, "TJC4827X543_011", 0, false);
    let root = tmp.path().join("project");
    hmi_parser::export(&input, &root).unwrap();
    assert!(hmi_parser::export(&input, &root).is_err());
    let old = tmp.path().join("previous.HMI");
    fs::write(&old, b"previous").unwrap();
    let bundle = root.join(".preserve/origin.hpo");
    let bytes = fs::read(&bundle).unwrap();
    fs::write(&bundle, b"bad").unwrap();
    assert!(hmi_cli::build(&root, Some(&old), true).is_err());
    assert_eq!(fs::read(&old).unwrap(), b"previous");
    fs::write(&bundle, &bytes).unwrap();
    let source = root.join("pages/p00000/page.toml");
    let mut page: PageSource = file::read_toml(&source).unwrap();
    page.objects[0].original_record = Some("pages/0/objects/2".into());
    fs::write(&source, file::toml(&page).unwrap()).unwrap();
    assert!(hmi_cli::build(&root, Some(&old), true).is_err());
    assert_eq!(fs::read(&old).unwrap(), b"previous");
    let (c, _) = parsed(fs::read(&input).unwrap());
    let mut resources = c
        .entries
        .iter()
        .filter(|e| !e.deleted)
        .map(|e| (e.name.clone(), c.data(e).unwrap().to_vec()))
        .collect::<Vec<_>>();
    resources.push(("0.i".into(), b"media".to_vec()));
    fs::write(&input, write::container(&resources).unwrap()).unwrap();
    let media_output = tmp.path().join("media-project");
    assert_eq!(
        hmi_parser::export(&input, &media_output)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "import_media"
    );
    assert!(!media_output.exists());
}
