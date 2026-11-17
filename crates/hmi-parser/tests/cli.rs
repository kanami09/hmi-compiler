use hmi_core::{
    controls,
    target::{Target, TargetConfig},
};
use hmi_format::write;
use std::{collections::BTreeMap, fs, process::Command};
#[test]
fn independent_parser_exports_new_source_and_rejects_existing_output() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let t = Target::resolve(&TargetConfig {
        profile: "TJC4832T035_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation: 0,
    })
    .unwrap();
    let attributes = controls::PAGE
        .attributes(
            "page0",
            &BTreeMap::from([
                ("w".into(), i64::from(t.canvas.width).into()),
                ("h".into(), i64::from(t.canvas.height).into()),
            ]),
        )
        .unwrap();
    let object = write::object(
        &attributes,
        &controls::PAGE_EVENTS
            .iter()
            .map(|e| ((*e).into(), Vec::new()))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let page = write::page("page0", &[object]).unwrap();
    let main =
        write::main_resource(&t.profile, t.header_tag().unwrap(), 0, &[("pa", "0.pa")]).unwrap();
    let input = root.join("source.HMI");
    fs::write(
        &input,
        write::container(&[
            (
                "Program.s".into(),
                hmi_core::target::DEFAULT_PROGRAM.as_bytes().to_vec(),
            ),
            ("0.pa".into(), page),
            ("main.HMI".into(), main),
        ])
        .unwrap(),
    )
    .unwrap();
    let inspect = Command::new(env!("CARGO_BIN_EXE_hmi-parse"))
        .arg("inspect")
        .arg(&input)
        .arg("--json")
        .output()
        .unwrap();
    assert!(inspect.status.success());
    let json: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(json["result"]["target"]["profile"], t.profile);
    let output = root.join("project");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_hmi-parse"))
            .arg("export")
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .arg("--json")
            .output()
            .unwrap()
    };
    let result = run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    assert!(output.join(".preserve/origin.hpo").is_file());
    assert!(!output.join(".build").exists());
    let before = fs::read(output.join("hmi.toml")).unwrap();
    assert!(!run().status.success());
    assert_eq!(fs::read(output.join("hmi.toml")).unwrap(), before);
}
