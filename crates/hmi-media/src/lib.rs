//! 常规图片编码与 ZI 字库方向适配。链接器只需消费输出的媒体目标。

use hmi_artifact::asset::{AssetInfo, AssetObject, FontInfo, ImageInfo, JpegInfo};
use hmi_core::{
    Result,
    asset::{Alpha, FontSource, ImageSource, ResizeMode},
    error, file,
    model::check_schema,
    sha256,
    target::Target,
};
use image::{
    DynamicImage, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Rgba, RgbaImage,
    imageops::{self, FilterType},
};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

const MAX_RGBA_BYTES: u64 = 64 * 1024 * 1024;
pub struct AssetProbe {
    pub info: AssetInfo,
    pub inputs: Vec<PathBuf>,
}
pub struct EncodingResult {
    pub object: AssetObject,
    pub inputs: Vec<PathBuf>,
}
struct PreparedImage {
    info: ImageInfo,
    pixels: RgbaImage,
    source: Vec<u8>,
    inputs: Vec<PathBuf>,
}

fn color(text: &str, alpha: bool) -> Result<[u8; 4]> {
    let digits = text
        .strip_prefix('#')
        .ok_or_else(|| error("image_color", "颜色须使用 #RRGGBB 或 #RRGGBBAA"))?;
    if digits.len() != if alpha { 8 } else { 6 } || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(error("image_color", "背景颜色的长度或十六进制数值无效"));
    }
    let mut value = [0, 0, 0, 255];
    for (index, channel) in value.iter_mut().enumerate().take(digits.len() / 2) {
        *channel = u8::from_str_radix(&digits[index * 2..index * 2 + 2], 16)
            .map_err(|_| error("image_color", "背景颜色无效"))?;
    }
    Ok(value)
}
fn dimensions(width: u32, height: u32) -> Result<(u16, u16)> {
    if width == 0
        || height == 0
        || width > 65535
        || height > 65535
        || u64::from(width) * u64::from(height) * 4 > MAX_RGBA_BYTES
    {
        return Err(error(
            "image_dimensions",
            "图片尺寸无效或解码后 RGBA 数据超过 64 MiB",
        ));
    }
    Ok((width as u16, height as u16))
}

