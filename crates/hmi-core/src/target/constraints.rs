//! 属性约束只记录已知例外：系列规则先应用，单独型号按同一属性覆盖。
use crate::{
    Result, controls,
    diagnostic::{Diagnostic, Location},
    error,
    model::check_schema,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const DATA: &str = include_str!("../../../../data/profiles/control-constraints.json");

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntegerConstraint {
    Range { min: i64, max: i64 },
    Values { values: Vec<i64> },
}

/// control 和 property 使用注册表的名称；约束不能超出通用属性的存储范围。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ControlPropertyConstraint {
    pub control: String,
    pub property: String,
    pub constraint: IntegerConstraint,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleSet {
    schema: String,
    #[serde(default)]
    series: BTreeMap<String, Vec<ControlPropertyConstraint>>,
    #[serde(default)]
    profiles: BTreeMap<String, Vec<ControlPropertyConstraint>>,
    #[serde(default)]
    control_support: ControlSupportTable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlSupport {
    control: String,
    supported: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlSupportTable {
    #[serde(default)]
    series: BTreeMap<String, Vec<ControlSupport>>,
    #[serde(default)]
    profiles: BTreeMap<String, Vec<ControlSupport>>,
}

pub(super) struct ResolvedRules {
    pub property_constraints: Vec<ControlPropertyConstraint>,
    pub control_support: BTreeMap<String, bool>,
    pub fingerprint: String,
}

impl ControlPropertyConstraint {
    fn property_definition(&self) -> Result<&'static controls::Property> {
        if self.control == "*" {
            return controls::presentation_property(&self.property)
                .ok_or_else(|| error("target_rules", "通配约束只适用于已登记的公共属性"));
        }
        (if self.control == "page" {
            &controls::PAGE
        } else {
            controls::by_name(&self.control)?
        })
        .all_properties()
        .find(|p| p.name == self.property)
        .ok_or_else(|| {
            error(
                "target_rules",
                format!(
                    "型号约束引用了不存在的属性 {}.{}",
                    self.control, self.property
                ),
            )
        })
    }

    fn validate(&self) -> Result<()> {
        let p = self.property_definition()?;
        if !matches!(
            p.storage,
            controls::Storage::U8
                | controls::Storage::U16
                | controls::Storage::I16
                | controls::Storage::I32
                | controls::Storage::U32
        ) {
            return Err(error(
                "target_rules",
                "当前型号属性约束适用于普通整数，不用于文本或资源引用",
            ));
        }
        let valid = match &self.constraint {
            IntegerConstraint::Range { min, max } => min <= max && *min >= p.min && *max <= p.max,
            IntegerConstraint::Values { values } => {
                !values.is_empty()
                    && values.iter().all(|v| (p.min..=p.max).contains(v))
                    && values.iter().collect::<BTreeSet<_>>().len() == values.len()
            }
        };
        if !valid {
            return Err(error(
                "target_rules",
                format!(
                    "{}.{} 的型号约束为空、重复、范围颠倒或超出通用范围",
                    self.control, self.property
                ),
            ));
        }
        Ok(())
    }

    fn check(&self, profile: &str, attributes: &[(String, Vec<u8>)]) -> Result<()> {
        let property = self.property_definition()?;
        let raw = attributes
            .iter()
            .find(|(key, _)| key == &self.property)
            .ok_or_else(|| {
                error(
                    "object_property",
                    format!("控件记录缺少属性 {}", self.property),
                )
            })?;
        // 使用属性注册宽度和符号解码，不能把有符号 val 当作 u32。
        let value = property
            .decode(&raw.1)?
            .as_integer()
            .ok_or_else(|| error("target_rules", "型号范围约束需要整数属性"))?;
        let (accepted, allowed) = match &self.constraint {
            IntegerConstraint::Range { min, max } => {
                ((*min..=*max).contains(&value), format!("{min}～{max}"))
            }
            IntegerConstraint::Values { values } => (
                values.contains(&value),
                values
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join("、"),
            ),
        };
        if !accepted {
            return Err(Diagnostic::error(
                "control_capability",
                format!(
                    "型号 {profile} 的 {}.{} 允许值为 {allowed}，当前值为 {value}",
                    self.control, self.property
                ),
            )
            .at(Location {
                property: Some(self.property.clone()),
                ..Default::default()
            })
            .into());
        }
        Ok(())
    }
}

impl RuleSet {
    fn validate(&self) -> Result<()> {
        check_schema(&self.schema, "hmi.control-constraints/1")?;
        for key in self.series.keys().chain(self.control_support.series.keys()) {
            if !["t0", "k0", "x5"].contains(&key.as_str()) {
                return Err(error(
                    "target_rules",
                    format!("型号约束的系列名称 {key} 未注册"),
                ));
            }
        }
        for key in self
            .profiles
            .keys()
            .chain(self.control_support.profiles.keys())
        {
            if !super::PROFILES.contains(&key.as_str()) {
                return Err(error(
                    "target_rules",
                    format!("型号约束引用的型号 {key} 未注册"),
                ));
            }
        }
        for entries in self.series.values().chain(self.profiles.values()) {
            let mut seen = BTreeSet::new();
            for entry in entries {
                entry.validate()?;
                if !seen.insert((&entry.control, &entry.property)) {
                    return Err(error(
                        "target_rules",
                        "同一系列或型号重复声明了控件属性约束",
                    ));
                }
            }
        }
        for entries in self
            .control_support
            .series
            .values()
            .chain(self.control_support.profiles.values())
        {
            let mut seen = BTreeSet::new();
            for entry in entries {
                controls::by_name(&entry.control)?;
                if !seen.insert(&entry.control) {
                    return Err(error(
                        "target_rules",
                        "同一系列或型号重复声明了控件支持状态",
                    ));
                }
            }
        }
        Ok(())
    }

    fn resolve(&self, series: &str, profile: &str) -> Vec<ControlPropertyConstraint> {
        let mut merged = BTreeMap::new();
        // 不存在条目就保持通用范围；型号条目覆盖系列中的同一控件／属性。
        for entries in [self.series.get(series), self.profiles.get(profile)]
            .into_iter()
            .flatten()
        {
            // 同一层先展开公共默认，再让具体控件覆盖；与JSON排列无关。
            for entry in entries.iter().filter(|e| e.control == "*") {
                for definition in controls::DEFINITIONS
                    .iter()
                    .chain(std::iter::once(&controls::PAGE))
                {
                    if definition
                        .presentation
                        .properties()
                        .iter()
                        .any(|p| p.name == entry.property)
                    {
                        let mut concrete = entry.clone();
                        concrete.control = definition.name.into();
                        merged.insert(
                            (concrete.control.clone(), concrete.property.clone()),
                            concrete,
                        );
                    }
                }
            }
            for entry in entries.iter().filter(|e| e.control != "*") {
                merged.insert(
                    (entry.control.clone(), entry.property.clone()),
                    entry.clone(),
                );
            }
        }
        merged.into_values().collect()
    }

    fn resolve_control_support(&self, series: &str, profile: &str) -> BTreeMap<String, bool> {
        let mut merged = BTreeMap::new();
        for entry in self
            .control_support
            .series
            .get(series)
            .into_iter()
            .flatten()
            .chain(
                self.control_support
                    .profiles
                    .get(profile)
                    .into_iter()
                    .flatten(),
            )
        {
            merged.insert(entry.control.clone(), entry.supported);
        }
        merged
    }
}

pub(super) fn resolve(series: &str, profile: &str) -> Result<ResolvedRules> {
    let data: RuleSet = serde_json::from_str(DATA)
        .map_err(|e| error("target_rules", format!("内置型号约束数据无效：{e}")))?;
    data.validate()?;
    // 对解析后的数据计算摘要，避免 CRLF／LF 或排版差异改变目标身份。
    let fingerprint = crate::sha256(&crate::file::json(&data)?);
    Ok(ResolvedRules {
        property_constraints: data.resolve(series, profile),
        control_support: data.resolve_control_support(series, profile),
        fingerprint,
    })
}

pub(super) fn check(
    profile: &str,
    rules: &[ControlPropertyConstraint],
    code: u8,
    attributes: &[(String, Vec<u8>)],
) -> Result<()> {
    let definition = controls::record_definition(code)?;
    for rule in rules.iter().filter(|r| r.control == definition.name) {
        rule.check(profile, attributes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_wildcards_expand_and_specific_profile_rules_win_independent_of_order() {
        let mut data: RuleSet = serde_json::from_str(DATA).unwrap();
        let rule = |control: &str, max| ControlPropertyConstraint {
            control: control.into(),
            property: "aph".into(),
            constraint: IntegerConstraint::Range { min: 0, max },
        };
        data.series.get_mut("k0").unwrap().push(rule("gauge", 100));
        data.profiles.insert(
            "TJC4832K035_011".into(),
            vec![rule("gauge", 10), rule("*", 20)],
        );
        data.validate().unwrap();
        let merged = data.resolve("k0", "TJC4832K035_011");
        assert!(!merged.iter().any(|r| r.control == "*"));
        assert_eq!(
            merged
                .iter()
                .find(|r| r.control == "gauge" && r.property == "aph"),
            Some(&rule("gauge", 10))
        );
        assert_eq!(
            merged
                .iter()
                .find(|r| r.control == "text" && r.property == "aph"),
            Some(&rule("text", 20))
        );
        assert!(!merged.iter().any(|r| r.control == "hotspot"));
        assert!(
            merged
                .iter()
                .filter(|r| r.control == "page")
                .all(|r| r.property == "effect")
        );
        data.series
            .get_mut("k0")
            .unwrap()
            .push(ControlPropertyConstraint {
                control: "*".into(),
                property: "val".into(),
                constraint: IntegerConstraint::Values { values: vec![0] },
            });
        assert!(data.validate().is_err());
    }

    #[test]
    fn defaults_are_unrestricted_and_profile_rules_override_series_by_property() {
        let defaults = resolve("t0", "TJC4832T035_011").unwrap();
        assert!(
            defaults
                .property_constraints
                .iter()
                .all(|r| r.control != "slider" || r.property != "psta")
        );
        let mut data: RuleSet = serde_json::from_str(DATA).unwrap();
        let replacement = ControlPropertyConstraint {
            control: "slider".into(),
            property: "psta".into(),
            constraint: IntegerConstraint::Values { values: vec![0, 2] },
        };
        let extra = ControlPropertyConstraint {
            control: "slider".into(),
            property: "val".into(),
            constraint: IntegerConstraint::Range { min: 10, max: 100 },
        };
        data.profiles.insert(
            "TJC4832K035_011".into(),
            vec![extra.clone(), replacement.clone()],
        );
        data.validate().unwrap();
        let merged = data.resolve("k0", "TJC4832K035_011");
        assert_eq!(
            merged
                .into_iter()
                .filter(|r| r.control == "slider"
                    && !matches!(r.property.as_str(), "drag" | "aph" | "effect"))
                .collect::<Vec<_>>(),
            vec![replacement, extra]
        );
        assert!(
            data.resolve("t0", "TJC3224T028_011")
                .iter()
                .all(|r| r.control != "slider" || r.property != "psta")
        );
        assert_eq!(
            data.resolve("k0", "other")
                .iter()
                .filter(|r| r.control == "slider"
                    && !matches!(r.property.as_str(), "drag" | "aph" | "effect"))
                .count(),
            1
        );
    }

    #[test]
    fn control_support_defaults_and_profile_overrides_are_data_driven() {
        let mut data: RuleSet = serde_json::from_str(DATA).unwrap();
        assert_eq!(
            data.resolve_control_support("k0", "TJC4832K035_011")
                .get("switch"),
            Some(&false)
        );
        assert!(
            data.resolve_control_support("x5", "TJC4827X543_011")
                .is_empty()
        );
        data.control_support.profiles.insert(
            "TJC4832K035_011".into(),
            vec![ControlSupport {
                control: "switch".into(),
                supported: true,
            }],
        );
        data.validate().unwrap();
        let merged = data.resolve_control_support("k0", "TJC4832K035_011");
        assert_eq!(merged.get("switch"), Some(&true));
        assert_eq!(merged.get("combo_box"), Some(&false));
        data.control_support
            .profiles
            .get_mut("TJC4832K035_011")
            .unwrap()
            .push(ControlSupport {
                control: "switch".into(),
                supported: false,
            });
        assert!(data.validate().is_err());
    }

    #[test]
    fn generic_rules_handle_signed_ranges_and_enum_sets_on_other_controls() {
        let signed16 = ControlPropertyConstraint {
            control: "gauge".into(),
            property: "up".into(),
            constraint: IntegerConstraint::Range { min: -10, max: 10 },
        };
        signed16.validate().unwrap();
        check(
            "test",
            &[signed16],
            122,
            &[("up".into(), (-2i16).to_le_bytes().to_vec())],
        )
        .unwrap();
        let range = ControlPropertyConstraint {
            control: "number".into(),
            property: "val".into(),
            constraint: IntegerConstraint::Range {
                min: -100,
                max: 100,
            },
        };
        let values = ControlPropertyConstraint {
            control: "checkbox".into(),
            property: "style".into(),
            constraint: IntegerConstraint::Values {
                values: vec![0, 2, 4],
            },
        };
        range.validate().unwrap();
        values.validate().unwrap();
        let rules = [range, values];
        for value in [-100i32, -12, 100] {
            check(
                "test",
                &rules,
                54,
                &[("val".into(), value.to_le_bytes().to_vec())],
            )
            .unwrap();
        }
        assert!(
            check(
                "test",
                &rules,
                54,
                &[("val".into(), (-101i32).to_le_bytes().to_vec())]
            )
            .is_err()
        );
        assert!(check("test", &rules, 54, &[("val".into(), vec![0])]).is_err());
        for value in [0, 2, 4] {
            check("test", &rules, 56, &[("style".into(), vec![value])]).unwrap();
        }
        let e = check("test", &rules, 56, &[("style".into(), vec![1])]).unwrap_err();
        assert_eq!(
            e.as_diagnostic().unwrap().location.property.as_deref(),
            Some("style")
        );
        // 不同控件的同名属性不受另一条规则影响。
        check("test", &rules, 98, &[]).unwrap();
    }

    #[test]
    fn invalid_configuration_is_rejected_before_constraints_are_applied() {
        for constraint in [
            IntegerConstraint::Range { min: 2, max: 1 },
            IntegerConstraint::Range { min: -1, max: 2 },
            IntegerConstraint::Values { values: vec![] },
            IntegerConstraint::Values { values: vec![0, 0] },
            IntegerConstraint::Values { values: vec![3] },
        ] {
            assert!(
                ControlPropertyConstraint {
                    control: "slider".into(),
                    property: "psta".into(),
                    constraint
                }
                .validate()
                .is_err()
            );
        }
        for property in ["font", "txt", "missing"] {
            assert!(
                ControlPropertyConstraint {
                    control: "text".into(),
                    property: property.into(),
                    constraint: IntegerConstraint::Range { min: 0, max: 1 }
                }
                .validate()
                .is_err()
            );
        }
        let mut data: RuleSet = serde_json::from_str(DATA).unwrap();
        let duplicate = data.series["k0"][0].clone();
        data.series.get_mut("k0").unwrap().push(duplicate);
        assert!(data.validate().is_err());
        let mut unknown: RuleSet = serde_json::from_str(DATA).unwrap();
        unknown.profiles.insert("UNKNOWN".into(), vec![]);
        assert!(unknown.validate().is_err());
        assert!(
            serde_json::from_str::<RuleSet>(&DATA.replace("\"series\"", "\"serise\"")).is_err()
        );
    }
}
