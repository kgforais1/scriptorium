//! Conversion tests. Fixtures are generated at test time, so no binaries live in git.
//! CMYK-with-profile and JPEG 2000 inputs need macOS `sips` and skip elsewhere.

use super::convert::{display_frame, inspect, open, ConvertError, InputKind, Inspection};
use super::decode::{
    jp2_dimensions, jpeg_has_adobe_marker, jpeg_is_cmyk, parse_siz, Siz, JP2_MAX_PIXELS,
};
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
        // On macOS the fixture must exist: a silent skip there would hide a broken test.
        if cfg!(target_os = "macos") {
            panic!("macOS is missing {GENERIC_CMYK}");
        }
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
        if cfg!(target_os = "macos") {
            panic!("macOS sips could not write JPEG 2000");
        }
        eprintln!("skipping: needs macOS sips to write JPEG 2000");
        return;
    }
    expect(inspect(&jp2).unwrap(), InputKind::Jp2, 1, true, "jpg");
    let out = decoded(&jp2, 0);
    assert_eq!((out.width(), out.height()), (W, H));
    assert!(corner(&out, false, false) > 200, "marker survives decoding");
    // Grey with transparency is refused rather than losing its alpha in a JPEG.
    let la: ImageBuffer<image::LumaA<u8>, Vec<u8>> = ImageBuffer::from_fn(W, H, |x, _| {
        image::LumaA([(x * 5) as u8, if x < 20 { 255 } else { 40 }])
    });
    let mut la_png = Vec::new();
    PngEncoder::new(&mut la_png)
        .write_image(la.as_raw(), W, H, image::ExtendedColorType::La8)
        .unwrap();
    let la_src = d.file("la.png", &la_png);
    let la_jp2 = d.0.join("la.jp2");
    let la_written = sips(&[
        "-s",
        "format",
        "jp2",
        la_src.to_str().unwrap(),
        "--out",
        la_jp2.to_str().unwrap(),
    ]);
    assert!(
        la_written || !cfg!(target_os = "macos"),
        "macOS sips could not write grey+alpha JPEG 2000"
    );
    if la_written {
        let comps = jpeg2k::Image::from_bytes(&std::fs::read(&la_jp2).unwrap())
            .map(|i| i.num_components())
            .ok();
        if comps == Some(2) {
            match display_frame(&la_jp2, 0) {
                Err(ConvertError::UnreadableFile(r)) => assert!(r.contains("transparency"), "{r}"),
                other => panic!("expected the transparency refusal, got {other:?}"),
            }
        } else {
            eprintln!("note: sips wrote {comps:?} components for grey+alpha; 2-component case not exercised");
        }
    }
    // A mid-tone pixel catches a channel-order or scaling bug the corner check would miss.
    let (want, got) = (
        picture().get_pixel(30, 40).0,
        out.to_rgb8().get_pixel(30, 40).0,
    );
    for k in 0..3 {
        assert!(
            (want[k] as i32 - got[k] as i32).abs() < 12,
            "JP2 pixel {want:?} vs {got:?}"
        );
    }
}

// ---- JPEG header walking ----

/// A JPEG header: SOI, the given segments, then a SOF0 with `components`, then SOS.
fn jpeg_header(before_sof: &[&[u8]], components: u8) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    for seg in before_sof {
        out.extend_from_slice(seg);
    }
    out.extend_from_slice(&[
        0xFF,
        0xC0,
        0,
        8 + 3 * components as u16 as u8,
        8,
        0,
        10,
        0,
        10,
        components,
    ]);
    out.extend(std::iter::repeat_n(0u8, 3 * components as usize));
    out.extend_from_slice(&[0xFF, 0xDA, 0, 2]);
    out
}

