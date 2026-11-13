use hmi_artifact::asset::{AssetInfo, AssetObject};
use hmi_core::target::{Target, TargetConfig};
use std::fs;
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/zi.rs"
    ));
}

fn target(profile: &str, orientation: u16) -> Target {
    Target::resolve(&TargetConfig {
        profile: profile.into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation,
    })
    .unwrap()
}

#[test]
fn source_zi_is_preserved_and_asset_is_self_contained_for_all_profiles() {
    let tmp = tempfile::tempdir().unwrap();
    let input = fixture::font();
    fs::write(tmp.path().join("font.zi"), &input).unwrap();
    let path = tmp.path().join("font.asset.toml");
    fs::write(
        &path,
        "schema='hmi.asset/1'\nkind='font'\nsource='font.zi'\n",
    )
    .unwrap();
    for profile in hmi_core::target::PROFILES {
        for orientation in [0, 90, 180, 270] {
            let t = target(profile, orientation);
            let probe = hmi_media::probe(&path, &t).unwrap();
            let asset = hmi_media::encode(&path, &t).unwrap().object;
            assert_eq!(probe.info, asset.metadata.info);
            assert_eq!(asset.metadata.members.len(), 1);
            assert_eq!(asset.metadata.members[0].extension, "zi");
            if orientation == 0 {
                assert_eq!(asset.member(0).unwrap(), input);
            }
            assert_eq!(
                hmi_format::font::inspect(asset.member(0).unwrap())
                    .unwrap()
                    .orientation,
                orientation
            );
            let decoded = AssetObject::decode(&asset.encode().unwrap()).unwrap();
            decoded.validate_for(&t).unwrap();
            assert!(
                decoded
                    .validate_for(&target(profile, (orientation + 90) % 360))
                    .is_err()
            );
        }
    }
    assert_eq!(fs::read(tmp.path().join("font.zi")).unwrap(), input);
}

#[test]
fn rejects_image_options_bad_stream_and_conflicting_asset_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("font.zi"), fixture::font()).unwrap();
    let path = tmp.path().join("font.asset.toml");
    let base = "schema='hmi.asset/1'\nkind='font'\nsource='font.zi'\n";
    let t = target("TJC4832T035_011", 0);
    for extra in [
        "encoding={quality=90}",
        "resize={width=1,height=1,mode='stretch'}",
        "orientation=90",
        "delivery={mode='external',path='sd0/font.zi'}",
    ] {
        fs::write(&path, format!("{base}{extra}\n")).unwrap();
        assert!(hmi_media::probe(&path, &t).is_err());
    }
    fs::write(&path, base).unwrap();
    let mut asset = hmi_media::encode(&path, &t).unwrap().object;
    let mut metadata = serde_json::to_value(&asset.metadata).unwrap();
    metadata["unexpected"] = true.into();
    assert!(serde_json::from_value::<hmi_artifact::asset::AssetMetadata>(metadata).is_err());
    let mut metadata = serde_json::to_value(&asset.metadata).unwrap();
    metadata["image"] = serde_json::json!({});
    assert!(serde_json::from_value::<hmi_artifact::asset::AssetMetadata>(metadata).is_err());
    if let AssetInfo::Font { font } = &mut asset.metadata.info {
        font.height += 1;
    }
    assert!(asset.validate().is_err());
    fs::write(tmp.path().join("font.zi"), [4, 255, 0, 10]).unwrap();
    assert!(hmi_media::probe(&path, &t).is_err());
}
