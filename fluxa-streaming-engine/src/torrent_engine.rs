use axum::body::Body;
use axum::extract::{Query, State, connect_info::ConnectInfo};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use librqbit::api::{TorrentDetailsResponse, TorrentIdOrHash};
use librqbit::dht::PersistentDhtConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, Api, PeerConnectionOptions, Session, SessionOptions,
    SessionPersistenceConfig, TorrentStatsState,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::SeekFrom;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::oneshot;
use tokio_util::io::ReaderStream;

#[derive(Deserialize)]
struct TorrRequest {
    action: String,
    link: Option<String>,
    hash: Option<String>,
    title: Option<String>,
    #[serde(default)]
    save_to_db: bool,
    // Optional file index to focus on right after add — prevents rqbit
    // from spreading peer slots across every file in the torrent.
    file_id: Option<usize>,
    #[serde(default)]
    role: FileRole,
}

#[derive(Deserialize)]
struct TorrSettings {
    #[serde(rename = "PreloadSize")]
    preload_size: Option<u64>,
}

#[derive(Deserialize)]
struct StreamQuery {
    link: String,
    title: Option<String>,
    index: Option<usize>,
    stat: Option<String>,
    access_token: Option<String>,
    #[serde(default)]
    role: FileRole,
}

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum FileRole {
    #[default]
    Video,
    Subtitle,
    Auxiliary,
}

#[derive(Default)]
struct TorrentFileFocus {
    primary_video: Option<usize>,
    auxiliary_files: HashSet<usize>,
}

#[derive(Clone)]
struct EngineState {
    api: Api,
    output_dir: PathBuf,
    preload_size: Arc<Mutex<u64>>,
    known_links: Arc<Mutex<HashMap<String, usize>>>,
    prioritized_files: Arc<Mutex<HashMap<usize, TorrentFileFocus>>>,
    // A per-link lock serializes retries for one magnet while allowing
    // unrelated metadata lookups to progress independently.
    in_flight_adds: Arc<AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
    pending_adds: Arc<Mutex<HashSet<String>>>,
    access_token: Arc<String>,
}

struct TorrentServerHandle {
    generation: u64,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

static TORRENT_SERVER: OnceLock<Mutex<Option<TorrentServerHandle>>> = OnceLock::new();
static TORRENT_GENERATION: AtomicU64 = AtomicU64::new(0);

fn torrent_server_handle() -> &'static Mutex<Option<TorrentServerHandle>> {
    TORRENT_SERVER.get_or_init(|| Mutex::new(None))
}

fn debug_log(message: impl AsRef<str>) {
    if std::env::var_os("FLUXA_TORRENT_DEBUG").is_some() {
        eprintln!("{}", message.as_ref());
    }
}

fn join_with_timeout(thread: thread::JoinHandle<()>, timeout: Duration) {
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    thread::spawn(move || {
        let _ = thread.join();
        let _ = done_tx.send(());
    });
    if done_rx.recv_timeout(timeout).is_err() {
        debug_log(format!(
            "[TorrServer] teardown did not finish within {timeout:?}, continuing without waiting"
        ));
    }
}

