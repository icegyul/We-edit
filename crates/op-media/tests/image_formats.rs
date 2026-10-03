//! Every picture format of a folder is probed as a still and decoded (run with
//! `OP_IMG_DIR=<folder of t.png, t.svg, t.avif...> cargo test -- --ignored`).

use op_media::video::VideoDecoder;

#[test]
#[ignore]
fn picture_formats_are_stills_and_decode() {
    let dir = std::path::PathBuf::from(std::env::var("OP_IMG_DIR").unwrap());
    let mut failed = Vec::new();
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let p = e.path();
        let ext = p
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_string();
        if !op_media::probe::STILL_EXTENSIONS.contains(&ext.as_str())
            || p.to_string_lossy().contains(".out.")
        {
            continue;
        }
        let result = op_media::probe(&p)
            .map_err(|e| e.to_string())
            .and_then(|a| {
                let v = a.video.clone().ok_or("no picture")?;
                if !a.is_still() {
                    return Err(format!("not a still: {:?}", a.kind));
                }
                let mut d =
                    VideoDecoder::open(&p, None, None, v.color, true).map_err(|e| e.to_string())?;
                let f = d.frame(0).map_err(|e| e.to_string())?;
                Ok((f.width, f.height))
            });
        eprintln!("{ext}: {result:?}");
        if result.is_err() {
            failed.push(ext);
        }
    }
    assert!(failed.is_empty(), "{failed:?}");
}
