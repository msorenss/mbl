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

    let commit =
        std::env::var("MBL_GIT_COMMIT").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "release".into())
        });

    let build_time =
        std::env::var("MBL_BUILD_TIMESTAMP").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| {
            std::process::Command::new("date")
                .args(["-u", "+%Y-%m-%d %H:%M:%S UTC"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "unknown".into())
        });

    println!("cargo:rustc-env=MBL_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=MBL_BUILD_TIMESTAMP={build_time}");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs");
}
