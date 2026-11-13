use hmi_artifact::asset::AssetObject;
use hmi_core::{
    asset::ImageSource,
    file,
    target::{Target, TargetConfig},
};
use image::ImageEncoder;
use std::{fs, path::Path};
fn target(orientation: u16) -> Target {
    Target::resolve(&TargetConfig {
        profile: "TJC4827X543_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation,
    })
    .unwrap()
}
fn bmp(w: u32, h: u32) -> Vec<u8> {
    let stride = (w * 3 + 3) & !3;
    let mut b = vec![0; (54 + stride * h) as usize];
    b[..2].copy_from_slice(b"BM");
    let len = b.len() as u32;
    b[2..6].copy_from_slice(&len.to_le_bytes());
    b[10..14].copy_from_slice(&54u32.to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());
    b[18..22].copy_from_slice(&w.to_le_bytes());
    b[22..26].copy_from_slice(&h.to_le_bytes());
    b[26..30].copy_from_slice(&[1, 0, 24, 0]);
    for y in 0..h {
        for x in 0..w {
            let rgb = match (x >= w / 2, y >= h / 2) {
                (false, false) => [255, 0, 0],
                (true, false) => [0, 255, 0],
                (false, true) => [0, 0, 255],
                _ => [255, 255, 0],
            };
            let p = (54 + (h - 1 - y) * stride + x * 3) as usize;
            b[p..p + 3].copy_from_slice(&[rgb[2], rgb[1], rgb[0]]);
        }
    }
    b
}
fn description(root: &Path, c: &ImageSource) -> std::path::PathBuf {
    let p = root.join("image.asset.toml");
    fs::write(&p, file::toml(c).unwrap()).unwrap();
    p
}

#[test]
fn padded_rotations_decode_to_expected_colors_and_keep_original_editor_source() {
    let tmp = tempfile::tempdir().unwrap();
    let source = bmp(61, 35);
    fs::write(tmp.path().join("source.bmp"), &source).unwrap();
    let p = description(
        tmp.path(),
        &ImageSource {
            source: "source.bmp".into(),
            ..Default::default()
        },
    );
    for orientation in [0, 90, 180, 270] {
        let asset = hmi_media::encode(&p, &target(orientation)).unwrap().object;
        let info = asset.metadata.info.image().unwrap();
        assert_eq!((info.width, info.height), (61, 35));
        assert_eq!(info.codec, "jpeg");
        assert_eq!(info.jpeg.as_ref().unwrap().quality, 80);
        assert_eq!(
            (
                info.jpeg.as_ref().unwrap().padded_width,
                info.jpeg.as_ref().unwrap().padded_height
            ),
            (64, 48)
        );
        let pixels = image::load_from_memory(&asset.member(0).unwrap()[44..])
            .unwrap()
            .to_rgb8();
        assert_eq!(
            pixels.dimensions(),
            (
                u32::from(info.storage_width),
                u32::from(info.storage_height)
            )
        );
        for (x, y, expected) in [
            (8, 8, [255, 0, 0]),
            (45, 8, [0, 255, 0]),
            (8, 26, [0, 0, 255]),
            (45, 26, [255, 255, 0]),
        ] {
            let (sx, sy) = match orientation {
                0 => (x, y),
                90 => (47 - y, x),
                180 => (63 - x, 47 - y),
                270 => (y, 63 - x),
                _ => unreachable!(),
            };
            let actual = pixels.get_pixel(sx, sy).0;
            assert!(
                actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) < 40),
                "{orientation}: {actual:?}"
            );
        }
        assert_eq!(&asset.member(1).unwrap()[27..], source);
        AssetObject::decode(&asset.encode().unwrap())
            .unwrap()
            .validate_for(&target(orientation))
            .unwrap();
    }
    assert_eq!(fs::read(tmp.path().join("source.bmp")).unwrap(), source);
}

#[test]
fn quality_is_explicit_baseline_420_even_at_100_and_target_metadata_is_checked() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("source.bmp"), bmp(64, 48)).unwrap();
    let mut c = ImageSource {
        source: "source.bmp".into(),
        ..Default::default()
    };
    let mut outputs = Vec::new();
    for quality in [1, 80, 100] {
        c.encoding.quality = Some(quality);
        let p = description(tmp.path(), &c);
        let asset = hmi_media::encode(&p, &target(0)).unwrap().object;
        assert_eq!(
            asset
                .metadata
                .info
                .image()
                .unwrap()
                .jpeg
                .as_ref()
                .unwrap()
                .quality,
            quality
        );
        assert_eq!(
            hmi_format::picture::jpeg::dimensions(&asset.member(0).unwrap()[44..]).unwrap(),
            (64, 48)
        );
        outputs.push(asset.payload.clone());
        let mut changed = asset.clone();
        if let hmi_artifact::asset::AssetInfo::Image { image } = &mut changed.metadata.info {
            image.storage_width += 1;
        }
        assert!(changed.validate().is_err());
        assert!(asset.validate_for(&target(90)).is_err());
    }
    assert_ne!(outputs[0], outputs[1]);
    assert_ne!(outputs[1], outputs[2]);
    for quality in [0, 101] {
        c.encoding.quality = Some(quality);
        let p = description(tmp.path(), &c);
        assert!(hmi_media::probe(&p, &target(0)).is_err());
    }
    c.encoding.quality = None;
    c.encoding.codec = "rgb565".into();
    let p = description(tmp.path(), &c);
    assert!(hmi_media::probe(&p, &target(0)).is_err());
}

#[test]
fn alpha_requires_explicit_flatten_and_small_images_use_48_pixel_storage() {
    let tmp = tempfile::tempdir().unwrap();
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &[255, 0, 0, 128].repeat(8 * 8),
            8,
            8,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    fs::write(tmp.path().join("source.png"), bytes).unwrap();
    let mut c = ImageSource {
        source: "source.png".into(),
        ..Default::default()
    };
    let p = description(tmp.path(), &c);
    assert!(hmi_media::probe(&p, &target(0)).is_err());
    c.encoding.alpha = hmi_core::asset::Alpha::Flatten;
    c.encoding.background = Some("#FFFFFF".into());
    let p = description(tmp.path(), &c);
    let asset = hmi_media::encode(&p, &target(0)).unwrap().object;
    let info = asset.metadata.info.image().unwrap();
    assert_eq!((info.width, info.height), (8, 8));
    assert_eq!((info.storage_width, info.storage_height), (48, 48));
    let decoded = image::load_from_memory(&asset.member(0).unwrap()[44..])
        .unwrap()
        .to_rgb8();
    let color = decoded.get_pixel(4, 4).0;
    assert!(
        color[0] > 220 && (100..160).contains(&color[1]) && (100..160).contains(&color[2]),
        "{color:?}"
    );
    let target = target(0);
    let probe = hmi_media::probe(&p, &target).unwrap();
    assert_eq!(probe.info, asset.metadata.info);
}
