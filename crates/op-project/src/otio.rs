//! OpenTimelineIO interchange (`.otio` JSON). OTIO references media instead of embedding it and
//! does not model every editor feature, so it is an interchange format, not our project model
//! (docs/data-model.md 10).

use std::collections::HashMap;
use std::sync::Arc;

use op_core::catalog::{self, EffectKind};
use op_core::*;
use serde_json::{Value as J, json};

use crate::ProjectError;

fn rt(frames: f64, rate: f64) -> J {
    json!({"OTIO_SCHEMA": "RationalTime.1", "rate": rate, "value": frames})
}

fn range(start: f64, dur: f64, rate: f64) -> J {
    json!({"OTIO_SCHEMA": "TimeRange.1", "start_time": rt(start, rate), "duration": rt(dur, rate)})
}

fn frames(d: Dur, rate: Rate) -> f64 {
    d.seconds() * rate.as_f64()
}

/// `file://` URL for a local path.
pub fn path_to_url(path: &str) -> String {
    let p = path.replace('\\', "/");
    let mut out = String::from("file://");
    if !p.starts_with('/') {
        out.push('/');
    }
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Local path from a `file://` URL (or a plain path).
pub fn url_to_path(url: &str) -> String {
    let rest = url
        .strip_prefix("file://localhost")
        .or_else(|| url.strip_prefix("file://"))
        .or_else(|| url.strip_prefix("file:"))
        .unwrap_or(url);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz"),
                16,
            )
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&out).into_owned();
    // "/C:/x" -> "C:/x"
    let b = s.as_bytes();
    if b.len() > 3 && b[0] == b'/' && b[2] == b':' && b[1].is_ascii_alphabetic() {
        return s[1..].to_string();
    }
    s
}

fn clip_json(p: &Project, seq: &Sequence, c: &Clip) -> J {
    let rate = seq.rate();
    let (media_rate, target) = match &c.source {
        ClipSource::Asset { asset, .. } => {
            let a = p.asset(*asset);
            (
                a.and_then(|a| a.frame_rate()).unwrap_or(rate),
                a.map(|a| path_to_url(&a.path)),
            )
        }
        _ => (rate, None),
    };
    let src = c.source_range();
    let mr = media_rate.as_f64();
    let reference = match target {
        Some(url) => {
            json!({"OTIO_SCHEMA": "ExternalReference.1", "name": "", "target_url": url, "available_range": null, "metadata": {}})
        }
        None => {
            json!({"OTIO_SCHEMA": "MissingReference.1", "name": p.source_name(&c.source), "available_range": null, "metadata": {}})
        }
    };
    let mut effects = Vec::new();
    if let Some(h) = c.hold {
        let _ = h;
        effects.push(json!({"OTIO_SCHEMA": "FreezeFrame.1", "name": "", "effect_name": "FreezeFrame", "time_scalar": 0.0, "metadata": {}}));
    } else if !c.speed.is_normal() || c.reverse {
        let s = c.speed.as_f64() * if c.reverse { -1.0 } else { 1.0 };
        effects.push(json!({"OTIO_SCHEMA": "LinearTimeWarp.1", "name": "", "effect_name": "LinearTimeWarp", "time_scalar": s, "metadata": {}}));
    }
    // OTIO source ranges are expressed in the timeline duration of the clip
    json!({
        "OTIO_SCHEMA": "Clip.2",
        "name": c.name,
        "enabled": c.enabled,
        "source_range": range(frames(src.start.since_zero(), media_rate), frames(c.duration, media_rate), mr),
        "media_references": {"DEFAULT_MEDIA": reference},
        "active_media_reference_key": "DEFAULT_MEDIA",
        "effects": effects,
        "markers": [],
        "metadata": {"openpremier": {"clip_id": c.id.0, "link": c.link.map(|l| l.0), "sequence_rate": rate.as_f64()}},
    })
}