#[test]
fn jpeg_walker_handles_fill_bytes_standalone_markers_and_truncation() {
    let app0: &[u8] = &[0xFF, 0xE0, 0, 4, b'J', b'F'];
    let adobe: &[u8] = &[0xFF, 0xEE, 0, 7, b'A', b'd', b'o', b'b', b'e'];
    assert!(jpeg_is_cmyk(&jpeg_header(&[app0], 4)));
    assert!(!jpeg_is_cmyk(&jpeg_header(&[app0], 3)));
    // Fill bytes before a marker, and standalone TEM / RST markers, must not desync the walk.
    assert!(jpeg_is_cmyk(&jpeg_header(&[&[0xFF, 0xFF, 0xFF], app0], 4)));
    assert!(jpeg_is_cmyk(&jpeg_header(
        &[&[0xFF, 0x01], &[0xFF, 0xD3], app0],
        4
    )));
    assert!(jpeg_has_adobe_marker(&jpeg_header(
        &[&[0xFF, 0xFF], adobe],
        4
    )));
    assert!(!jpeg_has_adobe_marker(&jpeg_header(&[app0], 4)));
    // Progressive SOF2 counts as a frame header too.
    let mut sof2 = jpeg_header(&[], 4);
    sof2[3] = 0xC2;
    assert!(jpeg_is_cmyk(&sof2));
    // Truncated or garbage headers answer "no", never panic.
    let full = jpeg_header(&[adobe], 4);
    for cut in 0..full.len() {
        let _ = (
            jpeg_is_cmyk(&full[..cut]),
            jpeg_has_adobe_marker(&full[..cut]),
        );
    }
    assert!(!jpeg_is_cmyk(b"not a jpeg"));
    assert!(
        !jpeg_is_cmyk(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 1]),
        "length below 2"
    );
    assert!(
        !jpeg_is_cmyk(&[0xFF, 0xD8, 0xFF, 0xFF, 0xFF]),
        "fill run to the end"
    );
    assert!(!jpeg_is_cmyk(&[0xFF, 0xD8, 0xFF]), "lone 0xFF after SOI");
}

#[test]
fn jpeg_with_a_wide_gamut_profile_converts() {
    let d = Dir::new("jpeg-icc");
    let mut bytes = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut bytes, 95);
    enc.set_icc_profile(adobe_rgb_icc()).unwrap();
    enc.encode_image(&picture()).unwrap();
    let p = d.file("wide.jpg", &bytes);
    expect(inspect(&p).unwrap(), InputKind::Jpeg, 1, true, "jpg");
    assert_ne!(
        decoded(&p, 0).to_rgb8().get_pixel(30, 40),
        picture().get_pixel(30, 40)
    );
}

fn grey_gamma_icc(gamma: f64) -> Vec<u8> {
    let white = lcms2::CIExyY {
        x: 0.3127,
        y: 0.3290,
        Y: 1.0,
    };
    lcms2::Profile::new_gray(&white, &lcms2::ToneCurve::new(gamma))
        .unwrap()
        .icc()
        .unwrap()
}

#[test]
fn grey_profile_on_a_colour_image_is_rejected_not_greyscaled() {
    let d = Dir::new("grey-on-colour");
    let p = d.file(
        "c.png",
        &png_with_icc(&picture(), Some(grey_gamma_icc(1.8))),
    );
    match display_frame(&p, 0) {
        Err(ConvertError::UnreadableFile(r)) => assert!(r.contains("grey ICC profile"), "{r}"),
        other => panic!("expected UnreadableFile, got {other:?}"),
    }
}

#[test]
fn deep_grey_tiff_stays_grey_in_its_jpeg() {
    use tiff::encoder::{colortype, TiffEncoder};
    let d = Dir::new("grey-tiff");
    let mut out = std::io::Cursor::new(Vec::new());
    let data: Vec<u16> = (0..W * H).map(|i| (i % W * 1600) as u16).collect();
    TiffEncoder::new(&mut out)
        .unwrap()
        .write_image::<colortype::Gray16>(W, H, &data)
        .unwrap();
    let p = d.file("g.tif", &out.into_inner());
    assert_eq!(decoded(&p, 0).color(), image::ColorType::L8);
}

#[test]
fn one_open_serves_every_frame() {
    let d = Dir::new("open-once");
    let img = picture();
    let p = d.file("m.tif", &tiff(&[img.clone(), img.clone()], None));
    let src = open(&p).unwrap();
    std::fs::remove_file(&p).unwrap();
    for f in 0..src.info.frames {
        assert!(
            src.frame(f).unwrap().bytes.is_some(),
            "frame {f} decodes from the kept buffer"
        );
    }
}

