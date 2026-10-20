//! Native preview server: WebSocket on `127.0.0.1`, protocol JSON for
//! commands/events, binary frames (16-byte header + JPEG).
//!
//! The engine is single-threaded (`!Send`), so it lives on its own thread;
//! the async side only moves messages. Bursts of `Seek` (scrubbing) are
//! coalesced: only the latest seek in a batch is rendered.

use std::io::Write;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use image::ExtendedColorType;
use image::codecs::jpeg::JpegEncoder;
use oxframe_core::Image;
use oxframe_protocol::{Command, FrameFormat};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::{Encoder, Output, PreviewEngine, Session};

/// Options of the native preview server.
#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// 0 picks a free port; the chosen one is printed as the first line of
    /// stdout.
    pub port: u16,
    /// If set, clients must connect to `/ws?token=<token>`.
    pub token: Option<String>,
    /// Size limit of the encoded-frame cache, in MiB.
    pub cache_mb: usize,
    /// JPEG quality of preview frames, 1..=100.
    pub jpeg_quality: u8,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            port: 0,
            token: std::env::var("OXFRAME_PREVIEW_TOKEN")
                .ok()
                .filter(|t| !t.is_empty()),
            cache_mb: 256,
            jpeg_quality: 80,
        }
    }
}

/// Encodes preview frames as JPEG.
pub fn jpeg_encoder(quality: u8) -> Encoder {
    Box::new(move |img: &Image| {
        let mut rgb = Vec::with_capacity((img.width * img.height * 3) as usize);
        for px in img.data.chunks_exact(4) {
            // Composite over black; preview frames are normally opaque.
            let a = u16::from(px[3]);
            rgb.extend(
                px[..3]
                    .iter()
                    .map(|&c| ((u16::from(c) * a + 127) / 255) as u8),
            );
        }
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, quality)
            .encode(&rgb, img.width, img.height, ExtendedColorType::Rgb8)
            .expect("JPEG encoding into memory");
        (FrameFormat::Jpeg, out)
    })
}

enum Inbound {
    Command(Command),
}

#[derive(Clone)]
struct AppState {
    commands: mpsc::Sender<Inbound>,
    /// Output channel of the current client; a new connection replaces it.
    client: Arc<Mutex<Option<UnboundedSender<Output>>>>,
    token: Option<String>,
}

/// Runs the server until `Shutdown` or the process is killed. `make_engine`
/// runs on the engine thread (engines are not `Send`).
///
/// # Errors
///
/// If the port cannot be bound.
pub fn serve<E, F>(make_engine: F, opts: ServerOptions) -> std::io::Result<()>
where
    E: PreviewEngine + 'static,
    F: FnOnce() -> Result<E, String> + Send + 'static,
{
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", opts.port)).await?;
        let port = listener.local_addr()?.port();

        let (cmd_tx, cmd_rx) = mpsc::channel::<Inbound>();
        let client: Arc<Mutex<Option<UnboundedSender<Output>>>> = Arc::default();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let engine_client = client.clone();
        let (cache_mb, quality) = (opts.cache_mb, opts.jpeg_quality);
        std::thread::Builder::new()
            .name("oxframe-engine".into())
            .spawn(move || {
                let engine = match make_engine() {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                let session = Session::new(engine, jpeg_encoder(quality), cache_mb);
                engine_loop(session, cmd_rx, engine_client);
            })?;
        // Fail before announcing the port if the project cannot load.
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(std::io::Error::other(e)),
            Err(_) => return Err(std::io::Error::other("engine thread died")),
        }

        // Contract with oxframe-cli: the first stdout line is the port.
        let mut stdout = std::io::stdout();
        writeln!(stdout, "{port}")?;
        stdout.flush()?;

        let state = AppState {
            commands: cmd_tx,
            client,
            token: opts.token,
        };
        let app = Router::new()
            .route("/ws", get(ws_handler))
            .route("/healthz", get(|| async { "ok" }))
            .with_state(state);
        axum::serve(listener, app).await
    })
}

fn engine_loop<E: PreviewEngine>(
    mut session: Session<E>,
    rx: mpsc::Receiver<Inbound>,
    client: Arc<Mutex<Option<UnboundedSender<Output>>>>,
) {
    let clock = Instant::now();
    let send = |outputs: Vec<Output>| {
        if outputs.is_empty() {
            return;
        }
        if let Some(tx) = client.lock().expect("client lock").as_ref() {
            for o in outputs {
                let _ = tx.send(o);
            }
        }
    };
    loop {
        let now = clock.elapsed().as_secs_f64();
        let wait = session
            .next_tick_in(now)
            .map_or(Duration::from_secs(3600), Duration::from_secs_f64);
        let mut batch = Vec::new();
        match rx.recv_timeout(wait) {
            Ok(Inbound::Command(c)) => batch.push(c),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        while let Ok(Inbound::Command(c)) = rx.try_recv() {
            batch.push(c);
        }
        let last_seek = batch
            .iter()
            .rposition(|c| matches!(c, Command::Seek { .. }));
        for (i, cmd) in batch.into_iter().enumerate() {
            if matches!(cmd, Command::Seek { .. }) && Some(i) != last_seek {
                continue;
            }
            if matches!(cmd, Command::Shutdown) {
                std::process::exit(0);
            }
            let now = clock.elapsed().as_secs_f64();
            send(session.handle(cmd, now));
        }
        send(session.tick(clock.elapsed().as_secs_f64()));
    }
}

#[derive(serde::Deserialize)]
struct WsQuery {
    token: Option<String>,
}

/// Browsers send `Origin`; only loopback pages may connect. Non-browser
/// clients (the CLI proxy) send none.
fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get("origin").and_then(|o| o.to_str().ok()) else {
        return true;
    };
    let host = origin
        .strip_prefix("http://")
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    matches!(host, "127.0.0.1" | "localhost")
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Query(q): Query<WsQuery>,
    State(state): State<AppState>,
) -> Response {
    if !origin_allowed(&headers) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    if state.token.is_some() && q.token != state.token {
        return (StatusCode::UNAUTHORIZED, "bad token").into_response();
    }
    ws.on_upgrade(move |socket| client(socket, state))
}

async fn client(mut socket: WebSocket, state: AppState) {
    let (tx, mut rx) = unbounded_channel::<Output>();
    *state.client.lock().expect("client lock") = Some(tx);
    loop {
        tokio::select! {
            out = rx.recv() => {
                let Some(out) = out else { break };
                let msg = match out {
                    Output::Event(e) => Message::Text(serde_json::to_string(&e).expect("event json").into()),
                    Output::Frame(bytes) => Message::Binary(bytes.into()),
                };
                if socket.send(msg).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(t))) => match serde_json::from_str::<Command>(&t) {
                        Ok(cmd) => {
                            if state.commands.send(Inbound::Command(cmd)).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let err = oxframe_protocol::Event::Error {
                                kind: oxframe_protocol::ErrorKind::Runtime,
                                message: format!("bad command: {e}"),
                                spans: Vec::new(),
                            };
                            let _ = socket.send(Message::Text(serde_json::to_string(&err).expect("json").into())).await;
                        }
                    },
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => {}
                }
            }
        }
    }
}
