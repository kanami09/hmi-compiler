//! 首批精确 CFR 调度：等间隔时间戳、源帧率整倍数抽帧、整帧截取；不做隐式补帧。
use super::*;
use hmi_artifact::asset::{AudioTrackInfo, VideoInfo};
use hmi_core::asset::VideoSource;

struct Prepared {
    info: VideoInfo,
    frames: Vec<RgbaImage>,
    inputs: Vec<PathBuf>,
    audio: Option<Vec<u8>>,
}

fn align_audio(
    decoded: &ffmpeg::DecodedAudio,
    video: &ffmpeg::DecodedVideo,
    start_ms: u32,
    duration_ms: u32,
) -> Result<(Vec<u8>, AudioTrackInfo)> {
    let rate = decoded.sample_rate;
    let channels = decoded.channels;
    let total = u64::from(duration_ms) * u64::from(rate);
    if !total.is_multiple_of(1000) {
        return Err(error(
            "audio_timing",
            "视频截取时长不能精确表示为音轨采样数",
        ));
    }
    let total = (total / 1000) as usize;
    let unit = usize::from(channels) * 2;
    if total
        .checked_mul(unit)
        .is_none_or(|n| n > hmi_format::wave::MAX_BYTES)
    {
        return Err(error("audio_length", "视频音轨超过 64 MiB"));
    }
    let vd = i128::from(video.time_base_den);
    let ad = i128::from(decoded.time_base_den);
    let fail = || error("audio_timing", "音画时间基准溢出或不能精确换算到输出采样");
    let v = i128::from(video.start_pts)
        .checked_mul(i128::from(video.time_base_num))
        .and_then(|n| n.checked_mul(ad))
        .ok_or_else(fail)?;
    let a = i128::from(decoded.start_pts)
        .checked_mul(i128::from(decoded.time_base_num))
        .and_then(|n| n.checked_mul(vd))
        .ok_or_else(fail)?;
    let common = vd * ad;
    let numerator = v
        .checked_sub(a)
        .and_then(|n| n.checked_mul(1000))
        .and_then(|n| n.checked_add(i128::from(start_ms) * common))
        .and_then(|n| n.checked_mul(i128::from(rate)))
        .ok_or_else(fail)?;
    let denominator = common * 1000;
    let offset = numerator / denominator;
    if offset * denominator != numerator {
        return Err(fail());
    }
    let available = decoded.pcm.len() / unit;
    let leading = if offset < 0 {
        usize::try_from(-offset).unwrap_or(usize::MAX).min(total)
    } else {
        0
    };
    let begin = if offset > 0 {
        usize::try_from(offset).unwrap_or(usize::MAX).min(available)
    } else {
        0
    };
    let copy = (total - leading).min(available - begin);
    let trailing = total - leading - copy;
    let mut pcm = vec![0; total * unit];
    pcm[leading * unit..(leading + copy) * unit]
        .copy_from_slice(&decoded.pcm[begin * unit..(begin + copy) * unit]);
    let bytes = hmi_format::wave::write(rate, channels, &pcm, &Vec::new(), Some("newaudio"))?;
    let wave = hmi_format::wave::inspect_native(&bytes)?;
    let info = AudioTrackInfo {
        source_codec: decoded.codec.clone(),
        sample_rate: rate,
        channels,
        content_samples: total as u32,
        sample_frames: wave.samples,
        padding_samples: (wave.samples - total as u32) as u8,
        leading_silence_samples: leading as u32,
        trailing_silence_samples: trailing as u32,
        source_rate: decoded.source_rate,
        source_channels: decoded.source_channels,
    };
    Ok((bytes, info))
}