fn prepare(path: &Path, target: &Target) -> Result<PreparedImage> {
    target.validate()?;
    let config: ImageSource = file::read_toml(path)?;
    check_schema(&config.schema, "hmi.asset/1")?;
    if config.kind != "image" || config.input != "source" || config.original.is_some() {
        return Err(error(
            "asset_input",
            "当前编码器支持 image 的 source 输入，尚未接入编码文件和原始载荷复用",
        ));
    }
    let codec = if config.encoding.codec == "auto" {
        target
            .capabilities
            .image_codecs
            .first()
            .map(String::as_str)
            .ok_or_else(|| error("image_backend", "当前目标尚未接入图片编码"))?
    } else {
        &config.encoding.codec
    };
    if !matches!(codec, "rgb565" | "jpeg")
        || !target.capabilities.image_codecs.iter().any(|c| c == codec)
    {
        return Err(error(
            "image_codec",
            format!("目标 {} 不支持图片编码 {codec}", target.profile),
        ));
    }
    let quality = config.encoding.quality.unwrap_or(80);
    if codec == "rgb565" && config.encoding.quality.is_some() {
        return Err(error("image_codec", "RGB565 不接受 quality 参数"));
    }
    if codec == "jpeg" && !(1..=100).contains(&quality) {
        return Err(error("image_quality", "JPEG quality 必须为 1～100"));
    }
    config.delivery.validate()?;
    if config.delivery.mode == "external"
        && (codec != "jpeg"
            || !target
                .capabilities
                .control_types
                .iter()
                .any(|c| c == "external_picture")
            || !config
                .delivery
                .path
                .as_deref()
                .is_some_and(|p| p.to_ascii_lowercase().ends_with(".xi")))
    {
        return Err(error(
            "asset_delivery",
            "外部图片当前需要支持外部图片控件的型号、JPEG 编码及 .xi 部署路径",
        ));
    }
    if config.encoding.alpha == Alpha::Preserve && config.encoding.background.is_some() {
        return Err(error(
            "image_alpha",
            "只有 alpha=flatten 时才能指定编码背景色",
        ));
    }
    let source_path = path.parent().unwrap_or(Path::new(".")).join(&config.source);
    let bytes = file::read(&source_path)?;
    let mut reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| file::at_file(e.into(), &source_path))?;
    let format = reader
        .format()
        .ok_or_else(|| error("image_format", "无法识别图片格式"))?;
    let source_format = match format {
        ImageFormat::Bmp => "bmp",
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        _ => return Err(error("image_format", "当前仅支持 BMP、PNG 和 JPG 输入")),
    };
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(65535);
    limits.max_image_height = Some(65535);
    limits.max_alloc = Some(MAX_RGBA_BYTES);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|e| file::at_file(error("image_decode", e.to_string()), &source_path))?;
    let (width, height) = decoder.dimensions();
    dimensions(width, height)?;
    let mut pixels = DynamicImage::from_decoder(decoder)
        .map_err(|e| file::at_file(error("image_decode", e.to_string()), &source_path))?
        .to_rgba8();
    if let Some(resize) = &config.resize {
        dimensions(resize.width, resize.height)?;
        pixels = match resize.mode {
            ResizeMode::Stretch => {
                if resize.background.is_some() {
                    return Err(error("image_resize", "stretch 不接受填充背景"));
                }
                imageops::resize(&pixels, resize.width, resize.height, FilterType::Triangle)
            }
            ResizeMode::Contain => {
                let background = color(
                    resize.background.as_deref().ok_or_else(|| {
                        error("image_resize", "contain 必须指定 #RRGGBBAA 填充背景")
                    })?,
                    true,
                )?;
                // 使用整数比例确定画布内尺寸，避免越界；原图不裁切。
                let (w, h) = if u64::from(resize.width) * u64::from(height)
                    <= u64::from(resize.height) * u64::from(width)
                {
                    (
                        resize.width,
                        (u64::from(height) * u64::from(resize.width) / u64::from(width)).max(1)
                            as u32,
                    )
                } else {
                    (
                        (u64::from(width) * u64::from(resize.height) / u64::from(height)).max(1)
                            as u32,
                        resize.height,
                    )
                };
                let resized = imageops::resize(&pixels, w, h, FilterType::Triangle);
                let mut canvas =
                    RgbaImage::from_pixel(resize.width, resize.height, Rgba(background));
                imageops::replace(
                    &mut canvas,
                    &resized,
                    i64::from((resize.width - w) / 2),
                    i64::from((resize.height - h) / 2),
                );
                canvas
            }
        };
    }
    if config.encoding.alpha == Alpha::Flatten {
        let background = color(
            config
                .encoding
                .background
                .as_deref()
                .ok_or_else(|| error("image_alpha", "alpha=flatten 必须指定 #RRGGBB 背景色"))?,
            false,
        )?;
        for pixel in pixels.pixels_mut() {
            let alpha = u32::from(pixel[3]);
            for channel in 0..3 {
                pixel[channel] = ((u32::from(pixel[channel]) * alpha
                    + u32::from(background[channel]) * (255 - alpha)
                    + 127)
                    / 255) as u8;
            }
            pixel[3] = 255;
        }
    } else if codec == "rgb565" && pixels.pixels().any(|pixel| pixel[3] != 255) {
        return Err(error(
            "image_alpha",
            "RGB565 不支持透明像素；请使用 alpha=flatten 并指定背景色",
        ));
    }
    let (width, height) = dimensions(pixels.width(), pixels.height())?;
    let has_alpha = codec == "jpeg" && pixels.pixels().any(|pixel| pixel[3] != 255);
    let preserve_bmp = format == ImageFormat::Bmp
        && config.resize.is_none()
        && config.encoding.alpha == Alpha::Preserve
        && bytes.len() >= 54
        && &bytes[..2] == b"BM"
        && u32::try_from(bytes.len())
            .ok()
            .is_some_and(|size| bytes[2..6] == size.to_le_bytes())
        && bytes[14..18] == 40u32.to_le_bytes()
        && bytes[26..30] == [1, 0, 24, 0]
        && bytes[30..34] == [0; 4]
        && bytes[18..22] == u32::from(width).to_le_bytes()
        && i32::from_le_bytes(bytes[22..26].try_into().unwrap()).unsigned_abs()
            == u32::from(height)
        && (u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize)
            .checked_add(((usize::from(width) * 3 + 3) & !3) * usize::from(height))
            == Some(bytes.len());
    let source = if has_alpha {
        // 透明编辑源使用 RGBA8 PNG，保留逻辑画布的八位透明度，不从七位预览反推。
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                pixels.as_raw(),
                u32::from(width),
                u32::from(height),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| error("image_encode", e.to_string()))?;
        png
    } else if preserve_bmp {
        bytes.clone()
    } else {
        // PNG/JPG、缩放或去透明后的编辑源统一为 BMP；工程原始输入仍保持不变。
        let rgb = DynamicImage::ImageRgba8(pixels.clone()).to_rgb8();
        let mut bmp = Vec::new();
        image::codecs::bmp::BmpEncoder::new(&mut bmp)
            .encode(
                rgb.as_raw(),
                u32::from(width),
                u32::from(height),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| error("image_encode", e.to_string()))?;
        bmp
    };
    let jpeg = if codec == "jpeg" {
        let (padded_width, padded_height) =
            hmi_format::picture::jpeg::padded_dimensions(width, height)?;
        dimensions(u32::from(padded_width), u32::from(padded_height))?;
        Some(JpegInfo {
            quality,
            padded_width,
            padded_height,
            sampling: "yuv420".into(),
            has_alpha,
        })
    } else {
        None
    };
    let (storage_width, storage_height) = if codec == "jpeg" {
        hmi_format::picture::jpeg::storage_dimensions(width, height, target.orientation)?
    } else if matches!(target.orientation, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    };
    let backend = if codec == "jpeg" {
        "image-rs/0.25.10+jpeg-encoder/0.6.1-yuv420-alpha7/2"
    } else {
        "image-rs/0.25.10-rgb565/1"
    };
    let info = ImageInfo {
        delivery: config.delivery,
        width,
        height,
        storage_width,
        storage_height,
        orientation: target.orientation,
        codec: codec.into(),
        alpha: config.encoding.alpha,
        background: config.encoding.background,
        resize: config.resize,
        source_format: source_format.into(),
        editor_source_format: if has_alpha { "png" } else { "bmp" }.into(),
        source_sha256: sha256(&bytes),
        backend: backend.into(),
        jpeg,
    };
    Ok(PreparedImage {
        info,
        pixels,
        source,
        inputs: vec![path.into(), source_path],
    })
}

