use hmi_core::{
    file,
    target::{Target, TargetConfig},
};
use std::{fs, process::Command};

#[test]
fn compiler_cli_preserves_event_lines_and_cannot_overwrite_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let target = Target::resolve(&TargetConfig {
        profile: "TJC4827X543_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation: 0,
    })
    .unwrap();
    let config = tmp.path().join("target.json");
    fs::write(&config, file::json(&target).unwrap()).unwrap();
    let source = tmp.path().join("page.toml");
    let mut page = hmi_page_compiler::empty_page("page0");
    page.events.insert("load".into(), "load.s".into());
    let mut variable = hmi_core::model::ObjectFile {
        name: "va0".into(),
        type_name: "variable".into(),
        properties: Default::default(),
        events: Default::default(),
    };
    variable.properties.insert("sta".into(), 1_i64.into());
    variable.properties.insert("val".into(), (-7_i64).into());
    variable.properties.insert("txt".into(), "测试A".into());
    page.objects.push(variable);
    fs::write(&source, file::toml(&page).unwrap()).unwrap();
    fs::write(tmp.path().join("load.s"), "sys0=1\r\n\r\nsys1=2\r\n").unwrap();
    let object = tmp.path().join("page.hmo");
    let run = |output: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_hmi-compile"))
            .args([
                "page".as_ref(),
                source.as_os_str(),
                "--target".as_ref(),
                config.as_os_str(),
                "-o".as_ref(),
                output.as_os_str(),
                "--json".as_ref(),
            ])
            .output()
            .unwrap()
    };
    let result = run(&object);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    assert!(result.stderr.is_empty());
    let compiled = hmi_artifact::CompiledObject::read(&object).unwrap();
    let page_object = hmi_format::page::Object::parse(compiled.record(0).unwrap()).unwrap();
    assert_eq!(
        page_object.events[0].lines,
        [b"sys0=1".to_vec(), vec![], b"sys1=2".to_vec()]
    );
    let inspected = Command::new(env!("CARGO_BIN_EXE_hmi-compile"))
        .args(["inspect".as_ref(), object.as_os_str(), "--json".as_ref()])
        .output()
        .unwrap();
    assert!(inspected.status.success());
    let json: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(json["result"]["objects"][1]["properties"]["val"], -7);
    assert_eq!(json["result"]["objects"][1]["properties"]["txt"], "测试A");
    assert_eq!(json["result"]["objects"][1]["id_pending"], true);
    let before = fs::read(&source).unwrap();
    assert_eq!(run(&source).status.code(), Some(1));
    assert_eq!(fs::read(&source).unwrap(), before);
}
