//! Shorts: a vertical 1080 x 1920 version of the active sequence, for Shorts, Reels and TikTok.
//!
//! The horizontal sequence is nested whole into a new vertical sequence (the original stays
//! untouched) and framed one of two ways: filling the frame (the middle of the picture, or
//! another stretch of it, cropped) or on a blurred copy of itself. Captions then go on the new
//! sequence like on any other (Window > Sentence Edit), in a design of the Captions panel.

use std::sync::Arc;

use op_core::catalog;
use op_core::*;
use op_timeline::SourceClip;

use crate::editor::Editor;

/// Frame of a shorts sequence.
pub const SHORTS_SIZE: (u32, u32) = (1080, 1920);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortsLayout {
    /// The picture fills the whole frame; the sides are cropped.
    Fill,
    /// The whole picture in the middle over a blurred, enlarged copy of itself.
    Blur,
}

/// Motion scale (percent, on top of fit-to-frame) and horizontal position that make a picture of
/// size `src` fill `frame`, looking at `focus_x` (0 left edge, 0.5 middle, 1 right edge).
pub fn framing(src: (u32, u32), frame: (u32, u32), focus_x: f64) -> (f64, f64) {
    let (sw, sh) = (f64::from(src.0), f64::from(src.1));
    let (fw, fh) = (f64::from(frame.0), f64::from(frame.1));
    let fit = (fw / sw).min(fh / sh);
    let fill = (fw / sw).max(fh / sh);
    // how many frame widths the filled picture is wide: the overflow can be shifted
    let wide = sw * fill / fw;
    let x = 0.5 + (0.5 - focus_x.clamp(0.0, 1.0)) * (wide - 1.0).max(0.0);
    (100.0 * fill / fit, x)
}

fn set(c: &mut Component, key: &str, v: Value) {
    if let Some(p) = c.param_mut(key) {
        p.value = v;
    }
}

impl Editor {
    /// Makes the vertical sequence "<name> 쇼츠" from the active (horizontal) one and opens it.
    pub fn make_shorts(&mut self, layout: ShortsLayout, focus_x: f64) -> Result<SequenceId, String> {
        let src = self.active.ok_or("Open a sequence first")?;
        let seq = self.project.sequence(src).ok_or("Open a sequence first")?.clone();
        let (sw, sh) = (seq.settings.width, seq.settings.height);
        if sh >= sw {
            return Err("이 시퀀스는 이미 세로(또는 정사각형)입니다".into());
        }
        let item = self
            .project
            .items
            .iter()
            .find_map(|(id, it)| matches!(it.kind, ItemKind::Sequence { sequence } if sequence == src).then_some(*id))
            .ok_or("The sequence is not in the project")?;
        let mut settings = seq.settings.clone();
        (settings.width, settings.height) = SHORTS_SIZE;
        let sid = self
            .new_sequence(&format!("{} 쇼츠", seq.name), settings)
            .ok_or("The sequence could not be created")?;
        let spec = SourceClip::from_item(&self.project, item).map_err(|e| e.to_string())?;
        let patch = self.patch(sid);
        let opts = self.opts();
        self.edit("Overwrite", |p| op_timeline::overwrite(p, sid, &spec, SeqTime::ZERO, &patch, opts))
            .ok_or("The sequence could not be placed in the new one")?;

        let (percent, x) = framing((sw, sh), SHORTS_SIZE, focus_x);
        let blur = catalog::find("op.video.gaussian_blur").ok_or("no blur effect")?;
        self.seq_edit("Shorts Layout", move |p, sid, _| {
            let mut ids = p.ids.clone();
            let s = p.sequence_mut(sid).ok_or(EditError::NotFound("sequence".into()))?;
            let first = s.video.first_mut().ok_or(EditError::Nothing)?;
            let first = Arc::make_mut(first);
            let mut front = Vec::new();
            for clip in first.clips.iter_mut() {
                clip.scale_to_frame = true;
                if layout == ShortsLayout::Blur {
                    // the whole picture stays in the middle in front, on a copy that fills the frame
                    let mut whole = clip.clone();
                    whole.id = ids.clip();
                    whole.link = None;
                    whole.group = None;
                    front.push(whole);
                    let mut b = Component::new(blur, &mut ids);
                    set(&mut b, "blurriness", Value::Float(60.0));
                    set(&mut b, "repeat_edge", Value::Bool(true));
                    clip.components.push(b);
                }
                if let Some(m) = clip.components.iter_mut().find(|c| c.effect == catalog::MOTION) {
                    set(m, "scale", Value::Float(percent));
                    set(m, "position", Value::Point([x, 0.5]));
                }
            }
            if !front.is_empty() {
                // V2 of a new sequence is empty; a sequence with a single video track gets one
                if let Some(v2) = s.video.get_mut(1) {
                    Arc::make_mut(v2).clips = front;
                } else {
                    let mut track = Track::new(ids.track(), TrackKind::Video);
                    track.clips = front;
                    s.video.push(Arc::new(track));
                }
            }
            p.ids = ids;
            Ok(())
        })
        .ok_or("The shorts layout could not be applied")?;
        self.selection.clear();
        Ok(sid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_picture_fills_the_tall_frame() {
        let (percent, x) = framing((1920, 1080), SHORTS_SIZE, 0.5);
        assert!((percent - 100.0 * (1920.0 / 1080.0) / (1080.0 / 1920.0)).abs() < 1e-6, "{percent}");
        assert!((x - 0.5).abs() < 1e-9);
        // looking at the left edge pushes the picture right until its left edge meets the frame's
        let (_, left) = framing((1920, 1080), SHORTS_SIZE, 0.0);
        let wide = 1920.0 * (1920.0 / 1080.0) / 1080.0;
        assert!((left - 0.5 - 0.5 * (wide - 1.0)).abs() < 1e-9, "{left}");
        let (_, right) = framing((1920, 1080), SHORTS_SIZE, 1.0);
        assert!((left + right - 1.0).abs() < 1e-9);
    }
}
