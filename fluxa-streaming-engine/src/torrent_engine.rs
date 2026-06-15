use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use librqbit::api::{TorrentDetailsResponse, TorrentIdOrHash};
use librqbit::{
    AddTorrent, AddTorrentOptions, Api, PeerConnectionOptions, Session, SessionOptions,
    TorrentStatsState,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use tokio::io::AsyncSeekExt;
use tokio::net::TcpListener;
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
}

#[derive(Clone)]
struct EngineState {
    api: Api,
    output_dir: PathBuf,
    preload_size: Arc<Mutex<u64>>,
    known_links: Arc<Mutex<HashMap<String, usize>>>,
}

struct TorrentServerHandle {
    stop: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

static TORRENT_SERVER: OnceLock<Mutex<Option<TorrentServerHandle>>> = OnceLock::new();

fn torrent_server_handle() -> &'static Mutex<Option<TorrentServerHandle>> {
    TORRENT_SERVER.get_or_init(|| Mutex::new(None))
}

pub fn start_torrent_server(cache_dir: &str, preferred_port: i32) -> Option<String> {
    // Stop any existing server first. The mutex is the single source of truth
    // for whether the server is running — no separate AtomicBool needed.
    {
        let mut guard = torrent_server_handle().lock().ok()?;
        if let Some(mut handle) = guard.take() {
            if let Some(stop) = handle.stop.take() {
                let _ = stop.send(());
            }
            if let Some(thread) = handle.thread.take() {
                let _ = thread.join();
            }
        }
    }

    let cache_dir = PathBuf::from(cache_dir);
    std::fs::create_dir_all(&cache_dir).ok()?;
    let bind_port = preferred_port.clamp(0, u16::MAX as i32) as u16;
    let std_listener = std::net::TcpListener::bind(("127.0.0.1", bind_port)).ok()?;
    std_listener.set_nonblocking(true).ok()?;
    let port = std_listener.local_addr().ok()?.port();
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let thread_cache_dir = cache_dir.clone();

    let thread = thread::spawn(move || {
        let worker_threads = std::thread::available_parallelism()
            .map(|n| n.get().clamp(4, 8))
            .unwrap_or(4);
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
            let listener = match TcpListener::from_std(std_listener) {
                Ok(listener) => listener,
                Err(error) => {
                    let _ = ready_tx.send(Err(error.to_string()));
                    return;
                }
            };

            let mut options = SessionOptions::default();
            options.disable_dht_persistence = true;
            options.defer_writes_up_to = Some(64);
            options.listen_port_range = Some(49152..65535);
            options.disable_upload = true;
            options.concurrent_init_limit = Some(2);
            options.trackers = [
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
            .collect();

            let session = match Session::new_with_opts(thread_cache_dir.clone(), options).await {
                Ok(session) => session,
                Err(error) => {
                    let _ = ready_tx.send(Err(format!("{error:#}")));
                    return;
                }
            };

            let state = EngineState {
                api: Api::new(session, None),
                output_dir: thread_cache_dir,
                preload_size: Arc::new(Mutex::new(10 * 1024 * 1024)),
                known_links: Arc::new(Mutex::new(HashMap::new())),
            };
            let app = Router::new()
                .route("/", get(root))
                .route("/settings", post(update_settings))
                .route("/torrents", post(torrents))
                .route("/stream/fname", get(stream_fname))
                .with_state(state);

            let server = axum::serve(listener, app).with_graceful_shutdown(async move {
                let _ = stop_rx.await;
            });
            let _ = ready_tx.send(Ok(()));
            let _ = server.await;
        });
    });

    match ready_rx.recv_timeout(Duration::from_secs(20)).ok()? {
        Ok(()) => {
            *torrent_server_handle().lock().ok()? = Some(TorrentServerHandle {
                stop: Some(stop_tx),
                thread: Some(thread),
            });
            serde_json::to_string(&json!({
                "url": format!("http://127.0.0.1:{port}"),
                "port": port,
                "cacheDir": cache_dir.to_string_lossy()
            }))
            .ok()
        }
        Err(_) => None,
    }
}

pub fn stop_torrent_server() -> bool {
    let Some(mut handle) = torrent_server_handle()
        .lock()
        .ok()
        .and_then(|mut guard| guard.take())
    else {
        return false;
    };
    if let Some(stop) = handle.stop.take() {
        let _ = stop.send(());
    }
    if let Some(thread) = handle.thread.take() {
        let _ = thread.join();
    }
    true
}

async fn root() -> impl IntoResponse {
    "Fluxa Rust Torrent Engine"
}

