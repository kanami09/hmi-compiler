//! 使用标准 GIF 解码处理局部帧、disposal 和透明；原 GIF 独立保留。
use super::*;
use hmi_artifact::asset::AnimationInfo;
use hmi_core::asset::AnimationSource;
use hmi_format::gmov;
use image::AnimationDecoder;

struct Prepared {
    info: AnimationInfo,
    frames: Vec<RgbaImage>,
    gif: Vec<u8>,
    inputs: Vec<PathBuf>,
}

fn prepare(path: &Path, target: &Target) -> Result<Prepared> {
    target.validate()?;
    if !target
        .capabilities
        .control_types
        .iter()
        .any(|c| c == "animation")
    {
        return Err(error("animation_backend", "当前目标尚未支持动画编码"));
    }
    let config: AnimationSource = file::read_toml(path)?;
    check_schema(&config.schema, "hmi.asset/1")?;
    if config.kind != "animation" || config.input != "source" {
        return Err(error(
            "asset_input",
            "当前动画编码需要 animation 的 source GIF 输入",
        ));
    }
    let settings = &config.encoding;
    if !(1..=100).contains(&settings.quality) || settings.timing != "native-compatible" {
        return Err(error(
            "animation_encoding",
            "动画 quality 须为 1～100，timing 须为 native-compatible",
        ));
    }
    if settings.alpha == Alpha::Preserve && settings.background.is_some() {
        return Err(error(
            "image_alpha",
            "只有 alpha=flatten 时才能指定编码背景色",
        ));
    }
    let background = if settings.alpha == Alpha::Flatten {
        Some(color(
            settings
                .background
                .as_deref()
                .ok_or_else(|| error("image_alpha", "alpha=flatten 必须指定 #RRGGBB 背景色"))?,
            false,
        )?)
    } else {
        None
    };
    config.delivery.validate_for("animation")?;
    let input = path.parent().unwrap_or(Path::new(".")).join(&config.source);
    let gif = file::read(&input)?;
    if gif.len() > 64 * 1024 * 1024 {
        return Err(error("animation_length", "GIF 源文件超过 64 MiB"));
    }
    let source = gmov::inspect_gif(&gif)?;
    dimensions(u32::from(source.width), u32::from(source.height))?;
    let (pw, ph) = gmov::storage_dimensions(source.width, source.height, 0)?;
    dimensions(u32::from(pw), u32::from(ph))?;
    let kept = gmov::normalize_timing(&source.delays);
    if kept.is_empty() {
        return Err(error(
            "animation_timing",
            "GIF 全部帧的延时均为零，不能生成可播放动画",
        ));
    }
    let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(&gif))
        .map_err(|e| error("gif_decode", e.to_string()))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(65535);
    limits.max_image_height = Some(65535);
    limits.max_alloc = Some(MAX_RGBA_BYTES);
    decoder
        .set_limits(limits)
        .map_err(|e| error("gif_decode", e.to_string()))?;
    let mut frames = Vec::new();
    let mut decoded = 0;
    let mut has_alpha = false;
    let mut total = 0usize;
    for (index, frame) in decoder.into_frames().enumerate() {
        let frame = frame.map_err(|e| error("gif_decode", e.to_string()))?;
        decoded += 1;
        if kept
            .get(frames.len())
            .is_none_or(|(i, _)| *i as usize != index)
        {
            continue;
        }
        let mut pixels = frame.into_buffer();
        if pixels.dimensions() != (u32::from(source.width), u32::from(source.height)) {
            return Err(error("gif_dimensions", "GIF 合成帧尺寸与逻辑画布不一致"));
        }
        total = total
            .checked_add(pixels.as_raw().len())
            .filter(|n| *n <= 128 * 1024 * 1024)
            .ok_or_else(|| error("animation_length", "动画保留帧的 RGBA 总数据超过 128 MiB"))?;
        if let Some(bg) = background {
            for pixel in pixels.pixels_mut() {
                let a = u32::from(pixel[3]);
                for c in 0..3 {
                    pixel[c] = ((u32::from(pixel[c]) * a + u32::from(bg[c]) * (255 - a) + 127)
                        / 255) as u8;
                }
                pixel[3] = 255;
            }
        }
        has_alpha |= pixels.pixels().any(|p| p[3] != 255);
        frames.push(pixels);
    }
    if decoded != source.delays.len() || frames.len() != kept.len() {
        return Err(error("gif_frames", "GIF 解码帧数与容器不一致"));
    }
    let adjusted = kept.len() != source.delays.len()
        || kept
            .iter()
            .enumerate()
            .any(|(i, (index, d))| *index as usize != i || u32::from(*d) != source.delays[i]);
    let info = AnimationInfo {
        delivery: config.delivery,
        width: source.width,
        height: source.height,
        orientation: target.orientation,
        quality: settings.quality,
        alpha: settings.alpha.clone(),
        background: settings.background.clone(),
        timing: settings.timing.clone(),
        source_frames: source.delays.len() as u32,
        source_duration_ms: source.delays.iter().sum(),
        frame_count: kept.len() as u32,
        duration_ms: kept.iter().map(|(_, d)| u32::from(*d)).sum(),
        timing_adjusted: adjusted,
        frame_indices: kept.iter().map(|(i, _)| *i).collect(),
        delays_ms: kept.iter().map(|(_, d)| *d).collect(),
        has_alpha,
        source_sha256: sha256(&gif),
        backend: "image-rs/0.25.10-gif+jpeg-encoder/0.6.1-gmov/1".into(),
    };
    Ok(Prepared {
        info,
        frames,
        gif,
        inputs: vec![path.into(), input],
    })
}

