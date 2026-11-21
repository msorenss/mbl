//! Analyse one image with the NPU (or the colour fallback) and print what it saw.
//!
//! `cargo run -p mbl-hailo --example analyze_frame -- frame.jpg`
//!
//! Used to check by eye that the description matches the picture.
#![allow(clippy::print_stdout, clippy::expect_used)]

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    assert!(!paths.is_empty(), "usage: analyze_frame <image>...");
    let engine = mbl_hailo::HailoEngine::default();
    for path in paths {
        println!("== {path}");
        let image = image::open(&path).expect("readable image");
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
}