async fn update_settings(
    State(state): State<EngineState>,
    Json(settings): Json<TorrSettings>,
) -> impl IntoResponse {
    if let Some(preload_mb) = settings.preload_size {
        if let Ok(mut preload_size) = state.preload_size.lock() {
            *preload_size = preload_mb.saturating_mul(1024 * 1024);
        }
    }
    (StatusCode::OK, Json(json!({})))
}

async fn torrents(State(state): State<EngineState>, Json(request): Json<TorrRequest>) -> Response {
    let _ = request.save_to_db;
    let action = request.action.to_ascii_lowercase();
    match action.as_str() {
        "add" => {
            match ensure_torrent(&state, request.link.as_deref(), request.title.as_deref(), request.file_id).await {
                Ok((id, details)) => {
                    let focus = request
                        .file_id
                        .or_else(|| largest_file_id(&details));
                    if let Some(file_id) = focus {
                        prioritize_stream_file(&state, id, file_id).await;
                    }
                    status_response(&state, id, Some(details))
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
                    match ensure_torrent(&state, request.link.as_deref(), request.title.as_deref(), None)
                        .await
                    {
                        Ok((id, _)) => id,
                        Err(error) => return error_response(StatusCode::BAD_REQUEST, error),
                    }
                }
            };
            status_response(&state, id, None).await.into_response()
        }
        "rem" | "remove" | "delete" => {
            if let Some(id) = lookup_known_link(&state, request.link.as_deref()) {
                let _ = state
                    .api
                    .api_torrent_action_forget(TorrentIdOrHash::Id(id))
                    .await;
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
) -> Response {
    let range_header = headers.get("Range").and_then(|v| v.to_str().ok()).unwrap_or("none");
    eprintln!("[TorrServer] stream_fname link={} stat={} range={range_header}", &query.link[..query.link.len().min(60)], query.stat.is_some());

    // Stat requests return immediately — no retry loop (used by Kotlin status polling)
    if query.stat.is_some() {
        return match ensure_torrent(&state, Some(&query.link), query.title.as_deref(), None).await {
            Ok((id, details)) => status_response(&state, id, Some(details)).await.into_response(),
            Err(_) => (StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({
                "stat": 0, "preload": 0, "file_stats": [], "download_speed": 0,
                "active_peers": 0, "total_peers": 0, "progress": 0
            }))).into_response(),
        };
    }

    // Stream request: ensure_torrent does its own add+lookup. Calling it
    // once is enough — if metadata isn't ready yet, return 503 and let the
    // player retry the GET. No outer retry loop (the old 60s loop just hid
    // the latency from the user without saving any time).
    let (id, details) = match ensure_torrent(&state, Some(&query.link), query.title.as_deref(), query.index).await {
        Ok(value) => value,
        Err(error) => {
            eprintln!("[TorrServer] ensure_torrent failed: {error}");
            return error_response(StatusCode::SERVICE_UNAVAILABLE, error);
        }
    };
    let file_id = query
        .index
        .unwrap_or_else(|| largest_file_id(&details).unwrap_or(0));
    eprintln!("[TorrServer] streaming torrent={id} file={file_id} files={}", details.files.as_ref().map(|f| f.len()).unwrap_or(0));
    prioritize_stream_file(&state, id, file_id).await;

    // Wait for rqbit to leave Initializing state before attempting to stream.
    // api_stream fails immediately with "invalid state: initializing" until this
    // transition happens, so polling was wasting 50ms slots per attempt.
    // wait_until_initialized uses a notify channel and fires as soon as it's ready.
    if let Ok(handle) = state.api.mgr_handle(TorrentIdOrHash::Id(id)) {
        if let Err(e) = tokio::time::timeout(
            Duration::from_secs(60),
            handle.wait_until_initialized(),
        )
        .await
        {
            eprintln!("[TorrServer] wait_until_initialized timed out torrent={id}: {e}");
            return error_response(StatusCode::SERVICE_UNAVAILABLE, "torrent init timed out");
        }
    }

    match state.api.api_stream(TorrentIdOrHash::Id(id), file_id) {
        Ok(mut stream) => {
            let mut status = StatusCode::OK;
            let mut output_headers = HeaderMap::new();
            output_headers.insert("Accept-Ranges", HeaderValue::from_static("bytes"));
            if let Ok(mime) = state.api.torrent_file_mime_type(TorrentIdOrHash::Id(id), file_id) {
                if let Ok(value) = HeaderValue::from_str(mime) {
                    output_headers.insert("Content-Type", value);
                }
            }
            let total_len = stream.len();
            if let Some((start, end)) = parse_range(headers.get("Range"), total_len) {
                match stream.seek(SeekFrom::Start(start)).await {
                    Ok(_) => {
                        status = StatusCode::PARTIAL_CONTENT;
                        let end = end.unwrap_or_else(|| total_len.saturating_sub(1));
                        let length = end.saturating_sub(start).saturating_add(1);
                        insert_header(&mut output_headers, "Content-Length", length.to_string());
                        insert_header(&mut output_headers, "Content-Range", format!("bytes {start}-{end}/{total_len}"));
                    }
                    Err(error) => {
                        eprintln!("[TorrServer] seek failed torrent={id} file={file_id} start={start} len={total_len}: {error}");
                        insert_header(&mut output_headers, "Content-Length", total_len.to_string());
                    }
                }
            } else {
                insert_header(&mut output_headers, "Content-Length", total_len.to_string());
            }
            let body = Body::from_stream(ReaderStream::with_capacity(stream, 65536));
            (status, output_headers, body).into_response()
        }
        Err(e) => {
            eprintln!("[TorrServer] api_stream failed torrent={id} file={file_id}: {e:#}");
            error_response(StatusCode::NOT_FOUND, format!("{e:#}"))
        }
    }
}

async fn ensure_torrent(
    state: &EngineState,
    link: Option<&str>,
    title: Option<&str>,
    only_file: Option<usize>,
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
    let mut options = AddTorrentOptions::default();
    options.overwrite = true;
    options.output_folder = Some(state.output_dir.to_string_lossy().into_owned());
    options.peer_opts = Some(PeerConnectionOptions {
        connect_timeout: Some(Duration::from_secs(5)),
        read_write_timeout: Some(Duration::from_secs(20)),
        ..Default::default()
    });
    // Limit rqbit initialization to just the target file so the Initializing
    // hash-check covers one file instead of every file in the torrent.
    if let Some(file_id) = only_file {
        options.only_files = Some(vec![file_id]);
    }
    let response = state
        .api
        .api_add_torrent(AddTorrent::Url(link.to_string().into()), Some(options))
        .await
        .map_err(|error| format!("{error:#}"))?;
    let id = response
        .id
        .ok_or_else(|| "torrent metadata is not ready".to_string())?;
    remember_link(state, link, id);
    if let Some(title) = title {
        remember_link(state, title, id);
    }
    Ok((id, response.details))
}

async fn status_response(
    state: &EngineState,
    id: usize,
    details: Option<TorrentDetailsResponse>,
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
    let loaded_size = stats
        .as_ref()
        .map(|stats| stats.progress_bytes.min(preload_size))
        .unwrap_or(0);
    let stat = match stats.as_ref().map(|stats| stats.state) {
        Some(TorrentStatsState::Live) if loaded_size >= preload_size && preload_size > 0 => 3,
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
        "preload": if preload_size == 0 { 0 } else { ((loaded_size as f64 / preload_size as f64) * 100.0).round() as i64 },
        "loaded_size": loaded_size,
        "preload_size": preload_size,
        "file_stats": file_stats
    }))
}

