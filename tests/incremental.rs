mod common;
use common::Workspace;
use hmi_compiler::{
    build::{self, BuildEvent, BuildMode, BuildOptions, BuildOutcome},
    container::Container,
    sha256,
    warning::WarningPolicy,
    workspace::{self, PageFile},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

fn run(w: &Workspace, mode: BuildMode) -> BuildOutcome {
    let outcome = workspace::build(
        &w.root,
        &w.temporary.path().join("output.HMI"),
        &BuildOptions {
            mode,
            ..Default::default()
        },
    );
    assert!(outcome.success, "{:?}", outcome.diagnostics);
    outcome
}
fn directory(w: &Workspace) -> PathBuf {
    w.root.join(".build/hmi-compiler")
}
fn output(w: &Workspace) -> Vec<u8> {
    fs::read(w.temporary.path().join("output.HMI")).unwrap()
}
fn read_page_at(w: &Workspace, index: usize) -> (PathBuf, PageFile) {
    let path = w
        .root
        .join(format!("pages/p{index:03}_page{index}/page.toml"));
    let data = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    (path, data)
}
fn write_page(path: &Path, page: &PageFile) {
    fs::write(path, workspace::page_to_toml(page).unwrap()).unwrap();
}
fn mutate_index(w: &Workspace, mutate: impl FnOnce(&mut serde_json::Value)) {
    let path = directory(w).join("manifest.json");
    let mut value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    mutate(&mut value);
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

#[test]
fn cold_hot_full_and_check_have_identical_outputs() {
    let w = Workspace::with_pages(2);
    let check = workspace::check(&w.root, &Default::default());
    assert!(check.success);
    assert!(!w.root.join(".build").exists());
    let cold = run(&w, BuildMode::Incremental);
    assert_eq!(
        (cold.report.rebuilt_pages, cold.report.reused_pages),
        (2, 0)
    );
    assert!(!cold.report.baseline_reused);
    assert_eq!(output(&w), w.bytes);
    let hot = run(&w, BuildMode::Incremental);
    assert_eq!(
        (
            hot.report.rebuilt_pages,
            hot.report.reused_pages,
            hot.report.diagnostic_rechecks
        ),
        (0, 2, 0)
    );
    assert!(
        hot.report.baseline_reused
            && hot.report.program_reused
            && !hot.report.global_diagnostic_rechecked
    );
    let manifest = fs::read(directory(&w).join("manifest.json")).unwrap();
    assert!(workspace::check(&w.root, &Default::default()).success);
    assert_eq!(
        manifest,
        fs::read(directory(&w).join("manifest.json")).unwrap()
    );
    let full = run(&w, BuildMode::Full);
    assert_eq!(
        (
            full.report.rebuilt_pages,
            full.report.reused_pages,
            full.report.diagnostic_rechecks
        ),
        (2, 0, 2)
    );
    assert!(!full.report.baseline_reused && !full.report.program_reused);
    assert_eq!(output(&w), w.bytes);
}

#[test]
fn one_page_change_is_detected_even_with_unchanged_timestamp() {
    let w = Workspace::with_pages(2);
    run(&w, BuildMode::Incremental);
    let path = w.page_path();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let mut page = w.read_page();
    page.objects[0].properties.insert("x".into(), 11.into());
    w.write_page(&page);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let built = run(&w, BuildMode::Incremental);
    assert_eq!(
        (built.report.rebuilt_pages, built.report.reused_pages),
        (1, 1)
    );
    assert_eq!(built.report.changed_resources, ["39.pa"]);
    let incremental = output(&w);
    run(&w, BuildMode::Full);
    assert_eq!(output(&w), incremental);
    let (path, page) = read_page_at(&w, 1);
    let script = path.parent().unwrap().join(&page.objects[0].events["up"]);
    fs::write(script, "dp=0\n// 单页脚本改变\n").unwrap();
    let built = run(&w, BuildMode::Incremental);
    assert_eq!(
        (built.report.rebuilt_pages, built.report.reused_pages),
        (1, 1)
    );
    assert_eq!(built.report.changed_resources, ["39.pa", "40.pa"]);
}

#[test]
fn warning_policy_replays_on_cache_hit_and_scope_invalidates_analysis() {
    let w = Workspace::with_pages(2);
    let page0 = w.read_page();
    let script = w
        .root
        .join("pages/p000_page0")
        .join(&page0.objects[0].events["up"]);
    fs::write(&script, "page1.b0.txt=\"x\"\n").unwrap();
    let initial = run(&w, BuildMode::Incremental);
    assert!(initial.diagnostics.iter().any(|d| d.code == "script_scope"));
    let previous = output(&w);
    let options = BuildOptions {
        warnings: WarningPolicy::parse(&["error".into()], false).unwrap(),
        ..Default::default()
    };
    let failed = workspace::build(&w.root, &w.temporary.path().join("output.HMI"), &options);
    assert!(!failed.success);
    assert_eq!(failed.report.reused_pages, 2);
    assert_eq!(failed.report.diagnostic_rechecks, 0);
    assert_eq!(output(&w), previous);
    let suppressed = workspace::build(
        &w.root,
        &w.temporary.path().join("output.HMI"),
        &BuildOptions {
            warnings: WarningPolicy::parse(&["error".into(), "no-script-scope".into()], false)
                .unwrap(),
            ..Default::default()
        },
    );
    assert!(suppressed.success && suppressed.diagnostics.is_empty());
    assert_eq!(suppressed.report.reused_pages, 2);
    let (path, mut page1) = read_page_at(&w, 1);
    page1.objects[0]
        .properties
        .insert("vscope".into(), 1.into());
    write_page(&path, &page1);
    let fixed = workspace::build(&w.root, &w.temporary.path().join("output.HMI"), &options);
    assert!(fixed.success, "{:?}", fixed.diagnostics);
    assert_eq!(
        (
            fixed.report.rebuilt_pages,
            fixed.report.reused_pages,
            fixed.report.diagnostic_rechecks
        ),
        (1, 1, 2)
    );
    assert!(fixed.report.pages[0].reused);
    assert!(!fixed.diagnostics.iter().any(|d| d.code == "script_scope"));
}

#[test]
fn global_code_rechecks_diagnostics_without_rebuilding_page_bytes() {
    let w = Workspace::with_pages(2);
    run(&w, BuildMode::Incremental);
    let path = w.root.join("program.s");
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b"// global edit\r\n");
    fs::write(path, bytes).unwrap();
    let outcome = run(&w, BuildMode::Incremental);
    assert_eq!(
        (
            outcome.report.rebuilt_pages,
            outcome.report.reused_pages,
            outcome.report.diagnostic_rechecks
        ),
        (0, 2, 2)
    );
    assert!(!outcome.report.program_reused && outcome.report.global_diagnostic_rechecked);
    assert_eq!(outcome.report.changed_resources, ["Program.s"]);
    let incremental = output(&w);
    run(&w, BuildMode::Full);
    assert_eq!(output(&w), incremental);
}

#[test]
fn corrupt_artifacts_diagnostics_baseline_and_versions_regenerate() {
    let w = Workspace::with_pages(2);
    run(&w, BuildMode::Incremental);
    let source = Container::parse(w.bytes.clone()).unwrap();
    let index = source.active("39.pa").unwrap().index;
    fs::write(directory(&w).join(format!("pages/{index}.pa")), b"corrupt").unwrap();
    let fixed = run(&w, BuildMode::Incremental);
    assert_eq!(
        (fixed.report.rebuilt_pages, fixed.report.reused_pages),
        (1, 1)
    );
    fs::write(
        directory(&w).join(format!("diagnostics/{index}.json")),
        b"corrupt",
    )
    .unwrap();
    let fixed = run(&w, BuildMode::Incremental);
    assert_eq!(
        (fixed.report.rebuilt_pages, fixed.report.diagnostic_rechecks),
        (0, 1)
    );
    fs::write(directory(&w).join("baseline.json"), b"corrupt").unwrap();
    let fixed = run(&w, BuildMode::Incremental);
    assert!(!fixed.report.baseline_reused);
    assert_eq!(fixed.report.reused_pages, 2);
    for (key, value) in [
        ("version", serde_json::json!(999)),
        ("tool", serde_json::json!("other-tool")),
        ("source", serde_json::json!("other-source")),
    ] {
        mutate_index(&w, |index| index[key] = value);
        let fixed = run(&w, BuildMode::Incremental);
        assert_eq!(fixed.report.rebuilt_pages, 2);
        assert!(!fixed.report.baseline_reused);
    }
    fs::write(directory(&w).join("manifest.json"), b"invalid json").unwrap();
    assert_eq!(run(&w, BuildMode::Incremental).report.rebuilt_pages, 2);
    assert_eq!(output(&w), w.bytes);
}

#[test]
fn failed_build_keeps_successful_page_cache_and_existing_output() {
    let w = Workspace::with_pages(2);
    let target = w.temporary.path().join("output.HMI");
    fs::write(&target, b"old target").unwrap();
    let original = w.read_page();
    let mut invalid = original.clone();
    invalid.objects[0].properties.insert("w".into(), 0.into());
    w.write_page(&invalid);
    let failed = workspace::build(&w.root, &target, &Default::default());
    assert!(!failed.success);
    assert_eq!(failed.report.pages.len(), 2);
    assert_eq!(fs::read(&target).unwrap(), b"old target");
    assert!(failed.report.pages[1].success);
    w.write_page(&original);
    let fixed = run(&w, BuildMode::Incremental);
    assert_eq!(
        (fixed.report.rebuilt_pages, fixed.report.reused_pages),
        (1, 1)
    );
    assert_eq!(output(&w), w.bytes);
}

#[test]
fn cache_io_failure_falls_back_and_clean_preserves_sources_and_target() {
    let w = Workspace::with_pages(2);
    fs::create_dir(w.root.join(".build")).unwrap();
    fs::write(directory(&w), b"not a directory").unwrap();
    let mut events = Vec::new();
    let target = w.temporary.path().join("output.HMI");
    let outcome = build::run(&w.root, Some(&target), &Default::default(), &mut |e| {
        events.push(e.clone())
    });
    assert!(outcome.success);
    assert_eq!(outcome.report.rebuilt_pages, 2);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, BuildEvent::Info(message) if message.contains("回退")))
    );
    fs::remove_file(directory(&w)).unwrap();
    run(&w, BuildMode::Incremental);
    fs::write(w.root.join(".build/user.txt"), b"keep").unwrap();
    let source = fs::read(w.root.join(".hmi/container.bin")).unwrap();
    let before = fs::read(&target).unwrap();
    assert!(build::clean(&w.root, &mut |_| {}).unwrap().removed);
    assert!(!directory(&w).exists());
    assert!(w.root.join(".build/user.txt").exists());
    assert_eq!(fs::read(w.root.join(".hmi/container.bin")).unwrap(), source);
    assert_eq!(fs::read(&target).unwrap(), before);
    assert!(!build::clean(&w.root, &mut |_| {}).unwrap().removed);
    assert_eq!(run(&w, BuildMode::Incremental).report.rebuilt_pages, 2);
}

