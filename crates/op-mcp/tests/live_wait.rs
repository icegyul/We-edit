//! An assistant that is told a transcription is still running asks again at once, so it asked
//! `get_sentences` 75 times for one question (each one a model turn: about 2 minutes and half a
//! dollar). The call now waits, in the assistant's own process, until the job is done.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use op_mcp::live::{Remote, Server};
use op_mcp::{Content, Target};
use serde_json::json;

/// A stand-in for the open window: answers `get_project` (the connection check) and says
/// `state: running` to `get_sentences` the first `running` times, then `state: ready`.
fn window(running: usize) -> (tempfile::TempDir, Arc<AtomicUsize>, Arc<AtomicBool>) {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(dir.path(), || {}).unwrap();
    let asked = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (a, s) = (asked.clone(), stop.clone());
    std::thread::spawn(move || {
        while !s.load(Ordering::Relaxed) {
            for req in server.pending() {
                let answer = match req.name.as_str() {
                    "get_sentences" => {
                        let n = a.fetch_add(1, Ordering::Relaxed);
                        if n < running { "state: running (Transcribing, 50%)\ncall get_sentences again (it waits for you)" } else { "state: ready\nlanguage: ko  sentences: 1" }
                    }
                    _ => "{}",
                };
                req.answer(Ok(vec![Content::Text(answer.into())]));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    (dir, asked, stop)
}

fn text(out: &[Content]) -> String {
    match &out[0] {
        Content::Block(b) => b["text"].as_str().unwrap_or("").to_string(),
        Content::Text(t) => t.clone(),
        _ => String::new(),
    }
}

#[test]
fn get_sentences_waits_until_the_transcription_is_done() {
    let (dir, asked, stop) = window(2);
    let mut remote = Remote::connect(dir.path()).expect("the window answers");
    let began = Instant::now();
    let out = remote.call("get_sentences", &json!({})).unwrap();
    assert!(text(&out).starts_with("state: ready"), "{}", text(&out));
    assert_eq!(asked.load(Ordering::Relaxed), 3, "two running answers were waited out, one call for the assistant");
    assert!(began.elapsed() >= Duration::from_millis(1800), "it did not wait");
    stop.store(true, Ordering::Relaxed);
}

#[test]
fn wait_seconds_zero_answers_at_once() {
    let (dir, asked, stop) = window(5);
    let mut remote = Remote::connect(dir.path()).unwrap();
    let began = Instant::now();
    let out = remote.call("get_sentences", &json!({"wait_seconds": 0})).unwrap();
    assert!(text(&out).starts_with("state: running"));
    assert_eq!(asked.load(Ordering::Relaxed), 1);
    assert!(began.elapsed() < Duration::from_millis(900));
    stop.store(true, Ordering::Relaxed);
}

#[test]
fn the_wait_ends_with_the_last_progress_when_the_time_is_up() {
    let (dir, asked, stop) = window(1000);
    let mut remote = Remote::connect(dir.path()).unwrap();
    let out = remote.call("get_sentences", &json!({"wait_seconds": 2})).unwrap();
    assert!(text(&out).starts_with("state: running"), "still running: the progress comes back");
    // about one question per second of waiting, not one per few milliseconds
    assert!((2..=3).contains(&asked.load(Ordering::Relaxed)), "{} questions", asked.load(Ordering::Relaxed));
    stop.store(true, Ordering::Relaxed);
}

#[test]
fn other_tools_are_not_held() {
    let (dir, _asked, stop) = window(0);
    let mut remote = Remote::connect(dir.path()).unwrap();
    let began = Instant::now();
    remote.call("get_project", &json!({})).unwrap();
    assert!(began.elapsed() < Duration::from_millis(500));
    stop.store(true, Ordering::Relaxed);
}
