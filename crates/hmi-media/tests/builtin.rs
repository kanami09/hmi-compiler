use hmi_core::{
    builtin::BuiltinImage,
    target::{Target, TargetConfig},
};
use std::{fs, process::Command};
#[test]
fn builtins_have_controlled_sources_and_share_the_picture_encoder_in_all_directions() {
    for orientation in [0, 90, 180, 270] {
        let t = Target::resolve(&TargetConfig {
            profile: "TJC4827X543_011".into(),
            format: "ver21234".into(),
            encoding: "utf-8".into(),
            orientation,
        })
        .unwrap();
        for id in BuiltinImage::ALL {
            let probe = hmi_media::builtin::probe(id, &t).unwrap();
            let encoded = hmi_media::builtin::encode(id, &t).unwrap();
            assert_eq!(probe.info, encoded.object.metadata.info);
            assert!(probe.inputs.is_empty());
            assert!(encoded.inputs.is_empty());
            let info = encoded.object.metadata.info.image().unwrap();
            assert_eq!(info.source_sha256, id.source_sha256());
            assert_eq!((info.width, info.height), (64, 64));
            assert!(info.jpeg.as_ref().unwrap().has_alpha);
            encoded.object.validate_for(&t).unwrap();
        }
    }
}
#[test]
fn builtin_command_works_independently_and_cannot_replace_its_target_input() {
    let tmp = tempfile::tempdir().unwrap();
    let t = Target::resolve(&TargetConfig {
        profile: "TJC4827X543_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation: 0,
    })
    .unwrap();
    let path = tmp.path().join("target.json");
    let bytes = serde_json::to_vec(&t).unwrap();
    fs::write(&path, &bytes).unwrap();
    let run = |output: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_hmi-media"))
            .args(["builtin", "browser-folder", "--target"])
            .arg(&path)
            .arg("-o")
            .arg(output)
            .arg("--json")
            .output()
            .unwrap()
    };
    let output = tmp.path().join("folder.hma");
    let result = run(&output);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let encoded = hmi_artifact::asset::AssetObject::read(&output).unwrap();
    assert_eq!(
        encoded.payload,
        hmi_media::builtin::encode(BuiltinImage::BrowserFolder, &t)
            .unwrap()
            .object
            .payload
    );
    assert!(!run(&path).status.success());
    assert_eq!(fs::read(path).unwrap(), bytes);
}
