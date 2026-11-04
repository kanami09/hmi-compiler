mod common;
use common::Workspace;
use hmi_compiler::{
    binary::*,
    container::{BACKUP_START, Container},
    page::Page,
    project::Project,
    workspace,
};
use std::collections::BTreeMap;
use std::fs;

#[test]
fn property_errors_keep_structured_source_lines() {
    let w = Workspace::new();
    let mut page = w.read_page();
    page.objects[0]
        .properties
        .insert("txt".into(), "文本长度超过当前容量".into());
    for newline in ["\n", "\r\n"] {
        let text = workspace::page_to_toml(&page)
            .unwrap()
            .replace('\n', newline);
        fs::write(w.page_path(), &text).unwrap();
        let expected = text
            .lines()
            .position(|line| line.trim_start().starts_with("txt ="))
            .unwrap()
            + 1;
        let error = workspace::prepare(&w.root).err().unwrap();
        let diagnostic = error.as_diagnostic().expect("需要结构化错误");
        assert_eq!(diagnostic.code, "text_capacity");
        assert_eq!(diagnostic.location.line, Some(expected));
        assert_eq!(diagnostic.location.object.as_deref(), Some("b0"));
        assert_eq!(diagnostic.location.property.as_deref(), Some("txt"));
        assert!(diagnostic.message.contains("UTF-8"));
    }
}

#[test]
fn exported_controls_use_toml11_inline_tables_and_roundtrip() {
    let w = Workspace::new();
    let text = fs::read_to_string(w.page_path()).unwrap();
    assert!(text.contains("[properties]"));
    assert_eq!(text.matches("[[objects]]").count(), 2);
    assert_eq!(text.matches("properties = {\n").count(), 2);
    assert!(text.contains("    x = 10, y = 20,\n    w = 100, h = 40,"));
    assert!(text.contains("events = { up = "));
    assert!(!text.contains("[objects.properties]"));
    assert!(!text.contains("[objects.events]"));
    assert!(!text.contains("events = {}"));
    assert!(!text.contains("[events]"));
    assert_eq!(workspace::prepare(&w.root).unwrap().bytes, w.bytes);
}

#[test]
fn page_formatter_quotes_keys_strings_and_preserves_object_order() {
    let w = Workspace::new();
    let mut page = w.read_page();
    page.objects[0].name = "引号\"与反斜杠\\".into();
    page.objects[0]
        .properties
        .insert("quoted.key".into(), "第一行\n\"第二行\"\\末尾\t".into());
    page.objects[0]
        .properties
        .insert(" spaced key ".into(), "raw:abcdef".into());
    page.objects[0]
        .events
        .insert("custom.event".into(), "events/a\"b.s".into());
    page.objects[1].events.clear();
    let text = workspace::page_to_toml(&page).unwrap();
    let parsed: workspace::PageFile = toml::from_str(&text).unwrap();
    assert_eq!(parsed, page);
    assert_eq!(workspace::page_to_toml(&page).unwrap(), text);
}

#[test]
fn lossless_after_reformat_move_and_missing_original() {
    let w = Workspace::new();
    let page = w.read_page();
    let reformatted = workspace::page_to_toml(&page)
        .unwrap()
        .replace("    ", "  ");
    fs::write(w.page_path(), format!("# AI 改动了排版\n{reformatted}")).unwrap();
    let moved = w.temporary.path().join("moved");
    fs::rename(&w.root, &moved).unwrap();
    assert_eq!(workspace::prepare(&moved).unwrap().bytes, w.bytes);
}

