use serde_json::json;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

const PROXY_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
const MAX_LOCAL_STREAM_CONNECTIONS: usize = 32;

pub(crate) fn build_proxy_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(90))
        .user_agent(PROXY_USER_AGENT)
        .build()
        .expect("proxy client build")
}

#[derive(Clone)]
pub(crate) struct LocalStreamConfig {
    pub(crate) id: String,
    pub(crate) target_url: String,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) client: Arc<reqwest::blocking::Client>,
    pub(crate) active_connections: Arc<AtomicUsize>,
    pub(crate) port: u16,
}

pub(crate) struct LocalStreamHandle {
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) thread: Option<thread::JoinHandle<()>>,
}

pub(crate) struct ParsedLocalRequest {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) headers: HashMap<String, String>,
}

pub(crate) static LOCAL_STREAM_SERVERS: OnceLock<Mutex<HashMap<String, LocalStreamHandle>>> =
    OnceLock::new();
pub(crate) static LOCAL_STREAM_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn local_stream_servers() -> &'static Mutex<HashMap<String, LocalStreamHandle>> {
    LOCAL_STREAM_SERVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn next_local_stream_id() -> String {
    let counter = LOCAL_STREAM_ID.fetch_add(1, Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    counter.hash(&mut hasher);
    now.hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

struct ActiveConnectionGuard {
    counter: Arc<AtomicUsize>,
}

impl ActiveConnectionGuard {
    fn try_acquire(counter: Arc<AtomicUsize>) -> Option<Self> {
        let previous = counter.fetch_add(1, Ordering::AcqRel);
        if previous >= MAX_LOCAL_STREAM_CONNECTIONS {
            counter.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Self { counter })
    }
}

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) fn parse_request(stream: &mut TcpStream) -> Option<ParsedLocalRequest> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next()?.to_ascii_uppercase();
    let path = request_parts.next()?.to_string();
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    Some(ParsedLocalRequest {
        method,
        path,
        headers,
    })
}

pub(crate) fn write_simple_response(stream: &mut TcpStream, status: &str) {
    let body = status.as_bytes();
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

pub(crate) fn retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

pub(crate) fn send_upstream_request(
    client: &reqwest::blocking::Client,
    config: &LocalStreamConfig,
    method: &str,
    request_headers: &HashMap<String, String>,
) -> Result<reqwest::blocking::Response, reqwest::Error> {
    let mut last_error = None;
    for attempt in 0..3 {
        let mut request = if method == "HEAD" {
            client.head(&config.target_url)
        } else {
            client.get(&config.target_url)
        };
        for (key, value) in config.headers.iter() {
            request = request.header(key, value);
        }
        if let Some(range) = request_headers.get("range") {
            request = request.header("Range", range);
        }
        match request.send() {
            Ok(response) if retryable_status(response.status()) && attempt < 2 => {
                thread::sleep(Duration::from_millis(80 * (attempt + 1) as u64));
            }
            Ok(response) => return Ok(response),
            Err(error) if attempt < 2 => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(80 * (attempt + 1) as u64));
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.expect("retry loop should keep the last error"))
}

pub(crate) fn handle_local_stream(mut stream: TcpStream, config: LocalStreamConfig) {
    let Some(_connection_guard) =
        ActiveConnectionGuard::try_acquire(config.active_connections.clone())
    else {
        write_simple_response(&mut stream, "503 Service Unavailable");
        return;
    };

    let Some(request) = parse_request(&mut stream) else {
        write_simple_response(&mut stream, "400 Bad Request");
        return;
    };
    if !request.path.starts_with(&format!("/stream/{}", config.id)) {
        write_simple_response(&mut stream, "404 Not Found");
        return;
    }
    if request.method != "GET" && request.method != "HEAD" {
        write_simple_response(&mut stream, "405 Method Not Allowed");
        return;
    }

    let mut response =
        match send_upstream_request(&config.client, &config, &request.method, &request.headers) {
            Ok(response) => response,
            Err(_) => {
                write_simple_response(&mut stream, "502 Bad Gateway");
                return;
            }
        };

    let status = response.status();
    let reason = status.canonical_reason().unwrap_or("OK");
    let _ = write!(stream, "HTTP/1.1 {} {}\r\n", status.as_u16(), reason);
    for name in [
        "content-type",
        "content-length",
        "content-range",
        "accept-ranges",
        "etag",
        "last-modified",
    ] {
        if let Some(value) = response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
        {
            let _ = write!(stream, "{}: {}\r\n", name, value);
        }
    }
    let _ = write!(stream, "Connection: close\r\n\r\n");
    if request.method != "HEAD" {
        let _ = std::io::copy(&mut response, &mut stream);
    }
}

pub(crate) fn start_local_stream_server(
    target_url: &str,
    headers_json: &str,
    preferred_port: i32,
) -> Option<String> {
    let headers = serde_json::from_str::<HashMap<String, String>>(headers_json).unwrap_or_default();
    let id = next_local_stream_id();
    let bind_port = preferred_port.clamp(0, u16::MAX as i32) as u16;
    let listener = TcpListener::bind(("0.0.0.0", bind_port)).ok()?;
    let port = listener.local_addr().ok()?.port();
    listener.set_nonblocking(true).ok()?;

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let config = LocalStreamConfig {
        id: id.clone(),
        target_url: target_url.to_string(),
        headers,
        client: Arc::new(build_proxy_client()),
        active_connections: Arc::new(AtomicUsize::new(0)),
        port,
    };
    let thread = thread::spawn(move || {
        while !thread_stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let request_config = config.clone();
                    thread::spawn(move || handle_local_stream(stream, request_config));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });
    local_stream_servers().lock().ok()?.insert(
        id.clone(),
        LocalStreamHandle {
            stop,
            thread: Some(thread),
        },
    );
    serde_json::to_string(&json!({
        "id": id.clone(),
        "url": format!("http://127.0.0.1:{port}/stream/{id}"),
        "port": port
    }))
    .ok()
}

pub(crate) fn stop_local_stream_server(id: &str) -> bool {
    let Some(mut handle) = local_stream_servers()
        .lock()
        .ok()
        .and_then(|mut servers| servers.remove(id))
    else {
        return false;
    };
    handle.stop.store(true, Ordering::Relaxed);
    if let Some(thread) = handle.thread.take() {
        let _ = thread.join();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{ActiveConnectionGuard, MAX_LOCAL_STREAM_CONNECTIONS};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn active_connection_guard_caps_and_releases_slots() {
        let counter = Arc::new(AtomicUsize::new(0));
        let guards: Vec<_> = (0..MAX_LOCAL_STREAM_CONNECTIONS)
            .map(|_| ActiveConnectionGuard::try_acquire(counter.clone()).unwrap())
            .collect();

        assert!(ActiveConnectionGuard::try_acquire(counter.clone()).is_none());
        assert_eq!(counter.load(Ordering::Acquire), MAX_LOCAL_STREAM_CONNECTIONS);

        drop(guards);
        assert_eq!(counter.load(Ordering::Acquire), 0);
        assert!(ActiveConnectionGuard::try_acquire(counter.clone()).is_some());
    }
}
