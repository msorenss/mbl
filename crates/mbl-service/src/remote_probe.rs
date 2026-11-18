//! Network probes for the remote-access diagnostic: public address, router WAN address
//! (UPnP IGD), DNS of the public host names and a TCP check of the forwarded port.
//!
//! Every probe is bounded by a short timeout and reports `None`/`false` instead of failing,
//! so the diagnostic always returns a report.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use tokio::net::{TcpStream, UdpSocket};

/// Echo service used to learn the public address. Only called when an admin runs the test.
pub const PUBLIC_IP_ECHO: &str = "https://api.ipify.org";

const SSDP_ADDR: &str = "239.255.255.250:1900";
const MAX_XML_BYTES: usize = 256 * 1024;
const WAN_SERVICES: [&str; 3] = [
    "urn:schemas-upnp-org:service:WANIPConnection:2",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
    "urn:schemas-upnp-org:service:WANPPPConnection:1",
];

/// Public address as seen from the internet.
pub async fn public_ip(timeout: Duration) -> Option<IpAddr> {
    let client = reqwest::Client::builder().timeout(timeout).build().ok()?;
    let text =
        client.get(PUBLIC_IP_ECHO).send().await.ok()?.error_for_status().ok()?.text().await.ok()?;
    text.trim().parse().ok()
}

/// Router WAN address through UPnP IGD `GetExternalIPAddress`.
/// `None` when no gateway answers or UPnP is disabled on the router.
pub async fn upnp_external_ip(timeout: Duration) -> Option<IpAddr> {
    let fut = async {
        for location in discover_gateways(timeout / 2).await {
            if let Some(ip) = external_ip_from(&location).await {
                return Some(ip);
            }
        }
        None
    };
    tokio::time::timeout(timeout, fut).await.ok().flatten()
}

async fn discover_gateways(wait: Duration) -> Vec<String> {
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0").await else { return Vec::new() };
    for st in [
        "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
        "urn:schemas-upnp-org:device:InternetGatewayDevice:2",
        WAN_SERVICES[1],
    ] {
        let search = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: {SSDP_ADDR}\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: {st}\r\n\r\n"
        );
        let _ = socket.send_to(search.as_bytes(), SSDP_ADDR).await;
    }
    let mut found = Vec::new();
    let mut buf = [0u8; 2048];
    let deadline = tokio::time::Instant::now() + wait;
    while let Ok(Ok((n, from))) =
        tokio::time::timeout_at(deadline, socket.recv_from(&mut buf)).await
    {
        let Some(location) = ssdp_location(&String::from_utf8_lossy(&buf[..n])) else { continue };
        // Only follow descriptions served by the device that answered, on the local network.
        if location_host(&location) == Some(from.ip())
            && mbl_remote::classify_ip(from.ip()) == mbl_remote::NetworkClass::Lan
            && !found.contains(&location)
        {
            found.push(location);
        }
    }
    found
}

fn ssdp_location(response: &str) -> Option<String> {
    response.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim().eq_ignore_ascii_case("location").then(|| value.trim().to_owned())
    })
}

fn location_host(url: &str) -> Option<IpAddr> {
    reqwest::Url::parse(url).ok()?.host_str()?.trim_matches(['[', ']']).parse().ok()
}

async fn fetch_limited(request: reqwest::RequestBuilder) -> Option<String> {
    let mut res = request.send().await.ok()?;
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.ok()? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_XML_BYTES {
            return None;
        }
    }
    String::from_utf8(body).ok()
}

async fn external_ip_from(location: &str) -> Option<IpAddr> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    let description = fetch_limited(client.get(location)).await?;
    let (service, control) = wan_control_url(&description)?;
    let control = reqwest::Url::parse(location).ok()?.join(&control).ok()?;
    if control.host_str() != reqwest::Url::parse(location).ok()?.host_str() {
        return None;
    }
    let body = format!(
        "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:GetExternalIPAddress xmlns:u=\"{service}\"/></s:Body></s:Envelope>"
    );
    let reply = fetch_limited(
        client
            .post(control)
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header("SOAPAction", format!("\"{service}#GetExternalIPAddress\""))
            .body(body),
    )
    .await?;
    tag_text(&reply, "NewExternalIPAddress")?.trim().parse().ok()
}