/// 探测会完整解码和检查变换，但不生成运行像素、.hma 或其他输出文件。
pub fn probe(path: &Path, target: &Target) -> Result<AssetProbe> {
    if is_font(path)? {
        let (info, _, inputs) = prepare_font(path, target).map_err(|e| file::at_file(e, path))?;
        return Ok(AssetProbe {
            info: AssetInfo::Font { font: info },
            inputs,
        });
    }
    let p = prepare(path, target).map_err(|e| file::at_file(e, path))?;
    Ok(AssetProbe {
        info: AssetInfo::Image { image: p.info },
        inputs: p.inputs,
    })
}

pub fn encode(path: &Path, target: &Target) -> Result<EncodingResult> {
    if is_font(path)? {
        let (info, bytes, inputs) =
            prepare_font(path, target).map_err(|e| file::at_file(e, path))?;
        let output = hmi_format::font::adapt(&bytes, target.orientation)?;
        return Ok(EncodingResult {
            object: AssetObject::font(info, output)?,
            inputs,
        });
    }
    let p = prepare(path, target).map_err(|e| file::at_file(e, path))?;
    if let Some(settings) = &p.info.jpeg {
        // 先在未旋转画布右侧／下侧补齐，再旋转存储光栅，避免裁剪位置错位。
        let mut padded = RgbaImage::from_pixel(
            u32::from(settings.padded_width),
            u32::from(settings.padded_height),
            Rgba([0, 0, 0, if settings.has_alpha { 0 } else { 255 }]),
        );
        imageops::replace(&mut padded, &p.pixels, 0, 0);
        let rotated = match p.info.orientation {
            0 => padded,
            90 => imageops::rotate90(&padded),
            180 => imageops::rotate180(&padded),
            270 => imageops::rotate270(&padded),
            _ => unreachable!("有效目标方向已校验"),
        };
        let alpha = settings.has_alpha.then(|| {
            hmi_format::picture::alpha7::encode(&rotated.pixels().map(|p| p[3]).collect::<Vec<_>>())
        });
        let rgb = DynamicImage::ImageRgba8(rotated).to_rgb8();
        let mut bytes = Vec::new();
        let mut encoder = jpeg_encoder::Encoder::new(&mut bytes, settings.quality);
        // 所有质量都显式使用 4:2:0 和非渐进帧，质量 100 也不切换私有编码。
        encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
        encoder.set_progressive(false);
        encoder
            .encode(
                rgb.as_raw(),
                p.info.storage_width,
                p.info.storage_height,
                jpeg_encoder::ColorType::Rgb,
            )
            .map_err(|e| error("image_encode", e.to_string()))?;
        let runtime = hmi_format::picture::jpeg::runtime_with_alpha(
            p.info.width,
            p.info.height,
            p.info.orientation,
            &bytes,
            alpha.as_deref(),
        )?;
        let source = hmi_format::picture::editor_source_with_format(
            p.info.width,
            p.info.height,
            &p.info.editor_source_format,
            &p.source,
        )?;
        return Ok(EncodingResult {
            object: AssetObject::image(p.info, runtime, source)?,
            inputs: p.inputs,
        });
    }
    let w = u32::from(p.info.width);
    let h = u32::from(p.info.height);
    let mut encoded = Vec::new();
    let mut push = |x: u32, y: u32| {
        let c = p.pixels.get_pixel(x, y);
        let value =
            ((u16::from(c[0]) >> 3) << 11) | ((u16::from(c[1]) >> 2) << 5) | (u16::from(c[2]) >> 3);
        encoded.extend(value.to_le_bytes());
    };
    match p.info.orientation {
        0 => {
            for y in 0..h {
                for x in 0..w {
                    push(x, y);
                }
            }
        }
        90 => {
            for x in 0..w {
                for y in (0..h).rev() {
                    push(x, y);
                }
            }
        }
        180 => {
            for y in (0..h).rev() {
                for x in (0..w).rev() {
                    push(x, y);
                }
            }
        }
        270 => {
            for x in (0..w).rev() {
                for y in 0..h {
                    push(x, y);
                }
            }
        }
        _ => return Err(error("image_orientation", "图片方向无效")),
    }
    let runtime =
        hmi_format::picture::runtime(p.info.width, p.info.height, p.info.orientation, &encoded)?;
    let source = hmi_format::picture::editor_source(p.info.width, p.info.height, &p.source)?;
    hmi_format::picture::validate(
        &runtime,
        &source,
        p.info.width,
        p.info.height,
        p.info.orientation,
    )?;
    Ok(EncodingResult {
        object: AssetObject::image(p.info, runtime, source)?,
        inputs: p.inputs,
    })
}

