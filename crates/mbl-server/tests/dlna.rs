//! DLNA media server: settings, descriptions, ContentDirectory Browse and media delivery.
#![allow(clippy::unwrap_used)]

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::{TestApp, seed_playback_fixtures};
use serde_json::{Value, json};

const SAMSUNG: &str = "SEC_HHP_[TV] Samsung Q60 Series/1.0";

fn text(res: &common::Res) -> String {
    match &res.body {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

async fn settings(app: &TestApp, admin: &str, body: Value) -> common::Res {
    app.call(Method::PATCH, "/api/v1/system/settings", Some(admin), Some(body)).await
}

async fn browse(app: &TestApp, object: &str, flag: &str) -> common::Res {
    let body = format!(
        r#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:Browse xmlns:u="urn:schemas-upnp-org:service:ContentDirectory:1"><ObjectID>{object}</ObjectID><BrowseFlag>{flag}</BrowseFlag><Filter>*</Filter><StartingIndex>0</StartingIndex><RequestedCount>0</RequestedCount><SortCriteria></SortCriteria></u:Browse></s:Body></s:Envelope>"#
    );
    let req = Request::builder()
        .method(Method::POST)
        .uri("/dlna/control")
        .header(header::HOST, "192.168.1.5:8420")
        .header(header::USER_AGENT, SAMSUNG)
        .header("SOAPACTION", "\"urn:schemas-upnp-org:service:ContentDirectory:1#Browse\"")
        .header(header::CONTENT_TYPE, "text/xml; charset=\"utf-8\"")
        .body(Body::from(body))
        .unwrap();
    app.send(req).await
}

async fn get(app: &TestApp, uri: &str, range: Option<&str>) -> common::Res {
    let mut req = Request::builder().uri(uri).header(header::USER_AGENT, SAMSUNG);
    if let Some(range) = range {
        req = req.header(header::RANGE, range);
    }
    app.send(req.body(Body::empty()).unwrap()).await
}

#[tokio::test]
async fn dlna_shares_only_chosen_libraries_and_ratings() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let libs = app.call(Method::GET, "/api/v1/libraries", Some(&admin), None).await;
    let lib = libs.body.as_array().unwrap()[0]["id"].as_i64().unwrap();

    // Off by default; nothing is served.
    let current = app.call(Method::GET, "/api/v1/system/settings", Some(&admin), None).await;
    assert_eq!(current.body["dlnaEnabled"], false);
    assert_eq!(current.body["dlnaLibraryIds"], json!([]));
    assert_eq!(get(&app, "/dlna/description.xml", None).await.status, StatusCode::NOT_FOUND);

    // Turned on without libraries still shares nothing.
    let res = settings(&app, &admin, json!({ "dlnaEnabled": true, "dlnaLibraryIds": [] })).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(get(&app, "/dlna/description.xml", None).await.status, StatusCode::NOT_FOUND);

    // Unknown libraries are refused.
    let res = settings(&app, &admin, json!({ "dlnaLibraryIds": [lib, 999] })).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);

    let res = settings(
        &app,
        &admin,
        json!({ "dlnaFriendlyName": "Lounge & TV", "dlnaLibraryIds": [lib, lib] }),
    )
    .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["dlnaLibraryIds"], json!([lib]));
    assert!(res.body["dlnaMaxRating"].is_null());

    let description = get(&app, "/dlna/description.xml", None).await;
    assert_eq!(description.status, StatusCode::OK);
    let xml = text(&description);
    assert!(xml.contains("urn:schemas-upnp-org:device:MediaServer:1"), "{xml}");
    assert!(xml.contains("<friendlyName>Lounge &amp; TV</friendlyName>"), "{xml}");
    assert!(text(&get(&app, "/dlna/content_directory.xml", None).await).contains("Browse"));

    // Root lists the shared library.
    let root = browse(&app, "0", "BrowseDirectChildren").await;
    assert_eq!(root.status, StatusCode::OK, "{}", text(&root));
    let root = text(&root);
    assert!(root.contains(&format!("container id=&quot;L{lib}&quot;")), "{root}");
    assert!(root.contains("<TotalMatches>1</TotalMatches>"), "{root}");

    // The library lists the movie with a playable resource and the show as a folder.
    let listing = text(&browse(&app, &format!("L{lib}"), "BrowseDirectChildren").await);
    let movie_url = format!("http://192.168.1.5:8420/dlna/media/{}.mp4", seeded.movie_part_id);
    assert!(listing.contains(&movie_url), "{listing}");
    assert!(listing.contains("http-get:*:video/mp4:DLNA.ORG_OP=01"), "{listing}");
    assert!(
        listing.contains(&format!("container id=&quot;I{}&quot;", seeded.show_id)),
        "{listing}"
    );
    assert!(listing.contains("<TotalMatches>2</TotalMatches>"), "{listing}");

    // Show → season → episode; Samsung gets its Matroska MIME type.
    let seasons =
        text(&browse(&app, &format!("I{}", seeded.show_id), "BrowseDirectChildren").await);
    assert!(seasons.contains(&format!("I{}", seeded.season_id)), "{seasons}");
    let episodes =
        text(&browse(&app, &format!("I{}", seeded.season_id), "BrowseDirectChildren").await);
    assert!(episodes.contains(&format!("/dlna/media/{}.mkv", seeded.ep1_part_id)), "{episodes}");
    assert!(episodes.contains("video/x-mkv"), "{episodes}");
    assert!(episodes.contains("&lt;dc:title&gt;1. "), "{episodes}");

    let meta = text(&browse(&app, &format!("I{}", seeded.movie_id), "BrowseMetadata").await);
    assert!(meta.contains(&format!("parentID=&quot;L{lib}&quot;")), "{meta}");
    let missing = browse(&app, "I999999", "BrowseDirectChildren").await;
    assert_eq!(missing.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(text(&missing).contains("<errorCode>701</errorCode>"));

    // Media is served with byte ranges and DLNA headers.
    let media =
        get(&app, &format!("/dlna/media/{}.mp4", seeded.movie_part_id), Some("bytes=0-3")).await;
    assert_eq!(media.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(media.headers["transfermode.dlna.org"], "Streaming");
    assert!(media.headers["contentfeatures.dlna.org"].to_str().unwrap().contains("DLNA.ORG_OP=01"));
    assert_eq!(media.headers[header::CONTENT_TYPE], "video/mp4");
    assert_eq!(text(&media), "synt");

    // A rating ceiling hides the R-rated movie from browse and from media requests.
    sqlx::query("UPDATE media_items SET content_rating = 'R' WHERE id = ?")
        .bind(seeded.movie_id)
        .execute(&db.write)
        .await
        .unwrap();
    let res =
        settings(&app, &admin, json!({ "dlnaLibraryIds": [lib], "dlnaMaxRating": "PG-13" })).await;
    assert_eq!(res.body["dlnaMaxRating"], "PG-13");
    let listing = text(&browse(&app, &format!("L{lib}"), "BrowseDirectChildren").await);
    assert!(!listing.contains(&movie_url), "{listing}");
    let hidden = get(&app, &format!("/dlna/media/{}.mp4", seeded.movie_part_id), None).await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND);

    // Subscriptions are accepted; unknown actions are UPnP errors.
    let subscribe = Request::builder()
        .method(Method::from_bytes(b"SUBSCRIBE").unwrap())
        .uri("/dlna/events")
        .body(Body::empty())
        .unwrap();
    let res = app.send(subscribe).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.headers["sid"].to_str().unwrap().starts_with("uuid:"));
    let req = Request::builder()
        .method(Method::POST)
        .uri("/dlna/control")
        .header(header::HOST, "192.168.1.5:8420")
        .header("SOAPACTION", "\"urn:schemas-upnp-org:service:ContentDirectory:1#Search\"")
        .body(Body::from("<x/>"))
        .unwrap();
    assert!(text(&app.send(req).await).contains("<errorCode>401</errorCode>"));

    // Clients outside the local network are refused, whatever they claim in headers.
    let mut wan = Request::builder()
        .uri(format!("/dlna/media/{}.mkv", seeded.ep1_part_id))
        .header("x-forwarded-for", "192.168.1.20")
        .body(Body::empty())
        .unwrap();
    wan.extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from(([8, 8, 8, 8], 5000))));
    assert_eq!(app.send(wan).await.status, StatusCode::FORBIDDEN);

    // Turning it off stops everything again.
    settings(&app, &admin, json!({ "dlnaEnabled": false })).await;
    assert_eq!(browse(&app, "0", "BrowseDirectChildren").await.status, StatusCode::NOT_FOUND);
    let off = get(&app, &format!("/dlna/media/{}.mkv", seeded.ep1_part_id), None).await;
    assert_eq!(off.status, StatusCode::NOT_FOUND);
}
