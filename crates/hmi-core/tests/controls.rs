use hmi_core::controls::by_name;
use std::collections::BTreeMap;

#[test]
fn variable_modes_keep_signed_value_and_utf8_text_with_byte_capacity() {
    let definition = by_name("variable").unwrap();
    let mut props = BTreeMap::from([
        ("sta".into(), toml::Value::Integer(1)),
        ("val".into(), toml::Value::Integer(-123456789)),
        ("txt".into(), toml::Value::String("测试A".into())),
        ("txt_maxl".into(), toml::Value::Integer(7)),
    ]);
    let string = definition.attributes("va0", &props).unwrap();
    definition.validate_record("va0", &string).unwrap();
    assert_eq!(
        string.iter().find(|(k, _)| k == "val").unwrap().1,
        (-123456789i32).to_le_bytes()
    );
    props.insert("sta".into(), toml::Value::Integer(0));
    let number = definition.attributes("va0", &props).unwrap();
    assert_eq!(
        string.iter().zip(&number).filter(|(a, b)| a != b).count(),
        1
    );
    props.insert("txt_maxl".into(), toml::Value::Integer(6));
    assert_eq!(
        definition
            .attributes("va0", &props)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "text_capacity"
    );
    props.insert("txt_maxl".into(), toml::Value::Integer(7));
    props.insert("val".into(), toml::Value::Integer(i64::from(i32::MAX) + 1));
    assert!(definition.attributes("va0", &props).is_err());
}

#[test]
fn hotspot_requires_geometry_and_timer_enforces_documented_bounds() {
    let hotspot = by_name("hotspot").unwrap();
    assert!(hotspot.attributes("m0", &BTreeMap::new()).is_err());
    let mut props = BTreeMap::from([
        ("x".into(), toml::Value::Integer(65535)),
        ("y".into(), toml::Value::Integer(0)),
        ("w".into(), toml::Value::Integer(2)),
        ("h".into(), toml::Value::Integer(1)),
    ]);
    assert_eq!(
        hotspot
            .attributes("m0", &props)
            .unwrap_err()
            .as_diagnostic()
            .unwrap()
            .code,
        "geometry"
    );
    props.insert("w".into(), toml::Value::Integer(1));
    hotspot.attributes("m0", &props).unwrap();
    let timer = by_name("timer").unwrap();
    for interval in [50, 65534] {
        timer
            .attributes(
                "tm0",
                &BTreeMap::from([("tim".into(), toml::Value::Integer(interval))]),
            )
            .unwrap();
    }
    for interval in [49, 65535] {
        assert!(
            timer
                .attributes(
                    "tm0",
                    &BTreeMap::from([("tim".into(), toml::Value::Integer(interval))])
                )
                .is_err()
        );
    }
    assert!(
        timer
            .attributes(
                "tm0",
                &BTreeMap::from([("id".into(), toml::Value::Integer(1))])
            )
            .is_err()
    );
    let mut attributes = timer.attributes("tm0", &BTreeMap::new()).unwrap();
    attributes.iter_mut().find(|(k, _)| k == "tim").unwrap().1 = vec![1];
    assert!(timer.validate_record("tm0", &attributes).is_err());
}
