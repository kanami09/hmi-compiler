//! 四组已验证的空白工程配置，不根据型号名猜测系列能力。

use crate::{Result, error, model::check_schema, sha256};
use serde::{Deserialize, Serialize};

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
    /// 只展开已经取得原生空白工程证据的配置与方向组合。
    pub fn resolve(config: &TargetConfig) -> Result<Self> {
        if config.format != "ver21234" || config.encoding != "utf-8" {
            return Err(error("target_format", "当前仅支持 ver21234、UTF-8 工程"));
        }
        let (width, height) = match (config.profile.as_str(), config.orientation) {
            ("TJC4832K035_011" | "TJC4832T035_011", 0) => (320, 480),
            ("TJC4832K035_011", 270) => (480, 320),
            ("TJC3224T028_011", 0) => (240, 320),
            ("TJC4827X543_011", 0) => (480, 272),
            _ => {
                return Err(error(
                    "target_profile",
                    format!(
                        "当前阶段不支持型号 {} 的 {}° 新建配置",
                        config.profile, config.orientation
                    ),
                ));
            }
        };
        let signature = format!(
            "blank-ver21234-1:{}:{}:{width}:{height}:{}",
            config.profile,
            config.orientation,
            sha256(DEFAULT_PROGRAM.as_bytes())
        );
        Ok(Self {
            schema: "hmi.target/1".into(),
            profile: config.profile.clone(),
            format: config.format.clone(),
            encoding: config.encoding.clone(),
            orientation: config.orientation,
            canvas: Canvas { width, height },
            capabilities: Capabilities {
                ordinary_controls: false,
                media_assets: false,
            },
            data_revision: 1,
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
                "目标配置与当前版本的型号数据不一致，请重新导出 target.json",
            ));
        }
        Ok(())
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