/// Exports one sequence as an OTIO timeline.
pub fn export(p: &Project, sid: SequenceId) -> Result<String, ProjectError> {
    let seq = p
        .sequence(sid)
        .ok_or_else(|| ProjectError::Format("no such sequence".into()))?;
    let rate = seq.rate();
    let r = rate.as_f64();
    let mut tracks = Vec::new();
    for kind in [TrackKind::Video, TrackKind::Audio] {
        for (i, t) in seq.tracks(kind).iter().enumerate() {
            let mut children = Vec::new();
            let mut cursor = SeqTime::ZERO;
            for (ci, c) in t.clips.iter().enumerate() {
                if c.start > cursor {
                    let gap = c.start - cursor;
                    children.push(json!({"OTIO_SCHEMA": "Gap.1", "name": "", "source_range": range(0.0, frames(gap, rate), r), "effects": [], "markers": [], "metadata": {}}));
                }
                // a transition into this clip sits between the previous item and this one
                if ci > 0
                    && let Some(tr) = t
                        .transitions
                        .iter()
                        .find(|tr| tr.to == Some(c.id) && tr.from.is_some())
                {
                    let before = tr.before_cut();
                    children.push(json!({
                        "OTIO_SCHEMA": "Transition.1",
                        "name": catalog::find(&tr.effect).map(|d| d.name).unwrap_or(""),
                        "transition_type": "SMPTE_Dissolve",
                        "in_offset": rt(frames(before, rate), r),
                        "out_offset": rt(frames(tr.duration - before, rate), r),
                        "metadata": {"openpremier": {"effect": tr.effect}},
                    }));
                }
                children.push(clip_json(p, seq, c));
                cursor = c.end();
            }
            let name = seq.track_name(TrackRef { kind, index: i });
            tracks.push(json!({
                "OTIO_SCHEMA": "Track.1",
                "name": name,
                "kind": if kind == TrackKind::Video { "Video" } else { "Audio" },
                "children": children,
                "source_range": null,
                "effects": [],
                "markers": [],
                "metadata": {},
                "enabled": t.enabled && !t.muted,
            }));
        }
    }
    let markers: Vec<J> = seq
        .markers
        .iter()
        .map(|m| {
            json!({
                "OTIO_SCHEMA": "Marker.2",
                "name": m.name,
                "comment": m.comment,
                "color": marker_color_name(m.color),
                "marked_range": range(frames(m.start.since_zero(), rate), frames(m.duration, rate), r),
                "metadata": {},
            })
        })
        .collect();
    let tl = json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": seq.name,
        "global_start_time": rt(frames(seq.settings.start_timecode, rate), r),
        "metadata": {"openpremier": {"width": seq.settings.width, "height": seq.settings.height, "audio_rate": seq.settings.audio_rate}},
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "name": "tracks",
            "children": tracks,
            "source_range": null,
            "effects": [],
            "markers": markers,
            "metadata": {},
        },
    });
    serde_json::to_string_pretty(&tl).map_err(|e| ProjectError::Format(e.to_string()))
}

fn marker_color_name(c: MarkerColor) -> &'static str {
    match c {
        MarkerColor::Green => "GREEN",
        MarkerColor::Red => "RED",
        MarkerColor::Magenta => "MAGENTA",
        MarkerColor::Orange => "ORANGE",
        MarkerColor::Yellow => "YELLOW",
        MarkerColor::White => "WHITE",
        MarkerColor::Blue => "BLUE",
        MarkerColor::Cyan => "CYAN",
    }
}

fn color_from_name(s: &str) -> MarkerColor {
    match s.to_ascii_uppercase().as_str() {
        "RED" => MarkerColor::Red,
        "MAGENTA" | "PINK" | "PURPLE" => MarkerColor::Magenta,
        "ORANGE" => MarkerColor::Orange,
        "YELLOW" => MarkerColor::Yellow,
        "WHITE" | "BLACK" => MarkerColor::White,
        "BLUE" => MarkerColor::Blue,
        "CYAN" => MarkerColor::Cyan,
        _ => MarkerColor::Green,
    }
}

/// Reads a RationalTime as a duration.
fn time_of(v: &J) -> Option<Dur> {
    let rate = v.get("rate")?.as_f64()?;
    let value = v.get("value")?.as_f64()?;
    (rate > 0.0 && value.is_finite()).then(|| Dur::from_seconds(value / rate))
}

fn range_of(v: &J) -> Option<(Dur, Dur)> {
    Some((time_of(v.get("start_time")?)?, time_of(v.get("duration")?)?))
}