pub fn start_torrent_server(
    cache_dir: &str,
    preferred_port: i32,
    access_token: &str,
) -> Option<String> {
    let mut guard = torrent_server_handle().lock().ok()?;
    if let Some(mut handle) = guard.take() {
        let teardown_start = std::time::Instant::now();
        if let Some(stop) = handle.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = handle.thread.take() {
            join_with_timeout(thread, Duration::from_secs(5));
        }
        debug_log(format!(
            "[TorrServer] previous server torn down in {:?}",
            teardown_start.elapsed()
        ));
    }
    let bootstrap_start = std::time::Instant::now();

    let cache_dir = PathBuf::from(cache_dir);
    std::fs::create_dir_all(&cache_dir).ok()?;
    let dht_config = PersistentDhtConfig {
        dump_interval: Some(Duration::from_secs(60)),
        config_filename: Some(cache_dir.parent()?.join("torrent-dht.json")),
    };
    let bind_port = preferred_port.clamp(0, u16::MAX as i32) as u16;
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u16, String>>();
    let thread_cache_dir = cache_dir.clone();
    let thread_dht_config = dht_config;
    let thread_access_token = access_token.trim().to_string();

    let thread = thread::spawn(move || {
        let worker_threads = std::thread::available_parallelism()
            .map(|n| n.get().clamp(2, 8))
            .unwrap_or(2);
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(worker_threads)
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = ready_tx.send(Err(error.to_string()));
                return;
            }
        };

        runtime.block_on(async move {
            let options = SessionOptions {
                disable_dht_persistence: false,
                dht_config: Some(thread_dht_config),
                fastresume: true,
                persistence: Some(SessionPersistenceConfig::Json {
                    folder: Some(thread_cache_dir.join("session")),
                }),
                defer_writes_up_to: Some(64),
                listen_port_range: Some(49152..65535),
                enable_upnp_port_forwarding: true,
                disable_upload: true,
                concurrent_init_limit: Some(2),
                trackers: [
                    "udp://tracker.opentrackr.org:1337/announce",
                    "udp://open.demonii.com:1337/announce",
                    "udp://tracker.openbittorrent.com:80/announce",
                    "udp://exodus.desync.com:6969/announce",
                    "udp://open.stealth.si:80/announce",
                    "udp://tracker.torrent.eu.org:451/announce",
                    "udp://tracker.tiny-vps.com:6969/announce",
                ]
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect(),
                ..Default::default()
            };

            let session = match tokio::time::timeout(
                Duration::from_secs(18),
                Session::new_with_opts(thread_cache_dir.clone(), options),
            )
            .await
            {
                Ok(Ok(session)) => session,
                Ok(Err(error)) => {
                    let _ = ready_tx.send(Err(format!("{error:#}")));
                    return;
                }
                Err(_) => {
                    let _ = ready_tx.send(Err("torrent session init timed out".to_string()));
                    return;
                }
            };

            let listener = match TcpListener::bind(("0.0.0.0", bind_port)).await {
                Ok(listener) => listener,
                Err(error) => {
                    let _ = ready_tx.send(Err(error.to_string()));
                    return;
                }
            };
            let port = match listener.local_addr() {
                Ok(addr) => addr.port(),
                Err(error) => {
                    let _ = ready_tx.send(Err(error.to_string()));
                    return;
                }
            };

            let state = EngineState {
                api: Api::new(session, None),
                output_dir: thread_cache_dir,
                preload_size: Arc::new(Mutex::new(10 * 1024 * 1024)),
                known_links: Arc::new(Mutex::new(HashMap::new())),
                prioritized_files: Arc::new(Mutex::new(HashMap::new())),
                in_flight_adds: Arc::new(AsyncMutex::new(HashMap::new())),
                pending_adds: Arc::new(Mutex::new(HashSet::new())),
                access_token: Arc::new(thread_access_token),
            };
            tokio::spawn(peer_stats_logger(state.clone()));
            let app = Router::new()
                .route("/", get(root))
                .route("/health", get(health))
                .route("/settings", post(update_settings))
                .route("/torrents", post(torrents))
                .route("/stream/fname", get(stream_fname))
                .with_state(state);

            let server = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            );
            let _ = ready_tx.send(Ok(port));
            tokio::select! {
                _ = server => {}
                _ = stop_rx => {}
            }
        });
        runtime.shutdown_timeout(Duration::from_secs(2));
    });

    match ready_rx.recv_timeout(Duration::from_secs(20)) {
        Ok(Ok(port)) => {
            debug_log(format!(
                "[TorrServer] new server bootstrapped in {:?}",
                bootstrap_start.elapsed()
            ));
            let generation = TORRENT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
            *guard = Some(TorrentServerHandle {
                generation,
                stop: Some(stop_tx),
                thread: Some(thread),
            });
            serde_json::to_string(&json!({
                "url": format!("http://127.0.0.1:{port}"),
                "port": port,
                "cacheDir": cache_dir.to_string_lossy(),
                "generation": generation
            }))
            .ok()
        }
        Ok(Err(error)) => {
            debug_log(format!("[TorrServer] startup failed: {error}"));
            let _ = stop_tx.send(());
            join_with_timeout(thread, Duration::from_secs(5));
            None
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            debug_log("[TorrServer] startup timed out");
            let _ = stop_tx.send(());
            None
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            join_with_timeout(thread, Duration::from_secs(5));
            None
        }
    }
}

