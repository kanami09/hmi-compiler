use hmi_artifact::asset::AssetObject;
use hmi_core::{
    asset::{Alpha, ImageSource, Resize, ResizeMode},
    file,
    target::{Target, TargetConfig},
};
use image::ImageEncoder;
use std::{fs, path::Path};

fn target(orientation: u16) -> Target {
    Target::resolve(&TargetConfig {
        profile: "TJC4832K035_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation,
    })
    .unwrap()
}
fn bmp() -> Vec<u8> {
    // 独立写入底向上 BGR 行和填充；六种颜色便于判断旋转和截位结果。
    let mut b = vec![0; 78];
    b[..2].copy_from_slice(b"BM");
    b[2..6].copy_from_slice(&78u32.to_le_bytes());
    b[10..14].copy_from_slice(&54u32.to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());
    b[18..22].copy_from_slice(&2u32.to_le_bytes());
    b[22..26].copy_from_slice(&3u32.to_le_bytes());
    b[26..30].copy_from_slice(&[1, 0, 24, 0]);
    b[54..60].copy_from_slice(&[255, 255, 0, 255, 255, 255]);
    b[62..68].copy_from_slice(&[255, 0, 0, 0, 255, 255]);
    b[70..76].copy_from_slice(&[0, 0, 255, 0, 255, 0]);
    b
}
fn description(root: &Path, config: &ImageSource) -> std::path::PathBuf {
    let path = root.join("image.asset.toml");
    fs::write(&path, file::toml(config).unwrap()).unwrap();
    path
}
fn colors(asset: &AssetObject) -> Vec<u16> {
    asset.member(0).unwrap()[24..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect()
}

#[test]
fn rgb565_rotation_preserves_logical_dimensions_and_bmp_source() {
    let tmp = tempfile::tempdir().unwrap();
    let bytes = bmp();
    fs::write(tmp.path().join("source.bmp"), &bytes).unwrap();
    let config = ImageSource {
        source: "source.bmp".into(),
        ..Default::default()
    };
    let path = description(tmp.path(), &config);
    for (orientation, expected) in [
        (0, vec![0xf800, 0x07e0, 0x001f, 0xffe0, 0x07ff, 0xffff]),
        (90, vec![0x07ff, 0x001f, 0xf800, 0xffff, 0xffe0, 0x07e0]),
        (180, vec![0xffff, 0x07ff, 0xffe0, 0x001f, 0x07e0, 0xf800]),
        (270, vec![0x07e0, 0xffe0, 0xffff, 0xf800, 0x001f, 0x07ff]),
    ] {
        let asset = hmi_media::encode(&path, &target(orientation))
            .unwrap()
            .object;
        assert_eq!(colors(&asset), expected);
        assert_eq!(
            (
                asset.metadata.info.image().unwrap().width,
                asset.metadata.info.image().unwrap().height
            ),
            (2, 3)
        );
        assert_eq!(&asset.member(1).unwrap()[27..], bytes);
        let encoded = asset.encode().unwrap();
        assert_eq!(
            AssetObject::decode(&encoded).unwrap().payload,
            asset.payload
        );
        let mut corrupted = encoded.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        assert_eq!(
            AssetObject::decode(&corrupted)
                .unwrap_err()
                .as_diagnostic()
                .unwrap()
                .code,
            "asset_hash"
        );
        let mut truncated = encoded;
        truncated.pop();
        assert!(AssetObject::decode(&truncated).is_err());
    }
}

#[test]
fn signature_png_alpha_and_resize_follow_explicit_parameters() {
    let tmp = tempfile::tempdir().unwrap();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[255, 0, 0, 128], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    fs::write(tmp.path().join("misleading.jpg"), png).unwrap();
    let mut config = ImageSource {
        source: "misleading.jpg".into(),
        ..Default::default()
    };
    let path = description(tmp.path(), &config);
    assert_eq!(
        hmi_media::probe(&path, &target(0))
            .err()
            .unwrap()
            .as_diagnostic()
            .unwrap()
            .code,
        "image_alpha"
    );
    config.resize = Some(Resize {
        width: 2,
        height: 3,
        mode: ResizeMode::Contain,
        background: Some("#FFFFFFFF".into()),
    });
    description(tmp.path(), &config);
    assert!(hmi_media::probe(&path, &target(0)).is_err()); // 填充不能隐式去除主体透明度。
    config.encoding.alpha = Alpha::Flatten;
    config.encoding.background = Some("#000000".into());
    description(tmp.path(), &config);
    let asset = hmi_media::encode(&path, &target(0)).unwrap().object;
    assert_eq!(asset.metadata.info.image().unwrap().source_format, "png");
    assert_eq!(
        colors(&asset),
        [0x8000, 0x8000, 0x8000, 0x8000, 0xffff, 0xffff]
    );
    assert_eq!(&asset.member(1).unwrap()[27..29], b"BM");
    let probe = hmi_media::probe(&path, &target(0)).unwrap();
    assert_eq!(probe.info, asset.metadata.info);
    config.encoding.quality = Some(80);
    description(tmp.path(), &config);
    assert!(hmi_media::encode(&path, &target(0)).is_err());
}

#[test]
fn jpeg_and_asset_target_validation_require_no_original_file_at_link_time() {
    let tmp = tempfile::tempdir().unwrap();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode(
            &[220, 80, 30].repeat(16 * 16),
            16,
            16,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    fs::write(tmp.path().join("source.data"), jpeg).unwrap();
    let config = ImageSource {
        source: "source.data".into(),
        ..Default::default()
    };
    let path = description(tmp.path(), &config);
    let asset = hmi_media::encode(&path, &target(0)).unwrap().object;
    assert_eq!(asset.metadata.info.image().unwrap().source_format, "jpeg");
    asset.validate_for(&target(0)).unwrap();
    assert!(asset.validate_for(&target(90)).is_err());
    let encoded = asset.encode().unwrap();
    tmp.close().unwrap();
    AssetObject::decode(&encoded)
        .unwrap()
        .validate_for(&target(0))
        .unwrap();
}
