//! A local HTTP fixture server: fixed routes on 127.0.0.1, no network.
//!
//! Shared by the host's tests and the command line's (`#[path]`); both use every helper.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// What a path answers.
#[derive(Clone, Debug)]
pub struct Route {
    status: u16,
    body: Vec<u8>,
    headers: Vec<(String, String)>,
    authorization: Option<String>,
    delay: Duration,
}

impl Route {
    /// `200` with this body.
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            headers: Vec::new(),
            authorization: None,
            delay: Duration::ZERO,
        }
    }

    /// An empty answer with this status.
    pub fn status(status: u16) -> Self {
        Self {
            status,
            ..Self::ok(Vec::new())
        }
    }

    /// `302` to `location`.
    pub fn redirect(location: &str) -> Self {
        let mut route = Self::status(302);
        route
            .headers
            .push(("Location".to_owned(), location.to_owned()));
        route
    }

    /// Answers only a request with exactly this `Authorization` header; any other gets `404`, as
    /// GitHub answers for a private repository.
    pub fn authorized(mut self, value: &str) -> Self {
        self.authorization = Some(value.to_owned());
        self
    }

    /// Waits this long before answering.
    pub fn delayed(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

/// A request the server answered.
#[derive(Clone, Debug)]
pub struct Request {
    /// The path.
    pub path: String,
    /// The `Authorization` header, if any.
    pub authorization: Option<String>,
}

/// The server. Stops when dropped.
pub struct Server {
    address: SocketAddr,
    routes: Arc<Mutex<HashMap<String, Route>>>,
    log: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Starts on a free port of 127.0.0.1.
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let routes: Arc<Mutex<HashMap<String, Route>>> = Arc::default();
        let log: Arc<Mutex<Vec<Request>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (routes, log, stop) = (routes.clone(), log.clone(), stop.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let (routes, log) = (routes.clone(), log.clone());
                    std::thread::spawn(move || answer(stream, &routes, &log));
                }
            })
        };
        Self {
            address,
            routes,
            log,
            stop,
            thread: Some(thread),
        }
    }

    /// The URL of `path` (which starts with `/`, or is empty for the base).
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    /// Sets what `path` answers.
    pub fn route(&self, path: &str, route: Route) {
        self.routes
            .lock()
            .expect("routes")
            .insert(path.to_owned(), route);
    }

    /// Answers `path` with `200` and `body`.
    pub fn serve(&self, path: &str, body: impl Into<Vec<u8>>) {
        self.route(path, Route::ok(body));
    }

    /// Every request answered so far.
    pub fn requests(&self) -> Vec<Request> {
        self.log.lock().expect("log").clone()
    }

    /// Stops the server and waits for it.
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.halt();
    }
}

fn answer(stream: TcpStream, routes: &Mutex<HashMap<String, Route>>, log: &Mutex<Vec<Request>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
    let mut authorization = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("authorization")
        {
            authorization = Some(value.trim().to_owned());
        }
    }
    log.lock().expect("log").push(Request {
        path: path.clone(),
        authorization: authorization.clone(),
    });

    let route = routes.lock().expect("routes").get(&path).cloned();
    let route = match route {
        Some(route) if route.authorization.is_none() || route.authorization == authorization => {
            route
        }
        _ => Route::status(404),
    };
    std::thread::sleep(route.delay);

    let mut stream = stream;
    let mut head = format!(
        "HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n",
        route.status,
        route.body.len()
    );
    for (name, value) in &route.headers {
        let _ = write!(head, "{name}: {value}\r\n");
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&route.body);
    let _ = stream.flush();
}

/// The JSON GitHub answers for a release: each asset's name, API URL, download URL and digest.
pub fn release_json(assets: &[(&str, &str, &str, Option<&str>)]) -> String {
    let items: Vec<serde_json::Value> = assets
        .iter()
        .map(|(name, api, download, digest)| {
            let mut item = serde_json::json!({
                "name": name,
                "url": api,
                "browser_download_url": download,
            });
            if let Some(digest) = digest {
                item["digest"] = serde_json::Value::String(format!("sha256:{digest}"));
            }
            item
        })
        .collect();
    serde_json::json!({ "assets": items }).to_string()
}

/// The sha256 of `bytes`, as hexadecimal digits.
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}
