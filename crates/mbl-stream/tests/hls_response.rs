#![allow(clippy::unwrap_used)]

use axum::http::{StatusCode, header};
use http_body_util::BodyExt;
use mbl_stream::hls::{MIME_M4S, serve_segment_file};

#[tokio::test]
async fn segment_streams_complete_file_with_length_and_no_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("0.m4s");
    let bytes = vec![42; 300 * 1024];
    tokio::fs::write(&path, &bytes).await.unwrap();

    let response = serve_segment_file(&path, MIME_M4S).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], MIME_M4S);
    assert_eq!(response.headers()[header::CONTENT_LENGTH], bytes.len().to_string());
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.into_body().collect().await.unwrap().to_bytes(), bytes);
}

#[test]
fn avc_metadata_comes_from_sample_entry_and_rejects_truncation() {
    fn atom(kind: &[u8; 4], body: Vec<u8>) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend(kind);
        out.extend(body);
        out
    }
    let mut entry = vec![0; 78];
    entry[24..26].copy_from_slice(&1280u16.to_be_bytes());
    entry[26..28].copy_from_slice(&640u16.to_be_bytes());
    entry.extend(atom(b"avcC", vec![1, 100, 0, 31]));
    let mut stsd = vec![0; 8];
    stsd.extend(atom(b"avc1", entry));
    let init = atom(
        b"moov",
        atom(b"trak", atom(b"mdia", atom(b"minf", atom(b"stbl", atom(b"stsd", stsd))))),
    );
    assert_eq!(mbl_stream::hls::avc_init_metadata(&init), Some((1280, 640, "avc1.64001f".into())));
    for n in 0..init.len() {
        assert!(mbl_stream::hls::avc_init_metadata(&init[..n]).is_none());
    }
}