fn is_font(path: &Path) -> Result<bool> {
    let value: toml::Value = file::read_toml(path)?;
    match value.get("kind").and_then(toml::Value::as_str) {
        Some("font") => Ok(true),
        Some("image") => Ok(false),
        _ => Err(error("asset_kind", "当前媒体编码器支持 image 和 font 资产")),
    }
}

fn prepare_font(path: &Path, target: &Target) -> Result<(FontInfo, Vec<u8>, Vec<PathBuf>)> {
    target.validate()?;
    if !target.capabilities.font_versions.contains(&6) {
        return Err(error("font_backend", "目标尚未接入 ZI v6 字库"));
    }
    let config: FontSource = file::read_toml(path)?;
    check_schema(&config.schema, "hmi.asset/1")?;
    if config.kind != "font" || config.input != "source" {
        return Err(error("asset_input", "字库编码器需要 font 的 source 输入"));
    }
    if config.delivery.mode != "embedded" || config.delivery.path.is_some() {
        return Err(error("asset_delivery", "当前字库路线仅支持内嵌资源"));
    }
    let source = path.parent().unwrap_or(Path::new(".")).join(&config.source);
    let bytes = file::read(&source)?;
    let header = hmi_format::font::inspect(&bytes).map_err(|e| file::at_file(e, &source))?;
    let info = FontInfo {
        version: 6,
        encoding: "utf-8".into(),
        height: header.height,
        glyph_count: header.glyph_count,
        orientation: target.orientation,
        source_orientation: header.orientation,
        source_sha256: sha256(&bytes),
        backend: "hmi-zi-v6-rle/1".into(),
    };
    Ok((info, bytes, vec![path.into(), source]))
}
