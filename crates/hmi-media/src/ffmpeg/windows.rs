//! 安全接口仅返回自有像素；原生指针不离开本模块，所有资源由 RAII 释放。
use super::*;
use hmi_core::file;
use libloading::os::windows::{
    LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, Library,
};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr,
    sync::{Arc, Mutex, OnceLock},
};

#[allow(non_camel_case_types, non_snake_case, dead_code)]
mod ffi {
    // 这些类型只经指针使用。可访问的结构由匹配头文件生成，并带 sizeof/offset_of 断言。
    macro_rules! opaque {($($name:ident),*)=>{$(#[repr(C)]pub struct $name{_private:[u8;0]})*};}
    opaque!(
        AVClass,
        AVBufferRef,
        AVFrameSideData,
        AVDictionary,
        AVIOContext,
        AVInputFormat,
        AVOutputFormat,
        AVCodec,
        AVCodecContext,
        AVChannelCustom,
        AVChapter,
        AVProgram,
        AVStreamGroup,
        SwsContext,
        SwrContext
    );
    macro_rules! enums {($($name:ident),*)=>{$(pub type $name=std::ffi::c_int;)*};}
    enums!(
        AVAlphaMode,
        AVChannelOrder,
        AVChromaLocation,
        AVCodecID,
        AVColorPrimaries,
        AVColorRange,
        AVColorSpace,
        AVColorTransferCharacteristic,
        AVDiscard,
        AVDurationEstimationMethod,
        AVFieldOrder,
        AVMediaType,
        AVPacketSideDataType,
        AVPictureType
    );
    include!("bindings.rs");
}
use ffi::*;

#[allow(clippy::type_complexity, non_snake_case)]
struct Api {
    av_get_sample_fmt: unsafe extern "C" fn(*const c_char) -> c_int,
    av_channel_layout_default: unsafe extern "C" fn(*mut AVChannelLayout, c_int),
    av_channel_layout_copy:
        unsafe extern "C" fn(*mut AVChannelLayout, *const AVChannelLayout) -> c_int,
    av_channel_layout_compare:
        unsafe extern "C" fn(*const AVChannelLayout, *const AVChannelLayout) -> c_int,
    av_channel_layout_uninit: unsafe extern "C" fn(*mut AVChannelLayout),
    swr_alloc_set_opts2: unsafe extern "C" fn(
        *mut *mut SwrContext,
        *const AVChannelLayout,
        c_int,
        c_int,
        *const AVChannelLayout,
        c_int,
        c_int,
        c_int,
        *mut c_void,
    ) -> c_int,
    swr_init: unsafe extern "C" fn(*mut SwrContext) -> c_int,
    swr_get_out_samples: unsafe extern "C" fn(*mut SwrContext, c_int) -> c_int,
    swr_convert: unsafe extern "C" fn(
        *mut SwrContext,
        *const *mut u8,
        c_int,
        *const *const u8,
        c_int,
    ) -> c_int,
    swr_free: unsafe extern "C" fn(*mut *mut SwrContext),
    avformat_open_input: unsafe extern "C" fn(
        *mut *mut AVFormatContext,
        *const c_char,
        *const AVInputFormat,
        *mut *mut AVDictionary,
    ) -> c_int,
    avformat_find_stream_info:
        unsafe extern "C" fn(*mut AVFormatContext, *mut *mut AVDictionary) -> c_int,
    avformat_close_input: unsafe extern "C" fn(*mut *mut AVFormatContext),
    av_guess_frame_rate:
        unsafe extern "C" fn(*mut AVFormatContext, *mut AVStream, *mut AVFrame) -> AVRational,
    av_read_frame: unsafe extern "C" fn(*mut AVFormatContext, *mut AVPacket) -> c_int,
    avcodec_find_decoder: unsafe extern "C" fn(c_int) -> *const AVCodec,
    avcodec_alloc_context3: unsafe extern "C" fn(*const AVCodec) -> *mut AVCodecContext,
    avcodec_parameters_to_context:
        unsafe extern "C" fn(*mut AVCodecContext, *const AVCodecParameters) -> c_int,
    avcodec_open2:
        unsafe extern "C" fn(*mut AVCodecContext, *const AVCodec, *mut *mut AVDictionary) -> c_int,
    avcodec_free_context: unsafe extern "C" fn(*mut *mut AVCodecContext),
    avcodec_send_packet: unsafe extern "C" fn(*mut AVCodecContext, *const AVPacket) -> c_int,
    avcodec_receive_frame: unsafe extern "C" fn(*mut AVCodecContext, *mut AVFrame) -> c_int,
    avcodec_get_name: unsafe extern "C" fn(c_int) -> *const c_char,
    av_packet_alloc: unsafe extern "C" fn() -> *mut AVPacket,
    av_packet_unref: unsafe extern "C" fn(*mut AVPacket),
    av_packet_free: unsafe extern "C" fn(*mut *mut AVPacket),
    av_frame_alloc: unsafe extern "C" fn() -> *mut AVFrame,
    av_frame_unref: unsafe extern "C" fn(*mut AVFrame),
    av_frame_free: unsafe extern "C" fn(*mut *mut AVFrame),
    av_dict_set:
        unsafe extern "C" fn(*mut *mut AVDictionary, *const c_char, *const c_char, c_int) -> c_int,
    av_dict_free: unsafe extern "C" fn(*mut *mut AVDictionary),
    av_strerror: unsafe extern "C" fn(c_int, *mut c_char, usize) -> c_int,
    av_opt_set_int: unsafe extern "C" fn(*mut c_void, *const c_char, i64, c_int) -> c_int,
    av_get_pix_fmt: unsafe extern "C" fn(*const c_char) -> c_int,
    av_get_pix_fmt_name: unsafe extern "C" fn(c_int) -> *const c_char,
    sws_getContext: unsafe extern "C" fn(
        c_int,
        c_int,
        c_int,
        c_int,
        c_int,
        c_int,
        c_int,
        *mut c_void,
        *mut c_void,
        *const f64,
    ) -> *mut SwsContext,
    sws_scale: unsafe extern "C" fn(
        *mut SwsContext,
        *const *const u8,
        *const c_int,
        c_int,
        c_int,
        *const *mut u8,
        *const c_int,
    ) -> c_int,
    sws_getCoefficients: unsafe extern "C" fn(c_int) -> *const c_int,
    sws_setColorspaceDetails: unsafe extern "C" fn(
        *mut SwsContext,
        *const c_int,
        c_int,
        *const c_int,
        c_int,
        c_int,
        c_int,
        c_int,
    ) -> c_int,
    sws_freeContext: unsafe extern "C" fn(*mut SwsContext),
    info: BackendInfo,
    paths: Vec<PathBuf>,
    _libraries: Vec<Library>,
}

fn ctext(pointer: *const c_char) -> Result<String> {
    if pointer.is_null() {
        return Err(error("ffmpeg_api", "FFmpeg 返回空文本指针"));
    }
    // SAFETY: 调用方传入由已验证 FFmpeg API 返回的 NUL 结尾字符串，库仍持有句柄。
    Ok(unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned())
}

impl Api {
    fn load() -> Result<Arc<Self>> {
        type Cache = Mutex<Option<(PathBuf, Arc<Api>)>>;
        static CACHE: OnceLock<Cache> = OnceLock::new();
        let requested = directory()?;
        let canonical = std::fs::canonicalize(&requested).map_err(|e| {
            error(
                "ffmpeg_load",
                format!(
                    "无法访问 FFmpeg 库目录 {}：{e}。请设置 HMI_FFMPEG_DIR",
                    requested.display()
                ),
            )
        })?;
        let mut state = CACHE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| error("ffmpeg_load", "FFmpeg 后端初始化状态无效"))?;
        if let Some((directory, api)) = &*state {
            if *directory != canonical {
                return Err(error(
                    "ffmpeg_path",
                    "FFmpeg 已从另一目录加载；请在新进程中切换库目录",
                ));
            }
            return Ok(Arc::clone(api));
        }
        // 一个进程固定一组库，避免依赖 DLL 同名造成混用，也避免重复加载和校验大文件。
        let api = Arc::new(Self::load_from(&canonical)?);
        *state = Some((canonical, Arc::clone(&api)));
        Ok(api)
    }
    fn load_from(directory: &Path) -> Result<Self> {
        let dir = file::absolute(directory)?;
        let mut libraries = Vec::new();
        let mut paths = Vec::new();
        let mut versions = std::collections::BTreeMap::new();
        let mut fingerprint = String::new();
        for (name, major, minor, patch) in super::LIBRARIES {
            let path = dir.join(format!("{name}-{major}.dll"));
            // SAFETY: 只加载绝对指定的库路径及该目录的依赖，不改 PATH 或进程 DLL 搜索目录。
            let library = unsafe {
                Library::load_with_flags(
                    &path,
                    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
                )
            }
            .map_err(|e| {
                error(
                    "ffmpeg_load",
                    format!(
                        "无法加载 {}：{e}。请通过 HMI_FFMPEG_DIR 指定匹配的 FFmpeg shared 库目录",
                        path.display()
                    ),
                )
            })?;
            let symbol = format!("{name}_version\0");
            // SAFETY: version API 在各 FFmpeg 主版本间具有相同无参签名，先核对版本再访问结构。
            let version = unsafe {
                let f: libloading::os::windows::Symbol<unsafe extern "C" fn() -> u32> = library
                    .get(symbol.as_bytes())
                    .map_err(|e| error("ffmpeg_symbol", e.to_string()))?;
                f()
            };
            if version >> 16 != major || version < (major << 16 | minor << 8 | patch) {
                return Err(error(
                    "ffmpeg_abi",
                    format!(
                        "{name} ABI 不匹配，期望 {major}.{minor}.{patch} 或同主版本的后续兼容版本"
                    ),
                ));
            }
            versions.insert(name.to_string(), version);
            fingerprint.push_str(&hmi_core::sha256(&file::read(&path)?));
            paths.push(path);
            libraries.push(library);
        }
        macro_rules! sym {($index:expr,$name:ident)=>{{
            // SAFETY: 库版本已核对；签名按匹配头文件声明，句柄随 Api 保留。
            unsafe{*libraries[$index].get(concat!(stringify!($name),"\0").as_bytes()).map_err(|e|error("ffmpeg_symbol",format!("缺少 {}：{e}",stringify!($name))))?}
        }}}
        // SAFETY: av_version_info / av_log_set_level 的签名与匹配头文件一致。
        let (version, set_log): (
            unsafe extern "C" fn() -> *const c_char,
            unsafe extern "C" fn(c_int),
        ) = (sym!(0, av_version_info), sym!(0, av_log_set_level));
        let version = ctext(unsafe { version() })?;
        // 错误通过返回码转换成工具链诊断，避免原生日志破坏 CLI 的结构化输出。
        unsafe { set_log(-8) };
        let info = BackendInfo {
            version,
            libraries: versions,
            fingerprint: hmi_core::sha256(fingerprint.as_bytes()),
        };
        Ok(Self {
            av_get_sample_fmt: sym!(0, av_get_sample_fmt),
            av_channel_layout_default: sym!(0, av_channel_layout_default),
            av_channel_layout_copy: sym!(0, av_channel_layout_copy),
            av_channel_layout_compare: sym!(0, av_channel_layout_compare),
            av_channel_layout_uninit: sym!(0, av_channel_layout_uninit),
            swr_alloc_set_opts2: sym!(1, swr_alloc_set_opts2),
            swr_init: sym!(1, swr_init),
            swr_get_out_samples: sym!(1, swr_get_out_samples),
            swr_convert: sym!(1, swr_convert),
            swr_free: sym!(1, swr_free),
            avformat_open_input: sym!(3, avformat_open_input),
            avformat_find_stream_info: sym!(3, avformat_find_stream_info),
            avformat_close_input: sym!(3, avformat_close_input),
            av_guess_frame_rate: sym!(3, av_guess_frame_rate),
            av_read_frame: sym!(3, av_read_frame),
            avcodec_find_decoder: sym!(2, avcodec_find_decoder),
            avcodec_alloc_context3: sym!(2, avcodec_alloc_context3),
            avcodec_parameters_to_context: sym!(2, avcodec_parameters_to_context),
            avcodec_open2: sym!(2, avcodec_open2),
            avcodec_free_context: sym!(2, avcodec_free_context),
            avcodec_send_packet: sym!(2, avcodec_send_packet),
            avcodec_receive_frame: sym!(2, avcodec_receive_frame),
            avcodec_get_name: sym!(2, avcodec_get_name),
            av_packet_alloc: sym!(2, av_packet_alloc),
            av_packet_unref: sym!(2, av_packet_unref),
            av_packet_free: sym!(2, av_packet_free),
            av_frame_alloc: sym!(0, av_frame_alloc),
            av_frame_unref: sym!(0, av_frame_unref),
            av_frame_free: sym!(0, av_frame_free),
            av_dict_set: sym!(0, av_dict_set),
            av_dict_free: sym!(0, av_dict_free),
            av_strerror: sym!(0, av_strerror),
            av_opt_set_int: sym!(0, av_opt_set_int),
            av_get_pix_fmt: sym!(0, av_get_pix_fmt),
            av_get_pix_fmt_name: sym!(0, av_get_pix_fmt_name),
            sws_getContext: sym!(4, sws_getContext),
            sws_scale: sym!(4, sws_scale),
            sws_getCoefficients: sym!(4, sws_getCoefficients),
            sws_setColorspaceDetails: sym!(4, sws_setColorspaceDetails),
            sws_freeContext: sym!(4, sws_freeContext),
            info,
            paths,
            _libraries: libraries,
        })
    }
    fn check(&self, status: c_int, operation: &str) -> Result<()> {
        if status >= 0 {
            return Ok(());
        }
        let mut buffer = [0i8; 256];
        // SAFETY: 提供可写缓冲区及其真实长度。
        unsafe { (self.av_strerror)(status, buffer.as_mut_ptr(), buffer.len()) };
        Err(error(
            "ffmpeg_decode",
            format!("{operation}失败（{status}）：{}", ctext(buffer.as_ptr())?),
        ))
    }
}

macro_rules! owned {
    ($name:ident,$ty:ty,$free:ident) => {
        struct $name<'a> {
            api: &'a Api,
            pointer: *mut $ty,
        }
        impl Drop for $name<'_> {
            fn drop(&mut self) {
                // SAFETY: 指针来自对应 FFmpeg 工厂，free 接受空指针；Api 句柄寿命覆盖资源。
                unsafe { (self.api.$free)(&mut self.pointer) }
            }
        }
    };
}
owned!(Input, AVFormatContext, avformat_close_input);
owned!(Decoder, AVCodecContext, avcodec_free_context);
owned!(Packet, AVPacket, av_packet_free);
owned!(Frame, AVFrame, av_frame_free);
owned!(Dictionary, AVDictionary, av_dict_free);
owned!(Resample, SwrContext, swr_free);
struct Layout<'a> {
    api: &'a Api,
    value: AVChannelLayout,
}
impl<'a> Layout<'a> {
    fn new(api: &'a Api) -> Self {
        // SAFETY: AVChannelLayout 的全零状态是官方规定的未初始化状态。
        Self {
            api,
            value: unsafe { std::mem::zeroed() },
        }
    }
}
impl Drop for Layout<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.av_channel_layout_uninit)(&mut self.value) }
    }
}
struct Scale<'a> {
    api: &'a Api,
    pointer: *mut SwsContext,
}

