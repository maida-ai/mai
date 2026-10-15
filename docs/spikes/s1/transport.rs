//! Loopback-only, bounded CGI bridge for tiny fixtures. Not a product HTTP server.
use super::*;
use std::{
    io::{BufRead, BufReader},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
pub struct Server {
    pub url: String,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    pub fn start(root: PathBuf) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let worker = thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve(stream, &root),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        });
        Self {
            url,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn serve(mut stream: TcpStream, root: &Path) {
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let request: Vec<_> = line.split_whitespace().collect();
    if request.len() != 3 {
        return;
    }
    let method = request[0].to_owned();
    let target = request[1].to_owned();
    let mut headers = std::collections::BTreeMap::new();
    let mut total = line.len();
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        total += line.len();
        assert!(total < 16384);
        if line == "\r\n" || line.is_empty() {
            break;
        }
        let (k, v) = line.split_once(':').unwrap();
        headers.insert(k.to_ascii_lowercase(), v.trim().to_owned());
    }
    // Public synthetic fixture credential; no real account/token is read or logged.
    if headers.get("authorization").map(String::as_str) != Some("Bearer s1-fixture-alice") {
        stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
        return;
    }
    assert!(
        !headers.contains_key("transfer-encoding"),
        "fixture does not implement chunked requests"
    );
    let length: usize = headers
        .get("content-length")
        .map(|s| s.parse().unwrap())
        .unwrap_or(0);
    assert!(length < 2_000_000);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    assert!(!path.contains("..") && !path.contains('%'));
    let mut child = command("git")
        .arg("http-backend")
        .env("GIT_PROJECT_ROOT", root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("PATH_INFO", path)
        .env("QUERY_STRING", query)
        .env("REQUEST_METHOD", method)
        .env(
            "CONTENT_TYPE",
            headers
                .get("content-type")
                .map(String::as_str)
                .unwrap_or(""),
        )
        .env("CONTENT_LENGTH", length.to_string())
        .env("REMOTE_USER", "alice")
        .env("S1_AUTH_AGENT", "alice")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&body).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "CGI failed");
    let boundary = out
        .stdout
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap();
    let header = String::from_utf8(out.stdout[..boundary].to_vec()).unwrap();
    let body = &out.stdout[boundary + 4..];
    let status = header
        .lines()
        .find_map(|l| l.strip_prefix("Status: "))
        .unwrap_or("200 OK");
    let fields = header
        .lines()
        .filter(|l| !l.starts_with("Status:"))
        .collect::<Vec<_>>()
        .join("\r\n");
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\n{fields}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}
