//! UPnP/DLNA media server protocol: device and service descriptions, SOAP envelopes,
//! DIDL-Lite browse results and SSDP discovery messages. Pure functions; the server wires them
//! to HTTP and UDP.

use std::fmt::Write as _;

pub const MEDIA_SERVER: &str = "urn:schemas-upnp-org:device:MediaServer:1";
pub const CONTENT_DIRECTORY: &str = "urn:schemas-upnp-org:service:ContentDirectory:1";
pub const CONNECTION_MANAGER: &str = "urn:schemas-upnp-org:service:ConnectionManager:1";
pub const SSDP_ADDR: &str = "239.255.255.250:1900";
/// Seconds a discovery announcement stays valid.
pub const MAX_AGE_SECS: u64 = 1800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseFlag {
    BrowseMetadata,
    BrowseDirectChildren,
}

impl BrowseFlag {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "BrowseMetadata" => Some(Self::BrowseMetadata),
            "BrowseDirectChildren" => Some(Self::BrowseDirectChildren),
            _ => None,
        }
    }
}

/// One entry in a DIDL-Lite result: a container (`res` empty) or a playable item.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DidlItem {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    /// UPnP class, e.g. `object.container.storageFolder` or `object.item.videoItem.movie`.
    pub class: String,
    pub child_count: Option<u32>,
    pub res: Option<DidlRes>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DidlRes {
    pub url: String,
    pub protocol_info: String,
    pub size_bytes: Option<u64>,
    pub duration_ms: Option<u64>,
}

impl DidlItem {
    pub fn is_container(&self) -> bool {
        self.class.starts_with("object.container")
    }
}

/// Root device description served at the SSDP `LOCATION`.
pub fn device_description(friendly_name: &str, udn: &str, version: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0" xmlns:dlna="urn:schemas-dlna-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>{MEDIA_SERVER}</deviceType>
    <dlna:X_DLNADOC>DMS-1.50</dlna:X_DLNADOC>
    <friendlyName>{name}</friendlyName>
    <manufacturer>Media Box Lounge</manufacturer>
    <manufacturerURL>https://github.com/msorenss/mbl</manufacturerURL>
    <modelDescription>Media Box Lounge media server</modelDescription>
    <modelName>MBL</modelName>
    <modelNumber>{version}</modelNumber>
    <UDN>uuid:{udn}</UDN>
    <serviceList>
      <service>
        <serviceType>{CONTENT_DIRECTORY}</serviceType>
        <serviceId>urn:upnp-org:serviceId:ContentDirectory</serviceId>
        <SCPDURL>/dlna/content_directory.xml</SCPDURL>
        <controlURL>/dlna/control</controlURL>
        <eventSubURL>/dlna/events</eventSubURL>
      </service>
      <service>
        <serviceType>{CONNECTION_MANAGER}</serviceType>
        <serviceId>urn:upnp-org:serviceId:ConnectionManager</serviceId>
        <SCPDURL>/dlna/connection_manager.xml</SCPDURL>
        <controlURL>/dlna/connection_control</controlURL>
        <eventSubURL>/dlna/connection_events</eventSubURL>
      </service>
    </serviceList>
  </device>
</root>"#,
        name = escape_xml(friendly_name),
        version = escape_xml(version),
        udn = escape_xml(udn),
    )
}

/// Action name and its `(argument, direction, state variable)` list.
type Action<'a> = (&'a str, &'a [(&'a str, &'a str, &'a str)]);

fn scpd(actions: &[Action<'_>], variables: &[(&str, &str, bool)]) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<scpd xmlns="urn:schemas-upnp-org:service-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <actionList>"#,
    );
    for (name, args) in actions {
        let _ = write!(out, "<action><name>{name}</name><argumentList>");
        for (arg, direction, variable) in *args {
            let _ = write!(
                out,
                "<argument><name>{arg}</name><direction>{direction}</direction>\
                 <relatedStateVariable>{variable}</relatedStateVariable></argument>"
            );
        }
        out.push_str("</argumentList></action>");
    }
    out.push_str("</actionList><serviceStateTable>");
    for (name, data_type, events) in variables {
        let events = if *events { "yes" } else { "no" };
        let _ = write!(
            out,
            "<stateVariable sendEvents=\"{events}\"><name>{name}</name><dataType>{data_type}</dataType></stateVariable>"
        );
    }
    out.push_str("</serviceStateTable></scpd>");
    out
}