#[test]
fn editable_fields_events_and_metadata_are_preserved() {
    let w = Workspace::new();
    let mut p = w.read_page();
    let button = &mut p.objects[0];
    button.properties.insert("txt".into(), "返回返回".into());
    button.properties.insert("x".into(), 211.into());
    button.properties.insert("y".into(), 205.into());
    button.properties.insert("w".into(), 140.into());
    button.properties.insert("h".into(), 44.into());
    p.objects[1]
        .properties
        .insert("val".into(), i32::MIN.into());
    p.events.insert("load".into(), "events/new.load.s".into());
    fs::write(
        w.root.join("pages/p000_page0/events/new.load.s"),
        "// 中文\r\n\r\nb0.txt=\"新内容\"  \r\n",
    )
    .unwrap();
    let path = button_path(&p);
    fs::write(
        w.root.join("pages/p000_page0").join(path),
        "\n// 尾随空格  \n\n",
    )
    .unwrap();
    w.write_page(&p);
    let prepared = workspace::prepare(&w.root).unwrap();
    assert_eq!(prepared.changed_resources, ["39.pa"]);
    assert_eq!(prepared.bytes, workspace::prepare(&w.root).unwrap().bytes);
    let parsed = Container::parse(prepared.bytes).unwrap();
    let page = Page::parse(parsed.data(parsed.active("39.pa").unwrap()).unwrap()).unwrap();
    let b = &page.objects[1];
    assert_eq!(b.integer("endx"), Some(350));
    assert_eq!(b.integer("endy"), Some(248));
    assert_eq!(b.attribute("txt").unwrap().value, "返回返回".as_bytes());
    assert_eq!(
        page.objects[2].attribute("val").unwrap().value,
        i32::MIN.to_le_bytes()
    );
    assert_eq!(page.indices[1].unknown, 0xABCE);
    let up = b.events.iter().find(|e| e.kind == "up").unwrap();
    assert_eq!(
        up.lines,
        [
            b"".to_vec(),
            b"// \xe5\xb0\xbe\xe9\x9a\x8f\xe7\xa9\xba\xe6\xa0\xbc  ".to_vec(),
            b"".to_vec()
        ]
    );
    let old = Container::parse(w.bytes).unwrap();
    for e in &old.entries {
        if e.name != "39.pa" {
            assert_eq!(
                old.data(e).unwrap(),
                parsed.data(&parsed.entries[e.index]).unwrap()
            );
        }
    }
    for a in &b.attributes {
        assert_eq!(b.records[a.record][15], 0xA7);
    }
    assert!(b.records.iter().any(|r| r == b"unrecognized-metadata"));
}

fn button_path(page: &workspace::PageFile) -> String {
    page.objects[0].events["up"].clone()
}

#[test]
fn event_reference_removal_clears_slot_and_empty_file_is_zero_lines() {
    let w = Workspace::new();
    let mut p = w.read_page();
    p.objects[0].events.remove("up");
    w.write_page(&p);
    let bytes = workspace::prepare(&w.root).unwrap().bytes;
    let c = Container::parse(bytes).unwrap();
    let p = Project::parse(&c).unwrap();
    assert!(
        p.pages[0].1.objects[1]
            .events
            .iter()
            .find(|e| e.kind == "up")
            .unwrap()
            .lines
            .is_empty()
    );
    assert!(workspace::script_lines(b"").unwrap().is_empty());
    assert_eq!(
        workspace::script_lines(b"\n").unwrap(),
        vec![Vec::<u8>::new()]
    );
    assert_eq!(
        workspace::script_lines(b"a\r\n\r\n").unwrap(),
        vec![b"a".to_vec(), Vec::new()]
    );
}

#[test]
fn control_value_ranges_reject_invalid_edits_and_accept_boundaries() {
    for (kind, key, min, max) in [
        (53, "val", 0, 1),
        (56, "val", 0, 1),
        (57, "val", 0, 1),
        (51, "tim", 50, 65534),
    ] {
        let w = Workspace::from_bytes(common::fixture_with_control(kind));
        let mut page = w.read_page();
        let output = w.temporary.path().join("output.HMI");
        fs::write(&output, b"previous").unwrap();
        for number in [min - 1, max + 1] {
            page.objects[0].properties.insert(key.into(), number.into());
            w.write_page(&page);
            let options = hmi_compiler::build::BuildOptions {
                warnings: hmi_compiler::warning::WarningPolicy::parse(&[], true).unwrap(),
                ..Default::default()
            };
            let outcome = workspace::build(&w.root, &output, &options);
            assert!(!outcome.success, "type={kind}, {key}={number}");
            let diagnostic = &outcome.diagnostics[0];
            if number >= 0 {
                assert_eq!(diagnostic.code, "property_range");
            }
            assert_eq!(diagnostic.location.object.as_deref(), Some("b0"));
            assert_eq!(diagnostic.location.property.as_deref(), Some(key));
            assert!(diagnostic.location.line.is_some());
            assert_eq!(fs::read(&output).unwrap(), b"previous");
        }
        for number in [min, max] {
            page.objects[0].properties.insert(key.into(), number.into());
            w.write_page(&page);
            let prepared = workspace::prepare(&w.root).unwrap();
            let container = Container::parse(prepared.bytes).unwrap();
            let project = Project::parse(&container).unwrap();
            assert_eq!(
                project.pages[0].1.objects[1].integer(key),
                Some(number as u32)
            );
        }
    }
}

