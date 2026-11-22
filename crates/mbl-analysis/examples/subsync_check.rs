//! Check subtitle matching against a real film: a cached speech map and WebVTT cues, shifted
//! and stretched on purpose. `cargo run -p mbl-analysis --example subsync_check -- 1.speech 1_2.vtt`
#![allow(clippy::print_stdout, clippy::expect_used)]

use mbl_analysis::subsync::{align_to_speech, unpack_speech};

fn time(t: &str) -> i64 {
    let (clock, ms) = t.trim().split_once('.').expect("time");
    let secs = clock.split(':').fold(0i64, |acc, p| acc * 60 + p.parse::<i64>().expect("number"));
    secs * 1000 + ms[..3].parse::<i64>().expect("ms")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let speech = unpack_speech(&std::fs::read(&args[1]).expect("speech map")).expect("valid map");
    let vtt = std::fs::read_to_string(&args[2]).expect("vtt");
    let cues: Vec<(i64, i64)> = vtt
        .lines()
        .filter_map(|l| l.split_once("-->"))
        .map(|(a, b)| (time(a), time(b.split_whitespace().next().expect("end"))))
        .collect();
    println!("{} cues, {} s of speech map", cues.len(), speech.len() / 50);
    for (label, scale, offset) in [
        ("as is", 1.0, 0),
        ("3 s late", 1.0, 3_000),
        ("1.5 s early", 1.0, -1_500),
        ("12 s late", 1.0, 12_000),
        ("timed for 25 fps", 23.976 / 25.0, 0),
        ("timed for 23.976 on 25", 25.0 / 23.976, 800),
    ] {
        let moved: Vec<(i64, i64)> = cues
            .iter()
            .map(|(a, b)| ((*a as f64 * scale) as i64 + offset, (*b as f64 * scale) as i64 + offset))
            .collect();
        let t = std::time::Instant::now();
        let found = align_to_speech(&speech, &moved, 30_000).expect("alignment");
        println!(
            "{label:>24}: offset {:>6} ms scale {:.4} score {:.2} → {:.2} apply={} ({} ms)",
            found.offset_ms,
            found.scale,
            found.unshifted_score,
            found.score,
            found.is_improvement(),
            t.elapsed().as_millis()
        );
    }
}
