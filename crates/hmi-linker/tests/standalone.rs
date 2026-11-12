use hmi_core::{
    file,
    target::{DEFAULT_PROGRAM, Target, TargetConfig},
};
use std::{fs, process::Command};

#[test]
fn standalone_link_matches_native_vector_without_any_source_files() {
    let sources = tempfile::tempdir().unwrap();
    let objects = tempfile::tempdir().unwrap();
    let target = Target::resolve(&TargetConfig {
        profile: "TJC4832K035_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation: 270,
    })
    .unwrap();
    let page_source = sources.path().join("page.toml");
    let program_source = sources.path().join("program.s");
    fs::write(
        &page_source,
        file::toml(&hmi_page_compiler::empty_page("page0")).unwrap(),
    )
    .unwrap();
    fs::write(&program_source, DEFAULT_PROGRAM).unwrap();
    hmi_page_compiler::compile_page(&page_source, &target)
        .unwrap()
        .object
        .write(&objects.path().join("page.hmo"), &[])
        .unwrap();
    hmi_page_compiler::compile_program(&program_source, &target)
        .unwrap()
        .object
        .write(&objects.path().join("program.hmo"), &[])
        .unwrap();
    fs::write(
        objects.path().join("target.json"),
        file::json(&target).unwrap(),
    )
    .unwrap();
    fs::write(objects.path().join("link.toml"),"schema='hmi.link/1'\nname='Panel'\ntarget='target.json'\nprogram='program.hmo'\n[pages]\nobjects=['page.hmo']\n").unwrap();
    sources.close().unwrap();
    let output_path = objects.path().join("output.HMI");
    let output = Command::new(env!("CARGO_BIN_EXE_hmi-link"))
        .args([
            objects.path().join("link.toml").as_os_str(),
            "-o".as_ref(),
            output_path.as_os_str(),
            "--json".as_ref(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        hmi_core::sha256(&fs::read(&output_path).unwrap()),
        "978ff2117ea8159157fb9e137c9d5618501b1037b05722961cd744cf2bf7cf9a"
    );
    let previous = fs::read(&output_path).unwrap();
    let program = objects.path().join("program.hmo");
    let mut bytes = fs::read(&program).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(program, bytes).unwrap();
    let failed = Command::new(env!("CARGO_BIN_EXE_hmi-link"))
        .args([
            objects.path().join("link.toml").as_os_str(),
            "-o".as_ref(),
            output_path.as_os_str(),
            "--json".as_ref(),
        ])
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(json["diagnostics"][0]["code"], "object_hash");
    assert_eq!(fs::read(output_path).unwrap(), previous);
}
