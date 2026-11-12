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
    let page = hmi_format::page::Page::parse(&compiled.payload).unwrap();
    assert_eq!(
        page.objects[0].events[0].lines,
        [b"sys0=1".to_vec(), vec![], b"sys1=2".to_vec()]
    );
    let before = fs::read(&source).unwrap();
    assert_eq!(run(&source).status.code(), Some(1));
    assert_eq!(fs::read(&source).unwrap(), before);
}
