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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub mode: String,
    pub path: Option<String>,
}
impl Delivery {
    /// 部署文件的后缀描述实际运行格式；输入文件的后缀不参与此判断。
    pub fn validate_for(&self, kind: &str) -> Result<()> {
        self.validate()?;
        if self.is_embedded() {
            return Ok(());
        }
        let extension = match kind {
            "image" => ".xi",
            "animation" => ".gmov",
            "video" => ".video",
            "audio" => ".wav",
            _ => return Err(error("asset_delivery", "此媒体类别不支持外部输出")),
        };
        if !self
            .path
            .as_ref()
            .unwrap()
            .to_ascii_lowercase()
            .ends_with(extension)
        {
            return Err(error(
                "asset_delivery",
                format!("{kind} 外部输出路径必须以 {extension} 结尾"),
            ));
        }
        Ok(())
    }
    pub fn is_embedded(&self) -> bool {
        self.mode == "embedded" && self.path.is_none()
    }
    pub fn validate(&self) -> Result<()> {
        match (self.mode.as_str(), self.path.as_deref()) {
            ("embedded", None) => Ok(()),
            ("external", Some(path)) => crate::deployment::validate(path),
            _ => Err(error(
                "asset_delivery",
                "delivery.mode 须为 embedded（不带 path）或 external（带 path）",
            )),
        }
    }
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
    pub original: Option<String>,
    #[serde(default)]
    pub delivery: Delivery,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationEncoding {
    #[serde(default = "animation_quality")]
    pub quality: u8,
    #[serde(default)]
    pub alpha: Alpha,
    pub background: Option<String>,
    #[serde(default = "animation_timing")]
    pub timing: String,
}
fn animation_quality() -> u8 {
    80
}
fn animation_timing() -> String {
    "native-compatible".into()
}
impl Default for AnimationEncoding {
    fn default() -> Self {
        Self {
            quality: 80,
            alpha: Alpha::Preserve,
            background: None,
            timing: animation_timing(),
        }
    }
}

/// GIF 是权威输入；loop/en 属于控件，不属于共享资产。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationSource {
    pub schema: String,
    pub kind: String,
    #[serde(default = "source_mode")]
    pub input: String,
    pub source: String,
    pub original: Option<String>,
    #[serde(default)]
    pub encoding: AnimationEncoding,
    #[serde(default)]
    pub delivery: Delivery,
}
fn source_mode() -> String {
    "source".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoEncoding {
    #[serde(default = "animation_quality")]
    pub quality: u8,
    pub fps: u8,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    #[serde(default)]
    pub start_ms: u32,
    pub duration_ms: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoAudio {
    #[serde(default = "audio_mode")]
    pub mode: String,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
}
fn audio_mode() -> String {
    "if-present".into()
}
impl Default for VideoAudio {
    fn default() -> Self {
        Self {
            mode: audio_mode(),
            sample_rate: None,
            channels: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoSource {
    pub schema: String,
    pub kind: String,
    #[serde(default = "source_mode")]
    pub input: String,
    pub source: String,
    pub original: Option<String>,
    pub encoding: VideoEncoding,
    #[serde(default)]
    pub clip: Clip,
    #[serde(default)]
    pub audio: VideoAudio,
    #[serde(default)]
    pub delivery: Delivery,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FfmpegInfo {
    pub version: String,
    pub libraries: std::collections::BTreeMap<String, u32>,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioEncoding {
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    #[serde(default = "sample_format")]
    pub sample_format: String,
}
fn sample_format() -> String {
    "s16".into()
}
impl Default for AudioEncoding {
    fn default() -> Self {
        Self {
            sample_rate: None,
            channels: None,
            sample_format: sample_format(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioSource {
    pub schema: String,
    pub kind: String,
    #[serde(default = "source_mode")]
    pub input: String,
    pub source: String,
    pub original: Option<String>,
    #[serde(default)]
    pub encoding: AudioEncoding,
    #[serde(default)]
    pub clip: Clip,
    #[serde(default)]
    pub delivery: Delivery,
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
