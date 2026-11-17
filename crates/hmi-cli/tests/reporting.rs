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