/// 输出 interleaved s16；时间戳保留为原始流时间基准，媒体层负责截取和音画对齐。
pub(super) fn audio(
    path: &Path,
    rate: u32,
    channels: u16,
    progress: &hmi_core::progress::Progress,
) -> Result<DecodedAudio> {
    progress.set("resample", 0, None, "sample_frames");
    let api = Api::load()?;
    if rate == 0 || rate > 384000 || ![1, 2].contains(&channels) {
        return Err(error("audio_format", "音频转换参数无效"));
    }
    let filename = CString::new(
        file::absolute(path)?
            .to_str()
            .ok_or_else(|| error("audio_path", "音频路径必须能表示为 UTF-8"))?,
    )
    .map_err(|_| error("audio_path", "音频路径不能包含 NUL"))?;
    let mut input = Input {
        api: &api,
        pointer: ptr::null_mut(),
    };
    let mut options = Dictionary {
        api: &api,
        pointer: ptr::null_mut(),
    };
    // SAFETY: 所有结构和函数均来自已经核对 ABI 的库；资源均由 RAII 持有。
    unsafe {
        api.check(
            (api.av_dict_set)(
                &mut options.pointer,
                c"protocol_whitelist".as_ptr(),
                c"file".as_ptr(),
                0,
            ),
            "设置音频本地协议",
        )?;
        api.check(
            (api.avformat_open_input)(
                &mut input.pointer,
                filename.as_ptr(),
                ptr::null(),
                &mut options.pointer,
            ),
            "打开音频",
        )?;
        api.check(
            (api.avformat_find_stream_info)(input.pointer, ptr::null_mut()),
            "读取音频流",
        )?;
        let format = &*input.pointer;
        if format.nb_streams == 0 || format.nb_streams > 64 || format.streams.is_null() {
            return Err(error("audio_streams", "音频流描述无效"));
        }
        let mut selected = None;
        for &stream in std::slice::from_raw_parts(format.streams, format.nb_streams as usize) {
            if stream.is_null() || (*stream).codecpar.is_null() {
                return Err(error("audio_streams", "音频流描述不完整"));
            }
            if (*(*stream).codecpar).codec_type == 1 && selected.replace(stream).is_some() {
                return Err(error("audio_streams", "当前不支持选择或混合多条音轨"));
            }
        }
        let stream = selected.ok_or_else(|| error("audio_streams", "输入没有音轨"))?;
        let tb = (*stream).time_base;
        if tb.num <= 0 || tb.den <= 0 {
            return Err(error("audio_timing", "音频没有有效时间基准"));
        }
        let parameters = (*stream).codecpar;
        let codec_id = (*parameters).codec_id;
        let codec = (api.avcodec_find_decoder)(codec_id);
        if codec.is_null() {
            return Err(error("audio_codec", "当前 FFmpeg 不支持该音频编码"));
        }
        let decoder = Decoder {
            api: &api,
            pointer: (api.avcodec_alloc_context3)(codec),
        };
        if decoder.pointer.is_null() {
            return Err(error("ffmpeg_memory", "无法分配音频解码器"));
        }
        api.check(
            (api.avcodec_parameters_to_context)(decoder.pointer, parameters),
            "设置音频解码参数",
        )?;
        api.check(
            (api.av_opt_set_int)(decoder.pointer.cast(), c"threads".as_ptr(), 1, 0),
            "设置音频解码线程数",
        )?;
        api.check(
            (api.avcodec_open2)(decoder.pointer, codec, ptr::null_mut()),
            "初始化音频解码器",
        )?;
        let packet = Packet {
            api: &api,
            pointer: (api.av_packet_alloc)(),
        };
        let frame = Frame {
            api: &api,
            pointer: (api.av_frame_alloc)(),
        };
        if packet.pointer.is_null() || frame.pointer.is_null() {
            return Err(error("ffmpeg_memory", "无法分配音频包或帧"));
        }
        let mut resampler = Resample {
            api: &api,
            pointer: ptr::null_mut(),
        };
        let mut original = Layout::new(&api);
        let mut source = Layout::new(&api);
        let mut output = Layout::new(&api);
        (api.av_channel_layout_default)(&mut output.value, c_int::from(channels));
        let mut pcm = Vec::new();
        let mut initial = None;
        let mut first_pts = None;
        let mut previous: Option<(i64, i32)> = None;
        let convert = |context: *mut SwrContext,
                       planes: *const *const u8,
                       count: c_int,
                       pcm: &mut Vec<u8>|
         -> Result<usize> {
            let capacity = (api.swr_get_out_samples)(context, count);
            api.check(capacity, "计算音频输出容量")?;
            let bytes = (capacity as usize)
                .checked_mul(usize::from(channels) * 2)
                .filter(|n| *n <= hmi_format::wave::MAX_BYTES)
                .ok_or_else(|| error("audio_length", "单次音频转换数据过大"))?;
            if capacity == 0 {
                return Ok(0);
            }
            let mut buffer = vec![0u8; bytes];
            let out = [buffer.as_mut_ptr()];
            let produced = (api.swr_convert)(context, out.as_ptr(), capacity, planes, count);
            api.check(produced, "转换音频采样")?;
            if produced > capacity {
                return Err(error("audio_length", "音频转换返回了无效长度"));
            }
            let length = produced as usize * usize::from(channels) * 2;
            if pcm
                .len()
                .checked_add(length)
                .is_none_or(|n| n > hmi_format::wave::MAX_BYTES)
            {
                return Err(error("audio_length", "音频采样数据超过 64 MiB"));
            }
            pcm.extend(&buffer[..length]);
            progress.set(
                "resample",
                (pcm.len() / (usize::from(channels) * 2)) as u64,
                None,
                "sample_frames",
            );
            Ok(produced as usize)
        };
        let mut drain = |flushing: bool| -> Result<()> {
            loop {
                let status = (api.avcodec_receive_frame)(decoder.pointer, frame.pointer);
                if status == HMI_ERROR_EOF {
                    return Ok(());
                }
                if status == HMI_EAGAIN {
                    if flushing {
                        return Err(error("audio_decode", "刷新后音频解码器仍请求输入"));
                    }
                    return Ok(());
                }
                api.check(status, "解码音频帧")?;
                let f = &*frame.pointer;
                if f.sample_rate <= 0
                    || f.sample_rate > 384000
                    || f.nb_samples <= 0
                    || f.nb_samples > 1_000_000
                    || f.ch_layout.nb_channels <= 0
                    || f.ch_layout.nb_channels > 32
                    || f.extended_data.is_null()
                {
                    return Err(error("audio_frame", "音频帧采样参数无效"));
                }
                if f.pts == i64::MIN {
                    return Err(error("audio_timing", "音频帧缺少时间戳"));
                }
                if let Some((pts, samples)) = previous
                    && (i128::from(f.pts) - i128::from(pts))
                        * i128::from(tb.num)
                        * i128::from(f.sample_rate)
                        != i128::from(samples) * i128::from(tb.den)
                {
                    return Err(error("audio_timing", "当前不支持带时间间隙或跳变的音轨"));
                }
                previous = Some((f.pts, f.nb_samples));
                first_pts.get_or_insert(f.pts);
                if let Some(shape) = initial {
                    if shape != (f.sample_rate, f.format, f.ch_layout.nb_channels)
                        || (api.av_channel_layout_compare)(&original.value, &f.ch_layout) != 0
                    {
                        return Err(error(
                            "audio_format",
                            "当前不支持中途改变采样参数或声道布局的音轨",
                        ));
                    }
                } else {
                    api.check(
                        (api.av_channel_layout_copy)(&mut original.value, &f.ch_layout),
                        "复制源声道布局",
                    )?;
                    if f.ch_layout.order == 0 {
                        if f.ch_layout.nb_channels > 2 {
                            return Err(error("audio_layout", "多声道音频必须提供明确的声道布局"));
                        }
                        (api.av_channel_layout_default)(&mut source.value, f.ch_layout.nb_channels);
                    } else {
                        api.check(
                            (api.av_channel_layout_copy)(&mut source.value, &f.ch_layout),
                            "复制转换声道布局",
                        )?;
                    }
                    api.check(
                        (api.swr_alloc_set_opts2)(
                            &mut resampler.pointer,
                            &output.value,
                            (api.av_get_sample_fmt)(c"s16".as_ptr()),
                            rate as c_int,
                            &source.value,
                            f.format,
                            f.sample_rate,
                            0,
                            ptr::null_mut(),
                        ),
                        "配置重采样器",
                    )?;
                    if resampler.pointer.is_null() {
                        return Err(error("ffmpeg_memory", "无法分配重采样器"));
                    }
                    api.check((api.swr_init)(resampler.pointer), "初始化重采样器")?;
                    initial = Some((f.sample_rate, f.format, f.ch_layout.nb_channels));
                }
                convert(
                    resampler.pointer,
                    f.extended_data.cast(),
                    f.nb_samples,
                    &mut pcm,
                )?;
                (api.av_frame_unref)(frame.pointer);
            }
        };
        loop {
            let status = (api.av_read_frame)(input.pointer, packet.pointer);
            if status == HMI_ERROR_EOF {
                break;
            }
            api.check(status, "读取音频包")?;
            if (*packet.pointer).stream_index == (*stream).index {
                loop {
                    let s = (api.avcodec_send_packet)(decoder.pointer, packet.pointer);
                    if s == HMI_EAGAIN {
                        drain(false)?;
                        continue;
                    }
                    api.check(s, "提交音频包")?;
                    break;
                }
                drain(false)?;
            }
            (api.av_packet_unref)(packet.pointer);
        }
        loop {
            let s = (api.avcodec_send_packet)(decoder.pointer, ptr::null());
            if s == HMI_EAGAIN {
                drain(false)?;
                continue;
            }
            api.check(s, "刷新音频解码器")?;
            break;
        }
        drain(true)?;
        let (source_rate, _, source_channels) =
            initial.ok_or_else(|| error("audio_samples", "音轨没有采样数据"))?;
        while convert(resampler.pointer, ptr::null(), 0, &mut pcm)? > 0 {}
        let count = (pcm.len() / (usize::from(channels) * 2)) as u64;
        progress.set("resample", count, Some(count), "sample_frames");
        Ok(DecodedAudio {
            pcm,
            sample_rate: rate,
            channels,
            source_rate: source_rate as u32,
            source_channels: source_channels as u16,
            start_pts: first_pts.unwrap(),
            time_base_num: tb.num,
            time_base_den: tb.den,
            codec: ctext((api.avcodec_get_name)(codec_id))?,
            backend: api.info.clone(),
            inputs: api.paths.clone(),
        })
    }
}
impl Drop for Scale<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.sws_freeContext)(self.pointer) }
    }
}