/// Stops the running torrent server. If `expected_generation` is given, the
/// stop is a no-op when a newer server has already replaced the one the
/// caller meant to stop (e.g. a stale stop racing a fast replay's start) —
/// otherwise a stop issued for an old session could tear down the session
/// that superseded it.
pub fn stop_torrent_server(expected_generation: Option<u64>) -> bool {
    let mut guard = match torrent_server_handle().lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    if let Some(expected) = expected_generation {
        if guard.as_ref().map(|h| h.generation) != Some(expected) {
            return false;
        }
    }
    let Some(mut handle) = guard.take() else {
        return false;
    };
    if let Some(stop) = handle.stop.take() {
        let _ = stop.send(());
    }
    if let Some(thread) = handle.thread.take() {
        join_with_timeout(thread, Duration::from_secs(5));
    }
    true
}

// Independent of UI stat polling, so the timeline is complete even if the
// frontend isn't actively hitting /stream/fname?stat. One line per known
// torrent every 2s, gated behind FLUXA_TORRENT_DEBUG like everything else
// here — meant to be diffed against Stremio/TorrServer runs to see whether
// peer discovery plateaus (queued/live flat) or throughput per peer is the
// bottleneck (live steady, download_speed low).
async fn peer_stats_logger(state: EngineState) {
    if std::env::var_os("FLUXA_TORRENT_DEBUG").is_none() {
        return;
    }
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let ids: HashSet<usize> = state
            .known_links
            .lock()
            .map(|links| links.values().copied().collect())
            .unwrap_or_default();
        for id in ids {
            let Ok(stats) = state.api.api_stats_v1(TorrentIdOrHash::Id(id)) else {
                continue;
            };
            let peers = stats.live.as_ref().map(|live| &live.snapshot.peer_stats);
            let download_bps = stats
                .live
                .as_ref()
                .map(|live| live.download_speed.mbps * 1024.0 * 1024.0)
                .unwrap_or(0.0);
            debug_log(format!(
                "[TorrServer][peers] torrent={id} state={:?} queued={} connecting={} live={} seen={} dead={} steals={} down={download_bps:.0}B/s progress={}/{} uploaded={}",
                stats.state,
                peers.map(|p| p.queued).unwrap_or(0),
                peers.map(|p| p.connecting).unwrap_or(0),
                peers.map(|p| p.live).unwrap_or(0),
                peers.map(|p| p.seen).unwrap_or(0),
                peers.map(|p| p.dead).unwrap_or(0),
                peers.map(|p| p.steals).unwrap_or(0),
                stats.progress_bytes,
                stats.total_bytes,
                stats.uploaded_bytes,
            ));
        }
    }
}

async fn root() -> impl IntoResponse {
    "Fluxa Rust Torrent Engine"
}

async fn health() -> impl IntoResponse {
    "ok"
}

