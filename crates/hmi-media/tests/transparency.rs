use hmi_artifact::asset::{AssetInfo, AssetObject};
use hmi_core::{
    asset::{Alpha, ImageSource, Resize, ResizeMode},
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
fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    image::codecs::png::PngEncoder::new(&mut b)
        .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
        .unwrap();
    b
}
fn description(root: &Path, c: &ImageSource) -> std::path::PathBuf {
    let p = root.join("image.asset.toml");
    fs::write(&p, file::toml(c).unwrap()).unwrap();
    p
}
fn quantize(a: u8) -> u8 {
    ((u16::from(a) * 127 / 255) * 255 / 127) as u8
}
fn rgba() -> Vec<u8> {
    let mut p = Vec::new();
    for y in 0..35 {
        for x in 0..53 {
            let a = [0, 1, 2, 64, 128, 253, 254, 255][(x / 7 + y / 9) % 8];
            p.extend([220, 80, 30, a]);
        }
    }
    p
}

#[test]
fn four_directions_keep_alpha_raster_alignment_and_lossless_editor_source() {
    let tmp = tempfile::tempdir().unwrap();
    let rgba = rgba();
    let bytes = png(53, 35, &rgba);
    fs::write(tmp.path().join("source.png"), &bytes).unwrap();
    let p = description(
        tmp.path(),
        &ImageSource {
            source: "source.png".into(),
            ..Default::default()
        },
    );
    for orientation in [0, 90, 180, 270] {
        let asset = hmi_media::encode(&p, &target(orientation)).unwrap().object;
        let info = asset.metadata.info.image().unwrap();
        assert!(info.jpeg.as_ref().unwrap().has_alpha);
        assert_eq!(info.editor_source_format, "png");
        let runtime = asset.member(0).unwrap();
        let parts = hmi_format::picture::jpeg::parts(runtime, 53, 35, orientation).unwrap();
        let color = image::load_from_memory(parts.jpeg).unwrap().to_rgb8();
        let (cx, cy) = match orientation {
            0 => (10, 10),
            90 => (37, 10),
            180 => (53, 37),
            270 => (10, 53),
            _ => unreachable!(),
        };
        assert!(
            color
                .get_pixel(cx, cy)
                .0
                .iter()
                .zip([220, 80, 30])
                .all(|(a, b)| a.abs_diff(b) < 30)
        );
        let alpha = hmi_format::picture::alpha7::decode(
            parts.alpha.unwrap(),
            usize::from(info.storage_width) * usize::from(info.storage_height),
        )
        .unwrap();
        assert_eq!(
            u32::from_le_bytes(runtime[20..24].try_into().unwrap()) as usize,
            20 + parts.jpeg.len()
        );
        let mut expected = vec![0; alpha.len()];
        for y in 0..35 {
            for x in 0..53 {
                let (sx, sy) = match orientation {
                    0 => (x, y),
                    90 => (47 - y, x),
                    180 => (63 - x, 47 - y),
                    270 => (y, 63 - x),
                    _ => unreachable!(),
                };
                expected[sy * usize::from(info.storage_width) + sx] =
                    quantize(rgba[(y * 53 + x) * 4 + 3]);
            }
        }
        assert_eq!(alpha, expected);
        let editor = image::load_from_memory(&asset.member(1).unwrap()[27..])
            .unwrap()
            .to_rgba8();
        assert_eq!(editor.as_raw(), &rgba);
        AssetObject::decode(&asset.encode().unwrap())
            .unwrap()
            .validate_for(&target(orientation))
            .unwrap();
    }
    assert_eq!(fs::read(tmp.path().join("source.png")).unwrap(), bytes);
}

