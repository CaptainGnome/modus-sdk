use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use url::Url;

const DNS_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostSpec {
    pub name: String,
    pub port: u16,
    pub wildcard: bool,
}

impl HostSpec {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim().to_lowercase();
        if raw.is_empty() {
            return Err("пустой хост".into());
        }
        if raw == "*" || raw.ends_with(".*") || raw == "*.com" || raw == "*.ru" {
            return Err("слишком широкий хост".into());
        }
        let (host_part, port) = if let Some((h, p)) = raw.rsplit_once(':') {
            if p.chars().all(|c| c.is_ascii_digit()) {
                let port: u16 = p.parse().map_err(|_| "неверный порт".to_string())?;
                (h.to_string(), port)
            } else {
                (raw.clone(), 443)
            }
        } else {
            (raw.clone(), 443)
        };
        let wildcard = host_part.starts_with("*.");
        let name = if wildcard {
            host_part[2..].to_string()
        } else {
            host_part
        };
        if name.is_empty() || name.starts_with('.') || name.contains('*') {
            return Err("некорректный хост".into());
        }
        Ok(Self {
            name,
            port,
            wildcard,
        })
    }

    pub fn matches(&self, host: &str, port: u16) -> bool {
        if port != self.port {
            return false;
        }
        let host = host.to_lowercase();
        if self.wildcard {
            host.ends_with(&format!(".{}", self.name)) && host != self.name
        } else {
            host == self.name
        }
    }

    pub fn canonical(&self) -> String {
        let host = if self.wildcard {
            format!("*.{}", self.name)
        } else {
            self.name.clone()
        };
        if self.port == 443 {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

pub fn require_broker_url(raw: &str, hosts: &[String]) -> Result<(), String> {
    let url = Url::parse(raw).map_err(|err| format!("broker_url: {err}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| "broker_url: нет хоста".to_string())?
        .to_lowercase();
    let port = url.port().unwrap_or(match url.scheme() {
        "https" => 443,
        "http" => 80,
        _ => return Err("broker_url: только http(s)".into()),
    });
    match url.scheme() {
        "https" => {}
        "http" => {
            if host != "127.0.0.1" && host != "localhost" && host != "::1" {
                return Err("broker_url: http только на loopback".into());
            }
        }
        _ => return Err("broker_url: только http(s)".into()),
    }
    let specs: Result<Vec<_>, _> = hosts.iter().map(|h| HostSpec::parse(h)).collect();
    let specs = specs?;
    if !specs.iter().any(|s| s.matches(&host, port)) {
        return Err(format!("broker_url: хост {host} не в allowlist манифеста"));
    }
    Ok(())
}

pub fn require_https_host(raw: &str, label: &str, hosts: &[String]) -> Result<(), String> {
    let url = Url::parse(raw).map_err(|err| format!("{label}: {err}"))?;
    if url.scheme() != "https" {
        return Err(format!("{label} должен быть https"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| format!("{label}: нет хоста"))?
        .to_lowercase();
    let port = url.port().unwrap_or(443);
    let specs: Result<Vec<_>, _> = hosts.iter().map(|h| HostSpec::parse(h)).collect();
    let specs = specs?;
    if !specs.iter().any(|s| s.matches(&host, port)) {
        return Err(format!("{label}: хост {host} не в allowlist манифеста"));
    }
    Ok(())
}

pub fn https_url_host(url: &str) -> Result<(String, u16), String> {
    let parsed = Url::parse(url).map_err(|err| format!("url: {err}"))?;
    match parsed.scheme() {
        "https" | "wss" => {}
        _ => return Err("только https/wss".into()),
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "нет хоста".to_string())?
        .to_lowercase();
    if parsed
        .host()
        .is_some_and(|h| matches!(h, url::Host::Ipv4(_) | url::Host::Ipv6(_)))
    {
        return Err("литеральный IP запрещён".into());
    }
    let port = parsed.port().unwrap_or(443);
    Ok((host, port))
}

pub fn allowed_by_manifest(host: &str, port: u16, specs: &[HostSpec]) -> bool {
    specs.iter().any(|spec| spec.matches(host, port))
}

pub fn is_blocked_name(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost")
}

pub fn forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            v.is_loopback()
                || v.is_private()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_broadcast()
                || v.is_multicast()
                || matches!(v.octets(), [100, b, ..] if (64..128).contains(&b))
                || matches!(v.octets(), [192, 0, 0, _])
                || matches!(v.octets(), [198, 18..=19, ..])
                || v.octets()[0] == 0
        }
        IpAddr::V6(v) => {
            if let Some(mapped) = v.to_ipv4_mapped() {
                return forbidden_ip(IpAddr::V4(mapped));
            }
            v.is_loopback()
                || v.is_unspecified()
                || v.is_multicast()
                || v.is_unique_local()
                || v.is_unicast_link_local()
        }
    }
}

pub fn url_without_query(raw: &str) -> String {
    let Ok(mut url) = Url::parse(raw) else {
        return raw.to_string();
    };
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

pub fn check_dev_target(
    url: &str,
    specs: &[HostSpec],
    resolve: bool,
) -> Result<(String, u16), String> {
    let (host, port) = https_url_host(url)?;
    if is_blocked_name(&host) {
        return Err(format!("запрещённый адрес {host}"));
    }
    if !allowed_by_manifest(&host, port, specs) {
        return Err(format!("хост {host} вне манифеста"));
    }
    if resolve {
        let addrs = resolve_addrs(&host, port)?;
        let mut any = false;
        for addr in addrs {
            any = true;
            if forbidden_ip(addr.ip()) {
                return Err(format!("запрещённый адрес {}", addr.ip()));
            }
        }
        if !any {
            return Err(format!("DNS {host}: нет адресов"));
        }
    }
    Ok((host, port))
}

fn resolve_addrs(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    let host_owned = host.to_string();
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("dns-resolve".into())
        .spawn(move || {
            let result = (host_owned.as_str(), port)
                .to_socket_addrs()
                .map(|iter| iter.collect::<Vec<_>>())
                .map_err(|err| err.to_string());
            let _ = tx.send(result);
        })
        .map_err(|err| format!("DNS {host}: {err}"))?;
    match rx.recv_timeout(DNS_TIMEOUT) {
        Ok(Ok(addrs)) => Ok(addrs),
        Ok(Err(err)) => Err(format!("DNS {host}: {err}")),
        Err(_) => Err(format!("DNS {host}: таймаут")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(hosts: &[&str]) -> Vec<HostSpec> {
        hosts.iter().map(|h| HostSpec::parse(h).unwrap()).collect()
    }

    #[test]
    fn rejects_http_and_literal_ip() {
        assert_eq!(
            https_url_host("http://example.com/").unwrap_err(),
            "только https/wss"
        );
        assert_eq!(
            https_url_host("wss://127.0.0.1/").unwrap_err(),
            "литеральный IP запрещён"
        );
    }

    #[test]
    fn replay_skips_dns_but_checks_manifest() {
        let specs = specs(&["example.com"]);
        check_dev_target("wss://example.com/", &specs, false).unwrap();
        let err = check_dev_target("wss://evil.example/", &specs, false).unwrap_err();
        assert!(err.contains("вне манифеста"), "{err}");
        let err = check_dev_target("wss://localhost/", &specs, false).unwrap_err();
        assert!(err.starts_with("запрещённый адрес"), "{err}");
    }

    #[test]
    fn forbidden_loopback() {
        assert!(forbidden_ip("127.0.0.1".parse().unwrap()));
        assert!(forbidden_ip("10.0.0.1".parse().unwrap()));
        assert!(!forbidden_ip("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn strips_query() {
        assert_eq!(
            url_without_query("https://api.example/x?foo=1"),
            "https://api.example/x"
        );
    }
}
