//! 数字与虚拟浮点数共享字形和对齐字段，各自保留原生格式字段。
use super::*;

const fn properties(xfloat: bool) -> [Property; 23] {
    let mut p = [SCOPE; 23];
    let mut i = 0;
    // 到 ycen 为止与文本相同；数值格式字段与文本容量分别保留。
    while i < 15 {
        p[i] = TEXT_PROPERTIES[i];
        i += 1;
    }
    p[6] = integer("sta", Storage::U8, 0, 3, DefaultValue::Integer(1));
    p[7] = integer("style", Storage::U8, 0, 4, DefaultValue::Integer(0));
    p[15] = integer(
        "val",
        Storage::I32,
        i32::MIN as i64,
        i32::MAX as i64,
        DefaultValue::Integer(0),
    );
    p[16] = integer(
        if xfloat { "vvs0" } else { "lenth" },
        Storage::U8,
        0,
        if xfloat { 10 } else { 15 },
        DefaultValue::Integer(0),
    );
    p[17] = integer(
        if xfloat { "vvs1" } else { "format" },
        Storage::U8,
        0,
        if xfloat { 8 } else { 2 },
        DefaultValue::Integer(if xfloat { 2 } else { 0 }),
    );
    p[18] = integer("isbr", Storage::U8, 0, 1, DefaultValue::Integer(1));
    p[19] = integer("spax", Storage::U8, 0, 255, DefaultValue::Integer(0));
    p[20] = integer("spay", Storage::U8, 0, 255, DefaultValue::Integer(0));
    p[21] = background_image("pic", "sta", 2, false);
    p[22] = background_image("picc", "sta", 0, true);
    p
}
const NUMBER_PROPERTIES: [Property; 23] = properties(false);
const XFLOAT_PROPERTIES: [Property; 23] = properties(true);

const fn fields(xfloat: bool) -> [Field; 39] {
    let mut f = TEXT_FIELDS;
    // 前 33 项完全同序，包括未启用的图片字段。保留 lenth 的原生拼写。
    f[33] = field("val", FieldValue::Property("val"));
    f[34] = if xfloat {
        field("vvs0", FieldValue::Property("vvs0"))
    } else {
        field("lenth", FieldValue::Property("lenth"))
    };
    f[35] = if xfloat {
        field("vvs1", FieldValue::Property("vvs1"))
    } else {
        field("format", FieldValue::Property("format"))
    };
    f
}
const NUMBER_FIELDS: [Field; 39] = fields(false);
const XFLOAT_FIELDS: [Field; 39] = fields(true);

pub(super) const NUMBER: Definition = Definition {
    name: "number",
    presentation: Presentation::Control,
    code: 54,
    properties: &NUMBER_PROPERTIES,
    fields: &NUMBER_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::DEFAULT,
};
pub(super) const XFLOAT: Definition = Definition {
    name: "xfloat",
    presentation: Presentation::Control,
    code: 59,
    properties: &XFLOAT_PROPERTIES,
    fields: &XFLOAT_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::DEFAULT,
};
