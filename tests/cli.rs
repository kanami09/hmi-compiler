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
    (output, result)
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
    assert_eq!(output.status.code(), Some(1));
    let (output, _) = run(&["build", directory, "-o", target, "--force", "--json"]);
    assert!(output.status.success());
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
fn deny_warnings_preserves_diagnostics_and_does_not_write() {
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
    let (output, result) = run(&["check", directory, "--deny-warnings", "--json"]);
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
        "--deny-warnings",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!out.exists());
}
