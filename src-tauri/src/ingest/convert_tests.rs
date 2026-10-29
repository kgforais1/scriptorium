//! Conversion tests. Fixtures are generated at test time, so no binaries live in git.
//! CMYK-with-profile and JPEG 2000 inputs need macOS `sips` and skip elsewhere.

use super::convert::{display_frame, inspect, ConvertError, InputKind, Inspection};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{ImageBuffer, ImageEncoder, Luma, Rgb, RgbImage};
use std::path::{Path, PathBuf};

const W: u32 = 40;
const H: u32 = 60;

/// A distinctive test picture: a colour gradient with a bright marker at the top-left corner,
/// so orientation fixes can be checked by finding the marker.
fn picture() -> RgbImage {
    ImageBuffer::from_fn(W, H, |x, y| {
        if x < 6 && y < 6 {
            Rgb([255, 255, 255])
        } else {
            Rgb([(x * 5) as u8, (y * 4) as u8, 90])
        }
    })
}

struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "scriptorium-convert-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn jpeg(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 95)
        .encode_image(img)
        .unwrap();
    out
}

fn png_with_icc(img: &RgbImage, icc: Option<Vec<u8>>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = PngEncoder::new(&mut out);
    if let Some(p) = icc {
        enc.set_icc_profile(p).unwrap();
    }
    enc.write_image(img.as_raw(), W, H, image::ExtendedColorType::Rgb8)
        .unwrap();
    out
}

/// Insert a minimal EXIF APP1 segment carrying `Orientation` right after the JPEG SOI.
fn with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let mut tiff = b"MM\0\x2a\0\0\0\x08".to_vec();
    tiff.extend_from_slice(&1u16.to_be_bytes()); // one IFD entry
    tiff.extend_from_slice(&0x0112u16.to_be_bytes()); // Orientation
    tiff.extend_from_slice(&3u16.to_be_bytes()); // SHORT
    tiff.extend_from_slice(&1u32.to_be_bytes()); // count
    tiff.extend_from_slice(&orientation.to_be_bytes());
    tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // value padding + next IFD = 0
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend(tiff);
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
    out.extend(app1);
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn adobe_rgb_icc() -> Vec<u8> {
    use lcms2::{CIExyY, CIExyYTRIPLE, Profile, ToneCurve};
    let white = CIExyY {
        x: 0.3127,
        y: 0.3290,
        Y: 1.0,
    };
    let primaries = CIExyYTRIPLE {
        Red: CIExyY {
            x: 0.64,
            y: 0.33,
            Y: 1.0,
        },
        Green: CIExyY {
            x: 0.21,
            y: 0.71,
            Y: 1.0,
        },
        Blue: CIExyY {
            x: 0.15,
            y: 0.06,
            Y: 1.0,
        },
    };
    let gamma = ToneCurve::new(2.2);
    Profile::new_rgb(&white, &primaries, &[&gamma, &gamma, &gamma])
        .unwrap()
        .icc()
        .unwrap()
}

fn tiff(frames: &[RgbImage], orientation: Option<u16>) -> Vec<u8> {
    use tiff::encoder::{colortype, TiffEncoder};
    use tiff::tags::Tag;
    let mut out = std::io::Cursor::new(Vec::new());
    let mut enc = TiffEncoder::new(&mut out).unwrap();
    for f in frames {
        let mut img = enc
            .new_image::<colortype::RGB8>(f.width(), f.height())
            .unwrap();
        if let Some(o) = orientation {
            img.encoder().write_tag(Tag::Orientation, o).unwrap();
        }
        img.write_data(f.as_raw()).unwrap();
    }
    out.into_inner()
}

fn decoded(path: &Path, frame: u32) -> image::DynamicImage {
    let d = display_frame(path, frame).unwrap();
    let bytes = d.bytes.unwrap_or_else(|| std::fs::read(path).unwrap());
    let img = image::load_from_memory(&bytes).unwrap();
    assert_eq!(
        (img.width(), img.height()),
        (d.width, d.height),
        "reported size matches bytes"
    );
    img
}