fn rate_of(v: &J) -> Option<f64> {
    v.get("rate").and_then(|r| r.as_f64()).filter(|r| *r > 0.0)
}

/// Imports an OTIO timeline into a new project. Media are referenced by path and must be probed
/// by the caller to fill in stream details.
pub fn import(text: &str, name: &str) -> Result<(Project, SequenceId), ProjectError> {
    let tl: J = serde_json::from_str(text).map_err(|e| ProjectError::Format(e.to_string()))?;
    if !tl
        .get("OTIO_SCHEMA")
        .and_then(|s| s.as_str())
        .is_some_and(|s| s.starts_with("Timeline."))
    {
        return Err(ProjectError::Format(
            "not an OpenTimelineIO timeline".into(),
        ));
    }
    let stack = tl
        .get("tracks")
        .ok_or_else(|| ProjectError::Format("the timeline has no tracks".into()))?;
    let empty = Vec::new();
    let tracks = stack
        .get("children")
        .and_then(|c| c.as_array())
        .unwrap_or(&empty);

    // the sequence rate: global start time rate, else the first clip's timeline rate
    let mut fps = tl.get("global_start_time").and_then(rate_of);
    if fps.is_none() {
        'outer: for t in tracks {
            for c in t
                .get("children")
                .and_then(|c| c.as_array())
                .unwrap_or(&empty)
            {
                if let Some(r) = c
                    .get("source_range")
                    .and_then(|r| r.get("duration"))
                    .and_then(rate_of)
                {
                    fps = Some(r);
                    break 'outer;
                }
            }
        }
    }
    let rate = Rate::from_f64(fps.unwrap_or(25.0));
    let meta = &tl["metadata"]["openpremier"];
    let settings = SequenceSettings {
        rate,
        width: meta["width"].as_u64().unwrap_or(1920) as u32,
        height: meta["height"].as_u64().unwrap_or(1080) as u32,
        audio_rate: meta["audio_rate"].as_u64().unwrap_or(48000) as u32,
        video_tracks: 1,
        audio_tracks: 1,
        ..SequenceSettings::default()
    };
    let mut p = Project::new(name);
    let seq_name = tl
        .get("name")
        .and_then(|n| n.as_str())
        .filter(|n| !n.is_empty())
        .unwrap_or(name)
        .to_string();
    let bin = p.root;
    let (sid, _) = p.add_sequence(bin, seq_name, settings);
    let mut urls: HashMap<String, (AssetId, ItemId)> = HashMap::new();
    let mut video: Vec<Arc<Track>> = Vec::new();
    let mut audio: Vec<Arc<Track>> = Vec::new();
    let mut links: HashMap<u64, LinkId> = HashMap::new();
    for t in tracks {
        let kind = match t.get("kind").and_then(|k| k.as_str()) {
            Some("Audio") => TrackKind::Audio,
            _ => TrackKind::Video,
        };
        let mut track = Track::new(p.ids.track(), kind);
        if kind == TrackKind::Audio {
            track.components = default_components(EffectKind::AudioFixed, &mut p.ids);
        }
        track.enabled = t.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true);
        let mut cursor = SeqTime::ZERO;
        let mut pending: Option<(Dur, Dur, String)> = None;
        let mut last_clip: Option<ClipId> = None;
        for item in t
            .get("children")
            .and_then(|c| c.as_array())
            .unwrap_or(&empty)
        {
            let schema = item
                .get("OTIO_SCHEMA")
                .and_then(|s| s.as_str())
                .unwrap_or("");
            if schema.starts_with("Gap.") {
                if let Some((_, d)) = item.get("source_range").and_then(range_of) {
                    cursor += d.round_frames(rate);
                }
                last_clip = None;
                continue;
            }
            if schema.starts_with("Transition.") {
                let inn = item
                    .get("in_offset")
                    .and_then(time_of)
                    .unwrap_or(Dur::ZERO)
                    .round_frames(rate);
                let out = item
                    .get("out_offset")
                    .and_then(time_of)
                    .unwrap_or(Dur::ZERO)
                    .round_frames(rate);
                let effect = item["metadata"]["openpremier"]["effect"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        if kind == TrackKind::Video {
                            catalog::CROSS_DISSOLVE.into()
                        } else {
                            catalog::CONSTANT_POWER.into()
                        }
                    });
                pending = Some((inn, out, effect));
                continue;
            }
            if !schema.starts_with("Clip.") {
                cursor += item
                    .get("source_range")
                    .and_then(range_of)
                    .map(|r| r.1)
                    .unwrap_or(Dur::ZERO);
                last_clip = None;
                continue;
            }
            let Some((src_start, dur)) = item.get("source_range").and_then(range_of) else {
                continue;
            };
            let dur = dur.round_frames(rate);
            if dur.0 <= 0 {
                continue;
            }
            let reference = {
                let key = item
                    .get("active_media_reference_key")
                    .and_then(|k| k.as_str())
                    .unwrap_or("DEFAULT_MEDIA");
                item.get("media_references")
                    .and_then(|m| m.get(key))
                    .or_else(|| item.get("media_reference"))
            };
            let url = reference
                .and_then(|r| r.get("target_url"))
                .and_then(|u| u.as_str());
            let clip_name = item
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let mut speed = Speed::NORMAL;
            let mut reverse = false;
            let mut hold = false;
            for e in item
                .get("effects")
                .and_then(|e| e.as_array())
                .unwrap_or(&empty)
            {
                let s = e.get("OTIO_SCHEMA").and_then(|s| s.as_str()).unwrap_or("");
                if s.starts_with("FreezeFrame") {
                    hold = true;
                } else if s.starts_with("LinearTimeWarp")
                    && let Some(ts) = e.get("time_scalar").and_then(|t| t.as_f64())
                    && ts != 0.0
                {
                    speed = Speed::from_percent(ts.abs() * 100.0);
                    reverse = ts < 0.0;
                }
            }
            let source = match url {
                Some(u) => {
                    let path = url_to_path(u);
                    let (asset, item_id) = match urls.get(&path) {
                        Some(x) => *x,
                        None => {
                            let a = placeholder_asset(&path, rate);
                            let (asset, item_id) = p.add_asset(bin, a);
                            urls.insert(path.clone(), (asset, item_id));
                            (asset, item_id)
                        }
                    };
                    ensure_stream(&mut p, asset, kind);
                    ClipSource::Asset {
                        asset,
                        item: Some(item_id),
                        stream: 0,
                    }
                }
                None => ClipSource::Graphic,
            };
            if matches!(source, ClipSource::Graphic) && kind == TrackKind::Audio {
                cursor += dur;
                continue;
            }
            let fixed = if kind == TrackKind::Video {
                EffectKind::VideoFixed
            } else {
                EffectKind::AudioFixed
            };
            let id = p.ids.clip();
            let components = default_components(fixed, &mut p.ids);
            let mut clip = Clip {
                id,
                name: clip_name,
                kind,
                source,
                start: cursor,
                duration: dur,
                source_in: SrcTime::ZERO + src_start,
                speed,
                reverse,
                hold: None,
                enabled: item
                    .get("enabled")
                    .and_then(|e| e.as_bool())
                    .unwrap_or(true),
                link: None,
                group: None,
                label: Label::None,
                components,
                gain_db: 0.0,
                scale_to_frame: false,
                channels: (kind == TrackKind::Audio).then_some(ChannelLayout::Stereo),
            };
            if hold {
                clip.hold = Some(clip.source_in);
            }
            if let Some(l) = item["metadata"]["openpremier"]["link"].as_u64() {
                clip.link = Some(*links.entry(l).or_insert_with(|| p.ids.link()));
            }
            if let (Some((inn, out, effect)), Some(prev)) = (pending.take(), last_clip)
                && inn + out > Dur::ZERO
            {
                track.transitions.push(Transition {
                    id: p.ids.transition(),
                    effect,
                    cut: cursor,
                    duration: inn + out,
                    alignment: Alignment::Custom(inn),
                    from: Some(prev),
                    to: Some(id),
                    params: vec![],
                });
            }
            cursor += dur;
            last_clip = Some(id);
            track.clips.push(clip);
        }
        match kind {
            TrackKind::Video => video.push(Arc::new(track)),
            TrackKind::Audio => audio.push(Arc::new(track)),
        }
    }
    let s = p.sequence_mut(sid).unwrap();
    if !video.is_empty() {
        s.video = video;
    }
    if !audio.is_empty() {
        s.audio = audio;
    }
    for m in stack
        .get("markers")
        .and_then(|m| m.as_array())
        .unwrap_or(&empty)
    {
        let Some((start, dur)) = m.get("marked_range").and_then(range_of) else {
            continue;
        };
        let id = p.ids.marker();
        let s = p.sequence_mut(sid).unwrap();
        let mut mk = Marker::new(id, SeqTime::ZERO + start.round_frames(rate));
        mk.duration = dur.round_frames(rate);
        mk.name = m.get("name").and_then(|n| n.as_str()).unwrap_or("").into();
        mk.comment = m
            .get("comment")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .into();
        mk.color = color_from_name(m.get("color").and_then(|c| c.as_str()).unwrap_or(""));
        s.markers.push(mk);
    }
    fit_assets(&mut p);
    p.normalize();
    p.validate().map_err(ProjectError::Invalid)?;
    Ok((p, sid))
}