#[test]
fn unchanged_legacy_control_values_are_preserved() {
    let container = Container::parse(common::fixture_with_control(56)).unwrap();
    let entry = container.active("39.pa").unwrap();
    let page = Page::parse(container.data(entry).unwrap()).unwrap();
    let mut objects: Vec<_> = page.objects.iter().map(|o| o.raw.clone()).collect();
    objects[1] = page.objects[1]
        .rebuild(&BTreeMap::from([("val".into(), vec![2])]), &BTreeMap::new())
        .unwrap();
    let bytes = container
        .rebuild(&BTreeMap::from([(
            entry.index,
            page.rebuild(&objects).unwrap(),
        )]))
        .unwrap();
    let w = Workspace::from_bytes(bytes);
    assert_eq!(workspace::prepare(&w.root).unwrap().bytes, w.bytes);
    let mut edited = w.read_page();
    edited.objects[0].properties.insert("bco".into(), 0.into());
    w.write_page(&edited);
    assert!(workspace::prepare(&w.root).is_ok());
    edited.objects[0].properties.insert("val".into(), 1.into());
    w.write_page(&edited);
    assert!(workspace::prepare(&w.root).is_ok());
}

#[test]
fn colliding_asset_names_export_distinct_files_and_roundtrip() {
    for names in [["a.b.zi", "a_2eb.zi"], ["A.zi", "a.zi"]] {
        let mut bytes = common::fixture();
        let container = Container::parse(bytes.clone()).unwrap();
        let font = container.active("0.zi").unwrap();
        let main = container.active("main.HMI").unwrap();
        let reference = main.offset + 96 + 16 + 8;
        bytes[reference..reference + 8].fill(0);
        bytes[reference..reference + names[1].len()].copy_from_slice(names[1].as_bytes());
        let crc = main_crc(&bytes[main.offset..main.offset + main.length]).unwrap();
        bytes[main.offset..main.offset + 4].copy_from_slice(&crc.to_le_bytes());
        for base in [0, BACKUP_START] {
            for (index, name) in [(0, names[0]), (font.index, names[1])] {
                let position = base + 4 + index * 28;
                bytes[position..position + 16].fill(0);
                bytes[position..position + name.len()].copy_from_slice(name.as_bytes());
                bytes[position + 24] = 0;
            }
        }
        common::fix_directory(&mut bytes);
        let w = Workspace::from_bytes(bytes);
        let manifest: workspace::Manifest =
            toml::from_str(&fs::read_to_string(w.root.join(".hmi/manifest.toml")).unwrap())
                .unwrap();
        assert_eq!(manifest.assets.len(), 2);
        assert_ne!(
            manifest.assets[0].path.to_ascii_lowercase(),
            manifest.assets[1].path.to_ascii_lowercase()
        );
        for asset in &manifest.assets {
            assert_eq!(
                hmi_compiler::sha256(&fs::read(w.root.join(&asset.path)).unwrap()),
                asset.sha256
            );
        }
        assert_eq!(workspace::prepare(&w.root).unwrap().bytes, w.bytes);
    }
}

#[test]
fn colliding_event_names_export_distinct_scripts_and_roundtrip() {
    let container = Container::parse(common::fixture()).unwrap();
    let entry = container.active("39.pa").unwrap();
    let page = Page::parse(container.data(entry).unwrap()).unwrap();
    let button = &page.objects[1];
    let mut records = button.records.clone();
    let event = button.events.iter().find(|e| e.kind == "up").unwrap();
    records[event.record] = b"codes.-2".to_vec();
    let end = records.len() - 1;
    records.splice(end..end, [b"codes_2e-1".to_vec(), b"sys0=1".to_vec()]);
    let mut raw = Vec::new();
    for record in records {
        raw.extend_from_slice(&(record.len() as u32).to_le_bytes());
        raw.extend(record);
    }
    let mut objects: Vec<_> = page.objects.iter().map(|o| o.raw.clone()).collect();
    objects[1] = raw;
    let bytes = container
        .rebuild(&BTreeMap::from([(
            entry.index,
            page.rebuild(&objects).unwrap(),
        )]))
        .unwrap();
    let w = Workspace::from_bytes(bytes);
    let exported = w.read_page();
    let events = &exported.objects[0].events;
    assert_ne!(events["."], events["_2e"]);
    assert_eq!(
        fs::read_to_string(w.root.join("pages/p000_page0").join(&events["_2e"])).unwrap(),
        "sys0=1\n"
    );
    assert_eq!(workspace::prepare(&w.root).unwrap().bytes, w.bytes);
}

