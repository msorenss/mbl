//! SSDP discovery for the DLNA media server (UDP 1900).
//!
//! Runs only while DLNA is turned on with at least one shared library: answers `M-SEARCH`
//! from the local network and announces `ssdp:alive` on each LAN interface, then `ssdp:byebye`
//! when DLNA is turned off or the server stops. Settings are rechecked every few seconds, so
//! no restart is needed.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use mbl_remote::dlna;
use mbl_remote::{NetworkClass, classify_ip};
use mbl_service::dlna::DlnaService;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

const MULTICAST: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
const PORT: u16 = 1900;
const ANNOUNCE_EVERY: Duration = Duration::from_secs(15 * 60);
const SETTINGS_EVERY: Duration = Duration::from_secs(5);

/// A LAN interface the server is announced on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Iface {
    ip: Ipv4Addr,
    netmask: Ipv4Addr,
}

impl Iface {
    fn contains(&self, other: Ipv4Addr) -> bool {
        let mask = u32::from(self.netmask);
        u32::from(self.ip) & mask == u32::from(other) & mask
    }
}

/// Supervise discovery until `stop`, starting and stopping it as DLNA settings change.
pub async fn run(dlna: DlnaService, bind: String, http_port: u16, stop: CancellationToken) {
    let bind_ip: Option<Ipv4Addr> = bind.parse().ok();
    if bind_ip.is_some_and(|ip| ip.is_loopback()) || bind == "localhost" {
        tracing::debug!("DLNA discovery skipped for loopback bind");
        return;
    }
    let mut worker: Option<(CancellationToken, tokio::task::JoinHandle<()>)> = None;
    loop {
        let active = dlna.settings().await.is_ok_and(|s| s.is_active());
        if worker.as_ref().is_some_and(|(_, task)| task.is_finished()) {
            worker = None;
        }
        match (active, &worker) {
            (true, None) => {
                let token = stop.child_token();
                let task = tokio::spawn(announce(
                    dlna.clone(),
                    bind_ip.filter(|ip| !ip.is_unspecified()),
                    http_port,
                    token.clone(),
                ));
                worker = Some((token, task));
            }
            (false, Some(_)) => {
                if let Some((token, task)) = worker.take() {
                    token.cancel();
                    let _ = task.await;
                }
            }
            _ => {}
        }
        tokio::select! {
            () = stop.cancelled() => break,
            () = tokio::time::sleep(SETTINGS_EVERY) => {}
        }
    }
    if let Some((token, task)) = worker {
        token.cancel();
        let _ = task.await;
    }
}

async fn announce(
    dlna: DlnaService,
    only: Option<Ipv4Addr>,
    http_port: u16,
    stop: CancellationToken,
) {
    let udn = match dlna.udn().await {
        Ok(udn) => udn,
        Err(error) => {
            tracing::warn!(%error, "DLNA discovery unavailable: no server id");
            return;
        }
    };
    let ifaces = lan_interfaces(only);
    if ifaces.is_empty() {
        tracing::warn!("DLNA discovery unavailable: no LAN interface");
        return;
    }
    let listener = match listen(&ifaces) {
        Ok(socket) => Arc::new(socket),
        Err(error) => {
            tracing::warn!(%error, "DLNA discovery unavailable: UDP port 1900 could not be opened");
            return;
        }
    };
    let senders: Vec<(Iface, UdpSocket)> =
        ifaces.iter().filter_map(|iface| sender(iface.ip).ok().map(|s| (*iface, s))).collect();
    let location = |ip: Ipv4Addr| format!("http://{ip}:{http_port}/dlna/description.xml");
    let send_all = |alive: bool| {
        let (senders, udn) = (&senders, &udn);
        async move {
            for (iface, socket) in senders {
                for nt in dlna::notification_types(udn) {
                    let message = dlna::notify(&nt, udn, &location(iface.ip), alive);
                    let _ = socket.send_to(message.as_bytes(), (MULTICAST, PORT)).await;
                }
            }
        }
    };
    tracing::info!("DLNA discovery started");
    send_all(true).await;
    let mut next_announce = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut buf = vec![0_u8; 2048];
    loop {
        tokio::select! {
            () = stop.cancelled() => break,
            () = tokio::time::sleep_until(next_announce) => {
                send_all(true).await;
                next_announce = tokio::time::Instant::now() + ANNOUNCE_EVERY;
            }
            received = listener.recv_from(&mut buf) => {
                let Ok((len, SocketAddr::V4(from))) = received else { continue };
                if classify_ip(IpAddr::V4(*from.ip())) != NetworkClass::Lan {
                    continue;
                }
                let Some(search) = dlna::parse_msearch(&buf[..len]) else { continue };
                let targets = dlna::matching_targets(&search.st, &udn);
                if targets.is_empty() {
                    continue;
                }
                let ip = ifaces.iter().find(|i| i.contains(*from.ip())).unwrap_or(&ifaces[0]).ip;
                let socket = listener.clone();
                let (udn, location) = (udn.clone(), location(ip));
                // Spread replies over part of MX so many servers do not answer at once.
                let delay = jitter_ms(search.mx.min(1) * 500);
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    for st in targets {
                        let reply = dlna::search_response(&st, &udn, &location, &http_date());
                        let _ = socket.send_to(reply.as_bytes(), from).await;
                    }
                });
            }
        }
    }
    send_all(false).await;
    tracing::info!("DLNA discovery stopped");
}