/// An asset known only by path, before probing.
pub fn placeholder_asset(path: &str, rate: Rate) -> MediaAsset {
    MediaAsset {
        id: AssetId(0),
        path: path.to_string(),
        proxy: None,
        kind: MediaKind::Video,
        video: None,
        audio: vec![],
        duration: Dur::ZERO,
        interpretation: Interpretation {
            frame_rate: Some(rate),
            ..Default::default()
        },
        file_size: 0,
        modified_unix: 0,
    }
}

fn ensure_stream(p: &mut Project, asset: AssetId, kind: TrackKind) {
    let a = Arc::make_mut(p.assets.get_mut(&asset).unwrap());
    let rate = a.interpretation.frame_rate.unwrap_or(Rate::FPS_25);
    match kind {
        TrackKind::Video if a.video.is_none() => {
            a.video = Some(VideoStream {
                index: 0,
                codec: String::new(),
                width: 1920,
                height: 1080,
                pixel_aspect: (1, 1),
                rate,
                frames: 1,
                pixel_format: String::new(),
                bit_depth: 8,
                alpha: AlphaMode::None,
                color: ColorInfo::default(),
                field_order: FieldOrder::Progressive,
                start: Dur::ZERO,
                rotation: 0,
                timecode: None,
            });
            a.kind = MediaKind::Video;
        }
        TrackKind::Audio if a.audio.is_empty() => {
            a.audio.push(AudioStream {
                index: 0,
                codec: String::new(),
                sample_rate: 48000,
                layout: ChannelLayout::Stereo,
                samples: 0,
                start: Dur::ZERO,
            });
            if a.video.is_none() {
                a.kind = MediaKind::Audio;
            }
        }
        _ => {}
    }
}

