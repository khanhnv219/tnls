//! TCP health check: can we open the tunnel's local endpoint?
//!
//! Process-alive ≠ tunnel-working, so the supervisor dials
//! `bind_address:local_port` with a short timeout. Only TCP connect —
//! never protocol-aware (MVP covers databases, HTTP, SOCKS alike).

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// How long a single dial may block the supervisor tick.
pub const HEALTH_TIMEOUT: Duration = Duration::from_millis(200);

/// Dial `endpoint` (`"127.0.0.1:5433"`). `true` = something accepts.
pub fn tcp_check(endpoint: &str, timeout: Duration) -> bool {
    let addr: SocketAddr = match endpoint.parse() {
        Ok(addr) => addr,
        Err(_) => return false,
    };
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_port_is_healthy() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        assert!(tcp_check(&endpoint, Duration::from_secs(2)));
    }

    #[test]
    fn closed_port_is_unhealthy() {
        // Bind then drop: port is (almost surely) closed afterwards.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let endpoint = format!("127.0.0.1:{port}");
        assert!(!tcp_check(&endpoint, Duration::from_millis(300)));
    }

    #[test]
    fn garbage_endpoint_is_unhealthy() {
        assert!(!tcp_check("not-an-endpoint", Duration::from_millis(100)));
        assert!(!tcp_check("127.0.0.1:notaport", Duration::from_millis(100)));
    }
}
