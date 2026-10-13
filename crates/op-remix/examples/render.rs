//! Renders a remix the way the timeline plays it (pieces joined by 40 ms constant-power
//! crossfades centred on each cut) and measures how audible every join is: how much the notes,
//! the loudness and the onsets change across it compared with the music's own changes from one
//! frame to the next. `cargo run -p op-remix --release --example render -- track.f32 target out.f32`

use std::sync::atomic::AtomicBool;

use op_remix::RATE;

fn read(path: &str) -> Vec<f32> {
    std::fs::read(path).expect("readable").chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a .f32 file");
    let target: f64 = args.next().and_then(|t| t.parse().ok()).expect("target seconds");
    let out_path = args.next();
    let pcm = read(&path);
    let a = op_remix::analyze(pcm.clone(), &mut |_| {}, &AtomicBool::new(false)).expect("analysis");
    let loops = a.loops(4.0, a.seconds * 0.9, 300);
    let p = op_remix::plan(&loops, &op_remix::PlanOptions { range: (0.0, a.seconds), target, min_loop: 4.0 });
    println!("{} loops; target {target:.1} s -> {:.2} s, {} move(s), {} pieces", loops.len(), p.duration, p.moves.len(), p.segments.len());
    for m in &p.moves {
        println!("  loop {:.3}-{:.3} (score {:.3}) x{}", m.at.start, m.at.end, m.at.score, m.times);
    }
    // render
    let half = (0.02 * RATE as f64) as usize;
    let at = |t: f64| (t * RATE as f64).round() as usize;
    let mut out: Vec<f32> = Vec::new();
    let mut cuts: Vec<usize> = Vec::new();
    for (k, (s, e)) in p.segments.iter().enumerate() {
        let (s, e) = (at(*s), at(*e));
        if k == 0 {
            out.extend_from_slice(&pcm[s..e]);
            continue;
        }
        // the previous piece goes on for `half` samples past its end, this one starts `half` earlier
        let prev_end = at(p.segments[k - 1].1);
        let cut = out.len();
        cuts.push(cut);
        let tail_from = out.len() - half;
        for i in 0..2 * half {
            let x = i as f32 / (2 * half) as f32;
            let (fo, fi) = ((x * std::f32::consts::FRAC_PI_2).cos(), (x * std::f32::consts::FRAC_PI_2).sin());
            let old = pcm.get(prev_end - half + i).copied().unwrap_or(0.0);
            let new = pcm.get(s.saturating_sub(half) + i).copied().unwrap_or(0.0);
            let v = old * fo + new * fi;
            if tail_from + i < out.len() {
                out[tail_from + i] = v;
            } else {
                out.push(v);
            }
        }
        out.extend_from_slice(&pcm[s + half..e]);
    }
    if let Some(o) = out_path {
        std::fs::write(o, out.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    }
    // how audible is each join? compare with the music's own frame-to-frame change
    let r = op_remix::analyze(out.clone(), &mut |_| {}, &AtomicBool::new(false)).expect("analysis of the result");
    let f = &r.features;
    let cos = |x: &[f32; 12], y: &[f32; 12]| {
        let d: f32 = x.iter().zip(y).map(|(a, b)| a * b).sum();
        d / ((x.iter().map(|v| v * v).sum::<f32>().sqrt() * y.iter().map(|v| v * v).sum::<f32>().sqrt()).max(1e-9))
    };
    let mut adj: Vec<f32> = (1..f.frames).map(|i| cos(&f.chroma[i - 1], &f.chroma[i])).collect();
    adj.sort_by(f32::total_cmp);
    let median_adj = adj[adj.len() / 2];
    let p10 = adj[adj.len() / 10];
    let mut loud_adj: Vec<f32> = (1..f.frames).map(|i| (f.loud[i] - f.loud[i - 1]).abs()).collect();
    loud_adj.sort_by(f32::total_cmp);
    let loud_p90 = loud_adj[loud_adj.len() * 9 / 10];
    println!("typical chroma similarity of neighbouring frames: median {median_adj:.3}, 10th percentile {p10:.3}; loudness change of neighbours: 90th percentile {loud_p90:.2} dB");
    // a join is judged against the music's own transitions at the same two places: from just
    // before the end of one piece to just after the start of the next is as natural as going on
    // past the end (or as arriving at the start from before it) when it changes the notes and
    // the loudness about as much
    let o = &a.features;
    for (c, k) in cuts.iter().zip(1..) {
        let (prev_end, start) = (p.segments[k - 1].1, p.segments[k].0);
        let (ef, sf) = ((prev_end * RATE as f64) as usize / op_remix::HOP, (start * RATE as f64) as usize / op_remix::HOP);
        let (e_b, e_n) = (ef.saturating_sub(3), (ef + 3).min(o.frames - 1));
        let (s_b, s_n) = (sf.saturating_sub(3), (sf + 3).min(o.frames - 1));
        let seam = (cos(&o.chroma[e_b], &o.chroma[s_n]), (o.loud[s_n] - o.loud[e_b]).abs());
        let nat_e = (cos(&o.chroma[e_b], &o.chroma[e_n]), (o.loud[e_n] - o.loud[e_b]).abs());
        let nat_s = (cos(&o.chroma[s_b], &o.chroma[s_n]), (o.loud[s_n] - o.loud[s_b]).abs());
        let verdict = if seam.0 + 0.1 >= nat_e.0.min(nat_s.0) && seam.1 <= nat_e.1.max(nat_s.1) + 3.0 { "natural" } else { "AUDIBLE?" };
        println!("  join at {:6.2} s: notes {:.2} (naturally {:.2}/{:.2}), loudness change {:.1} dB (naturally {:.1}/{:.1}) -> {verdict}", *c as f64 / RATE as f64, seam.0, nat_e.0, nat_s.0, seam.1, nat_e.1, nat_s.1);
    }
}
