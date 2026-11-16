//! 首批精确 CFR 调度：等间隔时间戳、源帧率整倍数抽帧、整帧截取；不做隐式补帧。
use super::*;
use hmi_artifact::asset::VideoInfo;
use hmi_core::asset::VideoSource;

struct Prepared {
    info: VideoInfo,
    frames: Vec<RgbaImage>,
    inputs: Vec<PathBuf>,
}

fn prepare(path: &Path, target: &Target) -> Result<Prepared> {
    target.validate()?;
    if !target
        .capabilities
        .control_types
        .iter()
        .any(|c| c == "video")
    {
        return Err(error("video_backend", "当前目标尚未支持视频编码"));
    }
    let config: VideoSource = file::read_toml(path)?;
    check_schema(&config.schema, "hmi.asset/1")?;
    if config.kind != "video" || config.input != "source" || !config.delivery.is_embedded() {
        return Err(error(
            "asset_input",
            "当前视频需要 video 的 source 输入，采用内嵌方式",
        ));
    }
    if !(1..=100).contains(&config.encoding.quality)
        || !hmi_format::video::FPS.contains(&config.encoding.fps)
    {
        return Err(error(
            "video_encoding",
            "视频 quality 须为 1～100；fps 须为 4、6、8、10、12、14、18 或 24",
        ));
    }
    if !matches!(
        config.audio.mode.as_str(),
        "drop" | "if-present" | "require"
    ) || (config.audio.mode == "drop"
        && (config.audio.sample_rate.is_some() || config.audio.channels.is_some()))
    {
        return Err(error(
            "video_audio",
            "audio.mode 须为 if-present、drop 或 require；drop 不接受音轨转换参数",
        ));
    }
    if config
        .audio
        .sample_rate
        .is_some_and(|n| ![8000, 11025, 22050, 24000, 32000, 44100].contains(&n))
        || config.audio.channels.is_some_and(|n| !(1..=2).contains(&n))
    {
        return Err(error("video_audio", "音轨采样率或声道数不受当前配置支持"));
    }
    let input = path.parent().unwrap_or(Path::new(".")).join(&config.source);
    let metadata = std::fs::metadata(&input).map_err(|e| file::at_file(e.into(), &input))?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
        return Err(error(
            "video_length",
            "视频输入须为不超过 128 MiB 的普通文件",
        ));
    }
    let bytes = file::read(&input)?;
    if bytes.len() > 128 * 1024 * 1024 {
        return Err(error("video_length", "视频源文件超过 128 MiB"));
    }
    let decoded = ffmpeg::decode(&input, config.audio.mode == "drop")
        .map_err(|e| file::at_file(e, &input))?;
    // 后端通过文件路径解码，前后核对输入，避免源文件改动后仍沿用旧摘要。
    if sha256(&file::read(&input)?) != sha256(&bytes) {
        return Err(file::at_file(
            error("input_changed", "视频源在解码期间发生变化，请重试"),
            &input,
        ));
    }
    if config.audio.mode == "require" && decoded.audio_streams == 0 {
        return Err(error("video_audio", "audio.mode=require，但输入中没有音轨"));
    }
    let first = &decoded.frames[0];
    let (width, height) = dimensions(first.width(), first.height())?;
    if width < 10 || height < 10 {
        return Err(error("video_dimensions", "视频宽高至少为 10"));
    }
    let num = u64::from(decoded.fps_num);
    let den = u64::from(decoded.fps_den);
    let fps = u64::from(config.encoding.fps);
    if num < fps * den {
        return Err(error("video_fps", "目标帧率不能高于源帧率"));
    }
    if num % (fps * den) != 0 {
        return Err(error(
            "video_timing",
            "当前仅支持源帧率为目标帧率整倍数的视频，尚未处理其他取帧边界",
        ));
    }
    let source_duration = decoded.frames.len() as u64 * 1000 * den;
    if !source_duration.is_multiple_of(num) {
        return Err(error(
            "video_timing",
            "源视频有效长度不能精确表示为整数毫秒",
        ));
    }
    let duration =
        config.clip.duration_ms.map(u64::from).unwrap_or_else(|| {
            (source_duration / num).saturating_sub(u64::from(config.clip.start_ms))
        });
    let start = u64::from(config.clip.start_ms);
    if duration == 0 || start + duration > source_duration / num {
        return Err(error("video_clip", "视频截取范围为空或超过有效画面长度"));
    }
    if start * num % (1000 * den) != 0
        || !((start + duration) * num).is_multiple_of(1000 * den)
        || !(duration * fps).is_multiple_of(1000)
    {
        return Err(error(
            "video_clip",
            "当前截取起止须与源帧边界对齐，时长还须包含整数个目标帧",
        ));
    }
    let start_frame = start * num / (1000 * den);
    let stride = num / (fps * den);
    let count = duration * fps / 1000;
    let frames = decoded
        .frames
        .into_iter()
        .enumerate()
        .filter_map(|(i, frame)| {
            let i = i as u64;
            ((start_frame..start_frame + count * stride).contains(&i)
                && (i - start_frame).is_multiple_of(stride))
            .then_some(frame)
        })
        .collect::<Vec<_>>();
    if frames.len() as u64 != count {
        return Err(error("video_frames", "截取帧数与调度结果不一致"));
    }
    let info = VideoInfo {
        width,
        height,
        orientation: target.orientation,
        quality: config.encoding.quality,
        fps: config.encoding.fps,
        frame_count: count as u32,
        duration_ms: duration as u32,
        source_fps_num: decoded.fps_num,
        source_fps_den: decoded.fps_den,
        source_frames: (source_duration / (1000 * den)) as u32,
        clip_start_ms: config.clip.start_ms,
        source_audio_streams: decoded.audio_streams,
        audio_mode: config.audio.mode,
        audio_applied: false,
        codec: decoded.codec,
        source_sha256: sha256(&bytes),
        backend: decoded.backend,
        encoder: "jpeg-encoder/0.6.1-video/1".into(),
    };
    let mut inputs = vec![path.into(), input];
    inputs.extend(decoded.inputs);
    Ok(Prepared {
        info,
        frames,
        inputs,
    })
}

pub(super) fn probe(path: &Path, target: &Target) -> Result<AssetProbe> {
    let p = prepare(path, target).map_err(|e| file::at_file(e, path))?;
    Ok(AssetProbe {
        info: AssetInfo::Video { video: p.info },
        inputs: p.inputs,
    })
}
pub(super) fn encode(path: &Path, target: &Target) -> Result<EncodingResult> {
    let p = prepare(path, target).map_err(|e| file::at_file(e, path))?;
    let mut frames = Vec::new();
    let mut total = 0;
    for pixels in p.frames {
        let (jpeg, _) = encode_jpeg_pixels(
            &pixels,
            (p.info.width, p.info.height),
            p.info.orientation,
            p.info.quality,
            false,
        )?;
        total += jpeg.len();
        if total > hmi_format::video::MAX_BYTES {
            return Err(error("video_length", "VIDEO 运行资源超过 128 MiB"));
        }
        frames.push(jpeg);
    }
    let bytes = hmi_format::video::write(
        p.info.width,
        p.info.height,
        p.info.orientation,
        p.info.quality,
        p.info.fps,
        &frames,
    )?;
    Ok(EncodingResult {
        object: AssetObject::video(p.info, bytes)?,
        inputs: p.inputs,
    })
}
