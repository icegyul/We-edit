//! CMX 3600 edit decision list export (one video track plus up to two audio channels).

use std::fmt::Write as _;

use op_core::*;

use crate::ProjectError;

fn reel(p: &Project, c: &Clip) -> String {
    let name = match &c.source {
        ClipSource::Asset { asset, .. } => p
            .asset(*asset)
            .map(|a| a.file_name().to_string())
            .unwrap_or_default(),
        _ => String::new(),
    };
    let stem: String = name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(&name)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    if stem.is_empty() {
        "AX".into()
    } else {
        stem.to_uppercase()
    }
}

/// Exports V1 (and A1/A2 as the audio channels of the events) of a sequence.
pub fn export(p: &Project, sid: SequenceId) -> Result<String, ProjectError> {
    let seq = p
        .sequence(sid)
        .ok_or_else(|| ProjectError::Format("no such sequence".into()))?;
    let rate = seq.rate();
    let fmt = TimecodeFormat::new(rate, seq.settings.drop_frame);
    let tc = |d: Dur| fmt.format(d);
    let start = seq.settings.start_timecode;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "TITLE: {}",
        seq.name
            .chars()
            .filter(|c| !c.is_control())
            .collect::<String>()
    );
    let _ = writeln!(
        out,
        "FCM: {}",
        if fmt.drop_frame {
            "DROP FRAME"
        } else {
            "NON-DROP FRAME"
        }
    );
    let _ = writeln!(out);
    let mut event = 0;
    let mut tracks: Vec<(TrackRef, &str)> = vec![(TrackRef::video(0), "V")];
    if seq.audio.len() > 1 {
        tracks.push((TrackRef::audio(0), "A"));
        tracks.push((TrackRef::audio(1), "A2"));
    } else if !seq.audio.is_empty() {
        tracks.push((TrackRef::audio(0), "A"));
    }
    for (r, channel) in tracks {
        let Some(t) = seq.track(r) else { continue };
        for c in &t.clips {
            event += 1;
            let src = c.source_range();
            let transition = t
                .transitions
                .iter()
                .find(|tr| tr.to == Some(c.id) && tr.from.is_some());
            let rec_in = c.start.since_zero() + start;
            let rec_out = c.end().since_zero() + start;
            match transition {
                Some(tr) => {
                    let frames = rate.dur_to_frames_round(tr.duration);
                    let _ = writeln!(
                        out,
                        "{event:03}  {:<8} {channel:<5} D    {frames:03} {} {} {} {}",
                        reel(p, c),
                        tc(src.start.since_zero()),
                        tc(src.end.since_zero()),
                        tc(rec_in),
                        tc(rec_out)
                    );
                }
                None => {
                    let _ = writeln!(
                        out,
                        "{event:03}  {:<8} {channel:<5} C        {} {} {} {}",
                        reel(p, c),
                        tc(src.start.since_zero()),
                        tc(src.end.since_zero()),
                        tc(rec_in),
                        tc(rec_out)
                    );
                }
            }
            if !c.speed.is_normal() || c.reverse {
                let fps = c.speed.as_f64() * rate.as_f64() * if c.reverse { -1.0 } else { 1.0 };
                let _ = writeln!(
                    out,
                    "M2   {:<8}       {fps:05.1}                {}",
                    reel(p, c),
                    tc(src.start.since_zero())
                );
            }
            let _ = writeln!(out, "* FROM CLIP NAME: {}", c.name);
            if let ClipSource::Asset { asset, .. } = c.source
                && let Some(a) = p.asset(asset)
            {
                let _ = writeln!(out, "* SOURCE FILE: {}", a.path);
            }
            let _ = writeln!(out);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_have_source_and_record_timecodes() {
        let (p, sid) = crate::otio::tests_support::sample_project();
        let edl = export(&p, sid).unwrap();
        assert!(edl.starts_with("TITLE: Edit"));
        assert!(
            edl.contains(
                "001  AB       V     C        00:00:01:00 00:00:05:00 00:00:00:00 00:00:04:00"
            ),
            "{edl}"
        );
        assert!(edl.contains(" D    025 "), "{edl}");
        assert!(edl.contains("* FROM CLIP NAME:"));
    }
}