/// ContentDirectory service description (browse only, no search).
pub fn content_directory_scpd() -> String {
    scpd(
        &[
            (
                "Browse",
                &[
                    ("ObjectID", "in", "A_ARG_TYPE_ObjectID"),
                    ("BrowseFlag", "in", "A_ARG_TYPE_BrowseFlag"),
                    ("Filter", "in", "A_ARG_TYPE_Filter"),
                    ("StartingIndex", "in", "A_ARG_TYPE_Index"),
                    ("RequestedCount", "in", "A_ARG_TYPE_Count"),
                    ("SortCriteria", "in", "A_ARG_TYPE_SortCriteria"),
                    ("Result", "out", "A_ARG_TYPE_Result"),
                    ("NumberReturned", "out", "A_ARG_TYPE_Count"),
                    ("TotalMatches", "out", "A_ARG_TYPE_Count"),
                    ("UpdateID", "out", "A_ARG_TYPE_UpdateID"),
                ],
            ),
            ("GetSearchCapabilities", &[("SearchCaps", "out", "SearchCapabilities")]),
            ("GetSortCapabilities", &[("SortCaps", "out", "SortCapabilities")]),
            ("GetSystemUpdateID", &[("Id", "out", "SystemUpdateID")]),
        ],
        &[
            ("A_ARG_TYPE_ObjectID", "string", false),
            ("A_ARG_TYPE_Result", "string", false),
            ("A_ARG_TYPE_BrowseFlag", "string", false),
            ("A_ARG_TYPE_Filter", "string", false),
            ("A_ARG_TYPE_SortCriteria", "string", false),
            ("A_ARG_TYPE_Index", "ui4", false),
            ("A_ARG_TYPE_Count", "ui4", false),
            ("A_ARG_TYPE_UpdateID", "ui4", false),
            ("SearchCapabilities", "string", false),
            ("SortCapabilities", "string", false),
            ("SystemUpdateID", "ui4", true),
        ],
    )
}

/// ConnectionManager service description.
pub fn connection_manager_scpd() -> String {
    scpd(
        &[
            (
                "GetProtocolInfo",
                &[("Source", "out", "SourceProtocolInfo"), ("Sink", "out", "SinkProtocolInfo")],
            ),
            ("GetCurrentConnectionIDs", &[("ConnectionIDs", "out", "CurrentConnectionIDs")]),
        ],
        &[
            ("SourceProtocolInfo", "string", true),
            ("SinkProtocolInfo", "string", true),
            ("CurrentConnectionIDs", "string", true),
        ],
    )
}

/// DIDL-Lite document for a browse result.
pub fn didl_lite(items: &[DidlItem]) -> String {
    let mut out = String::from(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns:dlna="urn:schemas-dlna-org:metadata-1-0/">"#,
    );
    for it in items {
        let (id, parent, title, class) = (
            escape_xml(&it.id),
            escape_xml(&it.parent_id),
            escape_xml(&it.title),
            escape_xml(&it.class),
        );
        if it.is_container() {
            let count = it.child_count.map(|c| format!(" childCount=\"{c}\"")).unwrap_or_default();
            let _ = write!(
                out,
                r#"<container id="{id}" parentID="{parent}" restricted="1" searchable="0"{count}><dc:title>{title}</dc:title><upnp:class>{class}</upnp:class></container>"#
            );
        } else {
            let res = it.res.as_ref().map(res_xml).unwrap_or_default();
            let _ = write!(
                out,
                r#"<item id="{id}" parentID="{parent}" restricted="1"><dc:title>{title}</dc:title><upnp:class>{class}</upnp:class>{res}</item>"#
            );
        }
    }
    out.push_str("</DIDL-Lite>");
    out
}

fn res_xml(res: &DidlRes) -> String {
    let mut attrs = format!(" protocolInfo=\"{}\"", escape_xml(&res.protocol_info));
    if let Some(size) = res.size_bytes {
        let _ = write!(attrs, " size=\"{size}\"");
    }
    if let Some(ms) = res.duration_ms {
        let _ = write!(attrs, " duration=\"{}\"", duration(ms));
    }
    format!("<res{attrs}>{}</res>", escape_xml(&res.url))
}

/// UPnP duration `H:MM:SS.mmm`.
pub fn duration(ms: u64) -> String {
    let secs = ms / 1000;
    format!("{}:{:02}:{:02}.{:03}", secs / 3600, (secs / 60) % 60, secs % 60, ms % 1000)
}