/// Service type and control URL of the first WAN connection service in a device description.
fn wan_control_url(description: &str) -> Option<(String, String)> {
    let mut rest = description;
    let mut best: Option<(usize, String, String)> = None;
    while let Some(start) = rest.find("<service>") {
        let block_end = rest[start..].find("</service>").map_or(rest.len(), |e| start + e);
        let block = &rest[start..block_end];
        if let (Some(kind), Some(control)) =
            (tag_text(block, "serviceType"), tag_text(block, "controlURL"))
            && let Some(rank) = WAN_SERVICES.iter().position(|s| *s == kind.trim())
            && best.as_ref().is_none_or(|(r, _, _)| rank < *r)
        {
            best = Some((rank, kind.trim().to_owned(), control.trim().to_owned()));
        }
        // `block_end` is past `<service>`, so the scan always advances.
        rest = &rest[block_end..];
    }
    best.map(|(_, kind, control)| (kind, control))
}

/// Text of the first `<name>` or `<prefix:name>` element.
fn tag_text<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let mut search = xml;
    loop {
        let open = search.find('<')?;
        let after = &search[open + 1..];
        let close = after.find('>')?;
        let tag = &after[..close];
        let local = tag.split_whitespace().next().unwrap_or("");
        let local = local.rsplit(':').next().unwrap_or(local);
        if local == name && !tag.starts_with('/') && !tag.ends_with('/') {
            let content = &after[close + 1..];
            let end = content.find("</")?;
            return Some(&content[..end]);
        }
        search = &after[close + 1..];
    }
}

/// Addresses a host name resolves to (empty when it does not resolve).
pub async fn resolve(name: &str, timeout: Duration) -> Vec<IpAddr> {
    if let Ok(ip) = name.parse::<IpAddr>() {
        return vec![ip];
    }
    match tokio::time::timeout(timeout, tokio::net::lookup_host((name, 443))).await {
        Ok(Ok(addresses)) => {
            let mut ips: Vec<IpAddr> = addresses.map(|a| a.ip()).collect();
            ips.sort();
            ips.dedup();
            ips
        }
        _ => Vec::new(),
    }
}

/// Whether a TCP connection to `ip:port` succeeds. From inside the network this depends on
/// the router supporting NAT loopback, so a failure is not proof that the port is closed.
pub async fn tcp_open(ip: IpAddr, port: u16, timeout: Duration) -> bool {
    matches!(
        tokio::time::timeout(timeout, TcpStream::connect(SocketAddr::new(ip, port))).await,
        Ok(Ok(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssdp_and_description() {
        let response = "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=120\r\nLocation: http://192.168.68.1:1900/igd.xml\r\nST: x\r\n\r\n";
        assert_eq!(ssdp_location(response).as_deref(), Some("http://192.168.68.1:1900/igd.xml"));
        assert_eq!(location_host("http://192.168.68.1:1900/igd.xml"), "192.168.68.1".parse().ok());

        let description = r#"<root><device><serviceList>
            <service><serviceType>urn:schemas-upnp-org:service:Layer3Forwarding:1</serviceType><controlURL>/l3f</controlURL></service>
            <service><serviceType>urn:schemas-upnp-org:service:WANPPPConnection:1</serviceType><controlURL>/ppp</controlURL></service>
            <service><serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType><controlURL>/ctl/IPConn</controlURL></service>
        </serviceList></device></root>"#;
        let (kind, control) = wan_control_url(description).unwrap();
        assert_eq!(kind, "urn:schemas-upnp-org:service:WANIPConnection:1");
        assert_eq!(control, "/ctl/IPConn");

        let reply = r#"<s:Envelope><s:Body><u:GetExternalIPAddressResponse xmlns:u="x"><NewExternalIPAddress>81.2.3.4</NewExternalIPAddress></u:GetExternalIPAddressResponse></s:Body></s:Envelope>"#;
        assert_eq!(tag_text(reply, "NewExternalIPAddress"), Some("81.2.3.4"));
        assert_eq!(wan_control_url("<root/>"), None);
    }
}
