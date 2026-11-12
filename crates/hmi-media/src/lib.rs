//! 常规图片解码与 K/T RGB565 编码。运行资源和编辑源作为同一个资产目标交付。

use hmi_artifact::asset::{AssetObject, ImageInfo};
use hmi_core::{
    Result,
    asset::{Alpha, ImageSource, ResizeMode},
    error, file,
    model::check_schema,
    sha256,
    target::Target,
};
use image::{
    DynamicImage, ImageDecoder, ImageFormat, ImageReader, Rgba, RgbaImage,
    imageops::{self, FilterType},
};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

const MAX_RGBA_BYTES: u64 = 64 * 1024 * 1024;
pub struct ImageProbe {
    pub info: ImageInfo,
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
    if !target
        .capabilities
        .image_codecs
        .iter()
        .any(|c| c == "rgb565")
    {
        return Err(error("image_backend", "当前目标尚未接入可用的图片编码路线"));
    }
    let config: ImageSource = file::read_toml(path)?;
    check_schema(&config.schema, "hmi.asset/1")?;
    if config.kind != "image" || config.input != "source" || config.original.is_some() {
        return Err(error(
            "asset_input",
            "当前编码器支持 image 的 source 输入，尚未接入编码文件和原始载荷复用",
        ));
    }
    if !matches!(config.encoding.codec.as_str(), "auto" | "rgb565")
        || config.encoding.quality.is_some()
    {
        return Err(error(
            "image_codec",
            "当前目标使用 RGB565，不接受 JPEG 编码或 quality 参数",
        ));
    }
    if config.delivery.mode != "embedded" || config.delivery.path.is_some() {
        return Err(error(
            "asset_delivery",
            "当前 RGB565 路线仅支持内嵌图片，不接受外部路径",
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
    } else if pixels.pixels().any(|pixel| pixel[3] != 255) {
        return Err(error(
            "image_alpha",
            "RGB565 不支持透明像素；请使用 alpha=flatten 并指定背景色",
        ));
    }
    let (width, height) = dimensions(pixels.width(), pixels.height())?;
    let preserve_bmp = format == ImageFormat::Bmp
        && config.resize.is_none()
        && config.encoding.alpha == Alpha::Preserve
        && bytes.len() >= 54
        && &bytes[..2] == b"BM"
        && bytes[14..18] == 40u32.to_le_bytes()
        && bytes[26..30] == [1, 0, 24, 0]
        && bytes[30..34] == [0; 4];
    let source = if preserve_bmp {
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
    let (storage_width, storage_height) = if matches!(target.orientation, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    };
    let info = ImageInfo {
        width,
        height,
        storage_width,
        storage_height,
        orientation: target.orientation,
        codec: "rgb565".into(),
        alpha: config.encoding.alpha,
        background: config.encoding.background,
        resize: config.resize,
        source_format: source_format.into(),
        editor_source_format: "bmp".into(),
        source_sha256: sha256(&bytes),
        backend: "image-rs/0.25.10-rgb565/1".into(),
    };
    Ok(PreparedImage {
        info,
        pixels,
        source,
        inputs: vec![path.into(), source_path],
    })
}

/// 探测会完整解码和检查变换，但不生成运行像素、.hma 或其他输出文件。
pub fn probe(path: &Path, target: &Target) -> Result<ImageProbe> {
    let p = prepare(path, target).map_err(|e| file::at_file(e, path))?;
    Ok(ImageProbe {
        info: p.info,
        inputs: p.inputs,
    })
}

pub fn encode(path: &Path, target: &Target) -> Result<EncodingResult> {
    let p = prepare(path, target).map_err(|e| file::at_file(e, path))?;
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