/// `protocolInfo` for a file served as is: streaming with byte-range seeking.
pub fn protocol_info(mime: &str) -> String {
    format!("http-get:*:{mime}:{}", content_features(mime))
}

/// `contentFeatures.dlna.org` value: byte seek (OP=01), not converted, streaming or interactive.
pub fn content_features(mime: &str) -> String {
    let flags = if mime.starts_with("image/") {
        "00D00000000000000000000000000000"
    } else {
        "01700000000000000000000000000000"
    };
    format!("DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS={flags}")
}

/// `transferMode.dlna.org` value for a MIME type.
pub fn transfer_mode(mime: &str) -> &'static str {
    if mime.starts_with("image/") { "Interactive" } else { "Streaming" }
}

/// Wrap an action response in a SOAP envelope. `args` are already-escaped XML text values.
pub fn soap_response(service: &str, action: &str, args: &[(&str, String)]) -> String {
    let mut body = String::new();
    for (name, value) in args {
        let _ = write!(body, "<{name}>{value}</{name}>");
    }
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body><u:{action}Response xmlns:u="{service}">{body}</u:{action}Response></s:Body></s:Envelope>"#
    )
}

/// UPnP error response body (sent with HTTP 500).
pub fn soap_fault(code: u16, description: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body><s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail><UPnPError xmlns="urn:schemas-upnp-org:control-1-0"><errorCode>{code}</errorCode><errorDescription>{}</errorDescription></UPnPError></detail></s:Fault></s:Body></s:Envelope>"#,
        escape_xml(description)
    )
}

/// Action name from a `SOAPACTION` header such as `"urn:...:ContentDirectory:1#Browse"`.
pub fn soap_action(header: &str) -> Option<&str> {
    let action = header.trim().trim_matches('"').rsplit_once('#')?.1;
    (!action.is_empty()).then_some(action)
}

/// Text of the first `<name>` element (any namespace prefix) in a SOAP body.
pub fn soap_arg(body: &str, name: &str) -> Option<String> {
    let mut rest = body;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        let end = rest.find('>')?;
        let tag = &rest[..end];
        let tag_name = tag.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        let local = tag_name.rsplit_once(':').map_or(tag_name, |(_, l)| l);
        if local == name && !tag.starts_with('/') {
            if tag.ends_with('/') {
                return Some(String::new());
            }
            let content = &rest[end + 1..];
            let close = content.find("</")?;
            return Some(unescape_xml(content[..close].trim()));
        }
        rest = &rest[end + 1..];
    }
    None
}

pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && !matches!(c, '\n' | '\r' | '\t') => {}
            c => out.push(c),
        }
    }
    out
}

fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Search targets this server answers to, in announcement order.
pub fn notification_types(udn: &str) -> Vec<String> {
    vec![
        "upnp:rootdevice".into(),
        format!("uuid:{udn}"),
        MEDIA_SERVER.into(),
        CONTENT_DIRECTORY.into(),
        CONNECTION_MANAGER.into(),
    ]
}

fn usn(udn: &str, nt: &str) -> String {
    if nt.starts_with("uuid:") { nt.to_owned() } else { format!("uuid:{udn}::{nt}") }
}

/// Search targets to answer for an `ST` header, or empty when it is not ours.
pub fn matching_targets(st: &str, udn: &str) -> Vec<String> {
    let st = st.trim();
    let ours = notification_types(udn);
    if st == "ssdp:all" {
        return ours;
    }
    ours.into_iter().filter(|nt| nt.eq_ignore_ascii_case(st)).collect()
}

/// A parsed `M-SEARCH` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MSearch {
    pub st: String,
    /// Maximum response delay in seconds (capped at 5).
    pub mx: u64,
}

/// Parse an SSDP datagram; only `M-SEARCH` with `MAN: "ssdp:discover"` is accepted.
pub fn parse_msearch(datagram: &[u8]) -> Option<MSearch> {
    let text = std::str::from_utf8(datagram).ok()?;
    let mut lines = text.split("\r\n").flat_map(|l| l.split('\n'));
    if !lines.next()?.trim().eq_ignore_ascii_case("M-SEARCH * HTTP/1.1") {
        return None;
    }
    let (mut st, mut man, mut mx) = (None, false, 1);
    for line in lines {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match key.trim().to_ascii_uppercase().as_str() {
            "ST" => st = Some(value.to_owned()),
            "MAN" => man = value.trim_matches('"') == "ssdp:discover",
            "MX" => mx = value.parse().unwrap_or(1),
            _ => {}
        }
    }
    (man).then_some(())?;
    Some(MSearch { st: st?, mx: mx.min(5) })
}

