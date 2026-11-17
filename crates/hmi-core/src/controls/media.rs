//! 三类播放控件共用条件媒体及路径；模式切换不清除非活动配置。
//! tim/stim/qty 不从媒体信息回填。
use super::*;

const FIELDS: [Field; 33] = {
    let mut f = drawable_fields();
    f[21] = field("from", FieldValue::Property("from"));
    f[22] = field("vid", FieldValue::Property("vid"));
    f[23] = field("path", FieldValue::Property("path"));
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
    presentation: Presentation::Control,
    code: 2,
    properties: &[
        SCOPE,
        integer("x", Storage::U16, 0, 65535, DefaultValue::Required),
        integer("y", Storage::U16, 0, 65535, DefaultValue::Required),
        integer("w", Storage::U16, 1, 65535, DefaultValue::Required),
        integer("h", Storage::U16, 1, 65535, DefaultValue::Required),
        integer("sendkey", Storage::U8, 0, 1, DefaultValue::Integer(0)),
        integer("from", Storage::U8, 0, 1, DefaultValue::Integer(0)),
        playback_resource(Storage::Animation),
        playback_path("animation"),
        integer("en", Storage::U8, 0, 2, DefaultValue::Integer(0)),
        integer("loop", Storage::U8, 0, 1, DefaultValue::Integer(0)),
        integer("dis", Storage::U16, 10, 1000, DefaultValue::Integer(100)),
        integer("tim", Storage::U32, 0, 4294967295, DefaultValue::Integer(0)),
    ],
    fields: &FIELDS,
    events: &["down", "up", "playend"],
    access: PropertyAccess::readonly(&["stim", "qty", "molloc_s", "molloc"]),
};

const VIDEO_FIELDS: [Field; 34] = {
    let mut f = drawable_fields();
    f[21] = field("from", FieldValue::Property("from"));
    f[22] = field("vid", FieldValue::Property("vid"));
    f[23] = field("path", FieldValue::Property("path"));
    f[24] = field("path_m", FieldValue::Constant(&[255, 0]));
    f[25] = field("en", FieldValue::Property("en"));
    f[26] = field("loop", FieldValue::Property("loop"));
    // 原生保存值为 1，含义不等同于 VIDEO 资源帧率，不按编码参数回填。
    f[27] = field("fps", FieldValue::Constant(&[1]));
    f[28] = field("dis", FieldValue::Property("dis"));
    f[29] = field("tim", FieldValue::Property("tim"));
    f[30] = field("stim", FieldValue::Constant(&[0; 4]));
    f[31] = field("qty", FieldValue::Constant(&[0; 4]));
    f[32] = field("molloc_s", FieldValue::Constant(&[0, 250, 0, 0]));
    f[33] = field("molloc", FieldValue::Constant(&[0, 250, 0, 0]));
    f
};
pub(super) const VIDEO: Definition = Definition {
    name: "video",
    presentation: Presentation::Control,
    code: 3,
    properties: &[
        SCOPE,
        integer("x", Storage::U16, 0, 65535, DefaultValue::Required),
        integer("y", Storage::U16, 0, 65535, DefaultValue::Required),
        integer("w", Storage::U16, 1, 65535, DefaultValue::Required),
        integer("h", Storage::U16, 1, 65535, DefaultValue::Required),
        integer("sendkey", Storage::U8, 0, 1, DefaultValue::Integer(0)),
        integer("from", Storage::U8, 0, 1, DefaultValue::Integer(0)),
        playback_resource(Storage::Video),
        playback_path("video"),
        integer("en", Storage::U8, 0, 2, DefaultValue::Integer(0)),
        integer("loop", Storage::U8, 0, 1, DefaultValue::Integer(0)),
        integer("dis", Storage::U16, 10, 1000, DefaultValue::Integer(100)),
        integer("tim", Storage::U32, 0, 4294967295, DefaultValue::Integer(0)),
    ],
    fields: &VIDEO_FIELDS,
    events: &["down", "up", "playend"],
    access: PropertyAccess::readonly(&["fps", "stim", "qty", "molloc_s", "molloc"]),
};

pub(super) const AUDIO: Definition = Definition {
    name: "audio",
    presentation: Presentation::None,
    code: 4,
    properties: &[
        SCOPE,
        integer("from", Storage::U8, 0, 1, DefaultValue::Integer(0)),
        playback_resource(Storage::Audio),
        playback_path("audio"),
        integer("en", Storage::U8, 0, 2, DefaultValue::Integer(0)),
        integer("loop", Storage::U8, 0, 1, DefaultValue::Integer(0)),
        integer("dis", Storage::U16, 10, 1000, DefaultValue::Integer(100)),
        integer("tim", Storage::U32, 0, 4294967295, DefaultValue::Integer(0)),
    ],
    fields: &[
        field("type", FieldValue::Type),
        field("id", FieldValue::Id),
        field("objname", FieldValue::Name),
        field("vscope", FieldValue::Property("vscope")),
        field("lockobj", FieldValue::Constant(&[0])),
        field("groupid0", FieldValue::Constant(&[0; 4])),
        field("groupid1", FieldValue::Constant(&[0; 4])),
        field("from", FieldValue::Property("from")),
        field("vid", FieldValue::Property("vid")),
        field("path", FieldValue::Property("path")),
        field("path_m", FieldValue::Constant(&[255, 0])),
        field("en", FieldValue::Property("en")),
        field("loop", FieldValue::Property("loop")),
        field("fps", FieldValue::Constant(&[1])),
        field("dis", FieldValue::Property("dis")),
        field("tim", FieldValue::Property("tim")),
        field("stim", FieldValue::Constant(&[0; 4])),
        field("qty", FieldValue::Constant(&[0; 4])),
        field("molloc_s", FieldValue::Constant(&[0, 250, 0, 0])),
        field("molloc", FieldValue::Constant(&[0, 250, 0, 0])),
    ],
    events: &["playend"],
    access: PropertyAccess::readonly(&["fps", "stim", "qty", "molloc_s", "molloc"]),
};

// vid 和 path 均完整保存；from=0 才要求内部资源，from=1 才要求外部路径。
const fn playback_resource(storage: Storage) -> Property {
    Property {
        name: "vid",
        storage,
        min: 0,
        max: 65535,
        default: DefaultValue::Integer(65535),
        resource_rule: Some(ResourceRule {
            active_when: Some(("from", 0)),
            crop: false,
        }),
    }
}
const fn playback_path(kind: &'static str) -> Property {
    Property {
        name: "path",
        storage: Storage::ExternalPath(kind),
        min: 0,
        max: 0,
        default: DefaultValue::Text(""),
        resource_rule: Some(ResourceRule {
            active_when: Some(("from", 1)),
            crop: false,
        }),
    }
}