/// Non-loopback IPv4 interfaces on private networks, skipping container and VPN bridges.
fn lan_interfaces(only: Option<Ipv4Addr>) -> Vec<Iface> {
    let Ok(addrs) = if_addrs::get_if_addrs() else { return Vec::new() };
    let skip = ["docker", "br-", "veth", "virbr", "tailscale", "tun", "wg", "zt", "cni", "flannel"];
    addrs
        .into_iter()
        .filter(|a| !a.is_loopback() && !skip.iter().any(|p| a.name.starts_with(p)))
        .filter_map(|a| match a.addr {
            if_addrs::IfAddr::V4(v4) => Some(Iface { ip: v4.ip, netmask: v4.netmask }),
            if_addrs::IfAddr::V6(_) => None,
        })
        .filter(|i| classify_ip(IpAddr::V4(i.ip)) == NetworkClass::Lan)
        .filter(|i| only.is_none_or(|ip| ip == i.ip))
        .collect()
}

/// Shared UDP 1900 socket joined to the SSDP group on every interface.
fn listen(ifaces: &[Iface]) -> std::io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, PORT).into())?;
    let mut joined = 0;
    for iface in ifaces {
        if socket.join_multicast_v4(&MULTICAST, &iface.ip).is_ok() {
            joined += 1;
        }
    }
    if joined == 0 {
        return Err(std::io::Error::other("could not join the SSDP multicast group"));
    }
    socket.set_nonblocking(true)?;
    UdpSocket::from_std(socket.into())
}

/// Socket that sends multicast announcements out of one interface.
fn sender(ip: Ipv4Addr) -> std::io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.bind(&SocketAddrV4::new(ip, 0).into())?;
    socket.set_multicast_if_v4(&ip)?;
    socket.set_multicast_ttl_v4(4)?;
    socket.set_nonblocking(true)?;
    UdpSocket::from_std(socket.into())
}

fn jitter_ms(max: u64) -> u64 {
    if max == 0 {
        return 0;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_nanos()));
    nanos % max
}

fn http_date() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc2822)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subnet_matching() {
        let iface = Iface {
            ip: Ipv4Addr::new(192, 168, 68, 147),
            netmask: Ipv4Addr::new(255, 255, 252, 0),
        };
        assert!(iface.contains(Ipv4Addr::new(192, 168, 69, 10)));
        assert!(!iface.contains(Ipv4Addr::new(192, 168, 72, 1)));
    }

    #[test]
    fn date_header_format() {
        let date = http_date();
        assert!(date.ends_with(" +0000"), "{date}");
        assert_eq!(date.split(' ').count(), 6, "{date}");
    }
}