#[test]
fn validation_rejects_unsafe_or_unsupported_changes() {
    let w = Workspace::new();
    let baseline = w.read_page();
    let cases = [
        ("txt", toml::Value::String("超出容量的中文文本".into())),
        ("w", 0.into()),
        ("x", 65535.into()),
        ("val", 256.into()),
        ("font", 9.into()),
        ("sta", 0.into()),
        ("opaque", "raw:123456".into()),
    ];
    for (key, value) in cases {
        let mut page = baseline.clone();
        page.objects[0].properties.insert(key.into(), value);
        w.write_page(&page);
        assert!(workspace::prepare(&w.root).is_err(), "未拒绝 {key}");
    }
    let mut page = baseline.clone();
    page.objects[1]
        .properties
        .insert("val".into(), (i32::MAX as i64 + 1).into());
    w.write_page(&page);
    assert!(workspace::prepare(&w.root).is_err());
    let mut page = baseline.clone();
    page.objects.swap(0, 1);
    w.write_page(&page);
    assert!(workspace::prepare(&w.root).is_err());
    let mut page = baseline.clone();
    page.objects[0].name = "renamed".into();
    w.write_page(&page);
    assert!(workspace::prepare(&w.root).is_err());
    let mut page = baseline.clone();
    page.objects[0]
        .events
        .insert("timer".into(), "events/new.s".into());
    w.write_page(&page);
    assert!(workspace::prepare(&w.root).is_err());
    for path in [
        "../outside.s",
        "/tmp/outside.s",
        "C:/outside.s",
        "events/../../outside.s",
        "events\\outside.s",
    ] {
        let mut page = baseline.clone();
        page.objects[0].events.insert("up".into(), path.into());
        w.write_page(&page);
        assert!(workspace::prepare(&w.root).is_err());
    }
    w.write_page(&baseline);
    fs::remove_file(w.root.join("pages/p000_page0").join(button_path(&baseline))).unwrap();
    assert!(workspace::prepare(&w.root).is_err());
}

#[test]
fn snapshot_assets_and_versions_are_checked() {
    let w = Workspace::new();
    let font = w.root.join("assets/fonts/resource_0.zi");
    let old = fs::read(&font).unwrap();
    fs::write(&font, b"changed").unwrap();
    assert!(workspace::prepare(&w.root).is_err());
    fs::write(&font, old).unwrap();
    let snapshot = w.root.join(".hmi/container.bin");
    let mut bytes = w.bytes.clone();
    bytes[0x10000] ^= 1;
    fs::write(&snapshot, bytes).unwrap();
    assert!(workspace::prepare(&w.root).is_err());
    fs::write(&snapshot, &w.bytes).unwrap();
    let project = w.root.join("project.toml");
    let text = fs::read_to_string(&project)
        .unwrap()
        .replace("format_version = 1", "format_version = 99");
    fs::write(project, text).unwrap();
    assert!(workspace::prepare(&w.root).is_err());
}

#[test]
fn failed_build_does_not_overwrite_and_force_export_preserves_user_files() {
    let w = Workspace::new();
    let output = w.temporary.path().join("output.HMI");
    fs::write(&output, b"keep").unwrap();
    let mut page = w.read_page();
    page.objects[0].properties.insert("w".into(), 0.into());
    w.write_page(&page);
    assert!(!workspace::build(&w.root, &output, &Default::default()).success);
    assert_eq!(fs::read(&output).unwrap(), b"keep");
    fs::write(w.root.join("notes.md"), "用户笔记").unwrap();
    let input = w.temporary.path().join("input.HMI");
    fs::write(&input, &w.bytes).unwrap();
    assert!(workspace::export(&input, &w.root, false).is_err());
    workspace::export(&input, &w.root.join("."), true).unwrap();
    assert_eq!(
        fs::read_to_string(w.root.join("notes.md")).unwrap(),
        "用户笔记"
    );
    assert!(workspace::build(&w.root, &output, &Default::default()).success);
    assert_eq!(fs::read(&output).unwrap(), w.bytes);
    assert!(
        !workspace::build(
            &w.root,
            &w.root.join(".hmi/container.bin"),
            &Default::default()
        )
        .success
    );
    assert!(workspace::build(&w.root, &output, &Default::default()).success);
}

