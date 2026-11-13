//! 单色选择与进度控件不依赖字库，各类型的模式字段分别定义。
use super::*;

const fn selection_properties<const N: usize>(checkbox: bool) -> [Property; N] {
    let mut p = [SCOPE; N];
    let mut i = 0;
    while i < 6 {
        p[i] = HOTSPOT_PROPERTIES[i];
        i += 1;
    }
    p[6] = integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(65535));
    p[7] = integer("pco", Storage::U16, 0, 65535, DefaultValue::Integer(1024));
    p[8] = integer("val", Storage::U8, 0, 1, DefaultValue::Integer(0));
    if checkbox {
        p[9] = integer("style", Storage::U8, 0, 4, DefaultValue::Integer(2));
        p[10] = integer("borderw", Storage::U8, 0, 255, DefaultValue::Integer(2));
        p[11] = integer("borderc", Storage::U16, 0, 65535, DefaultValue::Integer(0));
    }
    p
}
const CHECKBOX_PROPERTIES: [Property; 12] = selection_properties(true);
const RADIO_PROPERTIES: [Property; 9] = selection_properties(false);
const fn selection_fields<const N: usize>(checkbox: bool) -> [Field; N] {
    let mut f = drawable_fields();
    let start = if checkbox {
        f[21] = field("style", FieldValue::Property("style"));
        f[22] = field("borderw", FieldValue::Property("borderw"));
        f[23] = field("borderc", FieldValue::Property("borderc"));
        24
    } else {
        21
    };
    f[start] = field("bco", FieldValue::Property("bco"));
    f[start + 1] = field("pco", FieldValue::Property("pco"));
    f[start + 2] = field("val", FieldValue::Property("val"));
    f
}
const CHECKBOX_FIELDS: [Field; 27] = selection_fields(true);
const RADIO_FIELDS: [Field; 24] = selection_fields(false);

const PROGRESS_PROPERTIES: [Property; 12] = {
    let mut p = [SCOPE; 12];
    let mut i = 0;
    while i < 6 {
        p[i] = HOTSPOT_PROPERTIES[i];
        i += 1;
    }
    // 此类型 sta=0 才是单色，不能套用文本的 sta=1。
    p[6] = integer("sta", Storage::U8, 0, 0, DefaultValue::Integer(0));
    p[7] = integer("dez", Storage::U8, 0, 1, DefaultValue::Integer(0));
    p[8] = integer("val", Storage::U8, 0, 100, DefaultValue::Integer(0));
    p[9] = integer("dis", Storage::U8, 0, 100, DefaultValue::Integer(25));
    p[10] = integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(50712));
    p[11] = integer("pco", Storage::U16, 0, 65535, DefaultValue::Integer(1024));
    p
};
const PROGRESS_FIELDS: [Field; 29] = {
    let mut f = drawable_fields();
    f[21] = field("sta", FieldValue::Property("sta"));
    f[22] = field("dez", FieldValue::Property("dez"));
    f[23] = field("val", FieldValue::Property("val"));
    f[24] = field("dis", FieldValue::Property("dis"));
    f[25] = field("bco", FieldValue::Property("bco"));
    f[26] = field("bpic", FieldValue::Constant(&[255, 255]));
    f[27] = field("pco", FieldValue::Property("pco"));
    f[28] = field("ppic", FieldValue::Constant(&[255, 255]));
    f
};

pub(super) const CHECKBOX: Definition = Definition {
    name: "checkbox",
    code: 56,
    properties: &CHECKBOX_PROPERTIES,
    fields: &CHECKBOX_FIELDS,
    events: &["down", "up"],
    readonly_properties: &[],
};
pub(super) const RADIO: Definition = Definition {
    name: "radio",
    code: 57,
    properties: &RADIO_PROPERTIES,
    fields: &RADIO_FIELDS,
    events: &["down", "up"],
    readonly_properties: &[],
};
pub(super) const PROGRESS: Definition = Definition {
    name: "progress_bar",
    code: 106,
    properties: &PROGRESS_PROPERTIES,
    fields: &PROGRESS_FIELDS,
    events: &["down", "up"],
    readonly_properties: &[],
};
