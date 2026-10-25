use std::path::Path;

fn main() {
    let dist_dir = Path::new("../../web/dist");
    let index_file = dist_dir.join("index.html");
    if !index_file.exists() {
        let _ = std::fs::create_dir_all(dist_dir);
        let _ = std::fs::write(
            index_file,
            "<!doctype html><html lang=\"en\"><head><title>MBL</title></head><body><div id=\"root\"></div></body></html>",
        );
    }
    println!("cargo:rerun-if-changed=../../web/dist");
}
