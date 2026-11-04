mod common;
use common::Workspace;
use serde_json::Value;
use std::{
    fs,
    process::{Command, Output},
};

fn run(args: &[&str]) -> (Output, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_hmi-compiler"))
        .args(args)
        .output()
        .unwrap();
    let result: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "不是单个 JSON 输出：{e}；stdout={}；stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(result["schema_version"], 1);
    assert!(output.stderr.is_empty(), "JSON 模式不应输出普通日志");
    (output, result)
}

#[test]
fn classified_warnings_cli_and_real_errors() {
    let w = Workspace::new();
    let page = w.read_page();
    fs::write(
        w.root
            .join("pages/p000_page0")
            .join(&page.objects[0].events["up"]),
        "b[sys0].val=1\n",
    )
    .unwrap();
    let directory = w.root.to_str().unwrap();
    for flags in [
        vec!["-Wno-script-dynamic"],
        vec!["-Werror", "-Wno-script-dynamic"],
        vec!["-Wno-all"],
        vec!["-Werror", "-w"],
        vec!["-Werror", "-Wno-error=script-dynamic"],
    ] {
        let mut args = vec!["check", directory, "--json"];
        args.extend(flags);
        let (output, _) = run(&args);
        assert!(output.status.success(), "{args:?}");
    }
    for flag in ["-Werror", "-Werror=script-dynamic"] {
        let (output, result) = run(&["check", directory, "--json", flag]);
        assert_eq!(output.status.code(), Some(1));
        let diagnostic = &result["diagnostics"][0];
        assert_eq!(diagnostic["original_severity"], "warning");
        assert_eq!(diagnostic["location"]["event"], "up");
    }
    let (output, _) = run(&["check", directory, "--json", "-Wno-typo"]);
    assert_eq!(output.status.code(), Some(2));
    let mut page = page;
    page.objects[0].properties.insert("w".into(), 0.into());
    w.write_page(&page);
    let (output, result) = run(&["check", directory, "--json", "-w"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "geometry")
    );
}

#[test]
fn page_logs_are_ordered_colored_and_failure_keeps_output() {
    let w = Workspace::with_pages(2);
    let mut page = w.read_page();
    page.objects[0]
        .properties
        .insert("txt".into(), "超过容量的文本内容".into());
    w.write_page(&page);
    let directory = w.root.to_str().unwrap();
    let output = w.temporary.path().join("output.HMI");
    fs::write(&output, b"previous").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_hmi-compiler"))
        .args([
            "build",
            directory,
            "-o",
            output.to_str().unwrap(),
            "--color",
            "never",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    let log = String::from_utf8(result.stderr).unwrap();
    assert!(!log.contains("\x1b[") && !log.contains(">>") && !log.contains("CACHED"));
    assert!(log.contains("[ 50%] [ ERROR   ] page0.b0.txt:"));
    assert!(log.contains("[100%] [ SUCCESS ] page1"));
    assert!(log.find("page0.b0.txt").unwrap() < log.find("[ SUCCESS ] page1").unwrap());
    assert!(log.lines().last().unwrap().contains("构建失败"));
    assert_eq!(fs::read(&output).unwrap(), b"previous");
    let result = Command::new(env!("CARGO_BIN_EXE_hmi-compiler"))
        .args(["check", directory, "--color", "always"])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(String::from_utf8(result.stderr).unwrap().contains("\x1b["));
    let result = Command::new(env!("CARGO_BIN_EXE_hmi-compiler"))
        .args(["check", directory, "--color", "auto"])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(!String::from_utf8(result.stderr).unwrap().contains("\x1b["));
}

#[test]
fn all_commands_json_exit_codes_and_overwrite() {
    let w = Workspace::new();
    let input = w.temporary.path().join("input.HMI");
    fs::write(&input, &w.bytes).unwrap();
    let source = input.to_str().unwrap();
    let directory = w.root.to_str().unwrap();
    let (output, result) = run(&["inspect", source, "--pages", "--resources", "--json"]);
    assert!(output.status.success());
    assert_eq!(result["result"]["objects"], 3);
    assert_eq!(result["result"]["page_list"][0]["resource"], "39.pa");
    let (output, result) = run(&["check", directory, "--json"]);
    assert!(output.status.success());
    assert_eq!(result["result"]["byte_identical"], true);
    let out = w.temporary.path().join("output.HMI");
    let target = out.to_str().unwrap();
    let (output, _) = run(&["build", directory, "-o", target, "--json"]);
    assert!(output.status.success());
    assert_eq!(fs::read(&out).unwrap(), w.bytes);
    let (output, _) = run(&["build", directory, "-o", target, "--json"]);
    assert!(output.status.success());
    let (output, _) = run(&["rebuild", directory, "-o", target, "--json"]);
    assert!(output.status.success());
    let (output, _) = run(&["build", directory, "-o", target, "--force", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let (output, result) = run(&["diff", source, target, "--json"]);
    assert!(output.status.success());
    assert_eq!(result["result"]["byte_identical"], true);
    let other = w.temporary.path().join("another");
    let other = other.to_str().unwrap();
    let (output, _) = run(&["export", source, "-o", other, "--json"]);
    assert!(output.status.success());
    let (output, _) = run(&["export", source, "-o", other, "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let (output, _) = run(&["inspect", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let (output, result) = run(&["inspect", "does-not-exist.HMI", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(result["ok"], false);
    let mut invalid = w.bytes.clone();
    invalid[4 + 25] ^= 1;
    fs::write(&input, invalid).unwrap();
    let (output, result) = run(&["inspect", source, "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "directory_crc")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_hmi-compiler"))
        .args(["--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("export"));
}

#[test]
fn warning_errors_preserve_diagnostics_and_do_not_write() {
    let w = Workspace::new();
    let page = w.read_page();
    fs::write(
        w.root
            .join("pages/p000_page0")
            .join(&page.objects[0].events["up"]),
        "b[sys0].val=1\n",
    )
    .unwrap();
    let directory = w.root.to_str().unwrap();
    let (output, result) = run(&["check", directory, "-Werror", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "script_dynamic")
    );
    let out = w.temporary.path().join("denied.HMI");
    let (output, _) = run(&[
        "build",
        directory,
        "-o",
        out.to_str().unwrap(),
        "-Werror",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!out.exists());
}