/// Mean brightness of a 6×6 block at a corner of an image.
fn corner(img: &image::DynamicImage, right: bool, bottom: bool) -> u32 {
    let rgb = img.to_rgb8();
    let (x0, y0) = (
        if right { rgb.width() - 6 } else { 0 },
        if bottom { rgb.height() - 6 } else { 0 },
    );
    let mut sum = 0u32;
    for y in y0..y0 + 6 {
        for x in x0..x0 + 6 {
            sum += rgb.get_pixel(x, y).0.iter().map(|&c| c as u32).sum::<u32>();
        }
    }
    sum / (36 * 3)
}

fn expect(info: Inspection, kind: InputKind, frames: u32, convert: bool, ext: &str) {
    assert_eq!(
        (
            info.kind,
            info.frames,
            info.needs_conversion,
            info.display_ext
        ),
        (kind, frames, convert, ext)
    );
}

#[test]
fn extensions_map_to_kinds() {
    for (name, kind) in [
        ("a.JPG", Some(InputKind::Jpeg)),
        ("a.jpeg", Some(InputKind::Jpeg)),
        ("a.png", Some(InputKind::Png)),
        ("a.webp", Some(InputKind::Webp)),
        ("a.bmp", Some(InputKind::Bmp)),
        ("a.gif", Some(InputKind::Gif)),
        ("a.TIFF", Some(InputKind::Tiff)),
        ("a.tif", Some(InputKind::Tiff)),
        ("a.jp2", Some(InputKind::Jp2)),
        ("a.j2k", Some(InputKind::Jp2)),
        ("a.jpf", Some(InputKind::Jp2)),
        ("a.heic", None),
        ("noext", None),
    ] {
        assert_eq!(InputKind::from_path(Path::new(name)), kind, "{name}");
    }
    let d = Dir::new("ext");
    let p = d.file("x.heic", b"nope");
    assert_eq!(inspect(&p), Err(ConvertError::UnsupportedFormat));
}

#[test]
fn plain_srgb_jpeg_png_webp_copy_as_is() {
    let d = Dir::new("plain");
    let img = picture();
    let j = d.file("a.jpg", &jpeg(&img));
    expect(inspect(&j).unwrap(), InputKind::Jpeg, 1, false, "jpg");
    let p = d.file("a.png", &png_with_icc(&img, None));
    expect(inspect(&p).unwrap(), InputKind::Png, 1, false, "png");
    let mut webp = Vec::new();
    WebPEncoder::new_lossless(&mut webp)
        .write_image(img.as_raw(), W, H, image::ExtendedColorType::Rgb8)
        .unwrap();
    let w = d.file("a.webp", &webp);
    expect(inspect(&w).unwrap(), InputKind::Webp, 1, false, "webp");
    for path in [&j, &p, &w] {
        let out = display_frame(path, 0).unwrap();
        assert!(out.bytes.is_none(), "{path:?} is its own display image");
        assert_eq!((out.width, out.height), (W, H));
    }
}

#[test]
fn srgb_icc_counts_as_srgb_but_adobe_rgb_converts() {
    let d = Dir::new("icc");
    let img = picture();
    let srgb = lcms2::Profile::new_srgb().icc().unwrap();
    let keep = d.file("srgb.png", &png_with_icc(&img, Some(srgb)));
    expect(inspect(&keep).unwrap(), InputKind::Png, 1, false, "png");
    let wide = d.file("adobe.png", &png_with_icc(&img, Some(adobe_rgb_icc())));
    expect(inspect(&wide).unwrap(), InputKind::Png, 1, true, "png");
    let out = decoded(&wide, 0);
    // Adobe RGB green is more saturated than sRGB green, so converting changes the pixels.
    assert_ne!(out.to_rgb8().get_pixel(30, 40), img.get_pixel(30, 40));
}