#[test]
fn malformed_binary_never_panics_and_unknown_fields_roundtrip() {
    let original = common::fixture();
    for bytes in [
        vec![],
        original[..100].to_vec(),
        original[..original.len() - 1].to_vec(),
    ] {
        if let Ok(container) = Container::parse(bytes) {
            assert!(container.strict().is_err());
        }
    }
    let mut count = original.clone();
    put_u32(&mut count, 0, u32::MAX as usize).unwrap();
    put_u32(&mut count, BACKUP_START, u32::MAX as usize).unwrap();
    assert!(Container::parse(count).is_err());
    let mut duplicate = original.clone();
    for base in [0, BACKUP_START] {
        let index = base + 4 + 5 * 28;
        duplicate[index..index + 16].fill(0);
        duplicate[index..index + 3].copy_from_slice(b"0.i");
    }
    common::fix_directory(&mut duplicate);
    assert!(Container::parse(duplicate).unwrap().strict().is_err());
    let mut page = common::page();
    put_u32(&mut page, 56, u32::MAX as usize).unwrap();
    assert!(Page::parse(&page).is_err());
    let c = Container::parse(original.clone()).unwrap();
    assert_eq!(c.rebuild(&BTreeMap::new()).unwrap(), original);
    let p = Project::parse(&c).unwrap();
    for (_, page) in p.pages {
        for object in page.objects {
            assert_eq!(
                object.rebuild(&BTreeMap::new(), &BTreeMap::new()).unwrap(),
                object.raw
            );
        }
    }
}

#[test]
fn corrupted_page_counts_lengths_and_markers_do_not_panic() {
    let original = common::page();
    for offset in [4, 8, 12, 56, 60, 68, 72] {
        let mut bytes = original.clone();
        put_u32(&mut bytes, offset, u32::MAX as usize).unwrap();
        let result = std::panic::catch_unwind(|| Page::parse(&bytes));
        assert!(result.is_ok(), "偏移 {offset} 导致 panic");
    }
    for length in [0, 4, 55, 56, 90, original.len() - 1] {
        assert!(Page::parse(&original[..length]).is_err());
    }
    let object = hmi_compiler::page::Object::parse(&[5, 0, 0, 0, b'a', b't', b't', b'-', b'9']);
    assert!(object.is_err());
    let object = hmi_compiler::page::Object::parse(&[0xFF; 16]);
    assert!(object.is_err());
}

#[test]
fn global_code_and_script_diagnostics() {
    let w = Workspace::new();
    fs::write(w.root.join("program.s"), "// t9.bad\nint sys0=0\npage 0\n").unwrap();
    let mut page = w.read_page();
    let path = button_path(&page);
    fs::write(w.root.join("pages/p000_page0").join(path), "// missing.obj.bad\nb0.txt=\"missing.obj.bad\"\nmissing.txt=\"x\"\nb[99].val=1\nb[sys0].val=1\np[99].b[1].val=1\n").unwrap();
    page.objects[0].properties.insert("vscope".into(), 1.into());
    w.write_page(&page);
    let prepared = workspace::prepare(&w.root).unwrap();
    assert!(prepared.changed_resources.contains(&"Program.s".into()));
    assert!(
        prepared
            .diagnostics
            .iter()
            .any(|d| d.code == "script_object" && d.location.line == Some(3))
    );
    assert!(
        prepared
            .diagnostics
            .iter()
            .any(|d| d.code == "script_dynamic" && d.location.line == Some(5))
    );
    assert!(
        prepared
            .diagnostics
            .iter()
            .any(|d| d.code == "script_page" && d.location.line == Some(6))
    );
    assert!(
        !prepared
            .diagnostics
            .iter()
            .any(|d| d.location.line == Some(1) || d.location.line == Some(2))
    );
    let out = w.temporary.path().join("denied.HMI");
    assert!(
        !workspace::build(
            &w.root,
            &out,
            &hmi_compiler::build::BuildOptions {
                warnings: hmi_compiler::warning::WarningPolicy::parse(&["error".into()], false)
                    .unwrap(),
                ..Default::default()
            }
        )
        .success
    );
    assert!(!out.exists());
}

#[test]
fn diff_distinguishes_properties_events_and_container_only_changes() {
    let w = Workspace::new();
    let mut page = w.read_page();
    page.objects[0].properties.insert("w".into(), 140.into());
    page.objects[0].events.remove("up");
    w.write_page(&page);
    let old = Container::parse(w.bytes.clone()).unwrap();
    let new = Container::parse(workspace::prepare(&w.root).unwrap().bytes).unwrap();
    let report = hmi_compiler::diff::compare(&old, &new).unwrap();
    assert!(!report.active_resources_equal);
    assert!(
        report
            .changes
            .iter()
            .any(|c| c.kind == "property" && c.field.as_deref() == Some("w"))
    );
    assert!(
        report
            .changes
            .iter()
            .any(|c| c.kind == "event" && c.field.as_deref() == Some("up"))
    );
    let mut bytes = w.bytes;
    bytes[0x10000] ^= 1;
    let metadata = Container::parse(bytes).unwrap();
    let report = hmi_compiler::diff::compare(&old, &metadata).unwrap();
    assert!(report.active_resources_equal && report.container_changed && !report.byte_identical);
    assert!(report.changes.is_empty());
}