fn parse_range(value: Option<&HeaderValue>, length: u64) -> Option<(u64, Option<u64>)> {
    let raw = value?.to_str().ok()?.strip_prefix("bytes=")?;
    let (start, end) = raw.split_once('-')?;
    let start = start.parse::<u64>().ok()?;
    if start >= length {
        return None;
    }
    let end = end
        .parse::<u64>()
        .ok()
        .map(|end| end.min(length.saturating_sub(1)));
    Some((start, end))
}

fn insert_header(headers: &mut HeaderMap, key: &'static str, value: String) {
    if let Ok(value) = HeaderValue::from_str(&value) {
        headers.insert(key, value);
    }
}

fn largest_file_id(details: &TorrentDetailsResponse) -> Option<usize> {
    details
        .files
        .as_ref()?
        .iter()
        .enumerate()
        .max_by_key(|(_, file)| file.length)
        .map(|(idx, _)| idx)
}

async fn prioritize_stream_file(state: &EngineState, torrent_id: usize, file_id: usize) {
    let only_files = HashSet::from([file_id]);
    let _ = state
        .api
        .api_torrent_action_update_only_files(TorrentIdOrHash::Id(torrent_id), &only_files)
        .await;
}

fn lookup_known_link(state: &EngineState, link: Option<&str>) -> Option<usize> {
    let link = link?.trim();
    state.known_links.lock().ok()?.get(link).copied()
}

fn remember_link(state: &EngineState, link: &str, id: usize) {
    if let Ok(mut links) = state.known_links.lock() {
        if links.len() >= 64 {
            links.clear();
        }
        links.insert(link.to_string(), id);
    }
}

fn error_response(message_status: StatusCode, message: impl Into<String>) -> Response {
    (message_status, Json(json!({ "error": message.into() }))).into_response()
}