#[test]
fn sixteen_bit_png_becomes_8_bit() {
    let d = Dir::new("deep");
    let grey: ImageBuffer<Luma<u16>, Vec<u16>> =
        ImageBuffer::from_fn(W, H, |x, _| Luma([(x * 1600) as u16]));
    let mut bytes = Vec::new();
    let raw: Vec<u8> = grey.as_raw().iter().flat_map(|v| v.to_be_bytes()).collect();
    PngEncoder::new(&mut bytes)
        .write_image(&raw, W, H, image::ExtendedColorType::L16)
        .unwrap();
    let p = d.file("g16.png", &bytes);
    expect(inspect(&p).unwrap(), InputKind::Png, 1, true, "png");
    assert_eq!(decoded(&p, 0).color(), image::ColorType::L8);
}

#[test]
fn exif_orientation_is_applied_and_dropped() {
    let d = Dir::new("orient");
    let img = picture();
    let p = d.file("o6.jpg", &with_exif_orientation(&jpeg(&img), 6));
    expect(inspect(&p).unwrap(), InputKind::Jpeg, 1, true, "jpg");
    let out = decoded(&p, 0);
    assert_eq!((out.width(), out.height()), (H, W), "rotated 90° clockwise");
    assert!(
        corner(&out, true, false) > 200,
        "marker moves to the top-right corner"
    );
    let one = d.file("o1.jpg", &with_exif_orientation(&jpeg(&img), 1));
    expect(inspect(&one).unwrap(), InputKind::Jpeg, 1, false, "jpg");
}

#[test]
fn bmp_and_gif_become_png() {
    let d = Dir::new("bmpgif");
    let img = picture();
    let mut bmp = Vec::new();
    image::codecs::bmp::BmpEncoder::new(&mut bmp)
        .write_image(img.as_raw(), W, H, image::ExtendedColorType::Rgb8)
        .unwrap();
    let b = d.file("a.bmp", &bmp);
    expect(inspect(&b).unwrap(), InputKind::Bmp, 1, true, "png");
    assert_eq!(decoded(&b, 0).to_rgb8(), img, "BMP to PNG is lossless");

    let mut gif = Vec::new();
    {
        let mut enc = image::codecs::gif::GifEncoder::new(&mut gif);
        let frame =
            |i: &RgbImage| image::Frame::new(image::DynamicImage::ImageRgb8(i.clone()).to_rgba8());
        let flipped = image::imageops::flip_vertical(&img);
        enc.encode_frames(vec![frame(&img), frame(&flipped)])
            .unwrap();
    }
    let g = d.file("a.gif", &gif);
    expect(inspect(&g).unwrap(), InputKind::Gif, 1, true, "png");
    assert!(
        corner(&decoded(&g, 0), false, false) > 200,
        "GIF uses its first frame"
    );
}

#[test]
fn multi_page_tiff_yields_one_page_per_frame() {
    let d = Dir::new("tiff");
    let img = picture();
    let flipped = image::imageops::flip_vertical(&img);
    let p = d.file("m.tif", &tiff(&[img.clone(), flipped, img.clone()], None));
    expect(inspect(&p).unwrap(), InputKind::Tiff, 3, true, "jpg");
    assert!(corner(&decoded(&p, 0), false, false) > 200);
    assert!(
        corner(&decoded(&p, 1), false, true) > 200,
        "frame 1 is the flipped page"
    );
    assert!(corner(&decoded(&p, 2), false, false) > 200);
    assert!(matches!(
        display_frame(&p, 3),
        Err(ConvertError::UnreadableFile(_))
    ));
}

#[test]
fn tiff_orientation_tag_is_applied() {
    let d = Dir::new("tiff-orient");
    let p = d.file("o3.tif", &tiff(&[picture()], Some(3)));
    let out = decoded(&p, 0);
    assert_eq!((out.width(), out.height()), (W, H));
    assert!(
        corner(&out, true, true) > 200,
        "rotated 180°: marker at bottom-right"
    );
}

