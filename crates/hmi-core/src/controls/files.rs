//! 文件浏览器使用工程显式声明的图标；文件流初始未打开，不访问本机文件系统。
use super::*;

const BROWSER_PROPERTIES: &[Property] = &[
    integer("vscope", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("x", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("y", Storage::U16, 0, 65535, DefaultValue::Required),
    integer("w", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("h", Storage::U16, 1, 65535, DefaultValue::Required),
    integer("sendkey", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("sta", Storage::U8, 1, 1, DefaultValue::Integer(1)),
    integer("style", Storage::U8, 0, 4, DefaultValue::Integer(2)),
    integer("borderc", Storage::U16, 0, 65535, DefaultValue::Integer(0)),
    integer("borderw", Storage::U8, 0, 255, DefaultValue::Integer(2)),
    integer("font", Storage::Font, 0, 254, DefaultValue::Required),
    integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(65535)),
    integer("pco", Storage::U16, 0, 65535, DefaultValue::Integer(0)),
    integer("pco2", Storage::U16, 0, 65535, DefaultValue::Integer(65535)),
    integer("bco2", Storage::U16, 0, 65535, DefaultValue::Integer(31)),
    integer("xcen", Storage::U8, 0, 2, DefaultValue::Integer(1)),
    integer("left", Storage::U8, 0, 2, DefaultValue::Integer(1)),
    integer("ch", Storage::U8, 0, 32, DefaultValue::Integer(8)),
    integer("dir", Storage::Text, 0, 0, DefaultValue::Text("sd0/")),
    integer("filter", Storage::Text, 0, 0, DefaultValue::Text("*")),
    integer("dis", Storage::U8, 0, 255, DefaultValue::Integer(3)),
    integer("spax", Storage::U8, 0, 255, DefaultValue::Integer(24)),
    integer("spay", Storage::U8, 0, 255, DefaultValue::Integer(8)),
    integer("psta", Storage::U8, 0, 1, DefaultValue::Integer(0)),
    integer("pic1", Storage::Image, 0, 65534, DefaultValue::Required),
    integer("pic2", Storage::Image, 0, 65534, DefaultValue::Required),
    integer(
        "buffsize",
        Storage::U32,
        0,
        4294967295,
        DefaultValue::Integer(32000),
    ),
    integer("fwarning", Storage::U8, 0, 1, DefaultValue::Integer(1)),
];

const BROWSER_FIELDS: [Field; 58] = {
    let mut f = drawable_fields();
    f[21] = field("sta", FieldValue::Property("sta"));
    f[22] = field("style", FieldValue::Property("style"));
    f[23] = field("borderc", FieldValue::Property("borderc"));
    f[24] = field("borderw", FieldValue::Property("borderw"));
    f[25] = field("font", FieldValue::Property("font"));
    f[26] = field("bco", FieldValue::Property("bco"));
    f[27] = field("picc", FieldValue::Constant(&[255, 255]));
    f[28] = field("pic", FieldValue::Constant(&[255, 255]));
    f[29] = field("pco", FieldValue::Property("pco"));
    f[30] = field("pco2", FieldValue::Property("pco2"));
    f[31] = field("bco2", FieldValue::Property("bco2"));
    f[32] = field("xcen", FieldValue::Property("xcen"));
    f[33] = field("autoleft", FieldValue::Constant(&[0]));
    f[34] = field("left", FieldValue::Property("left"));
    f[35] = field("ch", FieldValue::Property("ch"));
    f[36] = field("dir", FieldValue::Property("dir"));
    f[37] = field("dir_m", FieldValue::Constant(&[255, 0]));
    f[38] = field("filter", FieldValue::Property("filter"));
    f[39] = field("filter_m", FieldValue::Constant(&[64, 0]));
    // 构建时不遍历目录，保留原生初始缓存，不根据本机文件系统填值。
    f[40] = field("val", FieldValue::Constant(&[255, 255, 255, 255]));
    f[41] = field("txt", FieldValue::Constant(&[]));
    f[42] = field("txt_m", FieldValue::Constant(&[255, 0]));
    f[43] = field("qty", FieldValue::Constant(&[0, 0, 0, 0]));
    f[44] = field("dis", FieldValue::Property("dis"));
    f[45] = field("spax", FieldValue::Property("spax"));
    f[46] = field("spay", FieldValue::Property("spay"));
    f[47] = field("maxval_y", FieldValue::Constant(&[0, 0, 0, 0]));
    f[48] = field("val_y", FieldValue::Constant(&[0, 0, 0, 0]));
    f[49] = field("psta", FieldValue::Property("psta"));
    f[50] = field("pic1", FieldValue::Property("pic1"));
    f[51] = field("pic2", FieldValue::Property("pic2"));
    f[52] = field("vvs2", FieldValue::Constant(&[]));
    f[53] = field("vvs2_m", FieldValue::Constant(&[64, 0]));
    f[54] = field("buffsize", FieldValue::Property("buffsize"));
    f[55] = field("buff", FieldValue::Constant(&[0, 0, 0, 0]));
    f[56] = field("up", FieldValue::Constant(&[0, 0, 0, 0]));
    f[57] = field("fwarning", FieldValue::Property("fwarning"));
    f
};

const STREAM_FIELDS: [Field; 17] = [
    field("type", FieldValue::Type),
    field("id", FieldValue::Id),
    field("objname", FieldValue::Name),
    field("vscope", FieldValue::Property("vscope")),
    field("lockobj", FieldValue::Constant(&[0])),
    field("groupid0", FieldValue::Constant(&[0, 0, 0, 0])),
    field("groupid1", FieldValue::Constant(&[0, 0, 0, 0])),
    field("val", FieldValue::Constant(&[0, 0, 0, 0])),
    field("qty", FieldValue::Constant(&[0, 0, 0, 0])),
    field("en", FieldValue::Constant(&[0])),
    // 原生方法分派字段，不作为用户可改的数值或事件槽。
    field("open", FieldValue::Constant(&[101, 1, 0, 0])),
    field("read", FieldValue::Constant(&[101, 3, 0, 0])),
    field("write", FieldValue::Constant(&[101, 3, 0, 0])),
    field("close", FieldValue::Constant(&[101, 0, 0, 0])),
    field("find", FieldValue::Constant(&[101, 1, 0, 0])),
    // 已审查配方中为 10752，不能因看似缓存就清零。
    field("molloc_s", FieldValue::Constant(&[0, 42, 0, 0])),
    field("molloc", FieldValue::Constant(&[0, 42, 0, 0])),
];

pub(super) const FILE_BROWSER: Definition = Definition {
    name: "file_browser",
    code: 65,
    properties: BROWSER_PROPERTIES,
    fields: &BROWSER_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess {
        readonly: &["txt", "qty", "maxval_y", "buff", "up"],
        page_local: &["txt"],
        compile_only: &["fwarning"],
    },
};

pub(super) const FILE_STREAM: Definition = Definition {
    name: "file_stream",
    code: 63,
    properties: &[integer(
        "vscope",
        Storage::U8,
        0,
        0,
        DefaultValue::Integer(0),
    )],
    fields: &STREAM_FIELDS,
    events: &[],
    access: PropertyAccess::readonly(&[
        "qty", "en", "open", "read", "write", "close", "find", "molloc_s", "molloc",
    ]),
};