async fn update_settings(
    State(state): State<EngineState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    Json(settings): Json<TorrSettings>,
) -> impl IntoResponse {
    if !request_authorized(&state, remote_addr, None) {
        return error_response(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    if let Some(preload_mb) = settings.preload_size {
        if let Ok(mut preload_size) = state.preload_size.lock() {
            *preload_size = preload_mb.saturating_mul(1024 * 1024);
        }
    }
    (StatusCode::OK, Json(json!({}))).into_response()
}

async fn torrents(
    State(state): State<EngineState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    Json(request): Json<TorrRequest>,
) -> Response {
    if !request_authorized(&state, remote_addr, None) {
        return error_response(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    let _ = request.save_to_db;
    let action = request.action.to_ascii_lowercase();
    match action.as_str() {
        "add" => {
            match ensure_torrent(
                &state,
                request.link.as_deref(),
                request.title.as_deref(),
                request.file_id,
                Duration::from_secs(90),
            )
            .await
            {
                Ok((id, details)) => {
                    let focus = request.file_id.or_else(|| largest_file_id(&details));
                    if let Some(file_id) = focus {
                        prioritize_stream_file(&state, id, file_id, request.role).await;
                    }
                    status_response(&state, id, Some(details), focus)
                        .await
                        .into_response()
                }
                Err(error) => error_response(StatusCode::BAD_REQUEST, error),
            }
        }
        "get" => {
            let id = match request
                .hash
                .as_deref()
                .and_then(|hash| hash.parse::<usize>().ok())
                .or_else(|| lookup_known_link(&state, request.link.as_deref()))
            {
                Some(id) => id,
                None => {
                    let resolving = match request.link.as_deref() {
                        Some(link) => add_is_pending(&state, link),
                        None => false,
                    };
                    return Json(empty_status_json(resolving)).into_response();
                }
            };
            if let Some(file_id) = request.file_id {
                prioritize_stream_file(&state, id, file_id, request.role).await;
            }
            status_response(&state, id, None, request.file_id)
                .await
                .into_response()
        }
        "rem" | "remove" | "delete" => {
            if let Some(id) = lookup_known_link(&state, request.link.as_deref()) {
                let _ = state
                    .api
                    .api_torrent_action_delete(TorrentIdOrHash::Id(id))
                    .await;
                if let Ok(mut links) = state.known_links.lock() {
                    links.retain(|_, known_id| *known_id != id);
                }
                if let Ok(mut files) = state.prioritized_files.lock() {
                    files.remove(&id);
                }
            }
            Json(json!({})).into_response()
        }
        _ => error_response(StatusCode::BAD_REQUEST, "unsupported torrent action"),
    }
}

async fn stream_fname(
    State(state): State<EngineState>,
    Query(query): Query<StreamQuery>,
    headers: HeaderMap,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
) -> Response {
    if !request_authorized(&state, remote_addr, query.access_token.as_deref()) {
        return error_response(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    let range_header = headers
        .get("Range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none");
    debug_log(format!(
        "[TorrServer] stream_fname link={} stat={} range={range_header}",
        &query.link[..query.link.len().min(60)],
        query.stat.is_some()
    ));

    // Stat requests return immediately. They must not start or block metadata
    // acquisition; otherwise UI polling can contend with the real stream GET.
    if query.stat.is_some() {
        if let Some(id) = lookup_known_link(&state, Some(&query.link)) {
            return status_response(&state, id, None, query.index)
                .await
                .into_response();
        }
        return Json(empty_status_json(add_is_pending(&state, &query.link))).into_response();
    }

    // Stream request: ensure_torrent does its own add+lookup. Calling it
    // once is enough — if metadata isn't ready yet, return 503 and let the
    // player retry the GET. No outer retry loop (the old 60s loop just hid
    // the latency from the user without saving any time).
    let (id, details) = match ensure_torrent(
        &state,
        Some(&query.link),
        query.title.as_deref(),
        query.index,
        Duration::from_secs(90),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            debug_log(format!("[TorrServer] ensure_torrent failed: {error}"));
            return error_response(StatusCode::SERVICE_UNAVAILABLE, error);
        }
    };
    let file_id = query
        .index
        .unwrap_or_else(|| largest_file_id(&details).unwrap_or(0));
    debug_log(format!(
        "[TorrServer] streaming torrent={id} file={file_id} files={}",
        details.files.as_ref().map(|f| f.len()).unwrap_or(0)
    ));
    prioritize_stream_file(&state, id, file_id, query.role).await;

    // Wait for rqbit to leave Initializing state before attempting to stream.
    // api_stream fails immediately with "invalid state: initializing" until this
    // transition happens, so polling was wasting 50ms slots per attempt.
    // wait_until_initialized uses a notify channel and fires as soon as it's ready.
    if let Ok(handle) = state.api.mgr_handle(TorrentIdOrHash::Id(id)) {
        if let Err(e) =
            tokio::time::timeout(Duration::from_secs(60), handle.wait_until_initialized()).await
        {
            debug_log(format!(
                "[TorrServer] wait_until_initialized timed out torrent={id}: {e}"
            ));
            return error_response(StatusCode::SERVICE_UNAVAILABLE, "torrent init timed out");
        }
    }

    match state.api.api_stream(TorrentIdOrHash::Id(id), file_id) {
        Ok(mut stream) => {
            let mut status = StatusCode::OK;
            let mut output_headers = HeaderMap::new();
            output_headers.insert("Accept-Ranges", HeaderValue::from_static("bytes"));
            if let Ok(mime) = state
                .api
                .torrent_file_mime_type(TorrentIdOrHash::Id(id), file_id)
            {
                if let Ok(value) = HeaderValue::from_str(mime) {
                    output_headers.insert("Content-Type", value);
                }
            }
            let total_len = stream.len();
            match parse_range(headers.get("Range"), total_len) {
                Ok(Some((start, end))) => {
                    if let Err(error) = stream.seek(SeekFrom::Start(start)).await {
                        debug_log(format!(
                            "[TorrServer] seek failed torrent={id} file={file_id} start={start} len={total_len}: {error}"
                        ));
                        return error_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "failed to seek stream",
                        );
                    }
                    status = StatusCode::PARTIAL_CONTENT;
                    let length = end.saturating_sub(start).saturating_add(1);
                    insert_header(&mut output_headers, "Content-Length", length.to_string());
                    insert_header(
                        &mut output_headers,
                        "Content-Range",
                        format!("bytes {start}-{end}/{total_len}"),
                    );
                    let body =
                        Body::from_stream(ReaderStream::with_capacity(stream.take(length), 65536));
                    (status, output_headers, body).into_response()
                }
                Ok(None) => {
                    insert_header(&mut output_headers, "Content-Length", total_len.to_string());
                    let body = Body::from_stream(ReaderStream::with_capacity(stream, 65536));
                    (status, output_headers, body).into_response()
                }
                Err(()) => range_not_satisfiable_response(total_len),
            }
        }
        Err(e) => {
            debug_log(format!(
                "[TorrServer] api_stream failed torrent={id} file={file_id}: {e:#}"
            ));
            error_response(StatusCode::NOT_FOUND, format!("{e:#}"))
        }
    }
}

async fn ensure_torrent(
    state: &EngineState,
    link: Option<&str>,
    title: Option<&str>,
    only_file: Option<usize>,
    metadata_timeout: Duration,
) -> Result<(usize, TorrentDetailsResponse), String> {
    let link = link
        .map(str::trim)
        .filter(|link| !link.is_empty())
        .ok_or_else(|| "missing torrent link".to_string())?;
    if let Some(id) = lookup_known_link(state, Some(link)) {
        let details = state
            .api
            .api_torrent_details(TorrentIdOrHash::Id(id))
            .map_err(|error| format!("{error:#}"))?;
        return Ok((id, details));
    }

    // Metadata acquisition only serializes per link. Different magnets can
    // resolve in parallel while a duplicate request joins this operation.
    let link_lock = {
        let mut in_flight = state.in_flight_adds.lock().await;
        in_flight
            .entry(link.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    };
    let lock_timeout = metadata_timeout
        .checked_add(Duration::from_secs(5))
        .unwrap_or(metadata_timeout);
    let _add_guard = match tokio::time::timeout(lock_timeout, link_lock.lock()).await {
        Ok(guard) => guard,
        Err(_) => return Err("torrent add already in progress".to_string()),
    };
    if let Some(id) = lookup_known_link(state, Some(link)) {
        let details = state
            .api
            .api_torrent_details(TorrentIdOrHash::Id(id))
            .map_err(|error| format!("{error:#}"))?;
        return Ok((id, details));
    }

    let mut options = AddTorrentOptions {
        overwrite: true,
        output_folder: Some(state.output_dir.to_string_lossy().into_owned()),
        peer_opts: Some(PeerConnectionOptions {
            connect_timeout: Some(Duration::from_millis(2500)),
            read_write_timeout: Some(Duration::from_secs(20)),
            ..Default::default()
        }),
        ..Default::default()
    };
    // Limit rqbit initialization to just the target file so the Initializing
    // hash-check covers one file instead of every file in the torrent.
    if let Some(file_id) = only_file {
        options.only_files = Some(vec![file_id]);
    }
    set_add_pending(state, link, true);
    let add_started = std::time::Instant::now();
    let response = tokio::time::timeout(
        metadata_timeout,
        state
            .api
            .api_add_torrent(AddTorrent::Url(link.to_string().into()), Some(options)),
    )
    .await;
    let response = match response {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            set_add_pending(state, link, false);
            return Err(format!("{error:#}"));
        }
        Err(_) => {
            set_add_pending(state, link, false);
            debug_log(format!(
                "[TorrServer][timing] metadata timed out after {:?} link={}",
                add_started.elapsed(),
                &link[..link.len().min(80)]
            ));
            return Err("torrent metadata timed out".to_string());
        }
    };
    let Some(id) = response.id else {
        set_add_pending(state, link, false);
        return Err("torrent metadata is not ready".to_string());
    };
    debug_log(format!(
        "[TorrServer][timing] metadata ready in {:?} torrent={id}",
        add_started.elapsed()
    ));
    remember_link(state, link, id);
    if let Some(title) = title {
        remember_link(state, title, id);
    }
    set_add_pending(state, link, false);
    Ok((id, response.details))
}

fn empty_status_json(resolving: bool) -> Value {
    json!({
        "hash": "",
        "title": "",
        "download_speed": 0.0,
        "active_peers": 0,
        "total_peers": 0,
        "progress": 0.0,
        "stat": 0,
        "stat_string": if resolving { "resolving" } else { "initializing" },
        "resolving": resolving,
        "preload": 0,
        "loaded_size": 0,
        "streamed_size": 0,
        "preload_size": 0,
        "file_stats": []
    })
}

fn set_add_pending(state: &EngineState, link: &str, pending: bool) {
    if let Ok(mut pending_adds) = state.pending_adds.lock() {
        if pending {
            pending_adds.insert(link.to_string());
        } else {
            pending_adds.remove(link);
        }
    }
}

fn add_is_pending(state: &EngineState, link: &str) -> bool {
    state
        .pending_adds
        .lock()
        .map(|pending| pending.contains(link.trim()))
        .unwrap_or(false)
}

async fn status_response(
    state: &EngineState,
    id: usize,
    details: Option<TorrentDetailsResponse>,
    focus_file: Option<usize>,
) -> Json<Value> {
    let details = details.or_else(|| state.api.api_torrent_details(TorrentIdOrHash::Id(id)).ok());
    let stats = state.api.api_stats_v1(TorrentIdOrHash::Id(id)).ok();
    let file_stats = details
        .as_ref()
        .and_then(|details| details.files.as_ref())
        .map(|files| {
            files
                .iter()
                .enumerate()
                .map(|(idx, file)| json!({ "id": idx, "path": file.name, "length": file.length }))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let progress = stats
        .as_ref()
        .map(|stats| {
            if stats.total_bytes == 0 {
                0.0
            } else {
                (stats.progress_bytes as f64 / stats.total_bytes as f64) * 100.0
            }
        })
        .unwrap_or(0.0);
    let download_speed = stats
        .as_ref()
        .and_then(|stats| stats.live.as_ref())
        .map(|live| live.download_speed.mbps * 1024.0 * 1024.0)
        .unwrap_or(0.0);
    let active_peers = stats
        .as_ref()
        .and_then(|stats| stats.live.as_ref())
        .map(|live| live.snapshot.peer_stats.live)
        .unwrap_or(0);
    let total_peers = stats
        .as_ref()
        .and_then(|stats| stats.live.as_ref())
        .map(|live| live.snapshot.peer_stats.seen)
        .unwrap_or(0);
    let preload_size = state.preload_size.lock().map(|value| *value).unwrap_or(0);
    // This is deliberately file-scoped. Torrent-wide progress and bytes
    // already delivered to the player say nothing about the selected file.
    // librqbit's public stats do not expose a per-range piece bitmap yet, so
    // this remains a conservative file-progress signal, not a claim that the
    // bytes are contiguous after an arbitrary seek.
    let loaded_size = focus_file
        .and_then(|file_id| {
            stats
                .as_ref()
                .and_then(|stats| stats.file_progress.get(file_id).copied())
        })
        .unwrap_or(0);
    let progress_loaded_size = loaded_size.min(preload_size);
    let stat = match stats.as_ref().map(|stats| stats.state) {
        Some(TorrentStatsState::Live)
            if progress_loaded_size >= preload_size && preload_size > 0 =>
        {
            3
        }
        Some(TorrentStatsState::Live) => 2,
        Some(TorrentStatsState::Initializing) => 0,
        Some(TorrentStatsState::Paused) => 1,
        Some(TorrentStatsState::Error) => -1,
        None => 0,
    };
    Json(json!({
        "hash": details.as_ref().map(|details| details.info_hash.clone()).unwrap_or_default(),
        "title": details.as_ref().and_then(|details| details.name.clone()).unwrap_or_default(),
        "download_speed": download_speed,
        "active_peers": active_peers,
        "total_peers": total_peers,
        "progress": progress,
        "stat": stat,
        "stat_string": stats.as_ref().map(|stats| stats.state.to_string()).unwrap_or_else(|| "initializing".to_string()),
        "error": stats.as_ref().and_then(|stats| stats.error.as_deref()),
        "preload": if preload_size == 0 { 0 } else { ((progress_loaded_size as f64 / preload_size as f64) * 100.0).round() as i64 },
        "loaded_size": loaded_size,
        "streamed_size": 0,
        "preload_size": preload_size,
        "file_stats": file_stats
    }))
}

mod http;

use http::*;
#[cfg(test)]
mod tests {
    use super::http::update_file_focus;
    use super::{FileRole, TorrentFileFocus, parse_range};
    use axum::http::HeaderValue;

    fn range(value: &str, length: u64) -> Result<Option<(u64, u64)>, ()> {
        parse_range(Some(&HeaderValue::from_str(value).unwrap()), length)
    }

    #[test]
    fn parses_open_ended_range() {
        assert_eq!(range("bytes=100-", 1000), Ok(Some((100, 999))));
    }

    #[test]
    fn parses_bounded_range() {
        assert_eq!(range("bytes=100-199", 1000), Ok(Some((100, 199))));
    }

    #[test]
    fn clamps_bounded_range_to_file_end() {
        assert_eq!(range("bytes=900-2000", 1000), Ok(Some((900, 999))));
    }

    #[test]
    fn parses_suffix_range() {
        assert_eq!(range("bytes=-200", 1000), Ok(Some((800, 999))));
    }

    #[test]
    fn rejects_unsatisfiable_and_malformed_ranges() {
        assert_eq!(range("bytes=1000-", 1000), Err(()));
        assert_eq!(range("bytes=200-100", 1000), Err(()));
        assert_eq!(range("items=0-1", 1000), Err(()));
        assert_eq!(range("bytes=0-1,2-3", 1000), Err(()));
        assert_eq!(range("bytes=-0", 1000), Err(()));
    }

    #[test]
    fn no_range_header_means_full_response() {
        assert_eq!(parse_range(None, 1000), Ok(None));
    }

    #[test]
    fn subtitle_keeps_current_primary_video_selected() {
        let mut focus = TorrentFileFocus::default();
        assert_eq!(
            update_file_focus(&mut focus, 1, FileRole::Video),
            Some([1].into_iter().collect())
        );
        assert_eq!(
            update_file_focus(&mut focus, 2, FileRole::Subtitle),
            Some([1, 2].into_iter().collect())
        );
        assert_eq!(focus.primary_video, Some(1));
    }

    #[test]
    fn new_video_replaces_old_video_and_clears_old_auxiliaries() {
        let mut focus = TorrentFileFocus::default();
        update_file_focus(&mut focus, 1, FileRole::Video);
        update_file_focus(&mut focus, 2, FileRole::Subtitle);
        assert_eq!(
            update_file_focus(&mut focus, 3, FileRole::Video),
            Some([3].into_iter().collect())
        );
        assert_eq!(focus.primary_video, Some(3));
        assert!(focus.auxiliary_files.is_empty());
    }
}
