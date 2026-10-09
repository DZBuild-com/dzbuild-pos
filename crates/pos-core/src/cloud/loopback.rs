//! The one-shot loopback redirect of RFC 8252 §7.3: bound to 127.0.0.1 on a port Windows picks,
//! it answers the browser's single `/oauth/callback` with a page in the attempt's language
//! ("you can close this tab"), and closes — also on cancel and after 10 minutes. Other requests
//! (favicon, a browser's pre-connect) get a 404 and are ignored.

use super::oauth::{CallbackParams, parse_callback};
use crate::error::AppError;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Response, Server};

pub const CALLBACK_PATH: &str = "/oauth/callback";
/// DZBuild's authorization codes and consent screens live 10 minutes.
pub const WAIT_LIMIT: Duration = Duration::from_secs(600);

/// Complete HTML documents, built by the shell from `src/locales/<lang>/sync.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pages {
    pub done: String,
    pub failed: String,
}

pub struct Loopback {
    server: Server,
    address: SocketAddr,
}

impl Loopback {
    pub fn bind() -> Result<Loopback, AppError> {
        let server = Server::http("127.0.0.1:0").map_err(|e| {
            tracing::warn!(target: "cloud", reason = %e, "the loopback listener could not start");
            AppError::sync("cloud_unavailable")
        })?;
        let address = server
            .server_addr()
            .to_ip()
            .filter(|a| a.ip().is_loopback())
            .ok_or_else(|| AppError::sync("cloud_unavailable"))?;
        Ok(Loopback { server, address })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn port(&self) -> u16 {
        self.address.port()
    }

    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}{CALLBACK_PATH}", self.port())
    }

    /// The callback's parameters (unchecked: `oauth::check_callback` decides). Consumes the
    /// listener: the socket closes when this returns.
    pub fn wait(
        self,
        pages: &Pages,
        limit: Duration,
        cancel: &AtomicBool,
    ) -> Result<CallbackParams, AppError> {
        let deadline = Instant::now() + limit;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(AppError::sync("cloud_link_cancelled"));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(AppError::sync("cloud_link_expired"));
            }
            let request = match self
                .server
                .recv_timeout(left.min(Duration::from_millis(250)))
            {
                Ok(Some(request)) => request,
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(target: "cloud", kind = ?e.kind(), "the loopback listener failed");
                    return Err(AppError::sync("cloud_unavailable"));
                }
            };
            let url = request.url().to_owned();
            let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
            if *request.method() != Method::Get || path != CALLBACK_PATH {
                let _ = request.respond(Response::empty(404));
                continue;
            }
            let params = parse_callback(query);
            let page = if params.code.is_some() && params.error.is_none() {
                &pages.done
            } else {
                &pages.failed
            };
            let mut response = Response::from_string(page.clone());
            for (name, value) in [
                ("Content-Type", "text/html; charset=utf-8"),
                ("Cache-Control", "no-store"),
                ("Referrer-Policy", "no-referrer"),
            ] {
                if let Ok(header) = Header::from_bytes(name.as_bytes(), value.as_bytes()) {
                    response = response.with_header(header);
                }
            }
            if let Err(e) = request.respond(response) {
                tracing::warn!(target: "cloud", kind = ?e.kind(), "the browser left before the page");
            }
            return Ok(params);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    fn pages() -> Pages {
        Pages {
            done: "<!doctype html><html lang=\"ar\" dir=\"rtl\"><p>تم الربط</p></html>".into(),
            failed: "<!doctype html><p>échec</p>".into(),
        }
    }

    fn get(url: &str) -> (u16, String) {
        let response = reqwest::blocking::get(url).unwrap();
        (response.status().as_u16(), response.text().unwrap())
    }

    #[test]
    fn the_listener_binds_loopback_only_and_closes_after_the_callback() {
        let loopback = Loopback::bind().unwrap();
        assert!(loopback.address().ip().is_loopback(), "never 0.0.0.0");
        let port = loopback.port();
        assert_eq!(
            loopback.redirect_uri(),
            format!("http://127.0.0.1:{port}/oauth/callback")
        );
        let waiter = std::thread::spawn(move || {
            loopback.wait(&pages(), Duration::from_secs(10), &AtomicBool::new(false))
        });
        assert_eq!(
            get(&format!("http://127.0.0.1:{port}/favicon.ico")).0,
            404,
            "other paths are ignored"
        );
        let (status, body) = get(&format!(
            "http://127.0.0.1:{port}/oauth/callback?code=c1&state=S"
        ));
        assert_eq!(status, 200);
        assert!(body.contains("تم الربط"), "UTF-8 Arabic page: {body}");
        let params = waiter.join().unwrap().unwrap();
        assert_eq!(
            (params.code.as_deref(), params.state.as_deref()),
            (Some("c1"), Some("S"))
        );
        let closed = (0..40).any(|_| {
            std::thread::sleep(Duration::from_millis(50));
            TcpStream::connect(("127.0.0.1", port)).is_err()
        });
        assert!(closed, "port {port} is still listening after the callback");
    }

    #[test]
    fn an_error_callback_shows_the_failure_page() {
        let loopback = Loopback::bind().unwrap();
        let port = loopback.port();
        let waiter = std::thread::spawn(move || {
            loopback.wait(&pages(), Duration::from_secs(10), &AtomicBool::new(false))
        });
        let (_, body) = get(&format!(
            "http://127.0.0.1:{port}/oauth/callback?error=access_denied&state=S"
        ));
        assert!(body.contains("échec"));
        assert_eq!(
            waiter.join().unwrap().unwrap().error.as_deref(),
            Some("access_denied")
        );
    }

    #[test]
    fn a_cancel_or_the_deadline_ends_the_wait() {
        let cancelled = Loopback::bind()
            .unwrap()
            .wait(&pages(), Duration::from_secs(10), &AtomicBool::new(true))
            .unwrap_err();
        assert_eq!(cancelled.code(), "cloud_link_cancelled");
        let expired = Loopback::bind()
            .unwrap()
            .wait(
                &pages(),
                Duration::from_millis(300),
                &AtomicBool::new(false),
            )
            .unwrap_err();
        assert_eq!(expired.code(), "cloud_link_expired");
    }
}