pub(super) fn probe(path: &Path, target: &Target) -> Result<AssetProbe> {
    let p = prepare(path, target).map_err(|e| file::at_file(e, path))?;
    Ok(AssetProbe {
        info: AssetInfo::Animation { animation: p.info },
        inputs: p.inputs,
    })
}

pub(super) fn encode(path: &Path, target: &Target) -> Result<EncodingResult> {
    let p = prepare(path, target).map_err(|e| file::at_file(e, path))?;
    let padded = gmov::storage_dimensions(p.info.width, p.info.height, 0)?;
    let storage = gmov::storage_dimensions(p.info.width, p.info.height, p.info.orientation)?;
    let mut frames = Vec::new();
    let mut encoded_size = 0usize;
    for (pixels, delay) in p.frames.iter().zip(&p.info.delays_ms) {
        let alpha = pixels.pixels().any(|p| p[3] != 255);
        let (jpeg, alpha) =
            encode_jpeg_pixels(pixels, padded, p.info.orientation, p.info.quality, alpha)?;
        let mut data = vec![0; 20];
        data[4..8].copy_from_slice(&u32::from(storage.0).to_le_bytes());
        data[8..12].copy_from_slice(&u32::from(storage.1).to_le_bytes());
        data.extend(&jpeg);
        let offset = if alpha.is_some() {
            data.len() as u32
        } else {
            0
        };
        if let Some(alpha) = alpha {
            data.extend(alpha);
        }
        encoded_size += data.len();
        if encoded_size > gmov::MAX_BYTES {
            return Err(error("animation_length", "动画运行资源超过 128 MiB"));
        }
        frames.push(gmov::EncodedFrame {
            data,
            delay_ms: *delay,
            alpha_offset: offset,
        });
    }
    let runtime = gmov::write_runtime(
        p.info.width,
        p.info.height,
        p.info.orientation,
        p.info.quality,
        p.info.alpha == Alpha::Preserve,
        &frames,
    )?;
    let source = gmov::write_source(&p.gif)?;
    Ok(EncodingResult {
        object: AssetObject::animation(p.info, runtime, source)?,
        inputs: p.inputs,
    })
}
