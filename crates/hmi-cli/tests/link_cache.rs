use hmi_core::{
    file,
    model::{AssetSource, ProjectSource},
    report::Report,
    target::TargetConfig,
};
use image::ImageEncoder;
use serde_json::Value;
use std::{fs, path::Path};

fn setup() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    hmi_cli::init(
        root,
        &TargetConfig {
            profile: "TJC4827X543_011".into(),
            format: "ver21234".into(),
            encoding: "utf-8".into(),
            orientation: 0,
        },
    )
    .unwrap();
    fs::create_dir(root.join("assets")).unwrap();
    let mut project: ProjectSource = file::read_toml(&root.join("hmi.toml")).unwrap();
    fs::write(
        root.join("other.toml"),
        file::toml(&hmi_page_compiler::empty_page("other")).unwrap(),
    )
    .unwrap();
    project.pages.sources.push("other.toml".into());
    for (name, color, external) in [
        ("red", [255, 0, 0, 255], false),
        ("green", [0, 255, 0, 255], true),
    ] {
        let mut png = vec![];
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&color.repeat(16), 4, 4, image::ExtendedColorType::Rgba8)
            .unwrap();
        fs::write(root.join(format!("assets/{name}.png")), png).unwrap();
        fs::write(
            root.join(format!("assets/{name}.toml")),
            format!(
                "schema='hmi.asset/1'\nkind='image'\nsource='{name}.png'\n{}",
                if external {
                    "[delivery]\nmode='external'\npath='sd0/logo.xi'\n"
                } else {
                    ""
                }
            ),
        )
        .unwrap();
        project.assets.items.push(AssetSource {
            key: name.into(),
            source: format!("assets/{name}.toml"),
        });
    }
    fs::write(root.join("hmi.toml"), file::toml(&project).unwrap()).unwrap();
    tmp
}
fn build(root: &Path, name: &str, force: bool) -> (hmi_linker::Linked, Value) {
    let report = Report::default();
    let output = root.join(name).join("Panel.HMI");
    let (_, linked, _) =
        hmi_cli::build_with_jobs(root, Some(&output), false, force, 2, &report).unwrap();
    (linked, report.result(Value::Null))
}
fn link_count(result: &Value, kind: &str) -> u64 {
    result["summary"]["link"][kind].as_u64().unwrap()
}

#[test]
fn cached_bundle_restores_external_files_at_new_outputs_and_corruption_rebuilds() {
    let tmp = setup();
    let root = tmp.path();
    let (cold, stats) = build(root, "first", false);
    assert_eq!(link_count(&stats, "processed"), 1);
    let (warm, stats) = build(root, "second", false);
    assert_eq!(link_count(&stats, "reused"), 1);
    assert_eq!(cold.bytes, warm.bytes);
    assert_eq!(cold.map, warm.map);
    assert_eq!(warm.external_files.len(), 1);
    assert_eq!(
        fs::read(root.join("second/sd0/logo.xi")).unwrap(),
        cold.external_files[0].bytes
    );
    let cache = fs::read_dir(root.join(".build/hmi-cache"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "hml"))
        .unwrap();
    let mut bytes = fs::read(&cache).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(&cache, bytes).unwrap();
    let (recovered, stats) = build(root, "second", false);
    assert_eq!(link_count(&stats, "processed"), 1);
    assert_eq!(recovered.bytes, cold.bytes);
    let (forced, stats) = build(root, "second", true);
    assert_eq!(link_count(&stats, "processed"), 1);
    assert_eq!(forced.bytes, cold.bytes);
    hmi_cli::clean(root).unwrap();
    assert!(!cache.exists());
    assert_eq!(
        fs::read(root.join("second/sd0/logo.xi")).unwrap(),
        cold.external_files[0].bytes
    );
}

#[test]
fn current_source_order_symbols_and_content_invalidate_the_link_result() {
    let tmp = setup();
    let root = tmp.path();
    build(root, "out", false);
    let path = root.join("hmi.toml");
    let mut project: ProjectSource = file::read_toml(&path).unwrap();
    project.pages.sources.reverse();
    fs::write(&path, file::toml(&project).unwrap()).unwrap();
    let (pages, stats) = build(root, "out", false);
    assert_eq!(link_count(&stats, "processed"), 1);
    assert_eq!(pages.map["pages"][0]["name"], "other");
    project.assets.items[0].key = "renamed".into();
    fs::write(&path, file::toml(&project).unwrap()).unwrap();
    let (renamed, stats) = build(root, "out", false);
    assert_eq!(link_count(&stats, "processed"), 1);
    assert_eq!(renamed.map["assets"][0]["key"], "renamed");
    fs::write(root.join("program.s"), "sys0=7\npage 0\n").unwrap();
    let (changed, stats) = build(root, "out", false);
    assert_eq!(link_count(&stats, "processed"), 1);
    assert_ne!(changed.bytes, renamed.bytes);
    fs::remove_file(root.join("assets/red.png")).unwrap();
    assert!(hmi_cli::build(root, Some(&root.join("out/Panel.HMI")), false).is_err());
    assert_eq!(fs::read(root.join("out/Panel.HMI")).unwrap(), changed.bytes);
}

#[test]
fn cache_hits_recheck_warning_policy_and_preserve_the_old_hmi_on_error() {
    let tmp = setup();
    let root = tmp.path();
    fs::write(root.join("program.s"), "int sys0=0\npage sys0\n").unwrap();
    let (first, _) = build(root, "out", false);
    assert!(!first.diagnostics.is_empty());
    let report = Report::default();
    let error = hmi_cli::build_with_jobs(
        root,
        Some(&root.join("out/Panel.HMI")),
        true,
        false,
        2,
        &report,
    )
    .unwrap_err();
    assert_eq!(error.as_diagnostic().unwrap().code, "script_page_target");
    assert_eq!(fs::read(root.join("out/Panel.HMI")).unwrap(), first.bytes);
    let (again, stats) = build(root, "out", false);
    assert_eq!(link_count(&stats, "reused"), 1);
    assert_eq!(again.diagnostics, first.diagnostics);
}
