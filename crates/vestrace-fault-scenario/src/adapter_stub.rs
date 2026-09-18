//! A minimal HTTP server standing in for the external system the scenario
//! dispatches effects to.
//!
//! It lives in the parent process, not the child under test, because the
//! question it answers — was the same effect dispatched twice? — can only be
//! answered truthfully by the party on the receiving end. The child can crash
//! at any of the five fault points; this stub cannot, since nothing in the
//! scenario's fault injection touches it.
//!
//! The parser is deliberately not a web framework: three fixed request shapes
//! (`POST /dispatch`, `GET /effects`, `GET /effects/{id}`) do not justify the
//! dependency surface, version churn, and startup cost a framework brings, and
//! a hand-rolled reader over `\r\n\r\n` is small enough to read in one sitting
//! and audit for correctness.
//!
//! # Two read-back routes, on purpose
//!
//! `GET /effects` answers `{"dispatches":N}` — the stub's own count, which is
//! the only truthful evidence anything has for `retry_attempted`, and which the
//! child reads directly.
//!
//! `GET /effects/{id}` answers the vocabulary that
//! `HttpExternalEffectReadBackAdapter` parses: `effect_applied`, `state_ref`,
//! `evidence_refs`, `evidence_strength`, and nothing else, because that reader
//! is `deny_unknown_fields`. The parent runs the deployment's own
//! `ExternalEffectRecoveryService` against this stub, and a stub that could not
//! answer the adapter the deployment actually ships would leave an effect
//! unresolvable for a reason belonging to the harness rather than to the system
//! under test — the observation would then be about the harness.
//!
//! The count is not re-derived for the second route. Both routes read the one
//! counter, which stays exactly what it was.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Stands in for the external effect sink. `dispatch_count` is the only
/// evidence the fault scenario has for `retry_attempted`, because the child
/// process reporting on itself would be the fox counting the henhouse.
pub struct AdapterStub {
    addr: std::net::SocketAddr,
    dispatches: Arc<AtomicUsize>,
    accept_loop: JoinHandle<()>,
}

impl AdapterStub {
    /// Binds to an OS-assigned loopback port so parallel test runs and
    /// parallel scenario runs never collide on a fixed port number.
    pub async fn start() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|error| format!("failed to bind adapter stub listener: {error}"))?;
        let addr = listener
            .local_addr()
            .map_err(|error| format!("failed to read adapter stub local address: {error}"))?;

        let dispatches = Arc::new(AtomicUsize::new(0));
        let accept_loop = tokio::spawn(accept_loop(listener, addr, Arc::clone(&dispatches)));

        Ok(Self {
            addr,
            dispatches,
            accept_loop,
        })
    }

    /// The URL the scenario under test dispatches effects to.
    pub fn dispatch_url(&self) -> String {
        format!("http://{}/dispatch", self.addr)
    }

    /// The URL reconciliation reads back from, to ask what the stub actually
    /// received rather than what the scenario claims it sent.
    pub fn read_back_url(&self) -> String {
        format!("http://{}/effects", self.addr)
    }

    /// Read directly rather than over HTTP, for callers already holding the
    /// stub in-process (the fault scenario's own reconciliation step).
    pub fn dispatch_count(&self) -> usize {
        self.dispatches.load(Ordering::SeqCst)
    }

    /// Stops accepting connections. Dropping the stub without calling this
    /// would leak the accept task for the rest of the test binary's run.
    pub async fn shutdown(self) {
        self.accept_loop.abort();
        let _ = self.accept_loop.await;
    }
}

async fn accept_loop(
    listener: TcpListener,
    addr: std::net::SocketAddr,
    dispatches: Arc<AtomicUsize>,
) {
    loop {
        let Ok((socket, _)) = listener.accept().await else {
            return;
        };
        let dispatches = Arc::clone(&dispatches);
        tokio::spawn(handle_connection(socket, addr, dispatches));
    }
}

/// The method and path of a request line, or `None` if it is not shaped like
/// one.
///
/// Split rather than prefix-matched: `starts_with("GET /effects")` cannot tell
/// `/effects` from `/effects/{id}`, and the two routes answer in different
/// vocabularies.
fn request_target(request_line: &str) -> Option<(&str, &str)> {
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    Some((method, path))
}

/// What the stub is willing to say about one effect, in the vocabulary
/// `HttpExternalEffectReadBackAdapter` parses.
///
/// `effect_applied` is the stub's own count and nothing else: a dispatch it
/// counted is one that arrived, and one it did not count is one that did not.
/// The count is per-stub rather than per-effect, which is exact here because an
/// invocation starts a stub of its own and drives a single effect through it —
/// and because a per-effect ledger would mean the stub deciding which effect a
/// request belonged to instead of reporting what reached it.
///
/// `external_resource_read_back` is the honest evidence strength: the far side
/// was asked directly and answered about the resource, which is what the name
/// means. Claiming a stronger one — a provider idempotency lookup, say — would
/// put a guarantee into the reconciliation that no part of this harness
/// provides.
fn read_back_body(addr: std::net::SocketAddr, effect_id: &str, dispatches: usize) -> String {
    serde_json::json!({
        "effect_applied": dispatches > 0,
        "state_ref": format!("http://{addr}/effects/{effect_id}#dispatches={dispatches}"),
        "evidence_refs": [format!("effect://{effect_id}/read-back")],
        "evidence_strength": "external_resource_read_back",
    })
    .to_string()
}

async fn handle_connection(
    mut socket: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    dispatches: Arc<AtomicUsize>,
) {
    let Some(request_line) = read_request_line(&mut socket).await else {
        return;
    };

    let response = match request_target(&request_line) {
        Some(("POST", "/dispatch")) => {
            let count = dispatches.fetch_add(1, Ordering::SeqCst) + 1;
            json_response(
                200,
                &format!("{{\"acknowledged\":true,\"dispatches\":{count}}}"),
            )
        }
        Some(("GET", "/effects")) => {
            let count = dispatches.load(Ordering::SeqCst);
            json_response(200, &format!("{{\"dispatches\":{count}}}"))
        }
        Some(("GET", path)) => match path.strip_prefix("/effects/") {
            Some(effect_id) if !effect_id.is_empty() => {
                let count = dispatches.load(Ordering::SeqCst);
                json_response(200, &read_back_body(addr, effect_id, count))
            }
            _ => json_response(404, "{\"error\":\"unknown route\"}"),
        },
        _ => json_response(404, "{\"error\":\"unknown route\"}"),
    };

    let _ = socket.write_all(response.as_bytes()).await;
    let _ = socket.shutdown().await;
}

/// Reads until the blank line that ends the HTTP header block and returns the
/// request line (the first line). The body is unused by either route this
/// stub serves, so it is never read.
async fn read_request_line(socket: &mut tokio::net::TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        let n = socket.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf);
    text.lines().next().map(|line| line.to_owned())
}

fn json_response(status: u16, body: &str) -> String {
    let status_text = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
