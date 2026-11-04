//! 分类警告开关；先收集原始诊断，再应用当前构建策略。
use crate::{
    Error, Result,
    diagnostic::{Diagnostic, Severity},
};
use std::collections::BTreeMap;

pub const CATEGORIES: &[(&str, &str)] = &[
    ("script-dynamic", "名称组使用动态 ID，无法静态确认目标"),
    ("script-page", "脚本引用不存在的页面"),
    ("script-object", "脚本引用不存在的控件"),
    ("script-property", "脚本引用控件不具备的属性"),
    ("script-scope", "跨页访问私有控件"),
    ("script-context", "全局代码依赖当前页面上下文"),
    ("script-page-target", "页面跳转目标无法静态确认"),
    ("property-resource", "属性引用无效的图片或字库 ID"),
    ("encoding", "尚未验证的工程编码"),
    ("resource-layout", "尚未支持的资源布局"),
];

#[derive(Clone, Debug, Default)]
struct CategoryPolicy {
    enabled: Option<bool>,
    error: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct WarningPolicy {
    enabled: bool,
    error: bool,
    suppressed: bool,
    categories: BTreeMap<String, CategoryPolicy>,
}

impl Default for WarningPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            error: false,
            suppressed: false,
            categories: BTreeMap::new(),
        }
    }
}

impl WarningPolicy {
    pub fn parse(options: &[String], suppressed: bool) -> Result<Self> {
        let mut policy = Self {
            suppressed,
            ..Self::default()
        };
        for option in options {
            match option.as_str() {
                "all" => policy.enabled = true,
                "no-all" => policy.enabled = false,
                "error" => policy.error = true,
                "no-error" => policy.error = false,
                _ => {
                    let (name, enabled, error) = if let Some(name) = option.strip_prefix("error=") {
                        (name, Some(true), Some(true))
                    } else if let Some(name) = option.strip_prefix("no-error=") {
                        (name, None, Some(false))
                    } else if let Some(name) = option.strip_prefix("no-") {
                        (name, Some(false), None)
                    } else {
                        (option.as_str(), Some(true), None)
                    };
                    if !CATEGORIES.iter().any(|(category, _)| *category == name) {
                        return Err(Error::Message(format!(
                            "未知警告类别：{name}（-W{option}），请使用 --help 查看支持的类别"
                        )));
                    }
                    let category = policy.categories.entry(name.into()).or_default();
                    if let Some(enabled) = enabled {
                        category.enabled = Some(enabled);
                    }
                    if let Some(error) = error {
                        category.error = Some(error);
                    }
                }
            }
        }
        Ok(policy)
    }

    pub fn apply(&self, raw: &Diagnostic) -> Option<Diagnostic> {
        if raw.severity == Severity::Error && raw.original_severity != Some(Severity::Warning) {
            return Some(raw.clone());
        }
        if self.suppressed {
            return None;
        }
        let name = raw
            .category
            .clone()
            .unwrap_or_else(|| raw.code.replace('_', "-"));
        let category = self.categories.get(&name);
        if !category.and_then(|c| c.enabled).unwrap_or(self.enabled) {
            return None;
        }
        let mut diagnostic = raw.clone();
        diagnostic.category = Some(name);
        if category.and_then(|c| c.error).unwrap_or(self.error) {
            diagnostic.severity = Severity::Error;
            diagnostic.original_severity = Some(Severity::Warning);
        } else {
            diagnostic.severity = Severity::Warning;
            diagnostic.original_severity = None;
        }
        Some(diagnostic)
    }

    pub fn filter(&self, raw: &[Diagnostic]) -> Vec<Diagnostic> {
        raw.iter().filter_map(|d| self.apply(d)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy(args: &[&str]) -> WarningPolicy {
        WarningPolicy::parse(&args.iter().map(|s| (*s).into()).collect::<Vec<_>>(), false).unwrap()
    }
    #[test]
    fn category_specificity_and_last_option_win() {
        let raw = Diagnostic::warning("script_dynamic", "动态 ID");
        for args in [
            vec!["no-all", "script-dynamic"],
            vec!["script-dynamic", "no-all"],
        ] {
            assert_eq!(
                policy(&args).apply(&raw).unwrap().severity,
                Severity::Warning
            );
        }
        assert!(
            policy(&["script-dynamic", "no-script-dynamic"])
                .apply(&raw)
                .is_none()
        );
        assert!(
            policy(&["error", "no-error=script-dynamic"])
                .apply(&raw)
                .is_some_and(|d| d.severity == Severity::Warning)
        );
        assert!(
            policy(&["no-script-dynamic", "no-error=script-dynamic"])
                .apply(&raw)
                .is_none()
        );
        assert_eq!(
            policy(&["no-all", "error=script-dynamic"])
                .apply(&raw)
                .unwrap()
                .severity,
            Severity::Error
        );
        assert_eq!(
            policy(&["error=script-dynamic", "no-error"])
                .apply(&raw)
                .unwrap()
                .severity,
            Severity::Error
        );
        assert_eq!(
            policy(&["error", "no-error"]).apply(&raw).unwrap().severity,
            Severity::Warning
        );
    }
    #[test]
    fn suppression_never_hides_real_errors() {
        let policy = WarningPolicy::parse(&["error".into()], true).unwrap();
        assert!(
            policy
                .apply(&Diagnostic::warning("script_scope", "作用域"))
                .is_none()
        );
        assert!(
            policy
                .apply(&Diagnostic::error("page_crc", "CRC 错误"))
                .is_some()
        );
        assert!(WarningPolicy::parse(&["no-typo".into()], false).is_err());
    }
    #[test]
    fn all_registered_categories_are_controllable() {
        for (name, _) in CATEGORIES {
            let raw = Diagnostic::warning(&name.replace('-', "_"), "test");
            assert!(policy(&[&format!("no-{name}")]).apply(&raw).is_none());
            let promoted = policy(&[&format!("error={name}")]).apply(&raw).unwrap();
            assert_eq!(promoted.original_severity, Some(Severity::Warning));
            assert_eq!(
                WarningPolicy::default().apply(&promoted).unwrap().severity,
                Severity::Warning
            );
        }
    }
}
