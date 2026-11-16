//! 已接入控件的属性、事件和有序存储布局。
//!
//! 默认值来自已审查的创建配方。只改变当前模式，不删除非活动属性；
//! id 是链接器填写的字段，模板中始终为零。

use crate::{
    Result,
    diagnostic::{Diagnostic, Location},
    error,
    model::check_name,
    sha256,
};
use serde::Serialize;
use std::collections::BTreeMap;

mod files;
mod graphics;
mod indicators;
mod interaction;
mod media;
mod numeric;
mod x_interaction;
pub use files::DATA_RECORD_LIMIT;

pub const PAGE_EVENTS: &[&str] = &["load", "loadend", "down", "up", "unload"];

#[derive(Clone, Copy, Debug, Serialize)]
pub enum Storage {
    U8,
    U16,
    I16,
    I32,
    U32,
    Text,
    Lines,
    Image,
    Font,
    ExternalPath,
    Animation,
    Video,
    Audio,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub enum DefaultValue {
    Required,
    Integer(i64),
    Text(&'static str),
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Property {
    pub name: &'static str,
    pub storage: Storage,
    pub min: i64,
    pub max: i64,
    pub default: DefaultValue,
    /// 图片启用条件与裁切规则；其他属性为 None。
    pub image_rule: Option<ImageRule>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ImageRule {
    pub active_when: Option<(&'static str, u8)>,
    pub crop: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub enum FieldValue {
    Type,
    Id,
    Name,
    Property(&'static str),
    Constant(&'static [u8]),
    EndX,
    EndY,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Field {
    pub name: &'static str,
    pub value: FieldValue,
}

/// 控件属性的运行时访问规则，集中声明读写和跨页面限制。
#[derive(Clone, Copy, Debug, Serialize)]
pub struct PropertyAccess {
    pub readonly: &'static [&'static str],
    pub page_local: &'static [&'static str],
    pub compile_only: &'static [&'static str],
}
impl PropertyAccess {
    pub const DEFAULT: Self = Self {
        readonly: &[],
        page_local: &[],
        compile_only: &[],
    };
    pub const fn readonly(fields: &'static [&'static str]) -> Self {
        Self {
            readonly: fields,
            ..Self::DEFAULT
        }
    }
}

/// 同一份定义用于源码检查和目标记录检查，避免两个阶段接受不同字段。
#[derive(Debug, Serialize)]
pub struct Definition {
    pub name: &'static str,
    pub code: u8,
    pub properties: &'static [Property],
    pub fields: &'static [Field],
    pub events: &'static [&'static str],
    /// 运行时读取／写入限制；与源码属性是否可设置分别声明。
    pub access: PropertyAccess,
}

const fn integer(
    name: &'static str,
    storage: Storage,
    min: i64,
    max: i64,
    default: DefaultValue,
) -> Property {
    Property {
        name,
        storage,
        min,
        max,
        default,
        image_rule: None,
    }
}
const fn field(name: &'static str, value: FieldValue) -> Field {
    Field { name, value }
}

const fn background_image(
    name: &'static str,
    mode: &'static str,
    value: u8,
    crop: bool,
) -> Property {
    Property {
        name,
        storage: Storage::Image,
        min: 0,
        max: 65535,
        default: DefaultValue::Integer(65535),
        image_rule: Some(ImageRule {
            active_when: Some((mode, value)),
            crop,
        }),
    }
}

const SCOPE: Property = integer("vscope", Storage::U8, 0, 1, DefaultValue::Integer(0));
const HOTSPOT_PROPERTIES: &[Property] = &[
    SCOPE,
    integer("x", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("y", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("w", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("h", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("sendkey", Storage::U8, 0, 1, DefaultValue::Integer(0)),
];
const VARIABLE_PROPERTIES: &[Property] = &[
    SCOPE,
    integer("sta", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    Property {
        name: "txt",
        storage: Storage::Text,
        min: 0,
        max: 0,
        default: DefaultValue::Text("newtxt"),
        image_rule: None,
    },
    integer(
        "txt_maxl",
        Storage::U16,
        0,
        65535,
        DefaultValue::Integer(10),
    ),
    integer(
        "val",
        Storage::I32,
        i32::MIN as i64,
        i32::MAX as i64,
        DefaultValue::Integer(0),
    ),
];
const TIMER_PROPERTIES: &[Property] = &[
    SCOPE,
    integer("tim", Storage::U16, 50, 65534, DefaultValue::Integer(400)),
    integer("en", Storage::U8, 0, 1, DefaultValue::Integer(1)),
];

const fn image_properties(name: &'static str) -> [Property; 7] {
    let mut properties = [SCOPE; 7];
    let mut index = 0;
    while index < 6 {
        properties[index] = HOTSPOT_PROPERTIES[index];
        index += 1;
    }
    properties[6] = integer(name, Storage::Image, 0, 65534, DefaultValue::Required);
    properties
}
const PICTURE_PROPERTIES: &[Property] = &image_properties("pic");
const CROP_PROPERTIES: &[Property] = &{
    let mut p = image_properties("picc");
    p[6].image_rule = Some(ImageRule {
        active_when: None,
        crop: true,
    });
    p
};

/// 文本和按钮保留全部原生字段，按最终模式判断必需的图片。
const fn text_properties<const N: usize>(button: bool) -> [Property; N] {
    let mut p = [SCOPE; N];
    let mut i = 0;
    while i < 6 {
        p[i] = HOTSPOT_PROPERTIES[i];
        i += 1;
    }
    p[6] = integer("sta", Storage::U8, 0, 2, DefaultValue::Integer(1));
    p[7] = integer(
        "style",
        Storage::U8,
        if button { 1 } else { 0 },
        if button { 4 } else { 1 },
        DefaultValue::Integer(if button { 4 } else { 0 }),
    );
    p[8] = integer("borderc", Storage::U16, 0, 65535, DefaultValue::Integer(0));
    p[9] = integer("borderw", Storage::U8, 0, 255, DefaultValue::Integer(2));
    p[10] = integer("font", Storage::Font, 0, 254, DefaultValue::Required);
    p[11] = integer(
        "bco",
        Storage::U16,
        0,
        65535,
        DefaultValue::Integer(if button { 50712 } else { 65535 }),
    );
    p[12] = integer("pco", Storage::U16, 0, 65535, DefaultValue::Integer(0));
    p[13] = integer("xcen", Storage::U8, 0, 2, DefaultValue::Integer(1));
    p[14] = integer("ycen", Storage::U8, 0, 2, DefaultValue::Integer(1));
    p[15] = Property {
        name: "txt",
        storage: Storage::Text,
        min: 0,
        max: 0,
        default: DefaultValue::Required,
        image_rule: None,
    };
    p[16] = integer(
        "txt_maxl",
        Storage::U16,
        0,
        65535,
        DefaultValue::Integer(10),
    );
    p[17] = integer("isbr", Storage::U8, 0, 1, DefaultValue::Integer(0));
    p[18] = integer("spax", Storage::U8, 0, 255, DefaultValue::Integer(0));
    p[19] = integer("spay", Storage::U8, 0, 255, DefaultValue::Integer(0));
    if button {
        p[20] = integer("bco2", Storage::U16, 0, 65535, DefaultValue::Integer(1024));
        p[21] = integer("pco2", Storage::U16, 0, 65535, DefaultValue::Integer(65535));
        p[22] = integer("val", Storage::U8, 0, 1, DefaultValue::Integer(0));
        p[23] = background_image("pic", "sta", 2, false);
        p[24] = background_image("pic2", "sta", 2, false);
        p[25] = background_image("picc", "sta", 0, true);
        p[26] = background_image("picc2", "sta", 0, true);
    } else {
        p[20] = integer("pw", Storage::U8, 0, 1, DefaultValue::Integer(0));
        p[21] = background_image("pic", "sta", 2, false);
        p[22] = background_image("picc", "sta", 0, true);
    }
    p
}
const TEXT_PROPERTIES: [Property; 23] = text_properties(false);
const BUTTON_PROPERTIES: [Property; 27] = text_properties(true);

const HOTSPOT_FIELDS: &[Field] = &[
    field("type", FieldValue::Type),
    field("id", FieldValue::Id),
    field("objname", FieldValue::Name),
    field("vscope", FieldValue::Property("vscope")),
    field("drag", FieldValue::Constant(&[0])),
    field("sendkey", FieldValue::Property("sendkey")),
    field("aph", FieldValue::Constant(&[127])),
    field("movex", FieldValue::Constant(&[0, 0])),
    field("movey", FieldValue::Constant(&[0, 0])),
    field("x", FieldValue::Property("x")),
    field("y", FieldValue::Property("y")),
    field("w", FieldValue::Property("w")),
    field("h", FieldValue::Property("h")),
    field("endx", FieldValue::EndX),
    field("endy", FieldValue::EndY),
    field("effect", FieldValue::Constant(&[0])),
    field("first", FieldValue::Constant(&[0])),
    field("time", FieldValue::Constant(&[0x2c, 1])),
    field("lockobj", FieldValue::Constant(&[0])),
    field("groupid0", FieldValue::Constant(&[0; 4])),
    field("groupid1", FieldValue::Constant(&[0; 4])),
];
const VARIABLE_FIELDS: &[Field] = &[
    field("type", FieldValue::Type),
    field("id", FieldValue::Id),
    field("objname", FieldValue::Name),
    field("vscope", FieldValue::Property("vscope")),
    field("lockobj", FieldValue::Constant(&[0])),
    field("groupid0", FieldValue::Constant(&[0; 4])),
    field("groupid1", FieldValue::Constant(&[0; 4])),
    field("sta", FieldValue::Property("sta")),
    field("txt", FieldValue::Property("txt")),
    field("txt_maxl", FieldValue::Property("txt_maxl")),
    field("val", FieldValue::Property("val")),
];

const fn drawable_fields<const N: usize>() -> [Field; N] {
    let mut fields = [HOTSPOT_FIELDS[0]; N];
    let mut index = 0;
    while index < HOTSPOT_FIELDS.len() {
        fields[index] = HOTSPOT_FIELDS[index];
        index += 1;
    }
    fields
}
const PICTURE_FIELDS: [Field; 22] = {
    let mut fields = drawable_fields();
    fields[21] = field("pic", FieldValue::Property("pic"));
    fields
};
const CROP_FIELDS: [Field; 24] = {
    let mut fields = drawable_fields();
    fields[21] = field("picc", FieldValue::Property("picc"));
    fields[22] = field("vvs0", FieldValue::Constant(&[0, 0]));
    fields[23] = field("vvs1", FieldValue::Constant(&[0, 0]));
    fields
};
const TEXT_FIELDS: [Field; 39] = {
    let mut fields = drawable_fields();
    fields[21] = field("sta", FieldValue::Property("sta"));
    fields[22] = field("style", FieldValue::Property("style"));
    fields[23] = field("key", FieldValue::Constant(&[255]));
    fields[24] = field("borderc", FieldValue::Property("borderc"));
    fields[25] = field("borderw", FieldValue::Property("borderw"));
    fields[26] = field("font", FieldValue::Property("font"));
    fields[27] = field("bco", FieldValue::Property("bco"));
    fields[28] = field("picc", FieldValue::Property("picc"));
    fields[29] = field("pic", FieldValue::Property("pic"));
    fields[30] = field("pco", FieldValue::Property("pco"));
    fields[31] = field("xcen", FieldValue::Property("xcen"));
    fields[32] = field("ycen", FieldValue::Property("ycen"));
    fields[33] = field("pw", FieldValue::Property("pw"));
    fields[34] = field("txt", FieldValue::Property("txt"));
    fields[35] = field("txt_maxl", FieldValue::Property("txt_maxl"));
    fields[36] = field("isbr", FieldValue::Property("isbr"));
    fields[37] = field("spax", FieldValue::Property("spax"));
    fields[38] = field("spay", FieldValue::Property("spay"));
    fields
};

const BUTTON_FIELDS: [Field; 42] = {
    let mut fields = drawable_fields();
    fields[21] = field("sta", FieldValue::Property("sta"));
    fields[22] = field("style", FieldValue::Property("style"));
    fields[23] = field("borderc", FieldValue::Property("borderc"));
    fields[24] = field("borderw", FieldValue::Property("borderw"));
    fields[25] = field("font", FieldValue::Property("font"));
    fields[26] = field("pic", FieldValue::Property("pic"));
    fields[27] = field("picc", FieldValue::Property("picc"));
    fields[28] = field("bco", FieldValue::Property("bco"));
    fields[29] = field("pic2", FieldValue::Property("pic2"));
    fields[30] = field("picc2", FieldValue::Property("picc2"));
    fields[31] = field("bco2", FieldValue::Property("bco2"));
    fields[32] = field("pco", FieldValue::Property("pco"));
    fields[33] = field("pco2", FieldValue::Property("pco2"));
    fields[34] = field("xcen", FieldValue::Property("xcen"));
    fields[35] = field("ycen", FieldValue::Property("ycen"));
    fields[36] = field("val", FieldValue::Property("val"));
    fields[37] = field("txt", FieldValue::Property("txt"));
    fields[38] = field("txt_maxl", FieldValue::Property("txt_maxl"));
    fields[39] = field("isbr", FieldValue::Property("isbr"));
    fields[40] = field("spax", FieldValue::Property("spax"));
    fields[41] = field("spay", FieldValue::Property("spay"));
    fields
};

const TIMER_FIELDS: &[Field] = &[
    field("type", FieldValue::Type),
    field("id", FieldValue::Id),
    field("objname", FieldValue::Name),
    field("vscope", FieldValue::Property("vscope")),
    field("lockobj", FieldValue::Constant(&[0])),
    field("groupid0", FieldValue::Constant(&[0; 4])),
    field("groupid1", FieldValue::Constant(&[0; 4])),
    field("tim", FieldValue::Property("tim")),
    field("en", FieldValue::Property("en")),
];

pub const DEFINITIONS: &[Definition] = &[
    media::ANIMATION,
    media::VIDEO,
    media::AUDIO,
    files::EXTERNAL_PICTURE,
    files::DATA_RECORD,
    files::FILE_BROWSER,
    files::FILE_STREAM,
    x_interaction::SWITCH,
    x_interaction::COMBO_BOX,
    x_interaction::TEXT_SELECT,
    x_interaction::SLIDING_TEXT,
    graphics::GAUGE,
    graphics::WAVEFORM,
    graphics::QRCODE,
    interaction::SLIDER,
    interaction::TOUCH_CAPTURE,
    interaction::DUAL_STATE_BUTTON,
    interaction::SCROLL_TEXT,
    numeric::NUMBER,
    numeric::XFLOAT,
    indicators::CHECKBOX,
    indicators::RADIO,
    indicators::PROGRESS,
    Definition {
        name: "text",
        code: 116,
        properties: &TEXT_PROPERTIES,
        fields: &TEXT_FIELDS,
        events: &["down", "up"],
        access: PropertyAccess::DEFAULT,
    },
    Definition {
        name: "button",
        code: 98,
        properties: &BUTTON_PROPERTIES,
        fields: &BUTTON_FIELDS,
        events: &["down", "up"],
        access: PropertyAccess::DEFAULT,
    },
    Definition {
        name: "hotspot",
        code: 109,
        properties: HOTSPOT_PROPERTIES,
        fields: HOTSPOT_FIELDS,
        events: &["down", "up"],
        access: PropertyAccess::DEFAULT,
    },
    Definition {
        name: "variable",
        code: 52,
        properties: VARIABLE_PROPERTIES,
        fields: VARIABLE_FIELDS,
        events: &[],
        access: PropertyAccess::DEFAULT,
    },
    Definition {
        name: "timer",
        code: 51,
        properties: TIMER_PROPERTIES,
        fields: TIMER_FIELDS,
        events: &["timer"],
        access: PropertyAccess::DEFAULT,
    },
    Definition {
        name: "picture",
        code: 112,
        properties: PICTURE_PROPERTIES,
        fields: &PICTURE_FIELDS,
        events: &["down", "up"],
        access: PropertyAccess::DEFAULT,
    },
    Definition {
        name: "crop",
        code: 113,
        properties: CROP_PROPERTIES,
        fields: &CROP_FIELDS,
        events: &["down", "up"],
        access: PropertyAccess::DEFAULT,
    },
];

pub fn by_name(name: &str) -> Result<&'static Definition> {
    DEFINITIONS.iter().find(|d| d.name == name).ok_or_else(|| {
        error(
            "control_type",
            format!(
                "当前尚未接入控件类型 {name}，已支持 {}",
                DEFINITIONS
                    .iter()
                    .map(|d| d.name)
                    .collect::<Vec<_>>()
                    .join("、")
            ),
        )
    })
}

pub fn by_code(code: u8) -> Result<&'static Definition> {
    DEFINITIONS
        .iter()
        .find(|d| d.code == code)
        .ok_or_else(|| error("control_type", format!("当前尚未接入控件类型码 {code}")))
}

/// 注册表变化必须使旧目标配置失效，即使文件格式版本没有变化。
pub fn fingerprint() -> String {
    sha256(&serde_json::to_vec(DEFINITIONS).expect("静态控件定义可以序列化"))
}

fn property_error(code: &str, property: &str, message: impl Into<String>) -> crate::Error {
    Diagnostic::error(code, message)
        .at(Location {
            property: Some(property.into()),
            ..Default::default()
        })
        .into()
}

impl Property {
    pub fn allows_none(&self) -> bool {
        matches!(self.storage, Storage::Image)
            && matches!(self.default, DefaultValue::Integer(65535))
    }
    pub fn image_active(&self, attributes: &[(String, Vec<u8>)]) -> bool {
        self.image_rule
            .and_then(|r| r.active_when)
            .is_none_or(|(field, value)| {
                attributes
                    .iter()
                    .any(|(k, v)| k == field && v.as_slice() == [value])
            })
    }
    fn encode(&self, value: &toml::Value) -> Result<Vec<u8>> {
        if matches!(
            self.storage,
            Storage::Text | Storage::Lines | Storage::ExternalPath
        ) {
            let bytes = value
                .as_str()
                .filter(|s| !s.contains('\0'))
                .map(|s| s.as_bytes().to_vec())
                .ok_or_else(|| {
                    property_error(
                        "property_type",
                        self.name,
                        format!("属性 {} 必须为不含 NUL 的字符串", self.name),
                    )
                })?;
            if matches!(self.storage, Storage::Lines) {
                let text = std::str::from_utf8(&bytes).unwrap();
                let lf = text.replace("\r\n", "\n");
                if lf.contains('\r') {
                    return Err(property_error(
                        "property_text",
                        self.name,
                        "多行文本请使用 LF 或 CRLF 换行",
                    ));
                }
                return Ok(lf.replace('\n', "\r\n").into_bytes());
            }
            return Ok(bytes);
        }
        let number = value.as_integer().ok_or_else(|| {
            property_error(
                "property_type",
                self.name,
                format!("属性 {} 必须为整数", self.name),
            )
        })?;
        if !(self.min..=self.max).contains(&number) {
            return Err(property_error(
                "property_range",
                self.name,
                format!(
                    "属性 {} 的值为 {number}，允许范围为 {}～{}",
                    self.name, self.min, self.max
                ),
            ));
        }
        Ok(match self.storage {
            Storage::U8 | Storage::Font => vec![number as u8],
            Storage::U16
            | Storage::Image
            | Storage::Animation
            | Storage::Video
            | Storage::Audio => (number as u16).to_le_bytes().to_vec(),
            Storage::I16 => (number as i16).to_le_bytes().to_vec(),
            Storage::I32 => (number as i32).to_le_bytes().to_vec(),
            Storage::U32 => (number as u32).to_le_bytes().to_vec(),
            Storage::Text | Storage::Lines | Storage::ExternalPath => unreachable!(),
        })
    }

    pub(crate) fn decode(&self, bytes: &[u8]) -> Result<toml::Value> {
        let invalid = || {
            property_error(
                "object_property",
                self.name,
                format!("目标记录中属性 {} 的宽度或编码无效", self.name),
            )
        };
        Ok(match self.storage {
            Storage::U8 | Storage::Font => toml::Value::Integer(i64::from(
                *bytes
                    .first()
                    .filter(|_| bytes.len() == 1)
                    .ok_or_else(invalid)?,
            )),
            Storage::U16
            | Storage::Image
            | Storage::Animation
            | Storage::Video
            | Storage::Audio => toml::Value::Integer(i64::from(u16::from_le_bytes(
                bytes.try_into().map_err(|_| invalid())?,
            ))),
            Storage::I16 => toml::Value::Integer(i64::from(i16::from_le_bytes(
                bytes.try_into().map_err(|_| invalid())?,
            ))),
            Storage::I32 => toml::Value::Integer(i64::from(i32::from_le_bytes(
                bytes.try_into().map_err(|_| invalid())?,
            ))),
            Storage::U32 => toml::Value::Integer(i64::from(u32::from_le_bytes(
                bytes.try_into().map_err(|_| invalid())?,
            ))),
            Storage::Text | Storage::Lines | Storage::ExternalPath => {
                toml::Value::String(std::str::from_utf8(bytes).map_err(|_| invalid())?.into())
            }
        })
    }
}

impl Definition {
    /// 保持布局顺序，先补齐已定义默认值，再编码；不会依据模式移除字段。
    pub fn attributes(
        &self,
        name: &str,
        properties: &BTreeMap<String, toml::Value>,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        check_name(name)?;
        for key in properties.keys() {
            if !self.properties.iter().any(|p| p.name == key) {
                return Err(property_error(
                    "control_property",
                    key,
                    format!("控件类型 {} 不支持可编辑属性 {key}", self.name),
                ));
            }
        }
        let mut values = BTreeMap::new();
        for property in self.properties {
            let default;
            let value = if let Some(value) = properties.get(property.name) {
                value
            } else {
                default = match property.default {
                    DefaultValue::Required => {
                        return Err(property_error(
                            "property_required",
                            property.name,
                            format!("控件类型 {} 必须指定属性 {}", self.name, property.name),
                        ));
                    }
                    DefaultValue::Integer(n) => toml::Value::Integer(n),
                    DefaultValue::Text(s) => toml::Value::String(s.into()),
                };
                &default
            };
            values.insert(property.name, property.encode(value)?);
        }
        if matches!(self.code, 52 | 53 | 55 | 58 | 61 | 62 | 67 | 116 | 98) {
            let capacity =
                u16::from_le_bytes(values["txt_maxl"].as_slice().try_into().unwrap()) as usize;
            if values["txt"].len() > capacity {
                return Err(property_error(
                    "text_capacity",
                    "txt",
                    format!(
                        "UTF-8 文本占用 {} 字节，超过 txt_maxl 的 {capacity} 字节容量",
                        values["txt"].len()
                    ),
                ));
            }
        }
        if matches!(self.code, 61 | 68) {
            let capacity =
                u16::from_le_bytes(values["path_m"].as_slice().try_into().unwrap()) as usize;
            if values["path"].len() > capacity {
                return Err(property_error(
                    "text_capacity",
                    "path",
                    format!(
                        "选项集合占用 {} 个 UTF-8 字节，超过 path_m 的 {capacity} 字节容量",
                        values["path"].len()
                    ),
                ));
            }
        }
        if self.code == 65 {
            for (field, capacity) in [("dir", 255), ("filter", 64)] {
                if values[field].len() > capacity {
                    return Err(property_error(
                        "text_capacity",
                        field,
                        format!(
                            "文件浏览器 {field} 占用 {} 个 UTF-8 字节，超过 {capacity} 字节容量",
                            values[field].len()
                        ),
                    ));
                }
            }
        }
        if self.code == 60 && values["path"].len() > 255 {
            return Err(property_error(
                "text_capacity",
                "path",
                "外部图片路径超过 255 个 UTF-8 字节",
            ));
        }
        if self.code == 66 {
            for (field, capacity) in [("path", 255), ("format", 256), ("dir", 255)] {
                if values[field].len() > capacity {
                    return Err(property_error(
                        "text_capacity",
                        field,
                        format!(
                            "数据记录 {field} 占用 {} 个 UTF-8 字节，超过 {capacity} 字节容量",
                            values[field].len()
                        ),
                    ));
                }
            }
            if values["path"].is_empty() {
                return Err(property_error(
                    "runtime_file_path",
                    "path",
                    "数据记录必须指定运行时文件路径",
                ));
            }
        }
        if self.code == 62 {
            let buffer =
                u16::from_le_bytes(values["path_m"].as_slice().try_into().unwrap()) as usize;
            let capacity =
                u16::from_le_bytes(values["txt_maxl"].as_slice().try_into().unwrap()) as usize;
            let rows = if values["txt"].is_empty() {
                0
            } else {
                values["txt"].iter().filter(|&&b| b == b'\n').count() + 1
            };
            let max_rows = (if buffer == 0 { capacity } else { buffer }) / 2;
            if rows > max_rows {
                return Err(property_error(
                    "text_capacity",
                    "path_m",
                    format!("滑动文本包含 {rows} 行，当前缓冲区最多支持 {max_rows} 行"),
                ));
            }
        }
        if self.code == 1 {
            let min = u16::from_le_bytes(values["minval"].as_slice().try_into().unwrap());
            let max = u16::from_le_bytes(values["maxval"].as_slice().try_into().unwrap());
            if min > max {
                return Err(property_error(
                    "slider_range",
                    "minval",
                    format!("滑块 minval={min} 大于 maxval={max}"),
                ));
            }
            // 原生范围仅限制滑动，不限制显式赋值，不能据此拒绝范围外的初始 val。
        }
        if self.code == 98 && !matches!(values["style"].as_slice(), [1] | [4]) {
            return Err(property_error(
                "property_range",
                "style",
                "当前按钮 style 支持 1（边框）和 4（3D_Auto）",
            ));
        }
        let endpoint = |position: &str, size: &str| -> Result<Vec<u8>> {
            let start = u16::from_le_bytes(values[position].as_slice().try_into().unwrap());
            let length = u16::from_le_bytes(values[size].as_slice().try_into().unwrap());
            let end = u32::from(start) + u32::from(length) - 1;
            let end = u16::try_from(end).map_err(|_| {
                property_error(
                    "geometry",
                    size,
                    format!("{position} + {size} - 1 超过 65535"),
                )
            })?;
            Ok(end.to_le_bytes().to_vec())
        };
        let attributes = self
            .fields
            .iter()
            .map(|field| {
                let value = match field.value {
                    FieldValue::Type => vec![self.code],
                    FieldValue::Id => vec![0],
                    FieldValue::Name => name.as_bytes().to_vec(),
                    FieldValue::Property(key) => values[key].clone(),
                    FieldValue::Constant(bytes) => bytes.to_vec(),
                    FieldValue::EndX => endpoint("x", "w")?,
                    FieldValue::EndY => endpoint("y", "h")?,
                };
                Ok((field.name.into(), value))
            })
            .collect::<Result<Vec<_>>>()?;
        for property in self.properties {
            if matches!(property.storage, Storage::Image)
                && property.image_active(&attributes)
                && attributes
                    .iter()
                    .any(|(k, v)| k == property.name && v == &[255, 255])
            {
                return Err(property_error(
                    "property_required",
                    property.name,
                    format!("当前 {} 模式必须指定图片属性 {}", self.name, property.name),
                ));
            }
        }
        Ok(attributes)
    }

    /// 链接前复查原始属性布局及约束，ID 仍必须为零占位值。
    pub fn validate_record(&self, name: &str, attributes: &[(String, Vec<u8>)]) -> Result<()> {
        let properties = self.properties_from_record(attributes)?;
        if self.attributes(name, &properties)? != attributes {
            return Err(error(
                "object_layout",
                format!(
                    "控件 {name} 的目标记录与 {} 创建布局不一致，请重新编译",
                    self.name
                ),
            ));
        }
        Ok(())
    }

    /// 按注册宽度和符号解码，供目标检查使用；变量 val 不能按 u32 显示。
    pub fn properties_from_record(
        &self,
        attributes: &[(String, Vec<u8>)],
    ) -> Result<BTreeMap<String, toml::Value>> {
        let mut properties = BTreeMap::new();
        for property in self.properties {
            let bytes = attributes
                .iter()
                .find(|(key, _)| key == property.name)
                .ok_or_else(|| {
                    property_error(
                        "object_property",
                        property.name,
                        format!("目标记录缺少属性 {}", property.name),
                    )
                })?;
            properties.insert(property.name.into(), property.decode(&bytes.1)?);
        }
        Ok(properties)
    }
}
