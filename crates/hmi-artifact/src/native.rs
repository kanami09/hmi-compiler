//! 已识别原生资源的描述恢复与打包，不解码再编码媒体，也不猜原始转换参数。
use crate::asset::*;
use hmi_core::{
    Result,
    asset::{Alpha, Delivery},
    error, sha256,
};
fn runtime(extension: &str, kind: [u8; 8], bytes: &[u8], embedded: bool) -> NativeMember {
    (
        MemberRole::Runtime,
        extension.into(),
        embedded.then_some(kind),
        bytes.to_vec(),
    )
}
pub fn image(bytes: &[u8], source: Option<&[u8]>) -> Result<AssetObject> {
    if bytes.len() < 24 || !(10..=13).contains(&bytes[0]) || bytes[1..3] != [0x60, 1] {
        return Err(error("native_image", "图片运行头不属于已接入格式"));
    }
    let width = u16::from_le_bytes(bytes[12..14].try_into().unwrap());
    let height = u16::from_le_bytes(bytes[14..16].try_into().unwrap());
    let orientation = u16::from(bytes[0] - 10) * 90;
    let source_format = source
        .map(|s| -> Result<String> {
            let format = s
                .get(24..27)
                .ok_or_else(|| error("native_image", "图片编辑源头部不完整"))?;
            Ok(std::str::from_utf8(format)
                .map_err(|_| error("native_image", "图片编辑源格式标记无效"))?
                .into())
        })
        .transpose()?
        .unwrap_or_else(|| "none".into());
    let (codec, storage_width, storage_height, jpeg) = match bytes[3] {
        0 => {
            hmi_format::picture::validate_rgb565_runtime(bytes, width, height, orientation)?;
            (
                "rgb565",
                if orientation.is_multiple_of(180) {
                    width
                } else {
                    height
                },
                if orientation.is_multiple_of(180) {
                    height
                } else {
                    width
                },
                None,
            )
        }
        4 => {
            let p = hmi_format::picture::jpeg::parts(bytes, width, height, orientation)?;
            let (w, h) = hmi_format::picture::jpeg::storage_dimensions(width, height, orientation)?;
            let (pw, ph) = hmi_format::picture::jpeg::padded_dimensions(width, height)?;
            (
                "jpeg",
                w,
                h,
                Some(JpegInfo {
                    quality: None,
                    padded_width: pw,
                    padded_height: ph,
                    sampling: "yuv420".into(),
                    has_alpha: p.alpha.is_some(),
                }),
            )
        }
        _ => {
            return Err(error(
                "native_image",
                format!("图片编码分支{}尚未接入，未写出工程", bytes[3]),
            ));
        }
    };
    let info = ImageInfo {
        delivery: Delivery::default(),
        width,
        height,
        storage_width,
        storage_height,
        orientation,
        codec: codec.into(),
        alpha: Alpha::Preserve,
        background: None,
        resize: None,
        source_format: "hmi".into(),
        editor_source_format: source_format,
        source_sha256: sha256(bytes),
        backend: "hmi-native-image/1".into(),
        jpeg,
    };
    let mut entries = vec![runtime("i", *b"i\0\0\0\0\0\0\0", bytes, true)];
    if let Some(source) = source {
        entries.push((MemberRole::Source, "is".into(), None, source.to_vec()));
    }
    AssetObject::native(AssetInfo::Image { image: info }, entries)
}
pub fn font(bytes: &[u8]) -> Result<AssetObject> {
    let h = hmi_format::font::inspect(bytes)?;
    AssetObject::font(
        FontInfo {
            version: 6,
            encoding: "utf-8".into(),
            height: h.height,
            glyph_count: h.glyph_count,
            orientation: h.orientation,
            source_orientation: h.orientation,
            source_sha256: sha256(bytes),
            backend: "hmi-native-zi/1".into(),
        },
        bytes.to_vec(),
    )
}
pub fn audio(bytes: &[u8], delivery: Delivery) -> Result<AssetObject> {
    let w = hmi_format::wave::inspect_native(bytes)?;
    AssetObject::audio(
        AudioInfo {
            delivery,
            source_codec: "native-pcm-s16le".into(),
            sample_rate: w.sample_rate,
            channels: w.channels,
            sample_frames: w.samples,
            content_samples: w.samples,
            padding_samples: 0,
            duration_ms: ((u64::from(w.samples) * 1000) / u64::from(w.sample_rate)) as u32,
            source_rate: w.sample_rate,
            source_channels: w.channels,
            source_bits: 16,
            source_samples: w.samples,
            clip_start_ms: 0,
            source_sha256: sha256(bytes),
            encoder: "hmi-native-wave/1".into(),
            backend: None,
        },
        bytes.to_vec(),
    )
}
pub fn video(bytes: &[u8], delivery: Delivery) -> Result<AssetObject> {
    let v = hmi_format::video::inspect(bytes)?;
    let audio = v
        .audio
        .map(|b| -> Result<AudioTrackInfo> {
            let w = hmi_format::wave::inspect_native(b)?;
            let samples = u64::from(v.duration_ms) * u64::from(w.sample_rate);
            if !samples.is_multiple_of(1000) {
                return Err(error("native_video", "音轨时长不是整数采样"));
            }
            let content = (samples / 1000) as u32;
            let padding = w
                .samples
                .checked_sub(content)
                .filter(|n| *n <= 1)
                .ok_or_else(|| error("asset_metadata", "VIDEO音轨采样数与时长不一致"))?;
            Ok(AudioTrackInfo {
                source_codec: "native-pcm-s16le".into(),
                sample_rate: w.sample_rate,
                channels: w.channels,
                content_samples: content,
                sample_frames: w.samples,
                padding_samples: padding as u8,
                leading_silence_samples: 0,
                trailing_silence_samples: 0,
                source_rate: w.sample_rate,
                source_channels: w.channels,
            })
        })
        .transpose()?;
    AssetObject::video(
        VideoInfo {
            delivery,
            width: v.width,
            height: v.height,
            orientation: v.orientation,
            quality: v.quality,
            fps: v.fps,
            frame_count: v.frames.len() as u32,
            duration_ms: v.duration_ms,
            source_fps_num: u32::from(v.fps),
            source_fps_den: 1,
            source_frames: v.frames.len() as u32,
            clip_start_ms: 0,
            source_audio_streams: u32::from(audio.is_some()),
            audio_mode: "if-present".into(),
            audio_applied: audio.is_some(),
            audio,
            codec: "video".into(),
            source_sha256: sha256(bytes),
            backend: None,
            encoder: "hmi-native-video/1".into(),
        },
        bytes.to_vec(),
    )
}
pub fn animation(bytes: &[u8], source: Option<&[u8]>) -> Result<AssetObject> {
    let g = hmi_format::gmov::inspect_runtime(bytes)?;
    let gif = source.map(hmi_format::gmov::inspect_source).transpose()?;
    let info = AnimationInfo {
        delivery: Delivery::default(),
        width: g.width,
        height: g.height,
        orientation: g.orientation,
        quality: g.quality,
        alpha: if g.alpha_enabled {
            Alpha::Preserve
        } else {
            Alpha::Flatten
        },
        background: None,
        timing: if source.is_some() {
            "native-preserved"
        } else {
            "native-runtime"
        }
        .into(),
        source_frames: gif.as_ref().map_or(0, |s| s.delays.len() as u32),
        source_duration_ms: gif.as_ref().map_or(0, |s| s.delays.iter().sum()),
        frame_count: g.frames.len() as u32,
        duration_ms: g.duration_ms,
        timing_adjusted: false,
        frame_indices: Vec::new(),
        delays_ms: g.frames.iter().map(|f| f.delay_ms).collect(),
        has_alpha: g.frames.iter().any(|f| f.alpha_offset != 0),
        source_sha256: sha256(bytes),
        backend: "hmi-native-gmov/1".into(),
    };
    let mut entries = vec![runtime("gmov", *b"gmov\0\0\0\0", bytes, true)];
    if let Some(source) = source {
        entries.push((MemberRole::Source, "gmovs".into(), None, source.to_vec()));
    }
    AssetObject::native(AssetInfo::Animation { animation: info }, entries)
}