#[test]
fn build_and_clean_share_the_same_exclusive_lock() {
    let w = Workspace::new();
    run(&w, BuildMode::Incremental);
    let lock_path = w.root.join(".build/hmi-compiler.lock");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    lock.lock().unwrap();
    let (sender, receiver) = mpsc::channel();
    let root = w.root.clone();
    let target = w.temporary.path().join("parallel.HMI");
    let thread = std::thread::spawn(move || {
        build::run(&root, Some(&target), &Default::default(), &mut |e| {
            if matches!(e, BuildEvent::Info(s) if s.contains("等待")) {
                sender.send(()).unwrap();
            }
        })
    });
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("构建必须等待工程锁");
    lock.unlock().unwrap();
    assert!(thread.join().unwrap().success);
    lock.lock().unwrap();
    let (sender, receiver) = mpsc::channel();
    let root = w.root.clone();
    let thread = std::thread::spawn(move || {
        build::clean(&root, &mut |s| {
            if s.contains("等待") {
                sender.send(()).unwrap();
            }
        })
    });
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("清理必须等待同一个工程锁");
    assert!(directory(&w).exists());
    lock.unlock().unwrap();
    assert!(thread.join().unwrap().unwrap().removed);
}

#[test]
fn output_cannot_overwrite_cache_and_snapshot_corruption_is_not_repaired() {
    let w = Workspace::new();
    run(&w, BuildMode::Incremental);
    let manifest = directory(&w).join("manifest.json");
    let before = fs::read(&manifest).unwrap();
    let failed = workspace::build(&w.root, &manifest, &Default::default());
    assert!(!failed.success);
    assert_eq!(fs::read(manifest).unwrap(), before);
    let snapshot = w.root.join(".hmi/container.bin");
    let mut bytes = w.bytes.clone();
    bytes[0x10000] ^= 1;
    fs::write(&snapshot, bytes).unwrap();
    let previous = output(&w);
    let failed = workspace::build(
        &w.root,
        &w.temporary.path().join("output.HMI"),
        &Default::default(),
    );
    assert!(!failed.success);
    assert_eq!(failed.diagnostics[0].code, "snapshot_hash");
    assert_eq!(output(&w), previous);
}