/// Unicast reply to an `M-SEARCH` for one search target.
pub fn search_response(st: &str, udn: &str, location: &str, date: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age={MAX_AGE_SECS}\r\nDATE: {date}\r\nEXT:\r\nLOCATION: {location}\r\nSERVER: {server}\r\nST: {st}\r\nUSN: {usn}\r\nContent-Length: 0\r\n\r\n",
        server = server_header(),
        usn = usn(udn, st),
    )
}

/// Multicast `NOTIFY` for one notification type; `alive` false sends `ssdp:byebye`.
pub fn notify(nt: &str, udn: &str, location: &str, alive: bool) -> String {
    let usn = usn(udn, nt);
    if alive {
        format!(
            "NOTIFY * HTTP/1.1\r\nHOST: {SSDP_ADDR}\r\nCACHE-CONTROL: max-age={MAX_AGE_SECS}\r\nLOCATION: {location}\r\nNT: {nt}\r\nNTS: ssdp:alive\r\nSERVER: {}\r\nUSN: {usn}\r\n\r\n",
            server_header()
        )
    } else {
        format!(
            "NOTIFY * HTTP/1.1\r\nHOST: {SSDP_ADDR}\r\nNT: {nt}\r\nNTS: ssdp:byebye\r\nUSN: {usn}\r\n\r\n"
        )
    }
}

fn server_header() -> String {
    format!("{}/1.0 UPnP/1.0 DLNADOC/1.50 MBL/{}", std::env::consts::OS, env!("CARGO_PKG_VERSION"))
}

/// Renderer family, from `User-Agent` and `X-AV-Client-Info`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DlnaRendererProfile {
    Generic,
    SamsungTv,
    LgWebOs,
}

impl DlnaRendererProfile {
    pub fn from_user_agent(ua: &str) -> Self {
        let l = ua.to_lowercase();
        if l.contains("samsung") || l.contains("tizen") || l.contains("sec_hhp") {
            Self::SamsungTv
        } else if l.contains("webos") || l.contains("web0s") || l.contains("lge") {
            Self::LgWebOs
        } else {
            Self::Generic
        }
    }

