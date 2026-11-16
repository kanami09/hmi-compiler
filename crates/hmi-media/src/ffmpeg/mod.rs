//! 可选的 FFmpeg DLL 后端。普通图片、GIF 和字库不加载这些库。
use hmi_core::{Result, error};
use std::path::{Path, PathBuf};

pub use hmi_core::asset::FfmpegInfo as BackendInfo;

pub(crate) struct DecodedVideo {
    pub frames: Vec<image::RgbaImage>,
    pub fps_num: u32,
    pub fps_den: u32,
    pub audio_streams: u32,
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

pub(crate) fn decode(path: &Path, drop_audio: bool) -> Result<DecodedVideo> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        windows::decode(path, drop_audio)
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = (path, drop_audio);
        Err(error(
            "ffmpeg_platform",
            "视频 DLL 后端当前仅支持 Windows x64",
        ))
    }
}
