//! 媒体输入、处理参数和资源符号；不包含媒体后端或容器读写。

use crate::{Result, error};
use serde::{Deserialize, Serialize};

pub fn check_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.starts_with("__hmi_")
        || !key.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_alphabetic() || b == b'_' || (i > 0 && (b.is_ascii_digit() || b == b'-'))
        })
    {
        return Err(error(
            "asset_key",
            format!(
                "资产符号 {key:?} 无效，请使用字母或下划线开头的字母、数字、下划线或连字符，且不要使用 __hmi_ 前缀"
            ),
        ));
    }
    Ok(())
}

/// 三种表示互斥；名称和数字都按最终资产表解释，不追踪历史。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceReference {
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub none: Option<bool>,
}
impl ResourceReference {
    pub fn validate(&self) -> Result<()> {
        if usize::from(self.name.is_some())
            + usize::from(self.id.is_some())
            + usize::from(self.none.is_some())
            != 1
            || self.none == Some(false)
        {
            return Err(error(
                "resource_reference",
                "资源引用须且只能使用 ref、id 或 none=true 中的一种",
            ));
        }
        if let Some(name) = &self.name {
            check_key(name)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Alpha {
    #[default]
    Preserve,
    Flatten,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Encoding {
    #[serde(default = "auto")]
    pub codec: String,
    pub quality: Option<u8>,
    #[serde(default)]
    pub alpha: Alpha,
    pub background: Option<String>,
}
impl Default for Encoding {
    fn default() -> Self {
        Self {
            codec: auto(),
            quality: None,
            alpha: Alpha::Preserve,
            background: None,
        }
    }
}
fn auto() -> String {
    "auto".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResizeMode {
    Stretch,
    Contain,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Resize {
    pub width: u32,
    pub height: u32,
    pub mode: ResizeMode,
    pub background: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub mode: String,
    pub path: Option<String>,
}
impl Default for Delivery {
    fn default() -> Self {
        Self {
            mode: "embedded".into(),
            path: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageSource {
    pub schema: String,
    pub kind: String,
    #[serde(default = "source_mode")]
    pub input: String,
    pub source: String,
    pub original: Option<String>,
    #[serde(default)]
    pub encoding: Encoding,
    pub resize: Option<Resize>,
    #[serde(default)]
    pub delivery: Delivery,
}

/// ZI 输入不接受图片参数；方向由目标推导，不向制字工具添加选项。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontSource {
    pub schema: String,
    pub kind: String,
    #[serde(default = "source_mode")]
    pub input: String,
    pub source: String,
    #[serde(default)]
    pub delivery: Delivery,
}
fn source_mode() -> String {
    "source".into()
}

impl Default for ImageSource {
    fn default() -> Self {
        Self {
            schema: "hmi.asset/1".into(),
            kind: "image".into(),
            input: "source".into(),
            source: String::new(),
            original: None,
            encoding: Encoding {
                codec: auto(),
                ..Encoding::default()
            },
            resize: None,
            delivery: Delivery::default(),
        }
    }
}
