//! Timecode labels. Drop-frame changes labels only, never durations or edit math (DM-TIME-004).

use serde::{Deserialize, Serialize};

use crate::time::{Dur, Rate};

/// How times are shown to the user.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum TimeDisplay {
    /// HH:MM:SS:FF, or HH;MM;SS;FF with drop-frame.
    #[default]
    Timecode,
    /// Absolute frame count.
    Frames,
    /// HH:MM:SS:sssss audio samples.
    Samples,
}

/// Formats a signed offset from the start of the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimecodeFormat {
    pub rate: Rate,
    pub drop_frame: bool,
    pub display: TimeDisplay,
    /// Sample rate for `TimeDisplay::Samples`.
    pub audio_rate: Rate,
}

impl TimecodeFormat {
    pub fn new(rate: Rate, drop_frame: bool) -> Self {
        TimecodeFormat {
            rate,
            drop_frame: drop_frame && rate.supports_drop_frame(),
            display: TimeDisplay::Timecode,
            audio_rate: Rate::HZ_48000,
        }
    }

    pub fn format(&self, d: Dur) -> String {
        let (neg, d) = if d.is_negative() { ("-", -d) } else { ("", d) };
        match self.display {
            TimeDisplay::Frames => format!("{neg}{}", self.rate.dur_to_frames_floor(d)),
            TimeDisplay::Samples => {
                let total = self.audio_rate.dur_to_frames_floor(d);
                let per_sec = self.audio_rate.num as i64 / self.audio_rate.den.max(1) as i64;
                let secs = total / per_sec.max(1);
                let rem = total % per_sec.max(1);
                format!(
                    "{neg}{:02}:{:02}:{:02}:{:05}",
                    secs / 3600,
                    (secs / 60) % 60,
                    secs % 60,
                    rem
                )
            }
            TimeDisplay::Timecode => {
                let frames = self.rate.dur_to_frames_floor(d);
                format!(
                    "{neg}{}",
                    frames_to_timecode(frames, self.rate, self.drop_frame)
                )
            }
        }
    }

