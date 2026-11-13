//! 无媒体依赖的指针、空数据波形和无中心 LOGO 的二维码。
use super::*;

const fn geometry_properties<const N: usize>() -> [Property; N] {
    let mut p = [SCOPE; N];
    let mut i = 0;
    while i < 6 {
        p[i] = HOTSPOT_PROPERTIES[i];
        i += 1;
    }
    p
}

const GAUGE_PROPERTIES: [Property; 17] = {
    let mut p = geometry_properties();
    p[6] = integer("sta", Storage::U8, 1, 1, DefaultValue::Integer(1));
    p[7] = integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(65535));
    p[8] = integer("val", Storage::U16, 0, 360, DefaultValue::Integer(0));
    // 尖长和尾长允许负数，必须按有符号两字节编码，32767 为自动尖长。
    p[9] = integer(
        "up",
        Storage::I16,
        -32768,
        32767,
        DefaultValue::Integer(32767),
    );
    p[10] = integer(
        "down",
        Storage::I16,
        -32768,
        32767,
        DefaultValue::Integer(30),
    );
    p[11] = integer("pco", Storage::U16, 0, 65535, DefaultValue::Integer(0));
    p[12] = integer("pco2", Storage::U16, 0, 65535, DefaultValue::Integer(63488));
    p[13] = integer("hig", Storage::U8, 0, 100, DefaultValue::Integer(24));
    p[14] = integer("vvs0", Storage::U8, 0, 128, DefaultValue::Integer(1));
    p[15] = integer("vvs1", Storage::U8, 0, 128, DefaultValue::Integer(7));
    p[16] = integer("vvs2", Storage::U8, 0, 128, DefaultValue::Integer(2));
    p
};
const GAUGE_FIELDS: [Field; 37] = {
    let mut f = drawable_fields();
    f[21] = field("sta", FieldValue::Property("sta"));
    f[22] = field("picc", FieldValue::Constant(&[255, 255]));
    f[23] = field("bco", FieldValue::Property("bco"));
    f[24] = field("pic", FieldValue::Constant(&[255, 255]));
    f[25] = field("val", FieldValue::Property("val"));
    f[26] = field("format", FieldValue::Constant(&[0, 0]));
    f[27] = field("up", FieldValue::Property("up"));
    f[28] = field("down", FieldValue::Property("down"));
    f[29] = field("left", FieldValue::Constant(&[0, 0]));
    f[30] = field("pco", FieldValue::Property("pco"));
    f[31] = field("pco2", FieldValue::Property("pco2"));
    f[32] = field("hig", FieldValue::Property("hig"));
    // 本批只开放已确认的形状属性，其他原生字段按配方初始化。
    f[33] = field("wid", FieldValue::Constant(&[2]));
    f[34] = field("vvs0", FieldValue::Property("vvs0"));
    f[35] = field("vvs1", FieldValue::Property("vvs1"));
    f[36] = field("vvs2", FieldValue::Property("vvs2"));
    f
};

