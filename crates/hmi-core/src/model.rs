//! 工程源码与链接描述。数组顺序决定 ID，名称只按当前工程解析。

use crate::{Result, error, target::TargetConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 页面和控件共用的可编辑属性表。
pub type Properties = BTreeMap<String, toml::Value>;

/// 单个控件的可编辑内容；不保存重命名历史或另一个对象身份。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ObjectFile {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default)]
    pub properties: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub events: BTreeMap<String, String>,
}

/// 编译器和静态分析器共用的页面内容。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PageFile {
    pub name: String,
    pub properties: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub events: BTreeMap<String, String>,
    #[serde(default)]
    pub objects: Vec<ObjectFile>,
}

/// 页面源码。重复声明字段是为了严格拒绝未知字段，不依赖 flatten 的宽松解析。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSource {
    pub schema: String,
    pub name: String,
    #[serde(default)]
    pub properties: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub events: BTreeMap<String, String>,
    #[serde(default)]
    pub objects: Vec<ObjectFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_record: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramSource {
    pub source: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildConfig {
    pub output: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSources {
    pub sources: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetSource {
    pub key: String,
    pub source: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetSources {
    #[serde(default)]
    pub items: Vec<AssetSource>,
}

/// 从零创建所需的完整清单；原始数据附件在后续导入阶段实现。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSource {
    pub schema: String,
    pub name: String,
    pub target: TargetConfig,
    pub program: ProgramSource,
    #[serde(default)]
    pub build: BuildConfig,
    pub pages: PageSources,
    #[serde(default)]
    pub assets: AssetSources,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageObjects {
    pub objects: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetObject {
    pub key: String,
    pub object: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetObjects {
    #[serde(default)]
    pub items: Vec<AssetObject>,
}

/// 链接器只接受编译产物，不能回到工程目录读取源码补全内容。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkSource {
    pub schema: String,
    pub name: String,
    pub target: String,
    pub program: String,
    pub pages: PageObjects,
    #[serde(default)]
    pub assets: AssetObjects,
}

/// 格式版本不匹配时明确拒绝，避免误用旧版工作区。
pub fn check_schema(actual: &str, expected: &str) -> Result<()> {
    if actual != expected {
        return Err(error(
            "schema_version",
            format!("不支持的格式版本 {actual}，当前要求 {expected}"),
        ));
    }
    Ok(())
}

/// 新建工程采用已接入的 ASCII 名称规则，避免接受无法写入页面头的名称。
pub fn check_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 16
        || !name
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
    {
        return Err(error(
            "name",
            format!(
                "名称 {name:?} 无效，须为 1～16 字节，以字母或下划线开头，只能包含字母、数字和下划线"
            ),
        ));
    }
    Ok(())
}
