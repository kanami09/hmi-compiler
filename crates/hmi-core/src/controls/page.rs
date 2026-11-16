//! 页面自身对象固定占本页 ID0，几何由目标提供，不接受普通控件式的用户坐标。
use super::*;
const FIELDS: [Field; 28] = {
    let mut f = drawable_fields();
    f[21] = field("up", FieldValue::Constant(&[255]));
    f[22] = field("down", FieldValue::Constant(&[255]));
    f[23] = field("left", FieldValue::Constant(&[255]));
    f[24] = field("right", FieldValue::Constant(&[255]));
    f[25] = field("sta", FieldValue::Property("sta"));
    f[26] = field("bco", FieldValue::Property("bco"));
    f[27] = field("pic", FieldValue::Property("pic"));
    f
};
pub const PAGE: Definition = Definition {
    name: "page",
    code: 121,
    properties: &[
        integer("vscope", Storage::U8, 0, 0, DefaultValue::Integer(0)),
        integer("x", Storage::U16, 0, 0, DefaultValue::Integer(0)),
        integer("y", Storage::U16, 0, 0, DefaultValue::Integer(0)),
        integer("w", Storage::U16, 1, 65535, DefaultValue::Required),
        integer("h", Storage::U16, 1, 65535, DefaultValue::Required),
        integer("sendkey", Storage::U8, 0, 0, DefaultValue::Integer(0)),
        integer("sta", Storage::U8, 0, 2, DefaultValue::Integer(1)),
        integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(65535)),
        background_image("pic", "sta", 2, true),
    ],
    fields: &FIELDS,
    events: PAGE_EVENTS,
    access: PropertyAccess::readonly(&["sta", "x", "y", "w", "h"]),
};