#[test]
fn truncated_passthrough_file_is_unreadable() {
    let d = Dir::new("truncated");
    let full = jpeg(&picture());
    let unreadable = |name: &str, bytes: &[u8]| {
        let p = d.file(name, bytes);
        let res = open(&p).and_then(|s| s.frame(0));
        assert!(
            matches!(res, Err(ConvertError::UnreadableFile(_))),
            "{name}: {res:?}"
        );
    };
    unreadable("cut.jpg", &full[..full.len() / 2]);

    // An EXIF thumbnail carries its own end-of-image marker before the main scan; it must
    // not make a truncated main image look complete.
    let thumb: &[u8] = &[0xFF, 0xD8, 0xFF, 0xD9];
    let mut app1 = vec![0xFF, 0xE1];
    app1.extend_from_slice(&((2 + 6 + thumb.len()) as u16).to_be_bytes());
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(thumb);
    let with_app1 = |body: &[u8]| -> Vec<u8> {
        let mut out = full[..2].to_vec();
        out.extend_from_slice(&app1);
        out.extend_from_slice(body);
        out
    };
    unreadable("cut-thumb.jpg", &with_app1(&full[2..full.len() / 2]));
    let p = d.file("thumb.jpg", &with_app1(&full[2..]));
    assert!(
        open(&p).and_then(|s| s.frame(0)).is_ok(),
        "a complete JPEG with a thumbnail is fine"
    );

    let png = png_with_icc(&picture(), None);
    unreadable("cut.png", &png[..png.len() / 2]);
}

#[test]
fn grey_alpha_png_with_a_grey_profile_keeps_its_alpha() {
    let d = Dir::new("grey-alpha");
    let la: ImageBuffer<image::LumaA<u8>, Vec<u8>> = ImageBuffer::from_fn(W, H, |x, _| {
        image::LumaA([(x * 5) as u8, if x < 20 { 255 } else { 40 }])
    });
    let mut bytes = Vec::new();
    let mut enc = PngEncoder::new(&mut bytes);
    enc.set_icc_profile(grey_gamma_icc(1.8)).unwrap();
    enc.write_image(la.as_raw(), W, H, image::ExtendedColorType::La8)
        .unwrap();
    let p = d.file("la.png", &bytes);
    let out = decoded(&p, 0).to_rgba8();
    assert_eq!(
        (out.get_pixel(5, 5).0[3], out.get_pixel(30, 5).0[3]),
        (255, 40),
        "alpha survives"
    );
}

#[test]
fn jp2_header_size_is_read_and_bounded() {
    // Raw codestream: SOC, SIZ with Xsiz/Ysiz and offsets.
    let mut j2k = vec![0xFF, 0x4F, 0xFF, 0x51, 0, 41, 0, 0];
    for v in [5000u32, 7000, 0, 0] {
        j2k.extend_from_slice(&v.to_be_bytes());
    }
    assert_eq!(jp2_dimensions(&j2k), Some((5000, 7000)));
    // A JP2 file: signature box, then a decoy `ihdr` inside a uuid box claiming 1×1, then
    // the real jp2h/ihdr. Only the real header counts.
    let ihdr = |h: u32, w: u32| -> Vec<u8> {
        let mut b = vec![0, 0, 0, 22];
        b.extend_from_slice(b"ihdr");
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&[0, 3, 7, 7, 0, 0]);
        b
    };
    let boxed = |kind: &[u8; 4], payload: &[u8]| -> Vec<u8> {
        let mut b = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend_from_slice(kind);
        b.extend_from_slice(payload);
        b
    };
    let mut jp2 = boxed(b"jP  ", b"\r\n\x87\n");
    jp2.extend(boxed(b"uuid", &ihdr(1, 1)));
    jp2.extend(boxed(b"jp2h", &ihdr(40_000, 50_000)));
    assert_eq!(
        jp2_dimensions(&jp2),
        Some((50_000, 40_000)),
        "the decoy ihdr is ignored"
    );
    const { assert!(50_000u64 * 40_000 > JP2_MAX_PIXELS) };
    // A file claiming 2 gigapixels is refused before OpenJPEG allocates anything.
    let d = Dir::new("jp2-bomb");
    let p = d.file("bomb.jp2", &jp2);
    match open(&p).and_then(|s| s.frame(0)) {
        Err(ConvertError::UnreadableFile(r)) => assert!(r.contains("limit"), "{r}"),
        other => panic!("expected the pixel limit, got {other:?}"),
    }
    assert_eq!(jp2_dimensions(b"nothing here"), None);
    assert_eq!(jp2_dimensions(&j2k[..10]), None, "truncated SIZ");
}

