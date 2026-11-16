//! 已接入型号的基础配置和当前控件能力，不根据型号名猜测系列能力。

use crate::{Result, error, model::check_schema, sha256};
use serde::{Deserialize, Serialize};

mod constraints;
pub use constraints::{ControlPropertyConstraint, IntegerConstraint};

pub const DEFAULT_PROGRAM: &str = include_str!("../../../data/profiles/default-program.s");

/// 用户写入 hmi.toml 的目标配置。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TargetConfig {
    pub profile: String,
    pub format: String,
    pub encoding: String,
    pub orientation: u16,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Canvas {
    pub width: u16,
    pub height: u16,
}

/// 明确记录当前实现能力，不能把空白工程通过解释为全部控件已支持。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub ordinary_controls: bool,
    pub media_assets: bool,
    #[serde(default)]
    pub control_types: Vec<String>,
    #[serde(default)]
    pub image_codecs: Vec<String>,
    #[serde(default)]
    pub font_versions: Vec<u8>,
    /// 只列出已知型号例外，缺省使用控件注册表的通用范围。
    #[serde(default)]
    pub control_property_constraints: Vec<ControlPropertyConstraint>,
}

/// 已展开的共同目标输入。消费者重新核对内置规则，拒绝被修改的几何或版本。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub schema: String,
    pub profile: String,
    pub format: String,
    pub encoding: String,
    pub orientation: u16,
    pub canvas: Canvas,
    pub capabilities: Capabilities,
    pub data_revision: u32,
    pub profile_data_sha256: String,
}

pub const PROFILES: &[&str] = &[
    "TJC4832K035_011",
    "TJC4832T035_011",
    "TJC3224T028_011",
    "TJC4827X543_011",
];

impl Target {
    /// 从型号的 0° 基础尺寸展开四个方向的新建配置。
    ///
    /// 这里处理新建页面。已有工程可能在切换方向后保留旧的序列化几何，
    /// 导入器不能用本规则直接覆盖原始页面记录。
    pub fn resolve(config: &TargetConfig) -> Result<Self> {
        if config.format != "ver21234" || config.encoding != "utf-8" {
            return Err(error("target_format", "当前仅支持 ver21234、UTF-8 工程"));
        }
        if !matches!(config.orientation, 0 | 90 | 180 | 270) {
            return Err(error(
                "target_orientation",
                "工程方向必须为 0、90、180 或 270",
            ));
        }
        let (base_width, base_height, series) = match config.profile.as_str() {
            "TJC4832K035_011" => (320, 480, "k0"),
            "TJC4832T035_011" => (320, 480, "t0"),
            "TJC3224T028_011" => (240, 320, "t0"),
            "TJC4827X543_011" => (480, 272, "x5"),
            _ => {
                return Err(error(
                    "target_profile",
                    format!("当前阶段尚未提供型号 {} 的新建配置", config.profile),
                ));
            }
        };
        let (width, height) = if matches!(config.orientation, 90 | 270) {
            (base_height, base_width)
        } else {
            (base_width, base_height)
        };
        let rules = constraints::resolve(series, &config.profile)?;
        let signature = format!(
            "page-ver21234-23:{}:{}:{width}:{height}:{}:{}:{}",
            config.profile,
            config.orientation,
            sha256(DEFAULT_PROGRAM.as_bytes()),
            crate::controls::fingerprint(),
            rules.fingerprint
        );
        Ok(Self {
            schema: "hmi.target/1".into(),
            profile: config.profile.clone(),
            format: config.format.clone(),
            encoding: config.encoding.clone(),
            orientation: config.orientation,
            canvas: Canvas { width, height },
            capabilities: Capabilities {
                ordinary_controls: true,
                media_assets: true,
                control_types: crate::controls::DEFINITIONS
                    .iter()
                    .filter(|d| rules.control_support.get(d.name).copied().unwrap_or(true))
                    .map(|d| d.name.into())
                    .collect(),
                image_codecs: if config.profile == "TJC4827X543_011" {
                    vec!["jpeg".into()]
                } else {
                    vec!["rgb565".into()]
                },
                font_versions: vec![6],
                control_property_constraints: rules.property_constraints,
            },
            data_revision: 23,
            profile_data_sha256: sha256(signature.as_bytes()),
        })
    }

    pub fn validate(&self) -> Result<()> {
        check_schema(&self.schema, "hmi.target/1")?;
        let expected = Self::resolve(&TargetConfig {
            profile: self.profile.clone(),
            format: self.format.clone(),
            encoding: self.encoding.clone(),
            orientation: self.orientation,
        })?;
        if self != &expected {
            return Err(error(
                "target_data",
                "目标配置与当前版本的数据不一致，请重新导出 target.json 并重新编译相关目标文件",
            ));
        }
        Ok(())
    }

    /// 编译与链接共用型号属性约束；调用者已校验目标和控件字段布局。
    pub fn check_control_properties(
        &self,
        code: u8,
        attributes: &[(String, Vec<u8>)],
    ) -> Result<()> {
        constraints::check(
            &self.profile,
            &self.capabilities.control_property_constraints,
            code,
            attributes,
        )
    }

    /// 此字节仅是已观察到的工程头标签，不作为通用系列能力位。
    pub fn header_tag(&self) -> Result<u8> {
        self.validate()?;
        Ok(match self.profile.as_str() {
            "TJC4832K035_011" => 1,
            "TJC4827X543_011" => 3,
            _ => 0,
        })
    }
}
