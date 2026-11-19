//! 可选的 FFmpeg DLL 后端。普通图片、GIF 和字库不加载这些库。
use hmi_core::{Result, error};
use std::path::{Path, PathBuf};

pub use hmi_core::asset::FfmpegInfo as BackendInfo;

const LIBRARIES: [(&str, u32, u32, u32); 5] = [
    ("avutil", 61, 1, 102),
    ("swresample", 7, 1, 102),
    ("avcodec", 63, 1, 102),
    ("avformat", 63, 1, 102),
    ("swscale", 10, 1, 102),
];

/// 返回可能使用的后端输入，不加载 DLL；实际依赖仍以编码结果的 inputs 为准。
pub fn dependency_paths() -> Result<Vec<PathBuf>> {
    let directory = directory()?;
    Ok(LIBRARIES
        .iter()
        .map(|(name, major, _, _)| directory.join(format!("{name}-{major}.dll")))
        .collect())
}

pub(crate) struct DecodedVideo {
    pub frames: Vec<image::RgbaImage>,
    pub fps_num: u32,
    pub fps_den: u32,
    pub audio_streams: u32,
    pub codec: String,
    pub backend: BackendInfo,
    pub inputs: Vec<PathBuf>,
    pub start_pts: i64,
    pub time_base_num: i32,
    pub time_base_den: i32,
}
pub(crate) struct DecodedAudio {
    pub pcm: Vec<u8>,
    pub sample_rate: u32,
    pub channels: u16,
    pub source_rate: u32,
    pub source_channels: u16,
    pub start_pts: i64,
    pub time_base_num: i32,
    pub time_base_den: i32,
    pub codec: String,
    pub backend: BackendInfo,
    pub inputs: Vec<PathBuf>,
}

pub fn directory() -> Result<PathBuf> {
    let path = if let Some(path) = std::env::var_os("HMI_FFMPEG_DIR") {
        PathBuf::from(path)
    } else {
        std::env::current_exe()?
            .parent()
            .ok_or_else(|| error("ffmpeg_path", "无法确定程序目录"))?
            .join("ffmpeg")
    };
    hmi_core::file::absolute(&path)
}

#[cfg(all(windows, target_arch = "x86_64"))]
mod windows;

pub fn backend_info() -> Result<BackendInfo> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        windows::backend_info()
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        Err(error(
            "ffmpeg_platform",
            "视频 DLL 后端当前仅支持 Windows x64",
        ))
    }
}

pub(crate) fn decode(
    path: &Path,
    drop_audio: bool,
    progress: &hmi_core::progress::Progress,
) -> Result<DecodedVideo> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        windows::decode(path, drop_audio, progress)
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = (path, drop_audio, progress);
        Err(error(
            "ffmpeg_platform",
            "视频 DLL 后端当前仅支持 Windows x64",
        ))
    }
}

pub(crate) fn audio(
    path: &Path,
    rate: u32,
    channels: u16,
    progress: &hmi_core::progress::Progress,
) -> Result<DecodedAudio> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        windows::audio(path, rate, channels, progress)
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = (path, rate, channels, progress);
        Err(error(
            "ffmpeg_platform",
            "音频转换当前仅支持 Windows x64 DLL 后端",
        ))
    }
}
