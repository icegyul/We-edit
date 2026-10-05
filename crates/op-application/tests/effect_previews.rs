//! Every video effect and transition renders a preview through the real renderer; the frames of
//! animated ones change over time (set OP_PREVIEW_DIR to also write them as raw RGBA files).

use op_core::catalog::{self, EffectKind};
use op_render::{Gpu, Renderer};

#[test]
fn every_video_effect_and_transition_has_a_preview() {
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) if std::env::var("OPENPREMIER_REQUIRE_GPU").is_ok_and(|v| v == "1") => {
            panic!("no GPU adapter: {e}")
        }
        Err(e) => {
            eprintln!("no GPU adapter, skipped: {e}");
            return;
        }
    };
    let mut r = Renderer::new(gpu);
    let dump = std::env::var("OP_PREVIEW_DIR")
        .ok()
        .map(std::path::PathBuf::from);
    if let Some(d) = &dump {
        std::fs::create_dir_all(d).unwrap();
    }
    let mut moving = 0;
    let mut total = 0;
    let mut still = Vec::new();
    for kind in [EffectKind::VideoEffect, EffectKind::VideoTransition] {
        for def in catalog::by_kind(kind) {
            let p = op_application::effect_preview::render(&mut r, def)
                .unwrap_or_else(|| panic!("{}: no preview", def.name));
            assert_eq!(p.frames.len(), op_application::effect_preview::FRAMES);
            assert!(
                p.frames
                    .iter()
                    .all(|f| f.len() == (p.width * p.height * 4) as usize)
            );
            total += 1;
            if p.frames.first() != p.frames.get(p.frames.len() / 2) {
                moving += 1;
            } else {
                still.push(def.name);
            }
            if let Some(d) = &dump {
                let name = def.id.replace('.', "_");
                let mut all = Vec::new();
                for f in &p.frames {
                    all.extend_from_slice(f);
                }
                std::fs::write(d.join(format!("{name}.rgba")), all).unwrap();
            }
        }
    }
    eprintln!("{moving} of {total} previews move; still: {still:?}");
    // transitions always move; most effects have an amount to sweep
    assert!(
        moving as f64 > total as f64 * 0.6,
        "{moving} of {total}: {still:?}"
    );
    assert!(
        op_application::effect_preview::render(
            &mut r,
            catalog::by_kind(EffectKind::AudioEffect).next().unwrap()
        )
        .is_none()
    );
}
