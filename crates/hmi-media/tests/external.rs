use hmi_artifact::asset::AssetObject;
use hmi_core::{
    asset::{Delivery, ImageSource},
    file,
    target::{Target, TargetConfig},
};
use image::ImageEncoder;
use std::fs;

fn target(orientation: u16) -> Target {
    Target::resolve(&TargetConfig {
        profile: "TJC4827X543_011".into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation,
    })
    .unwrap()
}

#[test]
fn xi_headers_rotated_pixels_and_alpha_match_the_external_contract() {
    let tmp = tempfile::tempdir().unwrap();
    for transparent in [false, true] {
        let mut pixels = Vec::new();
        for y in 0..35 {
            for x in 0..53 {
                pixels.extend([
                    220,
                    80,
                    30,
                    if transparent {
                        ((x * 7 + y * 11) % 256) as u8
                    } else {
                        255
                    },
                ]);
            }
        }
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&pixels, 53, 35, image::ExtendedColorType::Rgba8)
            .unwrap();
        fs::write(tmp.path().join("input.png"), png).unwrap();
        let config = ImageSource {
            source: "input.png".into(),
            delivery: Delivery {
                mode: "external".into(),
                path: Some("sd0/images/logo.xi".into()),
            },
            ..Default::default()
        };
        let path = tmp.path().join("image.asset.toml");
        fs::write(&path, file::toml(&config).unwrap()).unwrap();
        for orientation in [0, 90, 180, 270] {
            let probe = hmi_media::probe(&path, &target(orientation)).unwrap();
            let object = hmi_media::encode(&path, &target(orientation))
                .unwrap()
                .object;
            assert_eq!(probe.info, object.metadata.info);
            assert_eq!(object.metadata.members.len(), 1);
            assert_eq!(object.metadata.members[0].extension, "xi");
            assert!(object.metadata.members[0].reference_kind.is_none());
            let bytes = object.member(0).unwrap();
            assert_eq!(&bytes[..8], b"hmipicxi");
            assert_eq!(
                &bytes[8..16],
                &[10 + (orientation / 90) as u8, 0, 0, 1, 255, 255, 0, 0]
            );
            assert_eq!(&bytes[16..24], &[24, 0, 0, 0, 53, 0, 35, 0]);
            let header = hmi_format::picture::xi::inspect(bytes).unwrap();
            assert_eq!(
                (
                    header.width,
                    header.height,
                    header.orientation,
                    header.has_alpha
                ),
                (53, 35, orientation, transparent)
            );
            let split = u32::from_le_bytes(bytes[28..32].try_into().unwrap()) as usize;
            let jpeg_end = if split == 0 { bytes.len() } else { 32 + split };
            let jpeg = image::load_from_memory(&bytes[32..jpeg_end])
                .unwrap()
                .to_rgb8();
            let (sw, sh) = if orientation % 180 == 0 {
                (64, 48)
            } else {
                (48, 64)
            };
            assert_eq!(jpeg.dimensions(), (sw, sh));
            let (cx, cy) = match orientation {
                0 => (10, 10),
                90 => (37, 10),
                180 => (53, 37),
                _ => (10, 53),
            };
            assert!(
                jpeg.get_pixel(cx, cy)
                    .0
                    .iter()
                    .zip([220, 80, 30])
                    .all(|(a, b)| a.abs_diff(b) < 30)
            );
            if transparent {
                let alpha =
                    hmi_format::picture::alpha7::decode(&bytes[jpeg_end..], (sw * sh) as usize)
                        .unwrap();
                for y in 0..48 {
                    for x in 0..64 {
                        let (tx, ty) = match orientation {
                            0 => (x, y),
                            90 => (47 - y, x),
                            180 => (63 - x, 47 - y),
                            _ => (y, 63 - x),
                        };
                        let original = if x < 53 && y < 35 {
                            pixels[((y * 53 + x) * 4 + 3) as usize]
                        } else {
                            0
                        };
                        let expected = ((u16::from(original) * 127 / 255) * 255 / 127) as u8;
                        assert_eq!(alpha[(ty * sw + tx) as usize], expected);
                    }
                }
            } else {
                assert_eq!(split, 0);
            }
            AssetObject::decode(&object.encode().unwrap())
                .unwrap()
                .validate_for(&target(orientation))
                .unwrap();
        }
    }
}

#[test]
fn corrupted_xi_and_illegal_delivery_metadata_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &[200, 50, 20, 128].repeat(64 * 48),
            64,
            48,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    fs::write(tmp.path().join("input.png"), png).unwrap();
    let config = ImageSource {
        source: "input.png".into(),
        delivery: Delivery {
            mode: "external".into(),
            path: Some("sd0/logo.xi".into()),
        },
        ..Default::default()
    };
    let path = tmp.path().join("image.asset.toml");
    fs::write(&path, file::toml(&config).unwrap()).unwrap();
    let object = hmi_media::encode(&path, &target(0)).unwrap().object;
    let original = object.member(0).unwrap();
    for index in [0, 8, 9, 12, 14, 16, 20, 24, 28] {
        let mut bytes = original.to_vec();
        bytes[index] ^= 0x70;
        assert!(
            hmi_format::picture::xi::inspect(&bytes).is_err(),
            "offset {index}"
        );
    }
    for end in [0, 7, 31, 32, original.len() - 1] {
        assert!(hmi_format::picture::xi::inspect(&original[..end]).is_err());
    }
    let mut changed = object.clone();
    changed.payload.pop();
    changed.metadata.members[0].length -= 1;
    changed.metadata.members[0].sha256 = hmi_core::sha256(&changed.payload);
    assert!(changed.validate().is_err());
    for deployment in [
        "sd0/../input.png",
        "sd0/a\\b.xi",
        "sd0/CON.xi",
        "sd0/x./y.xi",
        "sd0//a.xi",
        "sd0/a.jpg",
        "ram/a.xi",
    ] {
        let mut c = config.clone();
        c.delivery.path = Some(deployment.into());
        fs::write(&path, file::toml(&c).unwrap()).unwrap();
        assert!(hmi_media::probe(&path, &target(0)).is_err(), "{deployment}");
    }
}
