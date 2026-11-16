//! 项目自绘图标：固定输入在程序中内嵌，check不落盘，编码仍走现有JPEG／透明度路线。
use super::*;
use hmi_core::builtin::BuiltinImage;
fn prepare(id: BuiltinImage, target: &Target) -> Result<PreparedImage> {
    target.validate()?;
    if !target
        .capabilities
        .control_types
        .iter()
        .any(|c| c == "file_browser")
    {
        return Err(error("builtin_target", "当前目标不支持文件浏览器内置图标"));
    }
    let source = match id {
        BuiltinImage::BrowserFolder => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/icons/browser-folder.png"
        ))
        .as_slice(),
        BuiltinImage::BrowserFile => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/icons/browser-file.png"
        ))
        .as_slice(),
    };
    if sha256(source) != id.source_sha256() {
        return Err(error("builtin_asset", "内置图标数据与登记摘要不一致"));
    }
    let pixels = image::load_from_memory(source)
        .map_err(|e| error("builtin_asset", e.to_string()))?
        .to_rgba8();
    let info = ImageInfo {
        delivery: Default::default(),
        width: 64,
        height: 64,
        storage_width: 64,
        storage_height: 64,
        orientation: target.orientation,
        codec: "jpeg".into(),
        alpha: Alpha::Preserve,
        background: None,
        resize: None,
        source_format: "png".into(),
        editor_source_format: "png".into(),
        source_sha256: id.source_sha256().into(),
        backend: "hmi-builtin-icons/1+jpeg-encoder/0.6.1".into(),
        jpeg: Some(JpegInfo {
            quality: Some(80),
            padded_width: 64,
            padded_height: 64,
            sampling: "yuv420".into(),
            has_alpha: true,
        }),
    };
    Ok(PreparedImage {
        info,
        pixels,
        source: source.to_vec(),
        inputs: Vec::new(),
    })
}
pub fn probe(id: BuiltinImage, target: &Target) -> Result<AssetProbe> {
    let p = prepare(id, target)?;
    Ok(AssetProbe {
        info: AssetInfo::Image { image: p.info },
        inputs: Vec::new(),
        action: "builtin",
    })
}
pub fn encode(id: BuiltinImage, target: &Target) -> Result<EncodingResult> {
    let mut r = super::encode_image(prepare(id, target)?)?;
    r.action = "builtin";
    Ok(r)
}
