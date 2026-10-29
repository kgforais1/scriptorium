//! Colour management: ICC profiles to sRGB, and CMYK to sRGB, through LittleCMS.

use super::convert::{unreadable, ConvertError};
use image::{DynamicImage, ImageBuffer};
use lcms2::{ColorSpaceSignature, Intent, PixelFormat, Profile, Transform};

/// Largest per-channel difference (0–255) at which a profile still counts as sRGB.
const SRGB_TOLERANCE: i16 = 2;
/// Rendering intent for every transform, so the sRGB probe and the conversion agree.
const INTENT: Intent = Intent::Perceptual;

// A corrupt embedded profile makes the page `UnreadableFile` rather than silently treating
// it as sRGB: the curator should know the colours cannot be trusted.

/// True when an embedded RGB or grey profile maps its probe colours onto sRGB within
/// `SRGB_TOLERANCE`, so the pixels can be used unchanged.
pub(super) fn is_srgb_equivalent(icc: &[u8]) -> Result<bool, ConvertError> {
    let src = Profile::new_icc(icc).map_err(unreadable)?;
    let srgb = Profile::new_srgb();
    let probes: [[u8; 3]; 7] = [
        [0, 0, 0],
        [255, 255, 255],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [128, 128, 128],
        [200, 120, 60],
    ];
    match src.color_space() {
        ColorSpaceSignature::RgbData => {
            let tr = Transform::new(&src, PixelFormat::RGB_8, &srgb, PixelFormat::RGB_8, INTENT)
                .map_err(unreadable)?;
            let mut out = probes;
            tr.transform_pixels(&probes, &mut out);
            Ok(close(&probes, &out))
        }
        ColorSpaceSignature::GrayData => {
            let greys: [u8; 5] = [0, 64, 128, 192, 255];
            let tr = Transform::new(&src, PixelFormat::GRAY_8, &srgb, PixelFormat::RGB_8, INTENT)
                .map_err(unreadable)?;
            let mut out = [[0u8; 3]; 5];
            tr.transform_pixels(&greys, &mut out);
            Ok(close(&greys.map(|g| [g, g, g]), &out))
        }
        // A CMYK or Lab profile on an RGB or grey image is a mismatch the webview cannot show.
        _ => Ok(false),
    }
}

fn close(want: &[[u8; 3]], got: &[[u8; 3]]) -> bool {
    want.iter()
        .zip(got)
        .all(|(a, b)| (0..3).all(|k| (a[k] as i16 - b[k] as i16).abs() <= SRGB_TOLERANCE))
}

/// Convert any decoded image to 8-bit sRGB, keeping alpha and grey where present. An image
/// with no profile is taken to be sRGB already.
pub(super) fn to_srgb8(
    img: DynamicImage,
    icc: Option<&[u8]>,
) -> Result<DynamicImage, ConvertError> {
    let is_grey = matches!(img.color(), image::ColorType::L8 | image::ColorType::L16);
    let has_alpha = img.color().has_alpha();
    let profile = match icc {
        Some(p) if !is_srgb_equivalent(p)? => Some(Profile::new_icc(p).map_err(unreadable)?),
        _ => None,
    };
    let Some(src) = profile else {
        return Ok(match (is_grey, has_alpha) {
            (true, false) => DynamicImage::ImageLuma8(img.to_luma8()),
            (_, true) => DynamicImage::ImageRgba8(img.to_rgba8()),
            _ => DynamicImage::ImageRgb8(img.to_rgb8()),
        });
    };
    let srgb = Profile::new_srgb();
    let intent = INTENT;
    if src.color_space() == ColorSpaceSignature::GrayData {
        if !is_grey {
            return Err(unreadable("grey ICC profile on a colour image"));
        }
        let grey = img.to_luma8();
        let tr = Transform::new(&src, PixelFormat::GRAY_8, &srgb, PixelFormat::RGB_8, intent)
            .map_err(unreadable)?;
        let mut out = vec![[0u8; 3]; grey.as_raw().len()];
        tr.transform_pixels(grey.as_raw(), &mut out);
        return rgb_image(img.width(), img.height(), out.concat());
    }
    if has_alpha {
        let rgba = img.to_rgba8();
        let tr = Transform::new(
            &src,
            PixelFormat::RGBA_8,
            &srgb,
            PixelFormat::RGBA_8,
            intent,
        )
        .map_err(unreadable)?;
        let mut out = rgba.clone();
        tr.transform_pixels(rgba.as_raw().as_slice(), out.as_mut());
        return Ok(DynamicImage::ImageRgba8(out));
    }
    let rgb = img.to_rgb8();
    let tr = Transform::new(&src, PixelFormat::RGB_8, &srgb, PixelFormat::RGB_8, intent)
        .map_err(unreadable)?;
    let mut out = rgb.clone();
    tr.transform_pixels(rgb.as_raw().as_slice(), out.as_mut());
    Ok(DynamicImage::ImageRgb8(out))
}

fn rgb_image(w: u32, h: u32, data: Vec<u8>) -> Result<DynamicImage, ConvertError> {
    ImageBuffer::from_raw(w, h, data)
        .map(DynamicImage::ImageRgb8)
        .ok_or_else(|| unreadable("pixel buffer has the wrong size"))
}

/// CMYK samples to sRGB. A CMYK image with no ICC profile is rejected, because the common
/// default CMYK profiles cannot be redistributed. `inverted` is true for Adobe-style JPEGs,
/// which store 255 for no ink.
pub(super) fn cmyk_pixels_to_srgb(
    w: u32,
    h: u32,
    cmyk: &[u8],
    icc: Option<&[u8]>,
    inverted: bool,
) -> Result<DynamicImage, ConvertError> {
    let icc = icc.ok_or_else(|| unreadable("CMYK image has no ICC profile"))?;
    let src = Profile::new_icc(icc).map_err(unreadable)?;
    if src.color_space() != ColorSpaceSignature::CmykData {
        return Err(unreadable("CMYK image carries a non-CMYK ICC profile"));
    }
    let fmt = if inverted {
        PixelFormat::CMYK_8_REV
    } else {
        PixelFormat::CMYK_8
    };
    let tr = Transform::new(&src, fmt, &Profile::new_srgb(), PixelFormat::RGB_8, INTENT)
        .map_err(unreadable)?;
    let px: Vec<[u8; 4]> = cmyk
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    if px.len() as u64 != u64::from(w) * u64::from(h) {
        return Err(unreadable("CMYK buffer has the wrong size"));
    }
    let mut out = vec![[0u8; 3]; px.len()];
    tr.transform_pixels(&px, &mut out);
    rgb_image(w, h, out.concat())
}

/// Decode a four-component JPEG as raw CMYK and convert it to sRGB with its embedded profile.
pub(super) fn cmyk_jpeg_to_srgb(bytes: &[u8]) -> Result<DynamicImage, ConvertError> {
    use zune_core::bytestream::ZCursor;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    use zune_jpeg::JpegDecoder;

    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::CMYK);
    let mut dec = JpegDecoder::new_with_options(ZCursor::new(bytes), opts);
    dec.decode_headers().map_err(unreadable)?;
    let icc = dec.icc_profile();
    let info = dec
        .info()
        .ok_or_else(|| unreadable("JPEG has no frame header"))?;
    let raw = dec.decode().map_err(unreadable)?;
    cmyk_pixels_to_srgb(
        u32::from(info.width),
        u32::from(info.height),
        &raw,
        icc.as_deref(),
        super::decode::jpeg_has_adobe_marker(bytes),
    )
}
