//! 动画只写播放配置，tim/stim/qty 不从媒体信息回填。
use super::*;

const FIELDS: [Field; 33] = {
    let mut f = drawable_fields();
    f[21] = field("from", FieldValue::Property("from"));
    f[22] = field("vid", FieldValue::Property("vid"));
    f[23] = field("path", FieldValue::Constant(&[]));
    f[24] = field("path_m", FieldValue::Constant(&[255, 0]));
    f[25] = field("en", FieldValue::Property("en"));
    f[26] = field("loop", FieldValue::Property("loop"));
    f[27] = field("dis", FieldValue::Property("dis"));
    f[28] = field("tim", FieldValue::Property("tim"));
    f[29] = field("stim", FieldValue::Constant(&[0; 4]));
    f[30] = field("qty", FieldValue::Constant(&[0; 4]));
    // 原生保留值为 12000，不按文件大小或帧数重新估算。
    f[31] = field("molloc_s", FieldValue::Constant(&[224, 46, 0, 0]));
    f[32] = field("molloc", FieldValue::Constant(&[224, 46, 0, 0]));
    f
};
pub(super) const ANIMATION: Definition = Definition {
    name: "animation",
    code: 2,
    properties: &[
        SCOPE,
        integer("x", Storage::U16, 0, 65535, DefaultValue::Required),
        integer("y", Storage::U16, 0, 65535, DefaultValue::Required),
        integer("w", Storage::U16, 1, 65535, DefaultValue::Required),
        integer("h", Storage::U16, 1, 65535, DefaultValue::Required),
        integer("sendkey", Storage::U8, 0, 1, DefaultValue::Integer(0)),
        integer("from", Storage::U8, 0, 0, DefaultValue::Integer(0)),
        integer("vid", Storage::Animation, 0, 65534, DefaultValue::Required),
        integer("en", Storage::U8, 0, 2, DefaultValue::Integer(0)),
        integer("loop", Storage::U8, 0, 1, DefaultValue::Integer(0)),
        integer("dis", Storage::U16, 10, 1000, DefaultValue::Integer(100)),
        integer("tim", Storage::U32, 0, 4294967295, DefaultValue::Integer(0)),
    ],
    fields: &FIELDS,
    events: &["down", "up", "playend"],
    access: PropertyAccess::readonly(&["stim", "qty", "molloc_s", "molloc"]),
};
