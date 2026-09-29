//! Thumbnails for the Project panel and the timeline.

use std::path::Path;

use op_core::MediaAsset;

use crate::video::VideoDecoder;
use crate::{MediaError, Result};

/// An RGBA8 picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Size that fits `(w, h)` inside `max_w` x `max_h` keeping the aspect ratio.
pub fn fit(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let s = (max_w as f64 / w.max(1) as f64)
        .min(max_h as f64 / h.max(1) as f64)
        .min(1.0);
    (
        ((w as f64 * s).round() as u32).max(1),
        ((h as f64 * s).round() as u32).max(1),
    )
}

/// A thumbnail of frame `index` of an asset's video.
pub fn thumbnail(asset: &MediaAsset, index: i64, max_w: u32, max_h: u32) -> Result<Image> {
    let v = asset
        .video
        .as_ref()
        .ok_or_else(|| MediaError::Unsupported("no video".into()))?;
    let mut dec = VideoDecoder::open(
        Path::new(&asset.path),
        Some(v.index),
        asset.frame_rate(),
        v.color,
        asset.is_still(),
    )?;
    let frame = dec.frame(index.clamp(0, (v.frames - 1).max(0)))?;
    let (dw, dh) = v.display_size();
    let (w, h) = fit(dw, dh, max_w, max_h);
    // scale in stored orientation, then rotate for display
    let (sw, sh) = if v.rotation.rem_euclid(180) == 90 {
        (h, w)
    } else {
        (w, h)
    };
    let rgba = frame.to_rgba8_scaled(sw, sh);
    Ok(rotate(
        Image {
            width: sw,
            height: sh,
            rgba,
        },
        v.rotation,
    ))
}

/// Rotates clockwise by 90, 180 or 270 degrees.
pub fn rotate(img: Image, degrees: i32) -> Image {
    let (w, h) = (img.width as usize, img.height as usize);
    let d = degrees.rem_euclid(360);
    if d == 0 {
        return img;
    }
    let (nw, nh) = if d == 180 { (w, h) } else { (h, w) };
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match d {
                90 => (h - 1 - y, x),
                180 => (w - 1 - x, h - 1 - y),
                _ => (y, w - 1 - x),
            };
            out[(ny * nw + nx) * 4..][..4].copy_from_slice(&img.rgba[(y * w + x) * 4..][..4]);
        }
    }
    Image {
        width: nw as u32,
        height: nh as u32,
        rgba: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_and_rotate() {
        assert_eq!(fit(1920, 1080, 320, 320), (320, 180));
        assert_eq!(fit(100, 50, 320, 320), (100, 50));
        let img = Image {
            width: 2,
            height: 1,
            rgba: vec![1, 1, 1, 1, 2, 2, 2, 2],
        };
        let r = rotate(img.clone(), 90);
        assert_eq!((r.width, r.height), (1, 2));
        assert_eq!(r.rgba[..4], [1, 1, 1, 1]);
        assert_eq!(rotate(rotate(img.clone(), 180), 180), img);
    }
}
