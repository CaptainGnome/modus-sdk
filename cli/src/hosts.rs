use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use url::Url;

const DNS_TIMEOUT: Duration = Duration::from_secs(3);

#[cfg(test)]
mod test_hooks {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    type LookupHook = Box<dyn Fn(&str, u16) -> Result<Vec<SocketAddr>, String> + Send + Sync>;
    type DialHook = Box<dyn Fn(&[SocketAddr]) -> Result<TcpStream, String> + Send + Sync>;

    fn lookup_hook() -> &'static Mutex<Option<LookupHook>> {
        static HOOK: OnceLock<Mutex<Option<LookupHook>>> = OnceLock::new();
        HOOK.get_or_init(|| Mutex::new(None))
    }

    fn dial_hook() -> &'static Mutex<Option<DialHook>> {
        static HOOK: OnceLock<Mutex<Option<DialHook>>> = OnceLock::new();
        HOOK.get_or_init(|| Mutex::new(None))
    }

    fn hook_serial() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct ClearHooks;

    impl Drop for ClearHooks {
        fn drop(&mut self) {
            *lookup_hook().lock().unwrap() = None;
            *dial_hook().lock().unwrap() = None;
        }
    }

    pub fn try_lookup(host: &str, port: u16) -> Option<Result<Vec<SocketAddr>, String>> {
        lookup_hook()
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|hook| hook(host, port)))
    }

    pub fn try_dial(addrs: &[SocketAddr]) -> Option<Result<TcpStream, String>> {
        dial_hook()
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|hook| hook(addrs)))
    }

    pub fn with_net_hooks<R>(
        lookup: impl Fn(&str, u16) -> Result<Vec<SocketAddr>, String> + Send + Sync + 'static,
        dial: Option<Box<dyn Fn(&[SocketAddr]) -> Result<TcpStream, String> + Send + Sync>>,
        f: impl FnOnce() -> R,
    ) -> R {
        let _serial = hook_serial().lock().unwrap_or_else(|e| e.into_inner());
        let _clear = ClearHooks;
        *lookup_hook().lock().unwrap() = Some(Box::new(lookup));
        if let Some(dial) = dial {
            *dial_hook().lock().unwrap() = Some(dial);
        }
        f()
    }

    pub fn with_lookup_hook<R>(
        hook: impl Fn(&str, u16) -> Result<Vec<SocketAddr>, String> + Send + Sync + 'static,
        f: impl FnOnce() -> R,
    ) -> R {
        with_net_hooks(hook, None, f)
    }

    #[allow(dead_code)]
    pub fn with_dns_serial<R>(f: impl FnOnce() -> R) -> R {
        let _serial = hook_serial().lock().unwrap_or_else(|e| e.into_inner());
        f()
    }
}

