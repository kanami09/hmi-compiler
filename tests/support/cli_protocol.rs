/// 各独立程序使用同一组协议边界用例，覆盖其各自的参数声明。
pub fn assert_protocol(binary: &str, tool: &str) {
    for (args, exit) in [
        (vec!["--json", "--help"], 0),
        (vec!["--json", "--version"], 0),
        (vec!["--json", "--bad-option"], 2),
    ] {
        let output = std::process::Command::new(binary)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(exit));
        assert!(output.stderr.is_empty());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["schema_version"], 2);
        assert_eq!(value["tool"]["name"], tool);
        assert_eq!(value["ok"], exit == 0);
        if exit != 0 {
            assert_eq!(value["diagnostics"][0]["stage"], "arguments");
        }
    }
    let output = std::process::Command::new(binary)
        .args(["--message-format", "jsonl", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["event"], "result");
    assert_eq!(value["data"]["ok"], true);
}