    /// Parses user input: a timecode with any of `:;.,` separators, a bare frame count, or a
    /// relative offset such as `+10` / `-1:00`. Relative values are returned as `Relative`.
    pub fn parse(&self, text: &str) -> Option<Parsed> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let (relative, sign, body) = match text.as_bytes()[0] {
            b'+' => (true, 1, &text[1..]),
            b'-' => (true, -1, &text[1..]),
            _ => (false, 1, text),
        };
        let frames = parse_frames(body.trim(), self.rate, self.drop_frame)?;
        let d = self.rate.frames_to_dur(frames * sign);
        Some(if relative {
            Parsed::Relative(d)
        } else {
            Parsed::Absolute(d)
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parsed {
    Absolute(Dur),
    Relative(Dur),
}

fn drop_params(rate: Rate) -> (i64, i64, i64) {
    // (dropped labels per minute, frames per ten minutes, frames per dropped minute)
    let base = rate.timecode_base() as i64;
    let drop = if base >= 60 { 4 } else { 2 };
    (drop, base * 600 - drop * 9, base * 60 - drop)
}

/// Frame count -> `HH:MM:SS:FF` (or `HH;MM;SS;FF` for drop-frame).
pub fn frames_to_timecode(frames: i64, rate: Rate, drop_frame: bool) -> String {
    let base = rate.timecode_base() as i64;
    let drop_frame = drop_frame && rate.supports_drop_frame();
    let mut n = frames.max(0);
    if drop_frame {
        let (drop, per_ten, per_min) = drop_params(rate);
        let tens = n / per_ten;
        let rem = n % per_ten;
        n += drop * 9 * tens;
        if rem > drop {
            n += drop * ((rem - drop) / per_min);
        }
    }
    let ff = n % base;
    let ss = (n / base) % 60;
    let mm = (n / (base * 60)) % 60;
    let hh = n / (base * 3600);
    let sep = if drop_frame { ';' } else { ':' };
    format!("{hh:02}{sep}{mm:02}{sep}{ss:02}{sep}{ff:02}")
}

/// Parses `HH:MM:SS:FF` (fewer fields are allowed: `SS:FF`, `MM:SS:FF`) or a bare frame number.
/// A bare number with no separators is read like Premiere's timecode fields: digits fill from the
/// right, so `1000` is 00:00:10:00.
pub fn parse_frames(text: &str, rate: Rate, drop_frame: bool) -> Option<i64> {
    let base = rate.timecode_base() as i64;
    let drop_frame = drop_frame && rate.supports_drop_frame();
    let parts: Vec<&str> = text.split([':', ';', '.', ',']).collect();
    let fields: Vec<i64> = if parts.len() == 1 {
        let digits = parts[0];
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) || digits.len() > 10 {
            return None;
        }
        // right-aligned pairs: ...HH MM SS FF
        let padded = format!("{digits:0>8}");
        let head_len = padded.len() - 6;
        vec![
            padded[..head_len].parse().ok()?,
            padded[head_len..head_len + 2].parse().ok()?,
            padded[head_len + 2..head_len + 4].parse().ok()?,
            padded[head_len + 4..].parse().ok()?,
        ]
    } else {
        if parts.len() > 4 {
            return None;
        }
        let mut v = Vec::new();
        for p in &parts {
            v.push(if p.is_empty() {
                0
            } else {
                p.parse::<i64>().ok()?
            });
        }
        while v.len() < 4 {
            v.insert(0, 0);
        }
        v
    };
    let (hh, mm, ss, ff) = (fields[0], fields[1], fields[2], fields[3]);
    if fields.iter().any(|v| *v < 0) {
        return None;
    }
    let mut n = hh * 3600 * base + mm * 60 * base + ss * base + ff;
    if drop_frame {
        let (drop, _, _) = drop_params(rate);
        let minutes = hh * 60 + mm;
        n -= drop * (minutes - minutes / 10);
    }
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_drop() {
        assert_eq!(frames_to_timecode(0, Rate::FPS_25, false), "00:00:00:00");
        assert_eq!(
            frames_to_timecode(25 * 3661 + 7, Rate::FPS_25, false),
            "01:01:01:07"
        );
        assert_eq!(
            frames_to_timecode(24, Rate::FPS_23_976, false),
            "00:00:01:00"
        );
    }

    #[test]
    fn drop_frame_labels_skip_two_frames_per_minute() {
        let r = Rate::FPS_29_97;
        assert_eq!(frames_to_timecode(1799, r, true), "00;00;59;29");
        assert_eq!(frames_to_timecode(1800, r, true), "00;01;00;02");
        assert_eq!(frames_to_timecode(17982, r, true), "00;10;00;00");
        assert_eq!(frames_to_timecode(107892, r, true), "01;00;00;00");
        for n in [0, 1799, 1800, 1801, 17981, 17982, 107_891, 107_892, 123_456] {
            let tc = frames_to_timecode(n, r, true);
            assert_eq!(parse_frames(&tc, r, true), Some(n), "{tc}");
        }
    }

    #[test]
    fn drop_frame_59_94() {
        let r = Rate::FPS_59_94;
        assert_eq!(frames_to_timecode(3600, r, true), "00;01;00;04");
        for n in [0, 3599, 3600, 35_964, 215_784] {
            assert_eq!(
                parse_frames(&frames_to_timecode(n, r, true), r, true),
                Some(n)
            );
        }
    }

    #[test]
    fn parse_shorthand() {
        let r = Rate::FPS_25;
        assert_eq!(parse_frames("1000", r, false), Some(250));
        assert_eq!(parse_frames("1:00", r, false), Some(25));
        assert_eq!(parse_frames("00:01:00:00", r, false), Some(1500));
        assert_eq!(parse_frames("x", r, false), None);
        let f = TimecodeFormat::new(r, false);
        assert_eq!(f.parse("+10"), Some(Parsed::Relative(r.frames_to_dur(10))));
        assert_eq!(
            f.parse("-1:00"),
            Some(Parsed::Relative(r.frames_to_dur(-25)))
        );
    }

    #[test]
    fn negative_and_other_displays() {
        let mut f = TimecodeFormat::new(Rate::FPS_25, false);
        assert_eq!(f.format(-Rate::FPS_25.frames_to_dur(25)), "-00:00:01:00");
        f.display = TimeDisplay::Frames;
        assert_eq!(f.format(Rate::FPS_25.frames_to_dur(30)), "30");
        f.display = TimeDisplay::Samples;
        assert_eq!(f.format(Dur::from_seconds(1.5)), "00:00:01:24000");
    }
}
