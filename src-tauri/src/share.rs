//! Адреса, по которым друзья заходят на сервер: локальная сеть, внешний IP с пробросом порта
//! через UPnP и IPv6. IP может меняться (динамический) — адрес считаем заново при каждом запуске.

use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::time::Duration;

use igd_next::aio::tokio::search_gateway;
use igd_next::{PortMappingProtocol, SearchOptions};
use serde::Serialize;

const DESCRIPTION: &str = "Shard Minecraft server";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub port: u16,
    /// Порт открыт на роутере
    pub upnp: bool,
    /// Почему UPnP не вышел: no_gateway — роутер не ответил (UPnP выключен), refused — отказал
    pub upnp_error: Option<&'static str>,
    /// Внешний адрес «ip:порт»
    pub public: Option<String>,
    /// Провайдер прячет за общим IP (CGNAT) — снаружи не зайти, нужен IPv6 или туннель
    pub cgnat: bool,
    /// Глобальный IPv6 «[адрес]:порт», если он есть
    pub ipv6: Option<String>,
}

/// IP этого компьютера в локальной сети. UDP-«подключение» ничего не отправляет,
/// оно только выбирает сетевую карту с маршрутом наружу.
pub fn lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}

fn global_ipv6() -> Option<IpAddr> {
    let socket = UdpSocket::bind("[::]:0").ok()?;
    socket.connect("[2001:4860:4860::8888]:80").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    // Глобальные адреса начинаются с 2000::/3; временные и локальные не подходят
    match ip {
        IpAddr::V6(v6) if v6.segments()[0] & 0xe000 == 0x2000 => Some(ip),
        _ => None,
    }
}

/// Серые адреса: частные сети и 100.64.0.0/10, который провайдеры отдают при CGNAT.
fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local() || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1])),
        IpAddr::V6(_) => false,
    }
}

async fn public_ip(http: &reqwest::Client) -> Option<IpAddr> {
    let response = http.get("https://api.ipify.org").timeout(Duration::from_secs(6)).send().await.ok()?;
    response.text().await.ok()?.trim().parse().ok()
}

/// Открывает порт на роутере и собирает адреса. Ошибки не роняют сервер: в худшем случае
/// останется адрес для локальной сети.
pub async fn open(http: &reqwest::Client, port: u16) -> Share {
    let mut share = Share { port, upnp: false, upnp_error: None, public: None, cgnat: false, ipv6: global_ipv6().map(|ip| format!("[{ip}]:{port}")) };
    let public = public_ip(http).await;

    let options = SearchOptions { timeout: Some(Duration::from_secs(3)), ..Default::default() };
    let mut router_ip = None;
    match (search_gateway(options).await, lan_ip()) {
        (Ok(gateway), Some(local)) => {
            let local = SocketAddr::new(local, port);
            // Часть роутеров принимает только бессрочные правила, часть — только с таймером
            let mut added = gateway.add_port(PortMappingProtocol::TCP, port, local, 0, DESCRIPTION).await.is_ok();
            if !added {
                added = gateway.add_port(PortMappingProtocol::TCP, port, local, 24 * 3600, DESCRIPTION).await.is_ok();
            }
            share.upnp = added;
            if !added {
                share.upnp_error = Some("refused");
            }
            router_ip = gateway.get_external_ip().await.ok();
        }
        _ => share.upnp_error = Some("no_gateway"),
    }

    // Роутер видит серый адрес или не тот, что видит интернет, — значит, впереди ещё NAT провайдера
    share.cgnat = router_ip.is_some_and(|r| is_private(r) || public.is_some_and(|p| p != r));
    share.public = public.filter(|ip| !is_private(*ip)).map(|ip| format!("{ip}:{port}"));
    share
}

/// Закрывает проброс порта после остановки сервера.
pub async fn close(port: u16) {
    let options = SearchOptions { timeout: Some(Duration::from_secs(3)), ..Default::default() };
    if let Ok(gateway) = search_gateway(options).await {
        let _ = gateway.remove_port(PortMappingProtocol::TCP, port).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn detects_grey_addresses() {
        assert!(is_private(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5))));
        assert!(is_private(IpAddr::V4(Ipv4Addr::new(100, 72, 0, 1))));
        assert!(!is_private(IpAddr::V4(Ipv4Addr::new(100, 200, 0, 1))));
        assert!(!is_private(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34))));
    }
}