fn prepare(
    path: &Path,
    target: &Target,
    progress: &hmi_core::progress::Progress,
) -> Result<Prepared> {
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
    if config.kind != "video" || config.input != "source" {
        return Err(error("asset_input", "当前视频需要 video 的 source 输入"));
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
    config.delivery.validate_for("video")?;
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
    let decoded = ffmpeg::decode(&input, config.audio.mode == "drop", progress)
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
    if config.audio.mode != "drop" && decoded.audio_streams > 1 {
        return Err(error("audio_streams", "当前不支持选择或混合多条音轨"));
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
    let mut audio = None;
    let mut track = None;
    let mut audio_inputs = Vec::new();
    if config.audio.mode != "drop" && decoded.audio_streams > 0 {
        let sound = ffmpeg::audio(
            &input,
            config.audio.sample_rate.unwrap_or(8000),
            u16::from(config.audio.channels.unwrap_or(1)),
            progress,
        )?;
        if sound.backend != decoded.backend || sha256(&file::read(&input)?) != sha256(&bytes) {
            return Err(error(
                "input_changed",
                "视频或音频后端在构建期间发生变化，请重试",
            ));
        }
        let (wave, info) = align_audio(&sound, &decoded, config.clip.start_ms, duration as u32)?;
        audio = Some(wave);
        track = Some(info);
        audio_inputs = sound.inputs;
    }
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
        delivery: config.delivery,
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
        audio_applied: audio.is_some(),
        audio: track,
        codec: decoded.codec,
        source_sha256: sha256(&bytes),
        backend: Some(decoded.backend),
        encoder: "jpeg-encoder/0.6.1-video/1".into(),
    };
    let mut inputs = vec![path.into(), input];
    inputs.extend(decoded.inputs);
    inputs.extend(audio_inputs);
    Ok(Prepared {
        info,
        frames,
        inputs,
        audio,
    })
}

pub(super) fn probe(path: &Path, target: &Target) -> Result<AssetProbe> {
    let p = prepare(path, target, &hmi_core::progress::Progress::default())
        .map_err(|e| file::at_file(e, path))?;
    Ok(AssetProbe {
        action: "source",
        info: AssetInfo::Video { video: p.info },
        inputs: p.inputs,
    })
}
pub(super) fn encode(
    path: &Path,
    target: &Target,
    progress: &hmi_core::progress::Progress,
) -> Result<EncodingResult> {
    let p = prepare(path, target, progress).map_err(|e| file::at_file(e, path))?;
    let mut frames = Vec::new();
    let mut total = 0;
    let frame_count = p.frames.len() as u64;
    progress.set("encode", 0, Some(frame_count), "frames");
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
        progress.set("encode", frames.len() as u64, Some(frame_count), "frames");
    }
    let bytes = hmi_format::video::write_with_audio(
        p.info.width,
        p.info.height,
        p.info.orientation,
        p.info.quality,
        p.info.fps,
        &frames,
        p.audio.as_deref(),
    )?;
    Ok(EncodingResult {
        action: "source",
        object: AssetObject::video(p.info, bytes)?,
        inputs: p.inputs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pair(audio_start: i64) -> (ffmpeg::DecodedVideo, ffmpeg::DecodedAudio) {
        let backend = hmi_core::asset::FfmpegInfo {
            version: "fixture".into(),
            libraries: Default::default(),
            fingerprint: "0".repeat(64),
        };
        (
            ffmpeg::DecodedVideo {
                frames: Vec::new(),
                fps_num: 4,
                fps_den: 1,
                audio_streams: 1,
                codec: "fixture".into(),
                backend: backend.clone(),
                inputs: Vec::new(),
                start_pts: 1000,
                time_base_num: 1,
                time_base_den: 1000,
            },
            ffmpeg::DecodedAudio {
                pcm: 3000i16.to_le_bytes().repeat(4000),
                sample_rate: 8000,
                channels: 1,
                source_rate: 8000,
                source_channels: 1,
                start_pts: audio_start,
                time_base_num: 1,
                time_base_den: 1000,
                codec: "fixture".into(),
                backend,
                inputs: Vec::new(),
            },
        )
    }
    #[test]
    fn audio_track_offsets_are_sample_exact_and_silence_is_explicit() {
        let (v, a) = pair(1250);
        let (bytes, info) = align_audio(&a, &v, 0, 500).unwrap();
        assert_eq!(info.leading_silence_samples, 2000);
        assert_eq!(info.trailing_silence_samples, 0);
        let w = hmi_format::wave::inspect_native(&bytes).unwrap();
        assert!(w.data[..4000].iter().all(|b| *b == 0));
        assert_eq!(&w.data[4000..], &a.pcm[..4000]);
        let (v, mut a) = pair(750);
        a.pcm.truncate(2000);
        let (_, info) = align_audio(&a, &v, 0, 500).unwrap();
        assert_eq!(info.trailing_silence_samples, 4000);
        let (mut v, a) = pair(0);
        v.start_pts = 1;
        v.time_base_den = 3;
        assert!(align_audio(&a, &v, 0, 500).is_err());
    }
}
