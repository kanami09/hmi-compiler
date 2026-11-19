use hmi_core::{
    file,
    model::{AssetSource, PageSource, ProjectSource},
    report::Report,
    target::TargetConfig,
};
use image::ImageEncoder;
use std::{
    fs,
    path::Path,
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    },
};

fn setup() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    hmi_cli::init(
        root,
        &TargetConfig {
            profile: "TJC4832K035_011".into(),
            format: "ver21234".into(),
            encoding: "utf-8".into(),
            orientation: 0,
        },
    )
    .unwrap();
    let mut project: ProjectSource = file::read_toml(&root.join("hmi.toml")).unwrap();
    for name in ["other", "last"] {
        fs::write(
            root.join(format!("{name}.toml")),
            file::toml(&hmi_page_compiler::empty_page(name)).unwrap(),
        )
        .unwrap();
        project.pages.sources.push(format!("{name}.toml"));
    }
    fs::create_dir(root.join("assets")).unwrap();
    for (name, color) in [("red", [255, 0, 0, 255]), ("green", [0, 255, 0, 255])] {
        let mut bytes = vec![];
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(&color.repeat(16), 4, 4, image::ExtendedColorType::Rgba8)
            .unwrap();
        fs::write(root.join(format!("assets/{name}.png")), bytes).unwrap();
        fs::write(
            root.join(format!("assets/{name}.toml")),
            format!("schema='hmi.asset/1'\nkind='image'\nsource='{name}.png'\n"),
        )
        .unwrap();
    }
    for (key, source) in [("first", "red"), ("duplicate", "red"), ("third", "green")] {
        project.assets.items.push(AssetSource {
            key: key.into(),
            source: format!("assets/{source}.toml"),
        });
    }
    fs::write(root.join("hmi.toml"), file::toml(&project).unwrap()).unwrap();
    temp
}
fn rendezvous() -> Report {
    let barrier = Arc::new(Barrier::new(2));
    let started = AtomicUsize::new(0);
    Report::new(Some(Arc::new(move |event| {
        // 要求两个编译任务真正重叠，避免靠耗时比较猜测是否并行。
        if event["event"] == "task_started"
            && event["task"]["stage"] == "compile"
            && started.fetch_add(1, Ordering::SeqCst) < 2
        {
            barrier.wait();
        }
    })))
}

#[test]
fn parallel_output_order_cache_and_cleanup_match_serial_build() {
    let tmp = setup();
    let root = tmp.path();
    let (_, serial, _) =
        hmi_cli::build_with_jobs(root, None, true, true, 1, &Report::default()).unwrap();
    let (_, parallel, stats) =
        hmi_cli::build_with_jobs(root, None, true, true, 4, &rendezvous()).unwrap();
    assert_eq!(parallel.bytes, serial.bytes);
    assert_eq!(parallel.map, serial.map);
    assert_eq!(stats.encoded, 3);
    let (_, warm, stats) =
        hmi_cli::build_with_jobs(root, None, true, false, 4, &Report::default()).unwrap();
    assert_eq!(warm.bytes, serial.bytes);
    assert_eq!(
        (
            stats.compiled,
            stats.encoded,
            stats.reused_objects,
            stats.reused_assets
        ),
        (0, 0, 4, 3)
    );
    let ledger: serde_json::Value = file::read_json(&root.join(".build/hmi-owned.json")).unwrap();
    for name in ledger["files"].as_object().unwrap().keys() {
        assert!(root.join(name).is_file());
    }
    assert!(hmi_cli::clean(root).unwrap() > 0);
    assert_eq!(
        fs::read(root.join("dist/Panel/Panel.HMI")).unwrap(),
        serial.bytes
    );
    assert!(!root.join(".build/hmi-cache").exists());
}

#[test]
fn parallel_failures_keep_old_hmi_and_their_own_stage() {
    let tmp = setup();
    let root = tmp.path();
    let (_, old, _) =
        hmi_cli::build_with_jobs(root, None, true, false, 1, &Report::default()).unwrap();
    for relative in ["pages/page0/page.toml", "other.toml"] {
        let path = root.join(relative);
        let mut page: PageSource = file::read_toml(&path).unwrap();
        page.properties.insert("bco".into(), 65536.into());
        fs::write(path, file::toml(&page).unwrap()).unwrap();
    }
    let barrier = Arc::new(Barrier::new(2));
    let report = Report::new(Some(Arc::new(move |event| {
        if event["event"] == "task_started"
            && event["task"]["group"] == "pages"
            && event["task"]["action"] == "compile"
        {
            barrier.wait();
        }
    })));
    let error = hmi_cli::build_with_jobs(root, None, true, false, 4, &report).unwrap_err();
    let diagnostics = error.diagnostics();
    assert_eq!(diagnostics.len(), 2);
    assert!(
        diagnostics
            .iter()
            .all(|d| d.stage.as_deref() == Some("compile"))
    );
    assert_eq!(
        fs::read(root.join("dist/Panel/Panel.HMI")).unwrap(),
        old.bytes
    );
    let result = report.result(serde_json::Value::Null);
    for count in result["summary"].as_object().unwrap().values() {
        assert_eq!(
            count["total"].as_u64().unwrap(),
            ["processed", "reused", "failed", "not_run"]
                .iter()
                .map(|k| count[k].as_u64().unwrap())
                .sum::<u64>()
        );
    }
}

#[test]
fn cli_job_limits_are_checked_before_execution() {
    for jobs in ["0", "257"] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_hmi"))
            .args(["--json", "build", "-j", jobs])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stderr.is_empty());
    }
    assert!(
        hmi_cli::build_with_jobs(Path::new("."), None, false, false, 0, &Report::default())
            .is_err()
    );
}
