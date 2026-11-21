//! Analyse one image with the NPU (or the colour fallback) and print what it saw.
//!
//! `cargo run -p mbl-hailo --example analyze_frame -- frame.jpg`
//!
//! Used to check by eye that the description matches the picture.
#![allow(clippy::print_stdout, clippy::expect_used)]

fn main() {
    let path = std::env::args().nth(1).expect("usage: analyze_frame <image>");
    let image = image::open(&path).expect("readable image");
    let engine = mbl_hailo::HailoEngine::default();
    let started = std::time::Instant::now();
    let analysis = engine.analyze_image(&image, None);
    println!("backend: {}", analysis.backend);
    println!("people:  {}", analysis.face_count);
    println!("objects: {}", analysis.objects.join(", "));
    println!("tags:    {}", analysis.tags.join(", "));
    println!("vlm:     {}", analysis.description.unwrap_or_default());
    println!("caption: {}", analysis.caption);
    println!("took:    {} ms", started.elapsed().as_millis());
}