#[cfg(unix)]
#[test]
fn cache_symlinks_are_rejected_without_touching_target() {
    let w = Workspace::new();
    let external = tempfile::tempdir().unwrap();
    fs::create_dir(w.root.join(".build")).unwrap();
    fs::write(external.path().join("sentinel"), b"keep").unwrap();
    std::os::unix::fs::symlink(external.path(), directory(&w)).unwrap();
    let outcome = workspace::build(
        &w.root,
        &w.temporary.path().join("out.HMI"),
        &Default::default(),
    );
    assert!(!outcome.success);
    assert_eq!(outcome.diagnostics[0].code, "cache_path");
    assert!(build::clean(&w.root, &mut |_| {}).is_err());
    assert_eq!(fs::read(external.path().join("sentinel")).unwrap(), b"keep");
}

#[test]
fn toml_line_changes_refresh_cached_property_diagnostics() {
    use hmi_compiler::page::Page;
    use std::collections::BTreeMap;
    let container = Container::parse(common::fixture()).unwrap();
    let entry = container.active("39.pa").unwrap();
    let original = Page::parse(container.data(entry).unwrap()).unwrap();
    let mut objects: Vec<_> = original.objects.iter().map(|o| o.raw.clone()).collect();
    objects[1] = original.objects[1]
        .rebuild(
            &BTreeMap::from([("font".into(), vec![9])]),
            &BTreeMap::new(),
        )
        .unwrap();
    let bytes = container
        .rebuild(&BTreeMap::from([(
            entry.index,
            original.rebuild(&objects).unwrap(),
        )]))
        .unwrap();
    let w = Workspace::from_bytes(bytes);
    let first = run(&w, BuildMode::Incremental);
    let first_line = first
        .diagnostics
        .iter()
        .find(|d| d.code == "property_resource")
        .unwrap()
        .location
        .line
        .unwrap();
    let file = w.page_path();
    let text = fs::read_to_string(&file).unwrap();
    fs::write(
        file,
        format!(
            "# first comment\r\n# second comment\r\n{}",
            text.replace('\n', "\r\n")
        ),
    )
    .unwrap();
    let second = run(&w, BuildMode::Incremental);
    assert_eq!(second.report.rebuilt_pages, 1);
    assert_eq!(
        second
            .diagnostics
            .iter()
            .find(|d| d.code == "property_resource")
            .unwrap()
            .location
            .line,
        Some(first_line + 2)
    );
    assert_eq!(sha256(&output(&w)), sha256(&w.bytes));
}
