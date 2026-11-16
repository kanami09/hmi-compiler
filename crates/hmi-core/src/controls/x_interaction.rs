//! X 系列交互控件；选项、当前值和原生缓存分别保存。
use super::*;

const SWITCH_PROPERTIES: &[Property] = &[
    integer("vscope", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("x", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("y", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("w", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("h", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("sendkey", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("dez", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("val", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(50712)),
    integer("pco", Storage::U16, 0, 65535, DefaultValue::Integer(65535)),
    integer("bco2", Storage::U16, 0, 65535, DefaultValue::Integer(1024)),
    integer("pco2", Storage::U16, 0, 65535, DefaultValue::Integer(65535)),
    integer("pco1", Storage::U16, 0, 65535, DefaultValue::Integer(0)),
    integer("font", Storage::Font, 0, 254, DefaultValue::Required),
    integer("dis", Storage::U8, 0, 100, DefaultValue::Integer(100)),
    integer("txt", Storage::Text, 0, 0, DefaultValue::Text("")),
    integer("txt_maxl", Storage::U16, 0, 24, DefaultValue::Integer(24)),
];

const SWITCH_FIELDS: [Field; 32] = {
    let mut f = drawable_fields();
    f[21] = field("dez", FieldValue::Property("dez"));
    f[22] = field("val", FieldValue::Property("val"));
    f[23] = field("bco", FieldValue::Property("bco"));
    f[24] = field("pco", FieldValue::Property("pco"));
    f[25] = field("bco2", FieldValue::Property("bco2"));
    f[26] = field("pco2", FieldValue::Property("pco2"));
    f[27] = field("pco1", FieldValue::Property("pco1"));
    f[28] = field("font", FieldValue::Property("font"));
    f[29] = field("dis", FieldValue::Property("dis"));
    f[30] = field("txt", FieldValue::Property("txt"));
    f[31] = field("txt_maxl", FieldValue::Property("txt_maxl"));
    f
};

pub(super) const SWITCH: Definition = Definition {
    name: "switch",
    code: 67,
    properties: SWITCH_PROPERTIES,
    fields: &SWITCH_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::DEFAULT,
};

const COMBO_BOX_PROPERTIES: &[Property] = &[
    integer("vscope", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("x", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("y", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("w", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("h", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("sendkey", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("sta", Storage::U8, 0, 3, DefaultValue::Integer(1)),
    background_image("pic", "sta", 2, false),
    background_image("picc", "sta", 0, true),
    integer("style", Storage::U8, 0, 4, DefaultValue::Integer(0)),
    integer("borderc", Storage::U16, 0, 65535, DefaultValue::Integer(0)),
    integer("borderw", Storage::U8, 0, 255, DefaultValue::Integer(2)),
    integer("font", Storage::Font, 0, 254, DefaultValue::Required),
    integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(65535)),
    integer("pco", Storage::U16, 0, 65535, DefaultValue::Integer(0)),
    integer("xcen", Storage::U8, 0, 2, DefaultValue::Integer(1)),
    integer("ycen", Storage::U8, 0, 2, DefaultValue::Integer(1)),
    integer("spax", Storage::U8, 0, 255, DefaultValue::Integer(0)),
    integer("dis", Storage::U8, 0, 100, DefaultValue::Integer(25)),
    integer("txt", Storage::Text, 0, 0, DefaultValue::Text("")),
    integer(
        "txt_maxl",
        Storage::U16,
        0,
        65535,
        DefaultValue::Integer(16),
    ),
    integer("up", Storage::U8, 0, 1, DefaultValue::Integer(1)),
    integer("pco3", Storage::U16, 0, 65535, DefaultValue::Integer(42292)),
    integer("bco1", Storage::U16, 0, 65535, DefaultValue::Integer(65535)),
    integer("pco1", Storage::U16, 0, 65535, DefaultValue::Integer(0)),
    integer("path", Storage::Lines, 0, 0, DefaultValue::Text("")),
    integer("path_m", Storage::U16, 0, 65535, DefaultValue::Integer(64)),
    integer("dir", Storage::U8, 0, 3, DefaultValue::Integer(1)),
    integer("qty", Storage::U8, 1, 254, DefaultValue::Integer(2)),
    integer("vvs0", Storage::U8, 0, 255, DefaultValue::Integer(8)),
    integer("val", Storage::U8, 0, 255, DefaultValue::Integer(0)),
    integer("bco2", Storage::U16, 0, 65535, DefaultValue::Integer(31)),
    integer("pco2", Storage::U16, 0, 65535, DefaultValue::Integer(65535)),
    integer("hig", Storage::U8, 16, 255, DefaultValue::Integer(72)),
    integer("down", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("mode", Storage::U8, 0, 2, DefaultValue::Integer(2)),
    integer("wid", Storage::U8, 8, 255, DefaultValue::Integer(12)),
    integer("vvs1", Storage::U8, 0, 255, DefaultValue::Integer(2)),
    integer("ch", Storage::U8, 0, 32, DefaultValue::Integer(8)),
];

const COMBO_BOX_FIELDS: [Field; 55] = {
    let mut f = drawable_fields();
    f[21] = field("sta", FieldValue::Property("sta"));
    f[22] = field("style", FieldValue::Property("style"));
    f[23] = field("borderc", FieldValue::Property("borderc"));
    f[24] = field("borderw", FieldValue::Property("borderw"));
    f[25] = field("font", FieldValue::Property("font"));
    f[26] = field("bco", FieldValue::Property("bco"));
    f[27] = field("picc", FieldValue::Property("picc"));
    f[28] = field("pic", FieldValue::Property("pic"));
    f[29] = field("pco", FieldValue::Property("pco"));
    f[30] = field("xcen", FieldValue::Property("xcen"));
    f[31] = field("ycen", FieldValue::Property("ycen"));
    f[32] = field("spax", FieldValue::Property("spax"));
    f[33] = field("dis", FieldValue::Property("dis"));
    f[34] = field("txt", FieldValue::Property("txt"));
    f[35] = field("txt_maxl", FieldValue::Property("txt_maxl"));
    f[36] = field("up", FieldValue::Property("up"));
    f[37] = field("pco3", FieldValue::Property("pco3"));
    f[38] = field("bco1", FieldValue::Property("bco1"));
    f[39] = field("pco1", FieldValue::Property("pco1"));
    f[40] = field("path", FieldValue::Property("path"));
    f[41] = field("path_m", FieldValue::Property("path_m"));
    f[42] = field("dir", FieldValue::Property("dir"));
    f[43] = field("qty", FieldValue::Property("qty"));
    f[44] = field("vvs0", FieldValue::Property("vvs0"));
    f[45] = field("val", FieldValue::Property("val"));
    f[46] = field("bco2", FieldValue::Property("bco2"));
    f[47] = field("pco2", FieldValue::Property("pco2"));
    f[48] = field("hig", FieldValue::Property("hig"));
    f[49] = field("down", FieldValue::Property("down"));
    f[50] = field("mode", FieldValue::Property("mode"));
    f[51] = field("wid", FieldValue::Property("wid"));
    f[52] = field("vvs1", FieldValue::Property("vvs1"));
    f[53] = field("ch", FieldValue::Property("ch"));
    f[54] = field("drastate", FieldValue::Constant(&[0]));
    f
};

pub(super) const COMBO_BOX: Definition = Definition {
    name: "combo_box",
    code: 61,
    properties: COMBO_BOX_PROPERTIES,
    fields: &COMBO_BOX_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::DEFAULT,
};

const TEXT_SELECT_PROPERTIES: &[Property] = &[
    integer("vscope", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("x", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("y", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("w", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("h", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("sendkey", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("sta", Storage::U8, 0, 3, DefaultValue::Integer(1)),
    background_image("pic", "sta", 2, false),
    background_image("picc", "sta", 0, true),
    integer("style", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("borderc", Storage::U16, 0, 65535, DefaultValue::Integer(0)),
    integer("borderw", Storage::U8, 0, 255, DefaultValue::Integer(2)),
    integer("font", Storage::Font, 0, 254, DefaultValue::Required),
    integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(65535)),
    integer("pco", Storage::U16, 0, 65535, DefaultValue::Integer(0)),
    integer("pco2", Storage::U16, 0, 65535, DefaultValue::Integer(63488)),
    integer("spax", Storage::U8, 0, 255, DefaultValue::Integer(0)),
    integer("hig", Storage::U8, 16, 255, DefaultValue::Integer(72)),
    integer("dis", Storage::U8, 0, 1, DefaultValue::Integer(1)),
    integer("pco1", Storage::U16, 0, 65535, DefaultValue::Integer(31)),
    integer("val", Storage::U32, 0, 4294967295, DefaultValue::Integer(0)),
    integer("ch", Storage::U8, 0, 32, DefaultValue::Integer(8)),
    integer("path", Storage::Lines, 0, 0, DefaultValue::Text("")),
    integer("path_m", Storage::U16, 0, 65535, DefaultValue::Integer(64)),
];

const TEXT_SELECT_FIELDS: [Field; 41] = {
    let mut f = drawable_fields();
    f[21] = field("sta", FieldValue::Property("sta"));
    f[22] = field("style", FieldValue::Property("style"));
    f[23] = field("borderc", FieldValue::Property("borderc"));
    f[24] = field("borderw", FieldValue::Property("borderw"));
    f[25] = field("font", FieldValue::Property("font"));
    f[26] = field("bco", FieldValue::Property("bco"));
    f[27] = field("picc", FieldValue::Property("picc"));
    f[28] = field("pic", FieldValue::Property("pic"));
    f[29] = field("pco", FieldValue::Property("pco"));
    f[30] = field("pco2", FieldValue::Property("pco2"));
    f[31] = field("spax", FieldValue::Property("spax"));
    f[32] = field("hig", FieldValue::Property("hig"));
    f[33] = field("dis", FieldValue::Property("dis"));
    f[34] = field("pco1", FieldValue::Property("pco1"));
    // 这是原生缓存，不根据源 val／path 推导当前文本。
    f[35] = field("txt", FieldValue::Constant(&[]));
    f[36] = field("txt_maxl", FieldValue::Constant(&[255, 0]));
    f[37] = field("val", FieldValue::Property("val"));
    f[38] = field("ch", FieldValue::Property("ch"));
    f[39] = field("path", FieldValue::Property("path"));
    f[40] = field("path_m", FieldValue::Property("path_m"));
    f
};

pub(super) const TEXT_SELECT: Definition = Definition {
    name: "text_select",
    code: 68,
    properties: TEXT_SELECT_PROPERTIES,
    fields: &TEXT_SELECT_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::readonly(&["txt"]),
};

const SLIDING_TEXT_PROPERTIES: &[Property] = &[
    integer("vscope", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("x", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("y", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("w", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("h", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("sendkey", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("sta", Storage::U8, 0, 3, DefaultValue::Integer(1)),
    background_image("pic", "sta", 2, false),
    background_image("picc", "sta", 0, true),
    integer("style", Storage::U8, 0, 4, DefaultValue::Integer(0)),
    integer("borderc", Storage::U16, 0, 65535, DefaultValue::Integer(0)),
    integer("borderw", Storage::U8, 0, 255, DefaultValue::Integer(2)),
    integer("font", Storage::Font, 0, 254, DefaultValue::Required),
    integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(65535)),
    integer("pco", Storage::U16, 0, 65535, DefaultValue::Integer(0)),
    integer("xcen", Storage::U8, 0, 2, DefaultValue::Integer(0)),
    integer("left", Storage::U8, 0, 2, DefaultValue::Integer(1)),
    integer("ch", Storage::U8, 0, 32, DefaultValue::Integer(8)),
    integer("txt", Storage::Lines, 0, 0, DefaultValue::Required),
    integer(
        "txt_maxl",
        Storage::U16,
        0,
        65535,
        DefaultValue::Integer(64),
    ),
    integer("isbr", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("spax", Storage::U8, 0, 255, DefaultValue::Integer(0)),
    integer("spay", Storage::U8, 0, 255, DefaultValue::Integer(0)),
    integer("path_m", Storage::U16, 0, 65535, DefaultValue::Integer(0)),
];

const SLIDING_TEXT_FIELDS: [Field; 44] = {
    let mut f = drawable_fields();
    f[21] = field("sta", FieldValue::Property("sta"));
    f[22] = field("style", FieldValue::Property("style"));
    f[23] = field("key", FieldValue::Constant(&[255]));
    f[24] = field("borderc", FieldValue::Property("borderc"));
    f[25] = field("borderw", FieldValue::Property("borderw"));
    f[26] = field("font", FieldValue::Property("font"));
    f[27] = field("bco", FieldValue::Property("bco"));
    f[28] = field("picc", FieldValue::Property("picc"));
    f[29] = field("pic", FieldValue::Property("pic"));
    f[30] = field("pco", FieldValue::Property("pco"));
    f[31] = field("xcen", FieldValue::Property("xcen"));
    f[32] = field("leftshow", FieldValue::Constant(&[0]));
    f[33] = field("left", FieldValue::Property("left"));
    f[34] = field("ch", FieldValue::Property("ch"));
    f[35] = field("txt", FieldValue::Property("txt"));
    f[36] = field("txt_maxl", FieldValue::Property("txt_maxl"));
    f[37] = field("isbr", FieldValue::Property("isbr"));
    f[38] = field("spax", FieldValue::Property("spax"));
    f[39] = field("spay", FieldValue::Property("spay"));
    f[40] = field("path", FieldValue::Constant(&[]));
    f[41] = field("path_m", FieldValue::Property("path_m"));
    // 最大滚动位置由运行时计算，初始位置固定为零。
    f[42] = field("maxval_y", FieldValue::Constant(&[0, 0, 0, 0]));
    f[43] = field("val_y", FieldValue::Constant(&[0, 0, 0, 0]));
    f
};

pub(super) const SLIDING_TEXT: Definition = Definition {
    name: "sliding_text",
    code: 62,
    properties: SLIDING_TEXT_PROPERTIES,
    fields: &SLIDING_TEXT_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::readonly(&["maxval_y"]),
};
