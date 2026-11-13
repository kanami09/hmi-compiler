use hmi_core::{
    model::ProjectSource,
    target::{Target, TargetConfig},
};

#[test]
fn compact_tables_preserve_page_and_asset_order_and_reject_typos() {
    let source = r#"
schema = "hmi.project/1"
name = "Panel"
[target]
profile = "TJC4827X543_011"
format = "ver21234"
encoding = "utf-8"
orientation = 0
[program]
source = "program.s"
[pages]
sources = ["pages/settings.toml", "pages/home.toml"]
[assets]
items = [{key="b",source="b.asset.toml"},{key="a",source="a.asset.toml"}]
"#;
    let parsed: ProjectSource = toml::from_str(source).unwrap();
    assert_eq!(
        parsed.pages.sources,
        ["pages/settings.toml", "pages/home.toml"]
    );
    assert_eq!(
        parsed
            .assets
            .items
            .iter()
            .map(|a| a.key.as_str())
            .collect::<Vec<_>>(),
        ["b", "a"]
    );
    assert!(toml::from_str::<ProjectSource>(&source.replace("sources =", "soruces =")).is_err());
}

#[test]
fn target_rejects_unknown_profiles_and_tampered_geometry() {
    assert_eq!(
        hmi_core::sha256(hmi_core::target::DEFAULT_PROGRAM.as_bytes()),
        "02923d87c331c2e3686cac8a2070e647393bafde7225b4b76a64a451f76ed246"
    );
    let config = TargetConfig {
        profile: "TJC4827X543_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation: 0,
    };
    let mut target = Target::resolve(&config).unwrap();
    target.capabilities.control_property_constraints.push(
        hmi_core::target::ControlPropertyConstraint {
            control: "slider".into(),
            property: "psta".into(),
            constraint: hmi_core::target::IntegerConstraint::Range { min: 0, max: 0 },
        },
    );
    assert!(target.validate().is_err());
    let mut target = Target::resolve(&config).unwrap();
    target.canvas.width = 1;
    assert!(target.validate().is_err());
    for orientation in [1, 45, 360, u16::MAX] {
        let mut invalid = config.clone();
        invalid.orientation = orientation;
        assert_eq!(
            Target::resolve(&invalid)
                .unwrap_err()
                .as_diagnostic()
                .unwrap()
                .code,
            "target_orientation"
        );
    }
    let mut unknown = config;
    unknown.profile = "TJC0000UNKNOWN".into();
    assert!(Target::resolve(&unknown).is_err());
}