pub(super) fn backend_info() -> Result<BackendInfo> {
    Ok(Api::load()?.info.clone())
}

pub(super) fn decode(
    path: &Path,
    drop_audio: bool,
    progress: &hmi_core::progress::Progress,
) -> Result<DecodedVideo> {
    progress.set("decode", 0, None, "frames");
    let _ = drop_audio; // 音轨策略在媒体层处理，视频解码仅报告流数量。
    let api = Api::load()?;
    let filename = CString::new(
        file::absolute(path)?
            .to_str()
            .ok_or_else(|| error("video_path", "视频路径必须可以表示为 UTF-8"))?,
    )
    .map_err(|_| error("video_path", "视频路径不能包含 NUL"))?;
    let mut input = Input {
        api: &api,
        pointer: ptr::null_mut(),
    };
    let mut options = Dictionary {
        api: &api,
        pointer: ptr::null_mut(),
    };
    // SAFETY: 所有资源使用匹配 ABI 的工厂创建，指针在整个调用期间由 RAII 句柄持有。
    unsafe {
        api.check(
            (api.av_dict_set)(
                &mut options.pointer,
                c"protocol_whitelist".as_ptr(),
                c"file".as_ptr(),
                0,
            ),
            "设置本地文件协议",
        )?;
        api.check(
            (api.avformat_open_input)(
                &mut input.pointer,
                filename.as_ptr(),
                ptr::null(),
                &mut options.pointer,
            ),
            "打开视频",
        )?;
        api.check(
            (api.avformat_find_stream_info)(input.pointer, ptr::null_mut()),
            "读取视频流",
        )?;
        let format = &*input.pointer;
        if format.nb_streams == 0 || format.nb_streams > 64 || format.streams.is_null() {
            return Err(error("video_streams", "视频流数量或描述无效"));
        }
        let mut video = None;
        let mut audio_streams = 0;
        for &stream in std::slice::from_raw_parts(format.streams, format.nb_streams as usize) {
            if stream.is_null() || (*stream).codecpar.is_null() {
                return Err(error("video_streams", "视频流描述不完整"));
            }
            let parameters = (*stream).codecpar;
            if (*parameters).codec_type == 1 {
                audio_streams += 1;
            }
            if (*parameters).codec_type == 0
                && (*stream).disposition & HMI_ATTACHED_PIC == 0
                && video.replace(stream).is_some()
            {
                return Err(error(
                    "video_streams",
                    "当前不支持多个视频流，请先明确需要使用的画面",
                ));
            }
        }
        let stream = video.ok_or_else(|| error("video_streams", "输入中没有可解码的视频流"))?;
        let parameters = (*stream).codecpar;
        super::super::dimensions((*parameters).width as u32, (*parameters).height as u32)?;
        let sar = (*stream).sample_aspect_ratio;
        if sar.num > 0 && sar.den > 0 && sar.num != sar.den {
            return Err(error("video_aspect", "当前仅支持方形像素的视频"));
        }
        if (*parameters).nb_coded_side_data < 0
            || (*parameters).nb_coded_side_data > 1024
            || ((*parameters).nb_coded_side_data > 0 && (*parameters).coded_side_data.is_null())
        {
            return Err(error("video_streams", "视频附加数据无效"));
        }
        if (*parameters).nb_coded_side_data > 0
            && std::slice::from_raw_parts(
                (*parameters).coded_side_data,
                (*parameters).nb_coded_side_data as usize,
            )
            .iter()
            .any(|d| d.type_ == HMI_DISPLAY_MATRIX)
        {
            return Err(error(
                "video_rotation",
                "当前尚未处理源视频的旋转矩阵，请先规范化源方向",
            ));
        }
        let fps = (api.av_guess_frame_rate)(input.pointer, stream, ptr::null_mut());
        let tb = (*stream).time_base;
        if fps.num <= 0 || fps.den <= 0 || tb.num <= 0 || tb.den <= 0 {
            return Err(error("video_timing", "源视频没有可用的帧率或时间基准"));
        }
        let codec_id = (*parameters).codec_id;
        let codec = (api.avcodec_find_decoder)(codec_id);
        if codec.is_null() {
            return Err(error("video_codec", "当前 FFmpeg 库不支持该视频编码"));
        }
        let decoder = Decoder {
            api: &api,
            pointer: (api.avcodec_alloc_context3)(codec),
        };
        if decoder.pointer.is_null() {
            return Err(error("ffmpeg_memory", "无法分配视频解码器"));
        }
        api.check(
            (api.avcodec_parameters_to_context)(decoder.pointer, parameters),
            "设置视频解码参数",
        )?;
        api.check(
            (api.av_opt_set_int)(decoder.pointer.cast(), c"threads".as_ptr(), 1, 0),
            "设置解码线程数",
        )?;
        api.check(
            (api.av_opt_set_int)(
                decoder.pointer.cast(),
                c"max_pixels".as_ptr(),
                (super::super::MAX_RGBA_BYTES / 4) as i64,
                0,
            ),
            "设置解码像素上限",
        )?;
        api.check(
            (api.avcodec_open2)(decoder.pointer, codec, ptr::null_mut()),
            "初始化视频解码器",
        )?;
        let packet = Packet {
            api: &api,
            pointer: (api.av_packet_alloc)(),
        };
        let frame = Frame {
            api: &api,
            pointer: (api.av_frame_alloc)(),
        };
        if packet.pointer.is_null() || frame.pointer.is_null() {
            return Err(error("ffmpeg_memory", "无法分配视频数据包或帧"));
        }
        let mut frames = Vec::new();
        let mut previous_pts = None;
        let mut first_pts = None;
        let mut initial = None;
        let mut total = 0usize;
        let mut scale: Option<Scale<'_>> = None;
        let mut drain = |flushing: bool| -> Result<bool> {
            loop {
                let status = (api.avcodec_receive_frame)(decoder.pointer, frame.pointer);
                if status == HMI_ERROR_EOF {
                    return Ok(true);
                }
                if status == HMI_EAGAIN {
                    if flushing {
                        return Err(error("ffmpeg_decode", "视频解码器刷新后仍请求输入"));
                    }
                    return Ok(false);
                }
                api.check(status, "解码视频帧")?;
                let f = &*frame.pointer;
                let (w, h) = super::super::dimensions(f.width as u32, f.height as u32)?;
                if f.sample_aspect_ratio.num > 0
                    && f.sample_aspect_ratio.den > 0
                    && f.sample_aspect_ratio.num != f.sample_aspect_ratio.den
                {
                    return Err(error("video_aspect", "当前仅支持方形像素的视频帧"));
                }
                if f.crop_left | f.crop_right | f.crop_top | f.crop_bottom != 0
                    || f.flags & HMI_INTERLACED != 0
                {
                    return Err(error("video_layout", "当前尚未支持帧裁剪标记或隔行视频"));
                }
                if matches!(f.color_trc, HMI_TRC_PQ | HMI_TRC_HLG) {
                    return Err(error("video_color", "当前尚未支持 HDR 色调转换"));
                }
                if f.pts == i64::MIN {
                    return Err(error("video_timing", "视频帧缺少有效时间戳"));
                }
                first_pts.get_or_insert(f.pts);
                if let Some(old) = previous_pts
                    && (i128::from(f.pts) - old) * i128::from(tb.num) * i128::from(fps.num)
                        != i128::from(tb.den) * i128::from(fps.den)
                {
                    return Err(error(
                        "video_vfr",
                        "当前仅支持时间戳精确等间隔的固定帧率视频",
                    ));
                }
                previous_pts = Some(i128::from(f.pts));
                if let Some(shape) = initial {
                    if shape != (w, h, f.format) {
                        return Err(error(
                            "video_layout",
                            "当前不支持中途改变尺寸或像素格式的视频",
                        ));
                    }
                } else {
                    initial = Some((w, h, f.format));
                    let context = (api.sws_getContext)(
                        f.width,
                        f.height,
                        f.format,
                        f.width,
                        f.height,
                        (api.av_get_pix_fmt)(c"rgba".as_ptr()),
                        HMI_SWS_BICUBIC,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null(),
                    );
                    if context.is_null() {
                        return Err(error("ffmpeg_scale", "无法创建视频色彩转换器"));
                    }
                    scale = Some(Scale {
                        api: &api,
                        pointer: context,
                    });
                }
                let matrix = match f.colorspace {
                    HMI_COLOR_BT709 => HMI_SWS_CS_709,
                    HMI_COLOR_BT470BG | HMI_COLOR_SMPTE170M | HMI_COLOR_UNSPECIFIED | 0 => {
                        HMI_SWS_CS_601
                    }
                    _ => {
                        return Err(error(
                            "video_color",
                            "当前仅支持 BT.601／BT.709 视频色彩矩阵",
                        ));
                    }
                };
                let coefficients = (api.sws_getCoefficients)(matrix);
                if coefficients.is_null() {
                    return Err(error("ffmpeg_scale", "无法获取视频色彩矩阵"));
                }
                let name = ctext((api.av_get_pix_fmt_name)(f.format))?;
                let range = c_int::from(
                    f.color_range == HMI_RANGE_JPEG
                        || (f.color_range == 0 && name.starts_with("yuvj")),
                );
                let context = scale.as_ref().unwrap().pointer;
                api.check(
                    (api.sws_setColorspaceDetails)(
                        context,
                        coefficients,
                        range,
                        coefficients,
                        1,
                        0,
                        1 << 16,
                        1 << 16,
                    ),
                    "设置视频色彩范围",
                )?;
                let size = usize::from(w) * usize::from(h) * 4;
                total = total
                    .checked_add(size)
                    .filter(|n| *n <= 128 * 1024 * 1024)
                    .ok_or_else(|| error("video_length", "视频解码 RGBA 总量超过 128 MiB"))?;
                if frames.len() >= 10000 {
                    return Err(error("video_frames", "当前视频最多支持 10000 个源帧"));
                }
                let mut rgba = vec![0u8; size];
                let output = [
                    rgba.as_mut_ptr(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                ];
                let strides = [c_int::from(w) * 4, 0, 0, 0];
                if (api.sws_scale)(
                    context,
                    f.data.as_ptr().cast(),
                    f.linesize.as_ptr(),
                    0,
                    f.height,
                    output.as_ptr(),
                    strides.as_ptr(),
                ) != f.height
                {
                    return Err(error("ffmpeg_scale", "视频帧色彩转换不完整"));
                }
                if rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 255) {
                    return Err(error("video_alpha", "当前 VIDEO 编码不支持透明视频"));
                }
                frames.push(
                    image::RgbaImage::from_raw(u32::from(w), u32::from(h), rgba)
                        .ok_or_else(|| error("video_layout", "视频像素尺寸不一致"))?,
                );
                progress.set("decode", frames.len() as u64, None, "frames");
                (api.av_frame_unref)(frame.pointer);
            }
        };
        loop {
            let status = (api.av_read_frame)(input.pointer, packet.pointer);
            if status == HMI_ERROR_EOF {
                break;
            }
            api.check(status, "读取视频数据包")?;
            if (*packet.pointer).stream_index == (*stream).index {
                loop {
                    let status = (api.avcodec_send_packet)(decoder.pointer, packet.pointer);
                    if status == HMI_EAGAIN {
                        drain(false)?;
                        continue;
                    }
                    api.check(status, "提交视频数据包")?;
                    break;
                }
                drain(false)?;
            }
            (api.av_packet_unref)(packet.pointer);
        }
        loop {
            let status = (api.avcodec_send_packet)(decoder.pointer, ptr::null());
            if status == HMI_EAGAIN {
                drain(false)?;
                continue;
            }
            api.check(status, "刷新视频解码器")?;
            break;
        }
        drain(true)?;
        if frames.is_empty() {
            return Err(error("video_frames", "视频没有可用画面"));
        }
        progress.set(
            "decode",
            frames.len() as u64,
            Some(frames.len() as u64),
            "frames",
        );
        let codec = ctext((api.avcodec_get_name)(codec_id))?;
        Ok(DecodedVideo {
            start_pts: first_pts.unwrap(),
            time_base_num: tb.num,
            time_base_den: tb.den,
            frames,
            fps_num: fps.num as u32,
            fps_den: fps.den as u32,
            audio_streams,
            codec,
            backend: api.info.clone(),
            inputs: api.paths.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_dll_directory_is_a_structured_error() {
        let temp = tempfile::tempdir().unwrap();
        let error = super::Api::load_from(temp.path()).err().unwrap();
        assert_eq!(error.as_diagnostic().unwrap().code, "ffmpeg_load");
    }
}