#[cfg(test)]
pub use test_hooks::{with_lookup_hook, with_net_hooks};

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
            return Err("empty host".into());
        }
        if raw == "*" || raw.ends_with(".*") || raw == "*.com" || raw == "*.ru" {
            return Err("host too broad".into());
        }
        let (host_part, port) = if let Some((h, p)) = raw.rsplit_once(':') {
            if p.chars().all(|c| c.is_ascii_digit()) {
                let port: u16 = p.parse().map_err(|_| "invalid port".to_string())?;
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
            return Err("invalid host".into());
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
        .ok_or_else(|| "broker_url: missing host".to_string())?
        .to_lowercase();
    let port = url.port().unwrap_or(match url.scheme() {
        "https" => 443,
        "http" => 80,
        _ => return Err("broker_url: http(s) only".into()),
    });
    match url.scheme() {
        "https" => {}
        "http" => {
            if host != "127.0.0.1" && host != "localhost" && host != "::1" {
                return Err("broker_url: http only on loopback".into());
            }
        }
        _ => return Err("broker_url: http(s) only".into()),
    }
    let specs: Result<Vec<_>, _> = hosts.iter().map(|h| HostSpec::parse(h)).collect();
    let specs = specs?;
    if !specs.iter().any(|s| s.matches(&host, port)) {
        return Err(format!("broker_url: host {host} not in manifest allowlist"));
    }
    Ok(())
}

pub fn require_https_host(raw: &str, label: &str, hosts: &[String]) -> Result<(), String> {
    let url = Url::parse(raw).map_err(|err| format!("{label}: {err}"))?;
    if url.scheme() != "https" {
        return Err(format!("{label} must be https"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| format!("{label}: missing host"))?
        .to_lowercase();
    let port = url.port().unwrap_or(443);
    let specs: Result<Vec<_>, _> = hosts.iter().map(|h| HostSpec::parse(h)).collect();
    let specs = specs?;
    if !specs.iter().any(|s| s.matches(&host, port)) {
        return Err(format!("{label}: host {host} not in manifest allowlist"));
    }
    Ok(())
}

pub fn https_url_host(url: &str) -> Result<(String, u16), String> {
    let parsed = Url::parse(url).map_err(|err| format!("url: {err}"))?;
    match parsed.scheme() {
        "https" | "wss" => {}
        _ => return Err("https/wss only".into()),
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "missing host".to_string())?
        .to_lowercase();
    if parsed
        .host()
        .is_some_and(|h| matches!(h, url::Host::Ipv4(_) | url::Host::Ipv6(_)))
    {
        return Err("literal IP forbidden".into());
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
        return Err(format!("forbidden address {host}"));
    }
    if !allowed_by_manifest(&host, port, specs) {
        return Err(format!("host {host} not in manifest"));
    }
    if resolve {
        let _ = resolve_pinned(&host, port)?;
    }
    Ok((host, port))
}

/// One `getaddrinfo` + reject if any address is private/loopback/metadata.
pub fn resolve_pinned(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    let addrs = resolve_addrs(host, port)?;
    if addrs.is_empty() {
        return Err(format!("DNS {host}: no addresses"));
    }
    for addr in &addrs {
        if forbidden_ip(addr.ip()) {
            return Err(format!("forbidden address {}", addr.ip()));
        }
    }
    Ok(addrs)
}

/// TCP connect using a pinned address list. No second DNS.
pub fn dial_tcp(addrs: &[SocketAddr], timeout: Duration) -> Result<TcpStream, String> {
    #[cfg(test)]
    if let Some(result) = test_hooks::try_dial(addrs) {
        return result;
    }
    let mut last = String::from("connect failed");
    for addr in addrs {
        match TcpStream::connect_timeout(addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(err) => last = err.to_string(),
        }
    }
    Err(last)
}

fn resolve_addrs(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    #[cfg(test)]
    if let Some(result) = test_hooks::try_lookup(host, port) {
        return result;
    }
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
        Err(_) => Err(format!("DNS {host}: timeout")),
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
            "https/wss only"
        );
        assert_eq!(
            https_url_host("wss://127.0.0.1/").unwrap_err(),
            "literal IP forbidden"
        );
    }

    #[test]
    fn replay_skips_dns_but_checks_manifest() {
        let specs = specs(&["example.com"]);
        check_dev_target("wss://example.com/", &specs, false).unwrap();
        let err = check_dev_target("wss://evil.example/", &specs, false).unwrap_err();
        assert!(err.contains("not in manifest"), "{err}");
        let err = check_dev_target("wss://localhost/", &specs, false).unwrap_err();
        assert!(err.starts_with("forbidden address"), "{err}");
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

    #[test]
    fn resolve_pinned_rejects_private_and_mixed() {
        use std::net::Ipv4Addr;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_c = Arc::clone(&calls);
        with_lookup_hook(
            move |_host, port| {
                calls_c.fetch_add(1, Ordering::SeqCst);
                Ok(vec![SocketAddr::from((Ipv4Addr::new(10, 0, 0, 1), port))])
            },
            || {
                let err = resolve_pinned("evil.example", 443).unwrap_err();
                assert!(err.contains("forbidden address"), "{err}");
            },
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        with_lookup_hook(
            |_host, port| {
                Ok(vec![
                    SocketAddr::from((Ipv4Addr::new(203, 0, 113, 5), port)),
                    SocketAddr::from((Ipv4Addr::new(127, 0, 0, 1), port)),
                ])
            },
            || {
                let err = resolve_pinned("mixed.example", 443).unwrap_err();
                assert!(err.contains("forbidden address"), "{err}");
            },
        );
    }

    #[test]
    fn pinned_connect_one_lookup_never_dials_second_private() {
        use std::net::{Ipv4Addr, TcpListener};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let loopback_port = listener.local_addr().unwrap().port();

        let lookups = Arc::new(AtomicUsize::new(0));
        let dialed = Arc::new(Mutex::new(Vec::<SocketAddr>::new()));
        let lookups_c = Arc::clone(&lookups);
        let dialed_c = Arc::clone(&dialed);

        with_net_hooks(
            move |_host, port| {
                let n = lookups_c.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Ok(vec![SocketAddr::from((Ipv4Addr::new(203, 0, 113, 5), port))])
                } else {
                    Ok(vec![SocketAddr::from((Ipv4Addr::LOCALHOST, loopback_port))])
                }
            },
            Some(Box::new(move |addrs| {
                dialed_c.lock().unwrap().extend_from_slice(addrs);
                Err("dial blocked in test".into())
            })),
            || {
                let addrs = resolve_pinned("pin.example", 443).unwrap();
                assert_eq!(addrs[0].ip(), Ipv4Addr::new(203, 0, 113, 5));
                let _ = dial_tcp(&addrs, Duration::from_millis(50)).unwrap_err();
            },
        );

        assert_eq!(lookups.load(Ordering::SeqCst), 1);
        let dialed = dialed.lock().unwrap().clone();
        assert_eq!(dialed.len(), 1);
        assert_eq!(dialed[0].ip(), Ipv4Addr::new(203, 0, 113, 5));
        assert!(listener.accept().is_err());
    }
}
