//! 输入层只检查、复用或重新封装已知格式；source 的实际编码仍由各后端负责。
use super::{AssetProbe, EncodingResult};
use hmi_artifact::asset::{
    AnimationInfo, AssetInfo, AssetObject, AudioInfo, AudioTrackInfo, FontInfo, ImageInfo,
    JpegInfo, SourceRequest, VideoInfo,
};
use hmi_core::{
    Result,
    asset::{Alpha, AnimationSource, AudioSource, Delivery, FontSource, ImageSource, VideoSource},
    error, file,
    model::check_schema,
    sha256,
    target::Target,
};
use std::path::{Path, PathBuf};

fn text<'a>(table: &'a toml::Table, key: &str) -> Result<&'a str> {
    table
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| error("asset_input", format!("必须指定非空字符串 {key}")))
}
fn read(path: &Path) -> Result<Vec<u8>> {
    if std::fs::metadata(path)
        .map_err(|e| file::at_file(e.into(), path))?
        .len()
        > 256 * 1024 * 1024
    {
        return Err(error("asset_length", "媒体输入或资产包不能超过 256 MiB"));
    }
    file::read(path)
}
fn source_path(path: &Path, table: &toml::Table) -> Result<PathBuf> {
    Ok(path
        .parent()
        .unwrap_or(Path::new("."))
        .join(text(table, "source")?))
}
fn request(path: &Path, kind: &str, target: &Target) -> Result<SourceRequest> {
    let mut settings = match kind {
        "image" => serde_json::to_value(file::read_toml::<ImageSource>(path)?),
        "font" => serde_json::to_value(file::read_toml::<FontSource>(path)?),
        "animation" => serde_json::to_value(file::read_toml::<AnimationSource>(path)?),
        "video" => serde_json::to_value(file::read_toml::<VideoSource>(path)?),
        "audio" => serde_json::to_value(file::read_toml::<AudioSource>(path)?),
        _ => return Err(error("asset_kind", "不支持的媒体类别")),
    }
    .map_err(|e| error("asset_input", e.to_string()))?;
    let map = settings.as_object_mut().unwrap();
    for key in ["schema", "source", "input", "original"] {
        map.remove(key);
    }
    if kind == "image" {
        let encoding = map.get_mut("encoding").unwrap();
        if encoding["codec"] == "auto" {
            encoding["codec"] = target
                .capabilities
                .image_codecs
                .first()
                .ok_or_else(|| error("image_codec", "目标没有图片编码路线"))?
                .clone()
                .into();
        }
        if encoding["codec"] == "jpeg" && encoding["quality"].is_null() {
            encoding["quality"] = 80.into();
        }
    }
    map.insert("target".into(),serde_json::json!({"format":target.format,"encoding":target.encoding,"orientation":if kind=="audio" {None}else{Some(target.orientation)}}));
    Ok(SourceRequest {
        schema: "hmi.source-request/1".into(),
        settings,
    })
}

