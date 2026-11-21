//! Transcribe an audio file with Whisper on the NPU and print the text.
//!
//! `cargo run -p mbl-hailo --example transcribe -- clip.wav [language]`
//!
//! Used to compare the text with what was said (for example with the subtitles).
#![allow(clippy::print_stdout, clippy::expect_used)]

fn main() {
    let path = std::env::args().nth(1).expect("usage: transcribe <audio> [language]");
    let language = std::env::args().nth(2);
    let output = std::process::Command::new("ffmpeg")
        .args(["-loglevel", "error", "-i", &path, "-ac", "1", "-ar", "16000", "-f", "f32le", "-"])
        .output()
        .expect("ffmpeg runs");
    let pcm: Vec<f32> =
        output.stdout.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
    let engine = mbl_hailo::HailoEngine::default();
    let started = std::time::Instant::now();
    match engine.transcribe_audio(&pcm, language.as_deref(), false) {
        Some((segments, model)) => {
            println!("model: {model}");
            for s in segments {
                println!("[{:6.1} – {:6.1}] {}", s.start_sec, s.end_sec, s.text.trim());
            }
        }
        None => println!("no transcription (no Whisper model, or the NPU refused)"),
    }
    println!(
        "took: {} ms for {:.1} s of audio",
        started.elapsed().as_millis(),
        pcm.len() as f32 / 16000.0
    );
}
