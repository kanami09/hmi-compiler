//! PCM16 可直接封装；格式／声道／采样率转换才使用可选 FFmpeg 后端。
use super::*;
use hmi_artifact::asset::AudioInfo;
use hmi_core::asset::AudioSource;
use hmi_format::wave;
pub(crate) const RATES: &[u32] = &[8000, 11025, 22050, 24000, 32000, 44100];

pub(crate) fn clip_samples(
    pcm: &[u8],
    rate: u32,
    channels: u16,
    start_ms: u32,
    duration_ms: Option<u32>,
) -> Result<Vec<u8>> {
    let unit = usize::from(channels) * 2;
    let start = u64::from(start_ms) * u64::from(rate);
    if !start.is_multiple_of(1000) {
        return Err(error("audio_clip", "音频起点必须与目标采样边界精确对齐"));
    }
    let start = (start / 1000) as usize;
    let available = pcm.len() / unit;
    let count = if let Some(ms) = duration_ms {
        let n = u64::from(ms) * u64::from(rate);
        if n == 0 || !n.is_multiple_of(1000) {
            return Err(error("audio_clip", "音频时长须包含整数个目标采样"));
        }
        (n / 1000) as usize
    } else {
        available.saturating_sub(start)
    };
    if count == 0 || start.checked_add(count).is_none_or(|end| end > available) {
        return Err(error("audio_clip", "音频截取范围为空或超出输入"));
    }
    Ok(pcm[start * unit..(start + count) * unit].to_vec())
}

fn prepare(
    path: &Path,
    target: &Target,
    progress: &hmi_core::progress::Progress,
) -> Result<(AudioInfo, Vec<u8>, Vec<PathBuf>)> {
    target.validate()?;
    if !target
        .capabilities
        .control_types
        .iter()
        .any(|c| c == "audio")
    {
        return Err(error("audio_backend", "当前目标尚未支持音频"));
    }
    let config: AudioSource = file::read_toml(path)?;
    check_schema(&config.schema, "hmi.asset/1")?;
    if config.kind != "audio" || config.input != "source" || config.encoding.sample_format != "s16"
    {
        return Err(error(
            "audio_input",
            "当前音频需要 source WAV 输入、s16 采样",
        ));
    }
    config.delivery.validate_for("audio")?;
    let input = path.parent().unwrap_or(Path::new(".")).join(&config.source);
    let metadata = std::fs::metadata(&input).map_err(|e| file::at_file(e.into(), &input))?;
    if !metadata.is_file() || metadata.len() > wave::MAX_BYTES as u64 + 4096 {
        return Err(error(
            "audio_length",
            "WAV 输入须为不超过 64 MiB 的普通文件",
        ));
    }
    let bytes = file::read(&input)?;
    let source = wave::inspect(&bytes)?;
    let rate = config.encoding.sample_rate.unwrap_or(source.sample_rate);
    let channels = config.encoding.channels.unwrap_or(source.channels);
    if !RATES.contains(&rate) || ![1, 2].contains(&channels) {
        return Err(error(
            "audio_format",
            "当前输出支持 8000/11025/22050/24000/32000/44100Hz 和单／双声道，请显式设置需要的转换参数",
        ));
    }
    let mut inputs = vec![path.into(), input.clone()];
    let mut backend = None;
    let mut source_codec = format!("wave-{}-{}", source.format, source.bits);
    let pcm = if source.original_format == 1
        && source.bits == 16
        && source.sample_rate == rate
        && source.channels == channels
    {
        source.data.to_vec()
    } else {
        let decoded = ffmpeg::audio(&input, rate, channels, progress)?;
        if sha256(&file::read(&input)?) != sha256(&bytes) {
            return Err(error("input_changed", "音频源在解码期间发生变化，请重试"));
        }
        source_codec = decoded.codec;
        backend = Some(decoded.backend);
        inputs.extend(decoded.inputs);
        decoded.pcm
    };
    let pcm = clip_samples(
        &pcm,
        rate,
        channels,
        config.clip.start_ms,
        config.clip.duration_ms,
    )?;
    let content_samples = (pcm.len() / (usize::from(channels) * 2)) as u32;
    let name = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");
    let output = wave::write(rate, channels, &pcm, &source.metadata, Some(name))?;
    let parsed = wave::inspect_native(&output)?;
    let info = AudioInfo {
        delivery: config.delivery,
        source_codec,
        sample_rate: rate,
        channels,
        sample_frames: parsed.samples,
        content_samples,
        padding_samples: (parsed.samples - content_samples) as u8,
        duration_ms: (u64::from(parsed.samples) * 1000 / u64::from(rate)) as u32,
        source_rate: source.sample_rate,
        source_channels: source.channels,
        source_bits: source.bits,
        source_samples: source.samples,
        clip_start_ms: config.clip.start_ms,
        source_sha256: sha256(&bytes),
        encoder: "hmi-pcm-wave/1".into(),
        backend,
    };
    Ok((info, output, inputs))
}
pub(super) fn probe(path: &Path, target: &Target) -> Result<AssetProbe> {
    let (info, _, inputs) = prepare(path, target, &hmi_core::progress::Progress::default())
        .map_err(|e| file::at_file(e, path))?;
    Ok(AssetProbe {
        action: "source",
        info: AssetInfo::Audio { audio: info },
        inputs,
    })
}
pub(super) fn encode(
    path: &Path,
    target: &Target,
    progress: &hmi_core::progress::Progress,
) -> Result<EncodingResult> {
    let (info, bytes, inputs) =
        prepare(path, target, progress).map_err(|e| file::at_file(e, path))?;
    Ok(EncodingResult {
        action: "source",
        object: AssetObject::audio(info, bytes)?,
        inputs,
    })
}
