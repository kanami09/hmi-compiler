//! `.hma` 自包含媒体目标：兼容约束、成员描述和编码载荷，不引用源文件。
use hmi_core::{
    Result,
    asset::{Alpha, Delivery, Resize},
    error, file,
    model::check_schema,
    sha256,
    target::Target,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
const MAGIC: &[u8; 8] = b"HMIASS01";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ImageInfo {
    #[serde(default, skip_serializing_if = "Delivery::is_embedded")]
    pub delivery: Delivery,
    pub width: u16,
    pub height: u16,
    pub storage_width: u16,
    pub storage_height: u16,
    pub orientation: u16,
    pub codec: String,
    pub alpha: Alpha,
    pub background: Option<String>,
    pub resize: Option<Resize>,
    pub source_format: String,
    pub editor_source_format: String,
    pub source_sha256: String,
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jpeg: Option<JpegInfo>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JpegInfo {
    pub quality: u8,
    pub padded_width: u16,
    pub padded_height: u16,
    pub sampling: String,
    #[serde(default)]
    pub has_alpha: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FontInfo {
    pub version: u8,
    pub encoding: String,
    pub height: u8,
    pub glyph_count: u32,
    pub orientation: u16,
    pub source_orientation: u16,
    pub source_sha256: String,
    pub backend: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AnimationInfo {
    pub width: u16,
    pub height: u16,
    pub orientation: u16,
    pub quality: u8,
    pub alpha: Alpha,
    pub background: Option<String>,
    pub timing: String,
    pub source_frames: u32,
    pub source_duration_ms: u32,
    pub frame_count: u32,
    pub duration_ms: u32,
    pub timing_adjusted: bool,
    pub frame_indices: Vec<u32>,
    pub delays_ms: Vec<u16>,
    pub has_alpha: bool,
    pub source_sha256: String,
    pub backend: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VideoInfo {
    pub width: u16,
    pub height: u16,
    pub orientation: u16,
    pub quality: u8,
    pub fps: u8,
    pub frame_count: u32,
    pub duration_ms: u32,
    pub source_fps_num: u32,
    pub source_fps_den: u32,
    pub source_frames: u32,
    pub clip_start_ms: u32,
    pub source_audio_streams: u32,
    pub audio_mode: String,
    pub audio_applied: bool,
    pub codec: String,
    pub source_sha256: String,
    pub backend: hmi_core::asset::FfmpegInfo,
    pub encoder: String,
}

/// 每种资产只携带自己的描述，不能同时声明为图片和字库。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssetInfo {
    Image { image: ImageInfo },
    Font { font: FontInfo },
    Animation { animation: AnimationInfo },
    Video { video: VideoInfo },
}
impl AssetInfo {
    pub fn image(&self) -> Option<&ImageInfo> {
        match self {
            Self::Image { image } => Some(image),
            _ => None,
        }
    }
    pub fn orientation(&self) -> u16 {
        match self {
            Self::Image { image } => image.orientation,
            Self::Font { font } => font.orientation,
            Self::Animation { animation } => animation.orientation,
            Self::Video { video } => video.orientation,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Image { .. } => "image",
            Self::Font { .. } => "font",
            Self::Animation { .. } => "animation",
            Self::Video { .. } => "video",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Runtime,
    Source,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub role: MemberRole,
    pub extension: String,
    pub reference_kind: Option<[u8; 8]>,
    pub offset: u32,
    pub length: u32,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetMetadata {
    pub schema: String,
    pub format: String,
    pub producer: String,
    #[serde(flatten)]
    pub info: AssetInfo,
    pub members: Vec<Member>,
}
#[derive(Clone, Debug)]
pub struct AssetObject {
    pub metadata: AssetMetadata,
    pub payload: Vec<u8>,
}

impl AssetObject {
    pub fn video(info: VideoInfo, bytes: Vec<u8>) -> Result<Self> {
        let member = Member {
            role: MemberRole::Runtime,
            extension: "v".into(),
            reference_kind: Some(*b"v\0\0\0\0\0\0\0"),
            offset: 0,
            length: u32::try_from(bytes.len())
                .map_err(|_| error("asset_length", "VIDEO 运行资源超过 u32 范围"))?,
            sha256: sha256(&bytes),
        };
        let object = Self {
            metadata: AssetMetadata {
                schema: "hmi.asset-object/1".into(),
                format: "ver21234".into(),
                producer: format!("hmi-media/{}", env!("CARGO_PKG_VERSION")),
                info: AssetInfo::Video { video: info },
                members: vec![member],
            },
            payload: bytes,
        };
        object.validate()?;
        Ok(object)
    }
    pub fn animation(info: AnimationInfo, runtime: Vec<u8>, source: Vec<u8>) -> Result<Self> {
        let split = runtime.len();
        let mut payload = runtime;
        payload.extend(&source);
        let members = vec![
            Member {
                role: MemberRole::Runtime,
                extension: "gmov".into(),
                reference_kind: Some(*b"gmov\0\0\0\0"),
                offset: 0,
                length: u32::try_from(split)
                    .map_err(|_| error("asset_length", "动画资源超过 u32 范围"))?,
                sha256: sha256(&payload[..split]),
            },
            Member {
                role: MemberRole::Source,
                extension: "gmovs".into(),
                reference_kind: None,
                offset: split as u32,
                length: u32::try_from(source.len())
                    .map_err(|_| error("asset_length", "动画编辑源超过 u32 范围"))?,
                sha256: sha256(&source),
            },
        ];
        let object = Self {
            metadata: AssetMetadata {
                schema: "hmi.asset-object/1".into(),
                format: "ver21234".into(),
                producer: format!("hmi-media/{}", env!("CARGO_PKG_VERSION")),
                info: AssetInfo::Animation { animation: info },
                members,
            },
            payload,
        };
        object.validate()?;
        Ok(object)
    }
    pub fn image(info: ImageInfo, runtime: Vec<u8>, source: Vec<u8>) -> Result<Self> {
        let mut members = Vec::new();
        let mut payload = Vec::new();
        // 外部图片只携带独立 XI，不占普通图片 ID，也没有内嵌编辑源。
        let entries = if info.delivery.mode == "external" {
            vec![(
                MemberRole::Runtime,
                "xi",
                None,
                hmi_format::picture::xi::from_runtime(
                    &runtime,
                    info.width,
                    info.height,
                    info.orientation,
                )?,
            )]
        } else {
            vec![
                (
                    MemberRole::Runtime,
                    "i",
                    Some([b'i', 0, 0, 0, 0, 0, 0, 0]),
                    runtime,
                ),
                (MemberRole::Source, "is", None, source),
            ]
        };
        for (role, extension, reference_kind, bytes) in entries {
            members.push(Member {
                role,
                extension: extension.into(),
                reference_kind,
                offset: u32::try_from(payload.len())
                    .map_err(|_| error("asset_length", "资产载荷偏移超过 u32 范围"))?,
                length: u32::try_from(bytes.len())
                    .map_err(|_| error("asset_length", "资产成员超过 u32 范围"))?,
                sha256: sha256(&bytes),
            });
            payload.extend(bytes);
        }
        let asset = Self {
            metadata: AssetMetadata {
                schema: "hmi.asset-object/1".into(),
                format: "ver21234".into(),
                producer: format!("hmi-media/{}", env!("CARGO_PKG_VERSION")),
                info: AssetInfo::Image { image: info },
                members,
            },
            payload,
        };
        asset.validate()?;
        Ok(asset)
    }
    pub fn font(info: FontInfo, bytes: Vec<u8>) -> Result<Self> {
        let member = Member {
            role: MemberRole::Runtime,
            extension: "zi".into(),
            reference_kind: Some([b'z', b'i', 0, 0, 0, 0, 0, 0]),
            offset: 0,
            length: u32::try_from(bytes.len())
                .map_err(|_| error("asset_length", "字库超过 u32 范围"))?,
            sha256: sha256(&bytes),
        };
        let asset = Self {
            metadata: AssetMetadata {
                schema: "hmi.asset-object/1".into(),
                format: "ver21234".into(),
                producer: format!("hmi-media/{}", env!("CARGO_PKG_VERSION")),
                info: AssetInfo::Font { font: info },
                members: vec![member],
            },
            payload: bytes,
        };
        asset.validate()?;
        Ok(asset)
    }
    pub fn member(&self, index: usize) -> Result<&[u8]> {
        let m = self
            .metadata
            .members
            .get(index)
            .ok_or_else(|| error("asset_member", "资产成员不存在"))?;
        let end = (m.offset as usize)
            .checked_add(m.length as usize)
            .ok_or_else(|| error("asset_length", "资产成员范围溢出"))?;
        self.payload
            .get(m.offset as usize..end)
            .ok_or_else(|| error("asset_length", "资产成员超出载荷范围"))
    }
    pub fn validate(&self) -> Result<()> {
        check_schema(&self.metadata.schema, "hmi.asset-object/1")?;
        if self.metadata.format != "ver21234" {
            return Err(error("asset_format", "不支持此资产的容器格式"));
        }
        let expected: Vec<_> = match &self.metadata.info {
            AssetInfo::Video { .. } => vec![(MemberRole::Runtime, "v", Some(*b"v\0\0\0\0\0\0\0"))],
            AssetInfo::Animation { .. } => vec![
                (MemberRole::Runtime, "gmov", Some(*b"gmov\0\0\0\0")),
                (MemberRole::Source, "gmovs", None),
            ],
            AssetInfo::Image { image } if image.delivery.mode == "external" => {
                vec![(MemberRole::Runtime, "xi", None)]
            }
            AssetInfo::Image { .. } => vec![
                (MemberRole::Runtime, "i", Some([b'i', 0, 0, 0, 0, 0, 0, 0])),
                (MemberRole::Source, "is", None),
            ],
            AssetInfo::Font { .. } => vec![(
                MemberRole::Runtime,
                "zi",
                Some([b'z', b'i', 0, 0, 0, 0, 0, 0]),
            )],
        };
        if self.metadata.members.len() != expected.len() {
            return Err(error("asset_member", "资产成员数量与类型不符"));
        }
        let mut cursor = 0;
        for (i, m) in self.metadata.members.iter().enumerate() {
            let e = &expected[i];
            if (&m.role, m.extension.as_str(), m.reference_kind) != (&e.0, e.1, e.2)
                || m.offset as usize != cursor
                || m.length == 0
            {
                return Err(error("asset_member", "资产成员角色、名称或布局无效"));
            }
            let bytes = self.member(i)?;
            if sha256(bytes) != m.sha256 {
                return Err(error("asset_hash", "资产成员摘要不匹配，请重新编码"));
            }
            cursor += bytes.len();
        }
        if cursor != self.payload.len() {
            return Err(error("asset_length", "资产成员未完整覆盖载荷"));
        }
        match &self.metadata.info {
            AssetInfo::Video { video } => self.validate_video(video),
            AssetInfo::Animation { animation } => self.validate_animation(animation),
            AssetInfo::Image { image } => self.validate_image(image),
            AssetInfo::Font { font } => {
                let h = hmi_format::font::inspect(self.member(0)?)?;
                if font.version != 6
                    || font.encoding != "utf-8"
                    || font.height != h.height
                    || font.glyph_count != h.glyph_count
                    || font.orientation != h.orientation
                    || !matches!(font.source_orientation, 0 | 90 | 180 | 270)
                    || !valid_digest(&font.source_sha256)
                {
                    return Err(error("asset_metadata", "字库描述与载荷不一致"));
                }
                Ok(())
            }
        }
    }
    fn validate_video(&self, info: &VideoInfo) -> Result<()> {
        let video = hmi_format::video::inspect(self.member(0)?)?;
        if (
            info.width,
            info.height,
            info.orientation,
            info.quality,
            info.fps,
            info.frame_count,
            info.duration_ms,
        ) != (
            video.width,
            video.height,
            video.orientation,
            video.quality,
            video.fps,
            video.frames.len() as u32,
            video.duration_ms,
        ) || info.source_fps_num == 0
            || info.source_fps_den == 0
            || info.audio_applied
            || !matches!(info.audio_mode.as_str(), "drop" | "if-present")
            || (info.source_audio_streams > 0 && info.audio_mode != "drop")
            || !valid_digest(&info.source_sha256)
            || !valid_digest(&info.backend.fingerprint)
        {
            return Err(error(
                "asset_metadata",
                "视频描述与运行帧、帧率或音轨策略不一致",
            ));
        }
        let unit = u64::from(info.source_fps_den) * u64::from(info.fps);
        let source_rate = u64::from(info.source_fps_num);
        let start = u64::from(info.clip_start_ms) * source_rate;
        let scale = 1000 * u64::from(info.source_fps_den);
        if source_rate % unit != 0
            || start % scale != 0
            || u64::from(info.duration_ms) * u64::from(info.fps)
                != u64::from(info.frame_count) * 1000
            || start / scale + u64::from(info.frame_count) * (source_rate / unit)
                > u64::from(info.source_frames)
        {
            return Err(error("asset_metadata", "视频截取与源帧映射不一致"));
        }
        Ok(())
    }
    fn validate_animation(&self, info: &AnimationInfo) -> Result<()> {
        let runtime = hmi_format::gmov::inspect_runtime(self.member(0)?)?;
        let source = hmi_format::gmov::inspect_source(self.member(1)?)?;
        let indices = &info.frame_indices;
        let expected = hmi_format::gmov::normalize_timing(&source.delays);
        let adjusted = expected.len() != source.delays.len()
            || expected.iter().enumerate().any(|(i, (index, delay))| {
                *index as usize != i || u32::from(*delay) != source.delays[i]
            });
        if (info.width, info.height, info.orientation, info.quality)
            != (
                runtime.width,
                runtime.height,
                runtime.orientation,
                runtime.quality,
            )
            || (source.width, source.height) != (info.width, info.height)
            || info.source_frames as usize != source.delays.len()
            || info.source_duration_ms != source.delays.iter().sum::<u32>()
            || info.timing != "native-compatible"
            || info.frame_count as usize != runtime.frames.len()
            || info.duration_ms != runtime.duration_ms
            || info.timing_adjusted != adjusted
            || expected
                != indices
                    .iter()
                    .copied()
                    .zip(info.delays_ms.iter().copied())
                    .collect::<Vec<_>>()
            || indices.len() != runtime.frames.len()
            || info.delays_ms
                != runtime
                    .frames
                    .iter()
                    .map(|f| f.delay_ms)
                    .collect::<Vec<_>>()
            || indices.iter().any(|i| *i >= info.source_frames)
            || indices.windows(2).any(|p| p[0] >= p[1])
            || (info.alpha == Alpha::Preserve) != runtime.alpha_enabled
            || (info.alpha == Alpha::Preserve && info.background.is_some())
            || info.has_alpha != runtime.frames.iter().any(|f| f.alpha_offset != 0)
            || info.source_sha256 != sha256(&self.member(1)?[hmi_format::gmov::HEADER..])
        {
            return Err(error(
                "asset_metadata",
                "动画描述与运行帧、时序或原 GIF 不一致",
            ));
        }
        Ok(())
    }
    fn validate_image(&self, info: &ImageInfo) -> Result<()> {
        info.delivery.validate()?;
        if !matches!(info.codec.as_str(), "rgb565" | "jpeg")
            || info.width == 0
            || info.height == 0
            || !matches!(info.editor_source_format.as_str(), "bmp" | "png")
            || !matches!(info.orientation, 0 | 90 | 180 | 270)
        {
            return Err(error(
                "asset_format",
                "当前资产类型、格式、尺寸或编码方式不受支持",
            ));
        }
        let expected = match info.codec.as_str() {
            "rgb565" => {
                if info.jpeg.is_some() {
                    return Err(error("asset_metadata", "RGB565 目标不能声明 JPEG 参数"));
                }
                if info.editor_source_format != "bmp" {
                    return Err(error("asset_format", "RGB565 使用 BMP 编辑源"));
                }
                if matches!(info.orientation, 90 | 270) {
                    (info.height, info.width)
                } else {
                    (info.width, info.height)
                }
            }
            "jpeg" => {
                let settings = info
                    .jpeg
                    .as_ref()
                    .ok_or_else(|| error("asset_metadata", "JPEG 目标缺少编码参数"))?;
                if !(1..=100).contains(&settings.quality)
                    || settings.sampling != "yuv420"
                    || (settings.padded_width, settings.padded_height)
                        != hmi_format::picture::jpeg::padded_dimensions(info.width, info.height)?
                {
                    return Err(error("asset_metadata", "JPEG 质量、补齐尺寸或采样格式无效"));
                }
                if settings.has_alpha
                    && (info.editor_source_format != "png" || info.alpha != Alpha::Preserve)
                {
                    return Err(error(
                        "asset_metadata",
                        "保留透明度的 JPEG 需要 PNG 编辑源和 preserve 策略",
                    ));
                }
                hmi_format::picture::jpeg::storage_dimensions(
                    info.width,
                    info.height,
                    info.orientation,
                )?
            }
            _ => unreachable!(),
        };
        if (info.storage_width, info.storage_height) != expected
            || !valid_digest(&info.source_sha256)
        {
            return Err(error("asset_metadata", "图片存储尺寸或源摘要无效"));
        }
        if info.delivery.mode == "external" {
            if info.codec != "jpeg"
                || !info
                    .delivery
                    .path
                    .as_deref()
                    .is_some_and(|p| p.to_ascii_lowercase().ends_with(".xi"))
            {
                return Err(error(
                    "asset_delivery",
                    "外部图片当前需要 JPEG 编码及 .xi 部署路径",
                ));
            }
            let header = hmi_format::picture::xi::inspect(self.member(0)?)?;
            if (
                header.width,
                header.height,
                header.orientation,
                header.has_alpha,
            ) != (
                info.width,
                info.height,
                info.orientation,
                info.jpeg.as_ref().unwrap().has_alpha,
            ) {
                return Err(error("asset_metadata", "外部 XI 与资产描述不一致"));
            }
            return Ok(());
        }
        if info.codec == "jpeg" {
            hmi_format::picture::jpeg::validate(
                self.member(0)?,
                self.member(1)?,
                info.width,
                info.height,
                info.orientation,
                &info.editor_source_format,
                info.jpeg.as_ref().unwrap().has_alpha,
            )?;
        } else {
            hmi_format::picture::validate(
                self.member(0)?,
                self.member(1)?,
                info.width,
                info.height,
                info.orientation,
            )?;
        }
        Ok(())
    }
    /// 媒体目标只依赖格式、编码和方向，不依赖页面控件注册表的修订号。
    pub fn validate_for(&self, target: &Target) -> Result<()> {
        self.validate()?;
        target.validate()?;
        if self.metadata.format != target.format
            || self.metadata.info.orientation() != target.orientation
            || match &self.metadata.info {
                AssetInfo::Video { .. } => !target
                    .capabilities
                    .control_types
                    .iter()
                    .any(|c| c == "video"),
                AssetInfo::Animation { .. } => !target
                    .capabilities
                    .control_types
                    .iter()
                    .any(|c| c == "animation"),
                AssetInfo::Image { image } => {
                    !target.capabilities.image_codecs.contains(&image.codec)
                        || (image.delivery.mode == "external"
                            && !target
                                .capabilities
                                .control_types
                                .iter()
                                .any(|c| c == "external_picture"))
                }
                AssetInfo::Font { font } => {
                    !target.capabilities.font_versions.contains(&font.version)
                }
            }
        {
            return Err(error(
                "asset_target",
                "媒体目标的格式、编码或方向与工程不匹配，请重新编码",
            ));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        super::envelope::encode(MAGIC, &self.metadata, &self.payload, "asset")
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let (metadata, payload) = super::envelope::decode(MAGIC, bytes, "asset")?;
        let asset = Self { metadata, payload };
        asset.validate()?;
        Ok(asset)
    }
    pub fn read(path: &Path) -> Result<Self> {
        Self::decode(&file::read(path)?).map_err(|e| file::at_file(e, path))
    }
    pub fn write(&self, path: &Path, inputs: &[&Path]) -> Result<()> {
        file::write_atomic(path, &self.encode()?, inputs)
    }
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