/// Grows placeholder asset durations so every clip's source range is covered.
pub fn fit_assets(p: &mut Project) {
    let mut need: HashMap<AssetId, Dur> = HashMap::new();
    for s in p.sequences.values() {
        for (_, c) in s.clips() {
            if let ClipSource::Asset { asset, .. } = c.source {
                let e = need.entry(asset).or_insert(Dur::ZERO);
                *e = (*e).max(c.source_range().end.since_zero());
            }
        }
    }
    for (id, end) in need {
        let Some(a) = p.assets.get_mut(&id) else {
            continue;
        };
        if a.available().is_some_and(|r| r.end.since_zero() >= end) {
            continue;
        }
        let a = Arc::make_mut(a);
        a.duration = a.duration.max(end);
        let rate = a.frame_rate().unwrap_or(Rate::FPS_25);
        if let Some(v) = &mut a.video {
            v.frames = v.frames.max(rate.dur_to_frames_ceil(end));
        }
        for s in &mut a.audio {
            s.samples = s.samples.max(s.rate().dur_to_frames_ceil(end));
        }
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use op_timeline::*;

    /// Two V/A clips, a third after a gap, a transition and a marker.
    pub(crate) fn sample_project() -> (Project, SequenceId) {
        let mut p = Project::new("x");
        let mut a = placeholder_asset("C:/media/a b.mov", Rate::FPS_25);
        a.duration = Dur::from_seconds(60.0);
        let (asset, item) = p.add_asset(p.root, a);
        ensure_stream(&mut p, asset, TrackKind::Video);
        ensure_stream(&mut p, asset, TrackKind::Audio);
        fit_assets(&mut p);
        {
            let a = Arc::make_mut(p.assets.get_mut(&asset).unwrap());
            a.video.as_mut().unwrap().frames = 1500;
            a.audio[0].samples = 48000 * 60;
        }
        let (sid, _) = p.add_sequence(p.root, "Edit", SequenceSettings::default());
        let spec = SourceClip::from_asset(
            &p,
            item,
            SrcRange::new(SrcTime::from_seconds(1.0), SrcTime::from_seconds(5.0)),
        )
        .unwrap();
        let (next, _) = p
            .transact(|p| {
                overwrite(
                    p,
                    sid,
                    &spec,
                    SeqTime::ZERO,
                    &Patch::default(),
                    EditOptions::default(),
                )
            })
            .unwrap();
        p = next;
        let spec = SourceClip::from_asset(
            &p,
            item,
            SrcRange::new(SrcTime::from_seconds(10.0), SrcTime::from_seconds(12.0)),
        )
        .unwrap();
        let (next, _) = p
            .transact(|p| {
                overwrite(
                    p,
                    sid,
                    &spec,
                    SeqTime::from_seconds(6.0),
                    &Patch::default(),
                    EditOptions::default(),
                )
            })
            .unwrap();
        p = next;
        let spec = SourceClip::from_asset(
            &p,
            item,
            SrcRange::new(SrcTime::from_seconds(20.0), SrcTime::from_seconds(23.0)),
        )
        .unwrap();
        let (next, _) = p
            .transact(|p| {
                overwrite(
                    p,
                    sid,
                    &spec,
                    SeqTime::from_seconds(8.0),
                    &Patch::default(),
                    EditOptions::default(),
                )
            })
            .unwrap();
        p = next;
        let (next, _) = p
            .transact(|p| {
                add_transition(
                    p,
                    sid,
                    TrackRef::video(0),
                    SeqTime::from_seconds(8.0),
                    catalog::CROSS_DISSOLVE,
                    Dur::from_seconds(1.0),
                    None,
                    EditOptions::default(),
                )
            })
            .unwrap();
        p = next;
        let (next, _) = p
            .transact(|p| add_marker(p, sid, SeqTime::from_seconds(2.0)))
            .unwrap();
        (next, sid)
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::sample_project;
    use super::*;

    #[test]
    fn urls_round_trip() {
        for path in ["C:/media/a b.mov", "/home/u/clip #1.mov", "C:/Música/ñ.wav"] {
            assert_eq!(url_to_path(&path_to_url(path)), path);
        }
        assert_eq!(url_to_path("file://localhost/C%3a/x.mov"), "C:/x.mov");
    }

    #[test]
    fn export_import_round_trip() {
        let (p, sid) = sample_project();
        let text = export(&p, sid).unwrap();
        let (q, qsid) = import(&text, "back").unwrap();
        let a = p.sequence(sid).unwrap();
        let b = q.sequence(qsid).unwrap();
        let layout = |s: &Sequence, r: TrackRef| -> Vec<(i64, i64, i64)> {
            s.track(r)
                .unwrap()
                .clips
                .iter()
                .map(|c| (c.start.ticks(), c.duration.ticks(), c.source_in.ticks()))
                .collect()
        };
        assert_eq!(layout(a, TrackRef::video(0)), layout(b, TrackRef::video(0)));
        assert_eq!(layout(a, TrackRef::audio(0)), layout(b, TrackRef::audio(0)));
        assert_eq!(b.video[0].transitions.len(), 1);
        assert_eq!(
            b.video[0].transitions[0].range(),
            a.video[0].transitions[0].range()
        );
        assert_eq!(b.markers.len(), 1);
        // links survive
        let v = &b.video[0].clips[0];
        let au = &b.audio[0].clips[0];
        assert!(v.link.is_some() && v.link == au.link);
        assert_eq!(q.assets.len(), 1);
    }
}