const WAVE_PROPERTIES: [Property; 18] = {
    let mut p = geometry_properties();
    p[6] = integer("sta", Storage::U8, 1, 1, DefaultValue::Integer(1));
    p[7] = integer("dir", Storage::U8, 0, 2, DefaultValue::Integer(0));
    p[8] = integer("ch", Storage::U8, 1, 4, DefaultValue::Integer(1));
    p[9] = integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(0));
    p[10] = integer("gdc", Storage::U16, 0, 65535, DefaultValue::Integer(1024));
    p[11] = integer("gdw", Storage::U8, 0, 255, DefaultValue::Integer(40));
    p[12] = integer("gdh", Storage::U8, 0, 255, DefaultValue::Integer(40));
    p[13] = integer(
        "pco0",
        Storage::U16,
        0,
        65535,
        DefaultValue::Integer(0xFBEF),
    );
    p[14] = integer(
        "pco1",
        Storage::U16,
        0,
        65535,
        DefaultValue::Integer(0xFFEF),
    );
    p[15] = integer("pco2", Storage::U16, 0, 65535, DefaultValue::Integer(63488));
    p[16] = integer("pco3", Storage::U16, 0, 65535, DefaultValue::Integer(2016));
    p[17] = integer("dis", Storage::U16, 10, 1000, DefaultValue::Integer(100));
    p
};
const WAVE_FIELDS: [Field; 40] = {
    let mut f = drawable_fields();
    f[21] = field("sta", FieldValue::Property("sta"));
    f[22] = field("dir", FieldValue::Property("dir"));
    f[23] = field("ch", FieldValue::Property("ch"));
    f[24] = field("bco", FieldValue::Property("bco"));
    f[25] = field("picc", FieldValue::Constant(&[255, 255]));
    f[26] = field("pic", FieldValue::Constant(&[255, 255]));
    f[27] = field("gdc", FieldValue::Property("gdc"));
    f[28] = field("gdw", FieldValue::Property("gdw"));
    f[29] = field("gdh", FieldValue::Property("gdh"));
    f[30] = field("objWid", FieldValue::Constant(&[0, 0]));
    f[31] = field("objHig", FieldValue::Constant(&[0, 0]));
    // 四个通道的颜色始终保存，不随当前 ch 删除非活动通道字段。
    f[32] = field("pco0", FieldValue::Property("pco0"));
    f[33] = field("pco1", FieldValue::Property("pco1"));
    f[34] = field("pco2", FieldValue::Property("pco2"));
    f[35] = field("pco3", FieldValue::Property("pco3"));
    f[36] = field("inittrue", FieldValue::Constant(&[0]));
    f[37] = field("dis", FieldValue::Property("dis"));
    f[38] = field("molloc_s", FieldValue::Constant(&[0; 4]));
    f[39] = field("molloc", FieldValue::Constant(&[0; 4]));
    f
};

const QR_PROPERTIES: [Property; 11] = {
    let mut p = geometry_properties();
    // QR 的 sta 是 LOGO 开关，与背景模式不是同一概念。
    p[6] = integer("sta", Storage::U8, 0, 0, DefaultValue::Integer(0));
    p[7] = integer("bco", Storage::U16, 0, 65535, DefaultValue::Integer(65535));
    p[8] = integer("pco", Storage::U16, 0, 65535, DefaultValue::Integer(0));
    p[9] = Property {
        name: "txt",
        storage: Storage::Text,
        min: 0,
        max: 0,
        default: DefaultValue::Required,
    };
    p[10] = integer("txt_maxl", Storage::U16, 0, 192, DefaultValue::Integer(16));
    p
};
const QR_FIELDS: [Field; 28] = {
    let mut f = drawable_fields();
    f[21] = field("sta", FieldValue::Property("sta"));
    f[22] = field("dis", FieldValue::Constant(&[8]));
    f[23] = field("bco", FieldValue::Property("bco"));
    f[24] = field("pco", FieldValue::Property("pco"));
    f[25] = field("pic", FieldValue::Constant(&[255, 255]));
    f[26] = field("txt", FieldValue::Property("txt"));
    f[27] = field("txt_maxl", FieldValue::Property("txt_maxl"));
    f
};

pub(super) const GAUGE: Definition = Definition {
    name: "gauge",
    code: 122,
    properties: &GAUGE_PROPERTIES,
    fields: &GAUGE_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::DEFAULT,
};
pub(super) const WAVEFORM: Definition = Definition {
    name: "waveform",
    code: 0,
    properties: &WAVE_PROPERTIES,
    fields: &WAVE_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::DEFAULT,
};
pub(super) const QRCODE: Definition = Definition {
    name: "qrcode",
    code: 58,
    properties: &QR_PROPERTIES,
    fields: &QR_FIELDS,
    events: &["down", "up"],
    access: PropertyAccess::DEFAULT,
};
