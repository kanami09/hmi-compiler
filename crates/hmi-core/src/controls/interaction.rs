//! 交互控件的存储配方；运行行为仍由控件和用户脚本决定。
use super::*;

const SLIDER_PROPERTIES: [Property; 24] = {
    let mut p = [SCOPE; 24];
    let mut i = 0;
    while i < 6 {
        p[i] = HOTSPOT_PROPERTIES[i];
        i += 1;
    }
    p[6] = integer("mode", Storage::U8, 0, 1, DefaultValue::Integer(0));
    p[7] = integer("sta", Storage::U8, 0, 2, DefaultValue::Integer(1));
    // 共同存储范围；各型号当前接入的游标模式由目标配置进一步约束。
    p[8] = integer("psta", Storage::U8, 0, 2, DefaultValue::Integer(0));
    p[9] = integer("wid", Storage::U8, 0, 255, DefaultValue::Integer(255));
    p[10] = integer("hig", Storage::U8, 0, 255, DefaultValue::Integer(255));
    p[11] = integer("dis", Storage::U8, 0, 100, DefaultValue::Integer(100));
    p[12] = integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(50712));
    p[13] = integer("bco1", Storage::U16, 0, 65535, DefaultValue::Integer(1024));
    p[14] = integer("pco", Storage::U16, 0, 65535, DefaultValue::Integer(63488));
    p[15] = integer("val", Storage::U16, 0, 65535, DefaultValue::Integer(0));
    p[16] = integer("maxval", Storage::U16, 0, 65535, DefaultValue::Integer(100));
    p[17] = integer("minval", Storage::U16, 0, 65535, DefaultValue::Integer(0));
    p[18] = integer("ch", Storage::U8, 0, 100, DefaultValue::Integer(25));
    p[19] = background_image("pic", "sta", 2, false);
    p[20] = background_image("pic1", "sta", 2, false);
    p[21] = background_image("picc", "sta", 0, true);
    p[22] = background_image("picc1", "sta", 0, true);
    p[23] = background_image("pic2", "psta", 1, false);
    p
};
const SLIDER_FIELDS: [Field; 39] = {
    let mut f = drawable_fields();
    f[21] = field("mode", FieldValue::Property("mode"));
    f[22] = field("sta", FieldValue::Property("sta"));
    f[23] = field("psta", FieldValue::Property("psta"));
    f[24] = field("wid", FieldValue::Property("wid"));
    f[25] = field("hig", FieldValue::Property("hig"));
    f[26] = field("dis", FieldValue::Property("dis"));
    f[27] = field("pic", FieldValue::Property("pic"));
    f[28] = field("picc", FieldValue::Property("picc"));
    f[29] = field("bco", FieldValue::Property("bco"));
    f[30] = field("pic1", FieldValue::Property("pic1"));
    f[31] = field("picc1", FieldValue::Property("picc1"));
    f[32] = field("bco1", FieldValue::Property("bco1"));
    f[33] = field("pic2", FieldValue::Property("pic2"));
    f[34] = field("pco", FieldValue::Property("pco"));
    f[35] = field("val", FieldValue::Property("val"));
    f[36] = field("maxval", FieldValue::Property("maxval"));
    f[37] = field("minval", FieldValue::Property("minval"));
    f[38] = field("ch", FieldValue::Property("ch"));
    f
};

const DUAL_PROPERTIES: [Property; 27] = {
    let mut p = BUTTON_PROPERTIES;
    p[7] = integer("style", Storage::U8, 0, 4, DefaultValue::Integer(4));
    p
};
// 两种按钮布局相同，val 的运行语义不同；不生成额外状态切换脚本。
pub(super) const DUAL_STATE_BUTTON: Definition = Definition {
    name: "dual_state_button",
    code: 53,
    properties: &DUAL_PROPERTIES,
    fields: &BUTTON_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::DEFAULT,
};

const SCROLL_PROPERTIES: [Property; 26] = {
    let mut p = [SCOPE; 26];
    let mut i = 0;
    while i < 20 {
        p[i] = TEXT_PROPERTIES[i];
        i += 1;
    }
    p[6] = integer("sta", Storage::U8, 0, 3, DefaultValue::Integer(1));
    p[7] = integer("style", Storage::U8, 0, 4, DefaultValue::Integer(0));
    p[13] = integer("xcen", Storage::U8, 0, 2, DefaultValue::Integer(0));
    p[16] = integer(
        "txt_maxl",
        Storage::U16,
        0,
        65535,
        DefaultValue::Integer(64),
    );
    p[20] = integer("dir", Storage::U8, 0, 3, DefaultValue::Integer(1));
    p[21] = integer("dis", Storage::U8, 2, 50, DefaultValue::Integer(4));
    p[22] = integer("tim", Storage::U16, 80, 65534, DefaultValue::Integer(80));
    p[23] = integer("en", Storage::U8, 0, 1, DefaultValue::Integer(1));
    p[24] = background_image("pic", "sta", 2, false);
    p[25] = background_image("picc", "sta", 0, true);
    p
};
const SCROLL_FIELDS: [Field; 46] = {
    let mut f = drawable_fields();
    let mut i = 21;
    while i < 33 {
        f[i] = TEXT_FIELDS[i];
        i += 1;
    }
    f[33] = field("dir", FieldValue::Property("dir"));
    f[34] = field("dis", FieldValue::Property("dis"));
    f[35] = field("tim", FieldValue::Property("tim"));
    f[36] = field("en", FieldValue::Property("en"));
    f[37] = field("txt", FieldValue::Property("txt"));
    f[38] = field("txt_maxl", FieldValue::Property("txt_maxl"));
    f[39] = field("isbr", FieldValue::Property("isbr"));
    f[40] = field("spax", FieldValue::Property("spax"));
    f[41] = field("spay", FieldValue::Property("spay"));
    // 四个原生保留字段没有已确认的编辑语义，固定初始化为零。
    f[42] = field("vvs0", FieldValue::Constant(&[0, 0]));
    f[43] = field("vvs1", FieldValue::Constant(&[0, 0]));
    f[44] = field("vvs2", FieldValue::Constant(&[0, 0]));
    f[45] = field("vvs3", FieldValue::Constant(&[0, 0]));
    f
};
const TOUCH_FIELDS: [Field; 8] = [
    field("type", FieldValue::Type),
    field("id", FieldValue::Id),
    field("objname", FieldValue::Name),
    field("vscope", FieldValue::Property("vscope")),
    field("lockobj", FieldValue::Constant(&[0])),
    field("groupid0", FieldValue::Constant(&[0; 4])),
    field("groupid1", FieldValue::Constant(&[0; 4])),
    field("val", FieldValue::Constant(&[0])),
];

pub(super) const SLIDER: Definition = Definition {
    name: "slider",
    code: 1,
    properties: &SLIDER_PROPERTIES,
    fields: &SLIDER_FIELDS,
    events: &["down", "up", "slide"],
    access: PropertyAccess::DEFAULT,
};
pub(super) const TOUCH_CAPTURE: Definition = Definition {
    name: "touch_cap",
    code: 5,
    properties: &[SCOPE],
    fields: &TOUCH_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::readonly(&["val"]),
};
pub(super) const SCROLL_TEXT: Definition = Definition {
    name: "scrolling_text",
    code: 55,
    properties: &SCROLL_PROPERTIES,
    fields: &SCROLL_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::DEFAULT,
};