    /// MIME type announced and served to this renderer for a file's own type.
    pub fn mime<'a>(&self, mime: &'a str) -> &'a str {
        match (self, mime) {
            (Self::SamsungTv, "video/x-matroska") => "video/x-mkv",
            (_, "video/mp2t") => "video/vnd.dlna.mpeg-tts",
            _ => mime,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn didl_escapes_and_formats_items() {
        let items = vec![
            DidlItem {
                id: "L1".into(),
                parent_id: "0".into(),
                title: "Movies & TV".into(),
                class: "object.container.storageFolder".into(),
                child_count: Some(3),
                res: None,
            },
            DidlItem {
                id: "I42".into(),
                parent_id: "L1".into(),
                title: "Inception <2010>".into(),
                class: "object.item.videoItem.movie".into(),
                child_count: None,
                res: Some(DidlRes {
                    url: "http://192.168.1.100:8420/dlna/media/42.mkv".into(),
                    protocol_info: protocol_info("video/x-matroska"),
                    size_bytes: Some(1_500_000_000),
                    duration_ms: Some(8_880_250),
                }),
            },
        ];
        let didl = didl_lite(&items);
        assert!(didl.contains(r#"<container id="L1" parentID="0" restricted="1" searchable="0" childCount="3"><dc:title>Movies &amp; TV</dc:title>"#));
        assert!(didl.contains("Inception &lt;2010&gt;"));
        assert!(didl.contains(r#"size="1500000000" duration="2:28:00.250""#));
        assert!(didl.contains("http-get:*:video/x-matroska:DLNA.ORG_OP=01"));
        assert!(didl.contains(">http://192.168.1.100:8420/dlna/media/42.mkv</res>"));
    }

    #[test]
    fn soap_arguments_and_actions() {
        let body = r#"<?xml version="1.0"?><s:Envelope xmlns:s="x"><s:Body><u:Browse xmlns:u="urn:schemas-upnp-org:service:ContentDirectory:1"><ObjectID>I7</ObjectID><BrowseFlag>BrowseDirectChildren</BrowseFlag><Filter>*</Filter><StartingIndex>10</StartingIndex><RequestedCount>20</RequestedCount><SortCriteria/></u:Browse></s:Body></s:Envelope>"#;
        assert_eq!(soap_arg(body, "ObjectID").as_deref(), Some("I7"));
        assert_eq!(soap_arg(body, "StartingIndex").as_deref(), Some("10"));
        assert_eq!(soap_arg(body, "SortCriteria").as_deref(), Some(""));
        assert_eq!(soap_arg(body, "Missing"), None);
        assert_eq!(
            soap_arg("<a:Filter>dc:title&amp;x</a:Filter>", "Filter").as_deref(),
            Some("dc:title&x")
        );
        assert_eq!(
            soap_action("\"urn:schemas-upnp-org:service:ContentDirectory:1#Browse\""),
            Some("Browse")
        );
        assert_eq!(soap_action("nothing"), None);
        assert_eq!(BrowseFlag::parse("BrowseMetadata"), Some(BrowseFlag::BrowseMetadata));
        assert_eq!(BrowseFlag::parse("Other"), None);
        let response =
            soap_response(CONTENT_DIRECTORY, "Browse", &[("NumberReturned", "1".into())]);
        assert!(response.contains("<u:BrowseResponse xmlns:u=\"urn:schemas-upnp-org:service:ContentDirectory:1\"><NumberReturned>1</NumberReturned>"));
        assert!(soap_fault(701, "No such object").contains("<errorCode>701</errorCode>"));
    }

    #[test]
    fn ssdp_search_and_notify() {
        let udn = "1234";
        let search = b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 9\r\nST: urn:schemas-upnp-org:device:MediaServer:1\r\n\r\n";
        let parsed = parse_msearch(search).unwrap();
        assert_eq!(parsed.mx, 5);
        assert_eq!(matching_targets(&parsed.st, udn), vec![MEDIA_SERVER.to_string()]);
        assert_eq!(matching_targets("ssdp:all", udn).len(), 5);
        assert_eq!(matching_targets("uuid:1234", udn), vec!["uuid:1234".to_string()]);
        assert!(matching_targets("urn:schemas-upnp-org:device:MediaRenderer:1", udn).is_empty());
        assert!(parse_msearch(b"M-SEARCH * HTTP/1.1\r\nST: ssdp:all\r\n\r\n").is_none());
        assert!(parse_msearch(b"NOTIFY * HTTP/1.1\r\n\r\n").is_none());

        let reply =
            search_response(MEDIA_SERVER, udn, "http://10.0.0.2:8420/dlna/description.xml", "now");
        assert!(reply.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(reply.contains("USN: uuid:1234::urn:schemas-upnp-org:device:MediaServer:1\r\n"));
        assert!(reply.contains("LOCATION: http://10.0.0.2:8420/dlna/description.xml\r\n"));
        let alive = notify("uuid:1234", udn, "http://x/d.xml", true);
        assert!(alive.contains("NTS: ssdp:alive") && alive.contains("USN: uuid:1234\r\n"));
        assert!(notify("upnp:rootdevice", udn, "http://x", false).contains("NTS: ssdp:byebye"));
    }

    #[test]
    fn descriptions_and_profiles() {
        let xml = device_description("Lounge & Co", "abc", "1.3.8");
        assert!(
            xml.contains(MEDIA_SERVER)
                && xml.contains("Lounge &amp; Co")
                && xml.contains("uuid:abc")
        );
        assert!(content_directory_scpd().contains("<name>Browse</name>"));
        assert!(connection_manager_scpd().contains("<name>GetProtocolInfo</name>"));
        assert_eq!(duration(3_723_004), "1:02:03.004");
        assert_eq!(
            DlnaRendererProfile::from_user_agent("SEC_HHP_[TV] Samsung Q60/1.0"),
            DlnaRendererProfile::SamsungTv
        );
        assert_eq!(
            DlnaRendererProfile::from_user_agent("Linux/4.4 UPnP/1.0 webOS/1.5"),
            DlnaRendererProfile::LgWebOs
        );
        assert_eq!(DlnaRendererProfile::SamsungTv.mime("video/x-matroska"), "video/x-mkv");
        assert_eq!(DlnaRendererProfile::LgWebOs.mime("video/x-matroska"), "video/x-matroska");
        assert!(content_features("image/jpeg").contains("FLAGS=00D"));
        assert_eq!(transfer_mode("audio/flac"), "Streaming");
    }
}