#[test]
fn contain_preserves_transparent_fill_and_flatten_removes_alpha() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("source.png"),
        png(2, 2, &[255, 0, 0, 128].repeat(4)),
    )
    .unwrap();
    let mut c = ImageSource {
        source: "source.png".into(),
        resize: Some(Resize {
            width: 9,
            height: 7,
            mode: ResizeMode::Contain,
            background: Some("#00000000".into()),
        }),
        ..Default::default()
    };
    let path = description(tmp.path(), &c);
    let preserved = hmi_media::encode(&path, &target(0)).unwrap().object;
    let editor = image::load_from_memory(&preserved.member(1).unwrap()[27..])
        .unwrap()
        .to_rgba8();
    assert_eq!(editor.get_pixel(0, 3)[3], 0);
    assert_eq!(editor.get_pixel(4, 3)[3], 128);
    let parts = hmi_format::picture::jpeg::parts(preserved.member(0).unwrap(), 9, 7, 0).unwrap();
    let alpha = hmi_format::picture::alpha7::decode(parts.alpha.unwrap(), 48 * 48).unwrap();
    assert_eq!(alpha[3 * 48], 0);
    assert_eq!(alpha[3 * 48 + 4], 126);
    c.encoding.alpha = Alpha::Flatten;
    c.encoding.background = Some("#00FF00".into());
    let path = description(tmp.path(), &c);
    let flattened = hmi_media::encode(&path, &target(0)).unwrap().object;
    assert!(
        !flattened
            .metadata
            .info
            .image()
            .unwrap()
            .jpeg
            .as_ref()
            .unwrap()
            .has_alpha
    );
    assert_eq!(
        flattened
            .metadata
            .info
            .image()
            .unwrap()
            .editor_source_format,
        "bmp"
    );
    assert!(
        hmi_format::picture::jpeg::parts(flattened.member(0).unwrap(), 9, 7, 0)
            .unwrap()
            .alpha
            .is_none()
    );
    let rgb = image::load_from_memory(&flattened.member(1).unwrap()[27..])
        .unwrap()
        .to_rgb8();
    assert_eq!(rgb.get_pixel(0, 3).0, [0, 255, 0]);
    assert_eq!(rgb.get_pixel(4, 3).0, [128, 127, 0]);
}

#[test]
fn corrupt_offsets_runs_png_crc_and_alpha_metadata_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("source.png"), png(53, 35, &rgba())).unwrap();
    let path = description(
        tmp.path(),
        &ImageSource {
            source: "source.png".into(),
            ..Default::default()
        },
    );
    let asset = hmi_media::encode(&path, &target(0)).unwrap().object;
    let runtime = asset.member(0).unwrap();
    for offset in [0u32, 1, 19, u32::MAX] {
        let mut bad = runtime.to_vec();
        bad[20..24].copy_from_slice(&offset.to_le_bytes());
        assert!(hmi_format::picture::jpeg::parts(&bad, 53, 35, 0).is_err());
    }
    let start = 24 + u32::from_le_bytes(runtime[20..24].try_into().unwrap()) as usize;
    let mut bad = runtime.to_vec();
    bad[start..start + 2].copy_from_slice(&[0, 0]);
    assert!(hmi_format::picture::jpeg::parts(&bad, 53, 35, 0).is_err());
    let mut source = asset.member(1).unwrap().to_vec();
    source[56] ^= 1;
    assert!(hmi_format::picture::validate_source(&source, 53, 35, "png").is_err());
    let mut wrong = asset.clone();
    if let AssetInfo::Image { image } = &mut wrong.metadata.info {
        image.jpeg.as_mut().unwrap().has_alpha = false;
    }
    assert!(wrong.validate().is_err());
}

#[test]
fn near_opaque_alpha_is_not_silently_discarded_but_rgb565_still_requires_flatten() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ImageSource {
        source: "source.png".into(),
        ..Default::default()
    };
    let path = description(tmp.path(), &c);
    for value in [253, 254, 255] {
        fs::write(
            tmp.path().join("source.png"),
            png(8, 8, &[200, 100, 50, value].repeat(64)),
        )
        .unwrap();
        let asset = hmi_media::encode(&path, &target(0)).unwrap().object;
        assert_eq!(
            asset
                .metadata
                .info
                .image()
                .unwrap()
                .jpeg
                .as_ref()
                .unwrap()
                .has_alpha,
            value != 255
        );
    }
    fs::write(
        tmp.path().join("source.png"),
        png(8, 8, &[200, 100, 50, 128].repeat(64)),
    )
    .unwrap();
    let t = Target::resolve(&TargetConfig {
        profile: "TJC4832K035_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation: 0,
    })
    .unwrap();
    assert!(hmi_media::encode(&path, &t).is_err());
}
