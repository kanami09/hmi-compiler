//! 文件图标的有序映射；模式和引用分别保存，最终数值串由链接器生成。
use crate::{Result, asset::ResourceReference, error};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const MAX_BYTES: usize = 64;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageMapEntry {
    pub pattern: String,
    pub pic: ResourceReference,
}
pub fn validate(entries: &[ImageMapEntry]) -> Result<()> {
    let mut patterns = BTreeSet::new();
    for entry in entries {
        let suffix = entry.pattern.strip_prefix("*.").filter(|s| !s.is_empty());
        if suffix.is_none_or(|s| {
            !s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
        }) {
            return Err(error(
                "icon_map_pattern",
                "图标模式须为 *.后缀，后缀仅含ASCII字母、数字、下划线、连字符或点",
            ));
        }
        if !patterns.insert(entry.pattern.to_ascii_lowercase()) {
            return Err(error("icon_map_pattern", "自定义图标的文件后缀不能重复"));
        }
        entry.pic.validate()?;
        if entry.pic.none.is_some() || entry.pic.id == Some(65535) {
            return Err(error(
                "resource_reference",
                "自定义图标必须引用有效内嵌图片，不能为none或65535",
            ));
        }
    }
    Ok(())
}
pub fn render(
    entries: &[ImageMapEntry],
    mut resolve: impl FnMut(&ResourceReference) -> Result<u16>,
) -> Result<String> {
    validate(entries)?;
    let mut text = String::new();
    for entry in entries {
        text.push_str(&entry.pattern);
        text.push('/');
        text.push_str(&resolve(&entry.pic)?.to_string());
    }
    if text.len() > MAX_BYTES {
        return Err(error(
            "icon_map_capacity",
            format!(
                "自定义图标映射占用{}字节，超过原生容量{MAX_BYTES}字节",
                text.len()
            ),
        ));
    }
    Ok(text)
}
pub fn parse_native(mut text: &str) -> Result<Vec<ImageMapEntry>> {
    let mut entries = Vec::new();
    while !text.is_empty() {
        let split = text
            .find('/')
            .ok_or_else(|| error("icon_map_format", "图标映射须使用 *.后缀/图片ID 格式"))?;
        let pattern = text[..split].to_string();
        let rest = &text[split + 1..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let id = rest[..digits]
            .parse::<u16>()
            .map_err(|_| error("icon_map_format", "图标映射中的图片ID无效"))?;
        entries.push(ImageMapEntry {
            pattern,
            pic: ResourceReference {
                name: None,
                id: Some(id),
                none: None,
            },
        });
        text = &rest[digits..];
    }
    validate(&entries)?;
    render(&entries, |r| Ok(r.id.unwrap()))?;
    Ok(entries)
}
pub fn parse_source(value: &toml::Value) -> Result<Vec<ImageMapEntry>> {
    let entries = if let Some(text) = value.as_str() {
        parse_native(text)?
    } else {
        value
            .clone()
            .try_into::<Vec<ImageMapEntry>>()
            .map_err(|_| {
                error(
                    "icon_map_format",
                    "vvs2须为原生映射字符串或包含pattern和pic的数组",
                )
            })?
    };
    validate(&entries)?;
    for entry in &entries {
        entry.pic.validate_source()?;
    }
    Ok(entries)
}
