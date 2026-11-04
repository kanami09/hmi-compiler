//! 解析页面时保留值的位置，语义检查直接生成结构化诊断。
use super::{ObjectFile, PageFile};
use crate::{
    Result,
    diagnostic::{Diagnostic, Location},
};
use serde::Deserialize;
use std::collections::BTreeMap;
use toml::Spanned;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceObject {
    name: Spanned<String>,
    #[serde(rename = "type")]
    type_name: Spanned<String>,
    properties: BTreeMap<String, Spanned<toml::Value>>,
    #[serde(default)]
    events: BTreeMap<String, Spanned<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePage {
    name: Spanned<String>,
    properties: BTreeMap<String, Spanned<toml::Value>>,
    #[serde(default)]
    events: BTreeMap<String, Spanned<String>>,
    #[serde(default)]
    objects: Vec<SourceObject>,
}

#[derive(Clone, Debug)]
pub struct PageDocument {
    pub page: PageFile,
    pub file: String,
    lines: BTreeMap<(usize, String), usize>,
}

pub fn line_at(bytes: &[u8], offset: usize) -> usize {
    1 + bytes[..offset.min(bytes.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
}

impl PageDocument {
    pub fn parse(bytes: &[u8], file: &str) -> Result<Self> {
        let text = std::str::from_utf8(bytes).map_err(|e| {
            Diagnostic::error("source_encoding", "页面描述不是有效的 UTF-8 文本").at(Location {
                file: Some(file.into()),
                line: Some(line_at(bytes, e.valid_up_to())),
                ..Location::default()
            })
        })?;
        let source: SourcePage = toml::from_str(text).map_err(|e: toml::de::Error| {
            Diagnostic::error("toml_syntax", e.message()).at(Location {
                file: Some(file.into()),
                line: e.span().map(|s| line_at(bytes, s.start)),
                ..Location::default()
            })
        })?;
        let mut lines = BTreeMap::new();
        lines.insert((0, "name".into()), line_at(bytes, source.name.span().start));
        let mut convert = |index, object: SourceObject| {
            lines.insert(
                (index, "name".into()),
                line_at(bytes, object.name.span().start),
            );
            lines.insert(
                (index, "type".into()),
                line_at(bytes, object.type_name.span().start),
            );
            let properties = object
                .properties
                .into_iter()
                .map(|(key, value)| {
                    lines.insert((index, key.clone()), line_at(bytes, value.span().start));
                    (key, value.into_inner())
                })
                .collect();
            let events = object
                .events
                .into_iter()
                .map(|(key, value)| {
                    lines.insert(
                        (index, format!("events.{key}")),
                        line_at(bytes, value.span().start),
                    );
                    (key, value.into_inner())
                })
                .collect();
            ObjectFile {
                name: object.name.into_inner(),
                type_name: object.type_name.into_inner(),
                properties,
                events,
            }
        };
        let own = convert(
            0,
            SourceObject {
                name: source.name,
                type_name: Spanned::new(0..0, "page".into()),
                properties: source.properties,
                events: source.events,
            },
        );
        let objects = source
            .objects
            .into_iter()
            .enumerate()
            .map(|(i, object)| convert(i + 1, object))
            .collect();
        Ok(Self {
            page: PageFile {
                name: own.name,
                properties: own.properties,
                events: own.events,
                objects,
            },
            file: file.into(),
            lines,
        })
    }

    pub fn location(&self, index: usize, field: Option<&str>) -> Location {
        let event = field
            .and_then(|f| f.strip_prefix("events."))
            .map(str::to_owned);
        Location {
            file: Some(self.file.clone()),
            page: Some(self.page.name.clone()),
            object: index
                .checked_sub(1)
                .and_then(|i| self.page.objects.get(i))
                .map(|o| o.name.clone()),
            object_index: Some(index),
            property: field.filter(|_| event.is_none()).map(str::to_owned),
            event,
            line: self
                .lines
                .get(&(index, field.unwrap_or("name").into()))
                .copied(),
            ..Location::default()
        }
    }

    pub fn error(
        &self,
        index: usize,
        field: Option<&str>,
        code: &str,
        message: impl Into<String>,
    ) -> crate::Error {
        Diagnostic::error(code, message)
            .at(self.location(index, field))
            .into()
    }

    pub fn locate(&self, diagnostic: &mut Diagnostic) {
        if diagnostic.location.line.is_none()
            && diagnostic
                .location
                .file
                .as_deref()
                .is_none_or(|p| p == self.file)
        {
            let index = diagnostic
                .location
                .object_index
                .or_else(|| {
                    diagnostic.location.object.as_ref().and_then(|name| {
                        self.page
                            .objects
                            .iter()
                            .position(|o| &o.name == name)
                            .map(|i| i + 1)
                    })
                })
                .unwrap_or(0);
            let field = diagnostic.location.property.as_deref();
            diagnostic.location = self.location(index, field);
        }
    }
}