/// original 是必须存在的输入附件；只有摘要、请求和兼容条件都相同才复用。
fn original(
    path: &Path,
    table: &toml::Table,
    kind: &str,
    target: &Target,
    request: &SourceRequest,
) -> Result<Option<EncodingResult>> {
    let Some(name) = table.get("original").and_then(|v| v.as_str()) else {
        return Ok(None);
    };
    let source = source_path(path, table)?;
    let bytes = read(&source)?;
    let attachment = path.parent().unwrap_or(Path::new(".")).join(name);
    let object = AssetObject::decode(&read(&attachment)?)?;
    if object.metadata.info.kind() != kind {
        return Err(error("asset_kind", "original 附件与媒体类别不一致"));
    }
    if object.metadata.info.source_sha256() == sha256(&bytes)
        && object.metadata.source_request.as_ref() == Some(request)
        && object.validate_for(target).is_ok()
    {
        return Ok(Some(EncodingResult {
            object,
            inputs: vec![path.into(), source, attachment],
            action: "original",
        }));
    }
    Ok(None)
}
fn attachment_input(
    path: &Path,
    table: &toml::Table,
    kind: &str,
    input: &str,
    target: &Target,
) -> Result<EncodingResult> {
    for key in table.keys() {
        if !matches!(
            key.as_str(),
            "schema" | "kind" | "input" | "source" | "delivery"
        ) {
            return Err(error(
                "asset_input_parameter",
                format!("{input} 输入不接受参数 {key}，需要改变编码时请使用 source"),
            ));
        }
    }
    let source = source_path(path, table)?;
    let bytes = read(&source)?;
    let delivery = table
        .get("delivery")
        .map(|v| v.clone().try_into::<Delivery>())
        .transpose()
        .map_err(|e| error("asset_delivery", e.to_string()))?;
    let mut object = if input == "native" {
        AssetObject::decode(&bytes)?
    } else {
        encoded(kind, &bytes, delivery.clone().unwrap_or_default())?
    };
    if object.metadata.info.kind() != kind {
        return Err(error("asset_kind", "资产包与声明的媒体类别不一致"));
    }
    if input == "native"
        && let Some(delivery) = delivery
    {
        delivery.validate_for(kind)?;
        let old = object.metadata.info.delivery().cloned().unwrap_or_default();
        if old.mode != delivery.mode {
            return Err(error(
                "asset_delivery",
                "native 输入保留包内成员，不改变内嵌／外部方式；请改用 source 或 encoded",
            ));
        }
        if delivery != old {
            match &mut object.metadata.info {
                AssetInfo::Image { image } => image.delivery = delivery,
                AssetInfo::Animation { animation } => animation.delivery = delivery,
                AssetInfo::Video { video } => video.delivery = delivery,
                AssetInfo::Audio { audio } => audio.delivery = delivery,
                AssetInfo::Font { .. } => {}
            }
            object.metadata.source_request = None;
        }
    }
    object.validate_for(target)?;
    Ok(EncodingResult {
        object,
        inputs: vec![path.into(), source],
        action: if input == "native" {
            "native"
        } else {
            "encoded"
        },
    })
}
fn description(path: &Path, target: &Target) -> Result<(toml::Table, String, String)> {
    target.validate()?;
    let table: toml::Table = file::read_toml(path)?;
    check_schema(text(&table, "schema")?, "hmi.asset/1")?;
    let kind = text(&table, "kind")?.to_string();
    if !matches!(
        kind.as_str(),
        "image" | "font" | "animation" | "video" | "audio"
    ) {
        return Err(error("asset_kind", "不支持的媒体类别"));
    }
    let input = table
        .get("input")
        .map(|_| text(&table, "input"))
        .transpose()?
        .unwrap_or("source")
        .to_string();
    if !matches!(input.as_str(), "source" | "encoded" | "native") {
        return Err(error(
            "asset_input",
            "input 必须为 source、encoded 或 native",
        ));
    }
    Ok((table, kind, input))
}
pub fn probe(path: &Path, target: &Target) -> Result<AssetProbe> {
    let run = || -> Result<AssetProbe> {
        let (table, kind, input) = description(path, target)?;
        if input != "source" {
            let r = attachment_input(path, &table, &kind, &input, target)?;
            return Ok(AssetProbe {
                info: r.object.metadata.info,
                inputs: r.inputs,
                action: r.action,
            });
        }
        let request = request(path, &kind, target)?;
        if let Some(r) = original(path, &table, &kind, target, &request)? {
            return Ok(AssetProbe {
                info: r.object.metadata.info,
                inputs: r.inputs,
                action: r.action,
            });
        }
        let mut p = super::probe_source(path, target, &kind)?;
        if let Some(name) = table.get("original").and_then(|v| v.as_str()) {
            p.inputs
                .push(path.parent().unwrap_or(Path::new(".")).join(name));
        }
        Ok(p)
    };
    run().map_err(|e| file::at_file(e, path))
}
pub fn encode(path: &Path, target: &Target) -> Result<EncodingResult> {
    let run = || -> Result<EncodingResult> {
        let (table, kind, input) = description(path, target)?;
        if input != "source" {
            return attachment_input(path, &table, &kind, &input, target);
        }
        let request = request(path, &kind, target)?;
        if let Some(r) = original(path, &table, &kind, target, &request)? {
            return Ok(r);
        }
        let mut r = super::encode_source(path, target, &kind)?;
        r.object.metadata.source_request = Some(request);
        if let Some(name) = table.get("original").and_then(|v| v.as_str()) {
            r.inputs
                .push(path.parent().unwrap_or(Path::new(".")).join(name));
        }
        r.object.validate_for(target)?;
        Ok(r)
    };
    run().map_err(|e| file::at_file(e, path))
}