#[test]
fn cmyk_tiff_without_profile_is_rejected() {
    use tiff::encoder::{colortype, TiffEncoder};
    let d = Dir::new("cmyk-tiff");
    let mut out = std::io::Cursor::new(Vec::new());
    TiffEncoder::new(&mut out)
        .unwrap()
        .write_image::<colortype::CMYK8>(W, H, &vec![40u8; (W * H * 4) as usize])
        .unwrap();
    let p = d.file("c.tif", &out.into_inner());
    match display_frame(&p, 0) {
        Err(ConvertError::UnreadableFile(r)) => assert!(r.contains("no ICC profile"), "{r}"),
        other => panic!("expected UnreadableFile, got {other:?}"),
    }
}

#[test]
fn garbage_is_unreadable() {
    let d = Dir::new("garbage");
    for name in ["x.jpg", "x.png", "x.tif", "x.jp2"] {
        let p = d.file(name, b"definitely not an image");
        let res = inspect(&p).and_then(|_| display_frame(&p, 0));
        assert!(
            matches!(res, Err(ConvertError::UnreadableFile(_))),
            "{name}: {res:?}"
        );
    }
}

// ---- macOS-only fixtures built with `sips` ----

const GENERIC_CMYK: &str = "/System/Library/ColorSync/Profiles/Generic CMYK Profile.icc";

fn sips(args: &[&str]) -> bool {
    std::process::Command::new("sips")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Remove every APP2 `ICC_PROFILE` segment from a JPEG.
fn strip_icc(jpeg: &[u8]) -> Vec<u8> {
    let mut out = jpeg[..2].to_vec();
    let mut i = 2;
    while i + 4 <= jpeg.len() && jpeg[i] == 0xFF && jpeg[i + 1] != 0xDA {
        let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
        let is_icc = jpeg[i + 1] == 0xE2 && jpeg.get(i + 4..i + 15) == Some(b"ICC_PROFILE");
        if !is_icc {
            out.extend_from_slice(&jpeg[i..i + 2 + len]);
        }
        i += 2 + len;
    }
    out.extend_from_slice(&jpeg[i..]);
    out
}

#[test]
fn cmyk_jpeg_converts_with_profile_and_is_rejected_without() {
    if !Path::new(GENERIC_CMYK).exists() {
        eprintln!("skipping: needs macOS {GENERIC_CMYK}");
        return;
    }
    let d = Dir::new("cmyk");
    let src = d.file("rgb.jpg", &jpeg(&picture()));
    let cmyk = d.0.join("cmyk.jpg");
    assert!(sips(&[
        "-m",
        GENERIC_CMYK,
        src.to_str().unwrap(),
        "--out",
        cmyk.to_str().unwrap()
    ]));
    expect(inspect(&cmyk).unwrap(), InputKind::Jpeg, 1, true, "jpg");
    let out = decoded(&cmyk, 0);
    let (want, got) = (
        picture().get_pixel(30, 40).0,
        out.to_rgb8().get_pixel(30, 40).0,
    );
    for k in 0..3 {
        assert!(
            (want[k] as i32 - got[k] as i32).abs() < 40,
            "CMYK round trip {want:?} vs {got:?}"
        );
    }
    let bare = d.file("bare.jpg", &strip_icc(&std::fs::read(&cmyk).unwrap()));
    match display_frame(&bare, 0) {
        Err(ConvertError::UnreadableFile(r)) => assert!(r.contains("no ICC profile"), "{r}"),
        other => panic!("expected UnreadableFile, got {other:?}"),
    }
}

#[test]
fn jpeg_2000_becomes_jpeg() {
    let d = Dir::new("jp2");
    let src = d.file("src.png", &png_with_icc(&picture(), None));
    let jp2 = d.0.join("a.jp2");
    if !sips(&[
        "-s",
        "format",
        "jp2",
        src.to_str().unwrap(),
        "--out",
        jp2.to_str().unwrap(),
    ]) {
        eprintln!("skipping: needs macOS sips to write JPEG 2000");
        return;
    }
    expect(inspect(&jp2).unwrap(), InputKind::Jp2, 1, true, "jpg");
    let out = decoded(&jp2, 0);
    assert_eq!((out.width(), out.height()), (W, H));
    assert!(corner(&out, false, false) > 200, "marker survives decoding");
}
