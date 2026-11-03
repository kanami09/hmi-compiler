//! 页面采用标准表，控件属性采用 TOML 1.1 多行内联表。
use super::{PageFile, serialize};
use crate::{Error, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

fn string(value: &str) -> String {
    toml::Value::String(value.into()).to_string()
}

fn key(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        value.into()
    } else {
        string(value)
    }
}

fn property_group(
    output: &mut String,
    properties: &BTreeMap<String, toml::Value>,
    rows: &[&[&str]],
    written: &mut BTreeSet<String>,
) {
    let previous = !written.is_empty();
    let mut lines = Vec::new();
    for row in rows {
        let mut pairs = Vec::new();
        for name in *row {
            if let Some(value) = properties.get(*name)
                && written.insert((*name).into())
            {
                pairs.push(format!("{} = {}", key(name), value));
            }
        }
        if !pairs.is_empty() {
            lines.push(format!("    {},\n", pairs.join(", ")));
        }
    }
    if !lines.is_empty() {
        if previous {
            output.push('\n');
        }
        for line in lines {
            output.push_str(&line);
        }
    }
}

fn inline_properties(output: &mut String, properties: &BTreeMap<String, toml::Value>) {
    if properties.is_empty() {
        output.push_str("properties = {}\n");
        return;
    }
    output.push_str("properties = {\n");
    let mut written = BTreeSet::new();
    property_group(
        output,
        properties,
        &[&["x", "y"], &["w", "h"]],
        &mut written,
    );
    property_group(
        output,
        properties,
        &[
            &["txt"],
            &["txt_maxl"],
            &["font"],
            &["xcen", "ycen"],
            &["isbr", "pw"],
            &["spax", "spay"],
        ],
        &mut written,
    );
    property_group(
        output,
        properties,
        &[
            &["sta"],
            &["style"],
            &["pco"],
            &["pco2"],
            &["bco"],
            &["bco2"],
            &["borderc"],
            &["borderw"],
            &["aph"],
            &["pic"],
            &["pic2"],
            &["picc"],
            &["picc2"],
            &["pic1"],
            &["picc1"],
            &["bpic"],
            &["ppic"],
        ],
        &mut written,
    );
    property_group(
        output,
        properties,
        &[&["vscope"], &["val"], &["tim"], &["en"]],
        &mut written,
    );
    let remaining: Vec<_> = properties
        .iter()
        .filter(|(name, _)| !written.contains(*name))
        .collect();
    if !remaining.is_empty() {
        if !written.is_empty() {
            output.push('\n');
        }
        for (name, value) in remaining {
            output.push_str(&format!("    {} = {},\n", key(name), value));
        }
    }
    output.push_str("}\n");
}

/// 导出一个页面，控件使用多行内联属性表；模型与项目格式版本保持不变。
pub fn page_to_toml(page: &PageFile) -> Result<String> {
    #[derive(Serialize)]
    struct Header<'a> {
        name: &'a str,
        properties: &'a BTreeMap<String, toml::Value>,
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        events: &'a BTreeMap<String, String>,
    }
    let header = Header {
        name: &page.name,
        properties: &page.properties,
        events: &page.events,
    };
    let mut output = String::from_utf8(serialize(&header)?).map_err(|e| Error(e.to_string()))?;
    for object in &page.objects {
        output.push_str("\n[[objects]]\n");
        output.push_str(&format!(
            "name = {}\ntype = {}\n",
            string(&object.name),
            string(&object.type_name)
        ));
        inline_properties(&mut output, &object.properties);
        if !object.events.is_empty() {
            let pairs: Vec<_> = object
                .events
                .iter()
                .map(|(name, path)| format!("{} = {}", key(name), string(path)))
                .collect();
            output.push_str(&format!("events = {{ {} }}\n", pairs.join(", ")));
        }
    }
    Ok(output)
}