fn encoded(kind: &str, bytes: &[u8], delivery: Delivery) -> Result<AssetObject> {
    delivery.validate_for(kind)?;
    let digest = sha256(bytes);
    match kind {
        "image" => {
            let h = hmi_format::picture::xi::inspect(bytes)?;
            super::dimensions(u32::from(h.width), u32::from(h.height))?;
            let offset = u32::from_le_bytes(bytes[28..32].try_into().unwrap()) as usize;
            let split = if offset == 0 {
                bytes.len()
            } else {
                32 + offset
            };
            let runtime = hmi_format::picture::jpeg::runtime_with_alpha(
                h.width,
                h.height,
                h.orientation,
                &bytes[32..split],
                if offset == 0 {
                    None
                } else {
                    Some(&bytes[split..])
                },
            )?;
            let (sw, sh) =
                hmi_format::picture::jpeg::storage_dimensions(h.width, h.height, h.orientation)?;
            let (pw, ph) = hmi_format::picture::jpeg::padded_dimensions(h.width, h.height)?;
            AssetObject::image(
                ImageInfo {
                    delivery,
                    width: h.width,
                    height: h.height,
                    storage_width: sw,
                    storage_height: sh,
                    orientation: h.orientation,
                    codec: "jpeg".into(),
                    alpha: Alpha::Preserve,
                    background: None,
                    resize: None,
                    source_format: "xi".into(),
                    editor_source_format: "none".into(),
                    source_sha256: digest,
                    backend: "hmi-xi-preserve/1".into(),
                    jpeg: Some(JpegInfo {
                        quality: None,
                        padded_width: pw,
                        padded_height: ph,
                        sampling: "yuv420".into(),
                        has_alpha: h.has_alpha,
                    }),
                },
                runtime,
                Vec::new(),
            )
        }
        "font" => {
            let h = hmi_format::font::inspect(bytes)?;
            AssetObject::font(
                FontInfo {
                    version: 6,
                    encoding: "utf-8".into(),
                    height: h.height,
                    glyph_count: h.glyph_count,
                    orientation: h.orientation,
                    source_orientation: h.orientation,
                    source_sha256: digest,
                    backend: "hmi-zi-preserve/1".into(),
                },
                bytes.to_vec(),
            )
        }
        "audio" => {
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
                    source_sha256: digest,
                    encoder: "hmi-wave-preserve/1".into(),
                    backend: None,
                },
                bytes.to_vec(),
            )
        }
        "video" => {
            let v = hmi_format::video::inspect(bytes)?;
            let audio = v
                .audio
                .map(|b| -> Result<AudioTrackInfo> {
                    let w = hmi_format::wave::inspect_native(b)?;
                    let content =
                        (u64::from(v.duration_ms) * u64::from(w.sample_rate) / 1000) as u32;
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
                    source_sha256: digest,
                    backend: None,
                    encoder: "hmi-video-preserve/1".into(),
                },
                bytes.to_vec(),
            )
        }
        "animation" => {
            if delivery.is_embedded() {
                return Err(error(
                    "asset_input",
                    "encoded 运行GMOV当前仅支持 external；内嵌动画请使用GIF或带编辑源的native资产包",
                ));
            }
            let g = hmi_format::gmov::inspect_runtime(bytes)?;
            AssetObject::animation(
                AnimationInfo {
                    delivery,
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
                    timing: "preserved".into(),
                    source_frames: g.frames.len() as u32,
                    source_duration_ms: g.duration_ms,
                    frame_count: g.frames.len() as u32,
                    duration_ms: g.duration_ms,
                    timing_adjusted: false,
                    frame_indices: (0..g.frames.len() as u32).collect(),
                    delays_ms: g.frames.iter().map(|f| f.delay_ms).collect(),
                    has_alpha: g.frames.iter().any(|f| f.alpha_offset != 0),
                    source_sha256: digest,
                    backend: "hmi-gmov-preserve/1".into(),
                },
                bytes.to_vec(),
                Vec::new(),
            )
        }
        _ => unreachable!(),
    }
}
