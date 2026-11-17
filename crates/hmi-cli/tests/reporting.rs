use serde_json::{Value, json};
use std::process::{Command, Output};
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hmi"))
        .args(args)
        .output()
        .unwrap()
}
fn parse(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn json_v2_covers_success_failure_arguments_help_and_version() {
    for (args, exit) in [
        (vec!["--json", "target", "list"], 0),
        (vec!["--json", "target", "show", "missing"], 1),
        (vec!["--json", "--bad-argument"], 2),
        (vec!["--json", "--help"], 0),
        (vec!["--json", "--version"], 0),
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(exit));
        assert!(output.stderr.is_empty());
        let value = parse(&output);
        assert_eq!(value["schema_version"], 2);
        assert_eq!(value["tool"]["name"], "hmi");
        assert_eq!(value["ok"], exit == 0);
        assert!(!String::from_utf8_lossy(&output.stdout).contains('\u{1b}'));
    }
}
#[test]
fn human_queries_are_available_even_in_quiet_mode() {
    let output = run(&[
        "-q",
        "target",
        "show",
        "TJC4827X543_011",
        "--orientation",
        "90",
    ]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("TJC4827X543_011"));
    assert!(stdout.contains("272×480"));
    assert!(output.stderr.is_empty());
}
#[test]
fn jsonl_ends_with_the_same_final_result_envelope() {
    let output = run(&["--message-format", "jsonl", "target", "list"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let events = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events.first().unwrap()["event"], "command_started");
    let final_event = events.last().unwrap();
    assert_eq!(final_event["event"], "result");
    assert_eq!(final_event["data"]["ok"], true);
    assert_eq!(
        final_event["data"]["result"]["profiles"],
        parse(&run(&["--json", "target", "list"]))["result"]["profiles"]
    );
}
#[test]
fn json_verbosity_does_not_change_the_result_or_enable_human_logs() {
    let normal = parse(&run(&["--json", "target", "list"]));
    for flag in ["-q", "-v"] {
        let output = run(&["--json", flag, "--color", "always", "target", "list"]);
        assert!(output.stderr.is_empty());
        let value = parse(&output);
        assert_eq!(value["result"]["profiles"], normal["result"]["profiles"]);
        assert_eq!(value["result"]["outputs"], normal["result"]["outputs"]);
    }
    let output = run(&["--json", "-v", "-q", "target", "list"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(parse(&output)["diagnostics"][0]["code"], "arguments");
}
#[test]
fn recoverable_link_diagnostics_are_not_truncated_on_failure() {
    let tmp = tempfile::tempdir().unwrap();
    hmi_cli::init(
        tmp.path(),
        &hmi_core::target::TargetConfig {
            profile: "TJC4827X543_011".into(),
            format: "ver21234".into(),
            encoding: "utf-8".into(),
            orientation: 0,
        },
    )
    .unwrap();
    std::fs::write(tmp.path().join("program.s"), "page 99\npage 98\n").unwrap();
    let prepared = hmi_cli::prepare(tmp.path()).unwrap();
    let error = prepared.check(false).unwrap_err();
    assert_eq!(
        error
            .diagnostics()
            .iter()
            .filter(|d| d.code == "script_page")
            .count(),
        2
    );
    assert_eq!(error.as_diagnostic().unwrap().code, "script_page");
    let output = Command::new(env!("CARGO_BIN_EXE_hmi"))
        .args(["--json", "check"])
        .arg(tmp.path())
        .output()
        .unwrap();
    let value = parse(&output);
    assert_eq!(value["ok"], false);
    assert_eq!(value["diagnostics"].as_array().unwrap().len(), 2);
    assert_eq!(value["diagnostics"][0]["stage"], json!("analysis"));
}

fn project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    hmi_cli::init(
        tmp.path(),
        &hmi_core::target::TargetConfig {
            profile: "TJC4827X543_011".into(),
            format: "ver21234".into(),
            encoding: "utf-8".into(),
            orientation: 0,
        },
    )
    .unwrap();
    tmp
}
fn project_run(root: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hmi"))
        .args(args)
        .arg(root)
        .output()
        .unwrap()
}
#[test]
fn build_reports_cache_partition_and_published_bytes() {
    let tmp = project();
    let cold = project_run(tmp.path(), &["--json", "build"]);
    assert!(cold.status.success());
    assert!(cold.stderr.is_empty());
    let cold = parse(&cold);
    assert_eq!(
        cold["result"]["summary"]["pages"],
        json!({"total":1,"processed":1,"reused":0,"failed":0,"not_run":0})
    );
    for output in cold["result"]["outputs"].as_array().unwrap() {
        assert_eq!(output["state"], "written");
        let bytes = std::fs::read(output["path"].as_str().unwrap()).unwrap();
        assert_eq!(output["size_bytes"], bytes.len());
        assert_eq!(output["sha256"], hmi_core::sha256(&bytes));
    }
    let warm = parse(&project_run(tmp.path(), &["--json", "build"]));
    assert_eq!(
        warm["result"]["summary"]["pages"],
        json!({"total":1,"processed":0,"reused":1,"failed":0,"not_run":0})
    );
    assert_eq!(warm["result"]["sha256"], cold["result"]["sha256"]);
    let quiet = project_run(tmp.path(), &["-q", "build"]);
    assert!(quiet.status.success());
    assert!(quiet.stdout.is_empty());
    assert!(quiet.stderr.is_empty());
    let verbose = project_run(tmp.path(), &["-v", "--color", "never", "build"]);
    assert!(verbose.status.success());
    assert!(verbose.stdout.is_empty());
    let text = String::from_utf8(verbose.stderr).unwrap();
    assert!(text.contains("复用 页面 page0"));
    assert!(text.contains("构建缓存"));
    assert!(!text.contains('\u{1b}'));
}
#[test]
fn failed_build_preserves_partial_statistics_and_does_not_claim_old_outputs() {
    use hmi_core::{
        file,
        model::{PageSource, ProjectSource},
    };
    let tmp = project();
    let cold = parse(&project_run(tmp.path(), &["--json", "build"]));
    let original = std::fs::read(cold["result"]["output"].as_str().unwrap()).unwrap();
    let path = tmp.path().join("pages/page0/page.toml");
    let mut page: PageSource = file::read_toml(&path).unwrap();
    page.properties.insert("bco".into(), 65536.into());
    std::fs::write(path, file::toml(&page).unwrap()).unwrap();
    let mut project: ProjectSource = file::read_toml(&tmp.path().join("hmi.toml")).unwrap();
    std::fs::write(
        tmp.path().join("other.toml"),
        file::toml(&hmi_page_compiler::empty_page("other")).unwrap(),
    )
    .unwrap();
    project.pages.sources.push("other.toml".into());
    std::fs::write(tmp.path().join("hmi.toml"), file::toml(&project).unwrap()).unwrap();
    let output = project_run(tmp.path(), &["--json", "build"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let failed = parse(&output);
    assert_eq!(failed["result"]["summary"]["program"]["reused"], 1);
    assert_eq!(
        failed["result"]["summary"]["pages"],
        json!({"total":2,"processed":0,"reused":0,"failed":1,"not_run":1})
    );
    assert_eq!(failed["diagnostics"][0]["stage"], "compile");
    for output in failed["result"]["outputs"].as_array().unwrap() {
        assert_eq!(output["state"], "not_written");
        assert!(output["sha256"].is_null());
    }
    assert_eq!(
        std::fs::read(cold["result"]["output"].as_str().unwrap()).unwrap(),
        original
    );
}
#[test]
fn build_jsonl_exposes_task_events_without_interleaving_human_logs() {
    let tmp = project();
    let output = project_run(tmp.path(), &["--message-format", "jsonl", "build"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let events = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(events.iter().any(|e| e["event"] == "task_started"));
    assert!(events.iter().any(|e| e["event"] == "output_written"));
    assert_eq!(events.last().unwrap()["event"], "result");
    assert_eq!(
        events.last().unwrap()["data"]["result"]["summary"]["pages"]["processed"],
        1
    );
}

#[test]
fn source_columns_use_unicode_characters_and_repeated_errors_on_one_line_survive() {
    let tmp = project();
    let text = "// 中文\n    page 99; page 98\n";
    std::fs::write(tmp.path().join("program.s"), text).unwrap();
    let output = project_run(tmp.path(), &["--json", "check"]);
    assert_eq!(output.status.code(), Some(1));
    let result = parse(&output);
    let diagnostics = result["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0]["location"]["column"], 10);
    assert_eq!(diagnostics[1]["location"]["column"], 19);
    for diagnostic in diagnostics {
        assert!(
            std::path::Path::new(diagnostic["location"]["file"].as_str().unwrap()).is_absolute()
        );
        let offset = diagnostic["location"]["offset"].as_u64().unwrap() as usize;
        assert_eq!(&text[offset..offset + 1], "9");
    }
    let human = project_run(tmp.path(), &["--color", "never", "check"]);
    assert!(human.stdout.is_empty());
    let stderr = String::from_utf8(human.stderr).unwrap();
    assert!(stderr.contains("program.s:2:10"));
    assert!(stderr.contains("^^"));
    assert!(stderr.contains("page 98"));
}

#[test]
fn init_plan_import_and_clean_report_their_actual_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("source");
    let output = Command::new(env!("CARGO_BIN_EXE_hmi"))
        .args(["--json", "init"])
        .arg(&root)
        .args(["--target", "TJC4827X543_011"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let result = parse(&output);
    assert_eq!(result["result"]["outputs"].as_array().unwrap().len(), 3);
    for output in result["result"]["outputs"].as_array().unwrap() {
        assert_eq!(output["state"], "written");
    }
    let plan = tmp.path().join("plan");
    let output = Command::new(env!("CARGO_BIN_EXE_hmi"))
        .args(["--json", "plan"])
        .arg(&root)
        .arg("-o")
        .arg(&plan)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        parse(&output)["result"]["outputs"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let built = parse(&project_run(&root, &["--json", "build"]));
    let hmi = built["result"]["output"].as_str().unwrap();
    let imported = tmp.path().join("imported");
    let output = Command::new(env!("CARGO_BIN_EXE_hmi"))
        .args(["--json", "import", hmi, "-o"])
        .arg(&imported)
        .output()
        .unwrap();
    assert!(output.status.success());
    let result = parse(&output);
    assert_eq!(result["result"]["outputs"][0]["state"], "written");
    assert_eq!(result["result"]["outputs"][0]["kind"], "project");
    let output = project_run(&root, &["--json", "clean"]);
    assert!(output.status.success());
    let result = parse(&output);
    assert_eq!(
        result["result"]["removed"].as_u64().unwrap() as usize,
        result["result"]["removed_files"].as_array().unwrap().len()
    );
    assert!(std::path::Path::new(hmi).is_file());
}

#[test]
fn partial_publication_reports_only_files_that_really_were_published() {
    let tmp = project();
    let linked = hmi_cli::prepare(tmp.path()).unwrap().link(true).unwrap();
    let output = tmp.path().join("out.HMI");
    let map = tmp.path().join("map.json");
    let conflict = output.clone();
    let report = hmi_core::report::Report::new(Some(std::sync::Arc::new(move |event| {
        // 故障注入发生在映射已发布、HMI尚未发布之间，不依赖权限或调度时机。
        if event["event"] == "output_written" && event["output"]["kind"] == "link_map" {
            std::fs::create_dir(&conflict).unwrap();
        }
    })));
    let publication =
        hmi_linker::publish::Publication::prepare(&linked, &output, Some(&map), &[]).unwrap();
    let error = publication.write_with_report(&report).unwrap_err();
    let diagnostic = error.as_diagnostic().unwrap();
    assert_eq!(diagnostic.code, "publish_partial");
    assert_eq!(
        diagnostic.details["published_files"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let result = report.result(Value::Null);
    let outputs = result["outputs"].as_array().unwrap();
    assert_eq!(
        outputs.iter().find(|o| o["kind"] == "link_map").unwrap()["state"],
        "written"
    );
    assert_eq!(
        outputs.iter().find(|o| o["kind"] == "hmi").unwrap()["state"],
        "not_written"
    );
    assert!(map.is_file());
    assert!(output.is_dir());
}

#[test]
fn implicit_browser_media_are_counted_during_check_and_build() {
    use hmi_core::{
        file,
        model::{AssetSource, ObjectFile, PageSource, ProjectSource},
    };
    mod zi {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/support/zi.rs"
        ));
    }
    let tmp = project();
    let root = tmp.path();
    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/font.zi"), zi::font()).unwrap();
    std::fs::write(
        root.join("assets/font.toml"),
        "schema='hmi.asset/1'\nkind='font'\nsource='font.zi'\n",
    )
    .unwrap();
    let mut p: ProjectSource = file::read_toml(&root.join("hmi.toml")).unwrap();
    p.assets.items.push(AssetSource {
        key: "ui".into(),
        source: "assets/font.toml".into(),
    });
    std::fs::write(root.join("hmi.toml"), file::toml(&p).unwrap()).unwrap();
    let page_path = root.join("pages/page0/page.toml");
    let mut page: PageSource = file::read_toml(&page_path).unwrap();
    page.objects.push(ObjectFile {
        original_record: None,
        name: "fb0".into(),
        type_name: "file_browser".into(),
        events: Default::default(),
        properties: std::collections::BTreeMap::from([
            ("x".into(), 20.into()),
            ("y".into(), 20.into()),
            ("w".into(), 200.into()),
            ("h".into(), 120.into()),
            (
                "font".into(),
                toml::Value::Table(toml::map::Map::from_iter([("ref".into(), "ui".into())])),
            ),
        ]),
    });
    std::fs::write(page_path, file::toml(&page).unwrap()).unwrap();
    for command in ["check", "build"] {
        let output = project_run(root, &["--json", command]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            parse(&output)["result"]["summary"]["media"],
            json!({"total":3,"processed":3,"reused":0,"failed":0,"not_run":0})
        );
    }
}