#[test]
fn contents_must_match_the_extension() {
    let d = Dir::new("renamed");
    // A TIFF renamed .png would otherwise be copied as a "PNG" the webview cannot show.
    let p = d.file("scan.png", &tiff(&[picture()], None));
    match inspect(&p) {
        Err(ConvertError::UnreadableFile(r)) => {
            assert!(r.contains("TIFF") && r.contains("PNG"), "{r}")
        }
        other => panic!("expected a format mismatch, got {other:?}"),
    }
    // `.jpeg` and `.tiff` spellings still match their contents.
    assert!(inspect(&d.file("a.jpeg", &jpeg(&picture()))).is_ok());
    assert!(inspect(&d.file("a.tiff", &tiff(&[picture()], None))).is_ok());
}

#[test]
fn signatures_for_every_format() {
    let d = Dir::new("sniff");
    let mut webp = Vec::new();
    WebPEncoder::new_lossless(&mut webp)
        .write_image(picture().as_raw(), W, H, image::ExtendedColorType::Rgb8)
        .unwrap();
    let mut gif = Vec::new();
    image::codecs::gif::GifEncoder::new(&mut gif)
        .encode_frames(vec![image::Frame::new(
            image::DynamicImage::ImageRgb8(picture()).to_rgba8(),
        )])
        .unwrap();
    // Each real file passes under its own extension and is refused under another one.
    for (ext, bytes, wrong) in [
        ("webp", webp, "png"),
        ("gif", gif, "jpg"),
        ("tif", tiff(&[picture()], None), "bmp"),
    ] {
        assert!(
            inspect(&d.file(&format!("ok.{ext}"), &bytes)).is_ok(),
            "{ext}"
        );
        assert!(
            inspect(&d.file(&format!("bad-{ext}.{wrong}"), &bytes)).is_err(),
            "{ext} as .{wrong}"
        );
    }
    // BigTIFF signatures are recognized (the decoder then decides whether it can read them).
    let big = d.file("big.tif", b"II+\0\x08\0\0\0rest-of-file");
    assert!(
        !matches!(inspect(&big), Err(ConvertError::UnreadableFile(r)) if r.contains("extension"))
    );
    // A text file that happens to start with "BM" still fails as an unreadable BMP.
    let fake = d.file("note.bmp", b"BM is short for bowel movement");
    assert!(matches!(
        inspect(&fake).and_then(|_| display_frame(&fake, 0)),
        Err(ConvertError::UnreadableFile(_))
    ));
}

#[test]
fn tiff_orientation_out_of_byte_range_is_ignored() {
    // 262 wraps to 6 (rotate 90°) if narrowed to a byte; it must count as no orientation.
    let d = Dir::new("tiff-orient-big");
    let p = d.file("o.tif", &tiff(&[picture()], Some(262)));
    let out = decoded(&p, 0);
    assert_eq!((out.width(), out.height()), (W, H), "not rotated");
    assert!(corner(&out, false, false) > 200, "marker stays top-left");
}

fn siz(x: u32, y: u32, tile: u32, comps: u16) -> Vec<u8> {
    let mut cs = vec![0xFF, 0x4F, 0xFF, 0x51, 0, 47, 0, 0];
    for v in [x, y, 0, 0, tile, tile, 0, 0] {
        cs.extend_from_slice(&v.to_be_bytes());
    }
    cs.extend_from_slice(&comps.to_be_bytes());
    cs
}

#[test]
fn jp2_siz_bounds_components_and_tiles() {
    assert_eq!(
        parse_siz(&siz(1000, 2000, 256, 3)),
        Some(Siz {
            width: 1000,
            height: 2000,
            components: 3,
            tiles: 4 * 8
        })
    );
    assert_eq!(parse_siz(&siz(10, 10, 0, 3)), None, "zero tile size");
    assert_eq!(parse_siz(&siz(10, 10, 4, 3)[..30]), None, "truncated");
    let d = Dir::new("jp2-siz");
    let refused = |name: &str, cs: Vec<u8>, want: &str| {
        let p = d.file(name, &cs);
        match open(&p).and_then(|s| s.frame(0)) {
            Err(ConvertError::UnreadableFile(r)) => assert!(r.contains(want), "{name}: {r}"),
            other => panic!("{name}: expected refusal, got {other:?}"),
        }
    };
    // A small image split into a million 1-pixel tiles, and one with 300 components.
    refused("tiles.j2k", siz(1000, 1000, 1, 3), "tiles");
    refused("comps.j2k", siz(100, 100, 100, 300), "components");
}
