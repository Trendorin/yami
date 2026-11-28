use crate::app::Msg;
use calloop::channel::Sender;
use std::collections::HashMap;
use std::ffi::CStr;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use yami_px::gfx::C;
use yami_px::theme::Theme;

const BUF_SIZE: usize = 64 * 1024;
const MAX_CONNS: usize = 16;
const MAX_SSE: usize = 4;
const MAX_HISTORY: usize = 50;
const MAX_HEADER_BYTES: usize = 32 * 1024;
const MAX_UPLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferStatus {
    Active,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct Transfer {
    pub id: u64,
    pub name: String,
    pub upload: bool,
    pub done: u64,
    pub total: u64,
    pub status: TransferStatus,
}

#[derive(Clone, Debug)]
pub struct SharedFile {
    pub id: u64,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct QrCode {
    pub size: usize,
    pub modules: Vec<bool>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ShareTheme {
    pub bg: String,
    pub panel: String,
    pub surface: String,
    pub surface_hi: String,
    pub border: String,
    pub accent: String,
    pub accent_dim: String,
    pub ember: String,
    pub text: String,
    pub text_dim: String,
    pub text_faint: String,
    pub on_accent: String,
    pub text_hi: String,
    pub shade: String,
    pub ok: String,
    pub warn: String,
}

fn css_hex(c: C) -> String {
    format!("#{:06x}", c.0 & 0x00ff_ffff)
}

impl Default for ShareTheme {
    fn default() -> Self {
        Self::from_theme(&Theme::new(0xc4161c, 0.5, 0.9, 0.0, 0.5, false))
    }
}

impl ShareTheme {
    pub fn from_theme(t: &Theme) -> Self {
        Self {
            bg: css_hex(t.bg),
            panel: css_hex(t.panel),
            surface: css_hex(t.surface),
            surface_hi: css_hex(t.surface_hi),
            border: css_hex(t.border),
            accent: css_hex(t.red),
            accent_dim: css_hex(t.blood),
            ember: css_hex(t.ember),
            text: css_hex(t.text),
            text_dim: css_hex(t.text_dim),
            text_faint: css_hex(t.text_faint),
            on_accent: css_hex(t.on_accent),
            text_hi: css_hex(t.text_hi),
            shade: css_hex(t.shade),
            ok: css_hex(t.ok),
            warn: css_hex(t.warn),
        }
    }

    fn css_vars(&self) -> String {
        format!(
            "--bg:{};--panel:{};--surface:{};--surface-hi:{};--border:{};--accent:{};--accent-dim:{};--ember:{};--text:{};--text-dim:{};--text-faint:{};--on-accent:{};--text-hi:{};--shade:{};--ok:{};--warn:{};",
            self.bg, self.panel, self.surface, self.surface_hi, self.border, self.accent, self.accent_dim, self.ember, self.text, self.text_dim, self.text_faint, self.on_accent, self.text_hi, self.shade, self.ok, self.warn
        )
    }
}

pub struct ShareState {
    pub active: bool,
    pub url: String,
    pub token: String,
    pub qr: Option<QrCode>,
    pub error: String,
    pub fw_error: String,
    pub fw_note: String,
    pub fw_busy: bool,
    pub files: Vec<SharedFile>,
    pub scroll: usize,
    pub active_transfers: Vec<Transfer>,
    pub history: Vec<Transfer>,
    next_file_id: u64,
    shared_list: Arc<Mutex<Vec<SharedFile>>>,
    theme_bus: Arc<(Mutex<(u64, ShareTheme)>, Condvar)>,
    server: Option<ServerHandle>,
}

impl Default for ShareState {
    fn default() -> Self {
        Self {
            active: false,
            url: String::new(),
            token: String::new(),
            qr: None,
            error: String::new(),
            fw_error: String::new(),
            fw_note: String::new(),
            fw_busy: false,
            files: Vec::new(),
            scroll: 0,
            active_transfers: Vec::new(),
            history: Vec::new(),
            next_file_id: 1,
            shared_list: Arc::new(Mutex::new(Vec::new())),
            theme_bus: Arc::new((Mutex::new((1, ShareTheme::default())), Condvar::new())),
            server: None,
        }
    }
}

struct ServerHandle {
    stop: Arc<AtomicBool>,
    wake_wr: RawFd,
    streams: Arc<Mutex<HashMap<u64, TcpStream>>>,
    theme_bus: Arc<(Mutex<(u64, ShareTheme)>, Condvar)>,
    listener_thread: Option<JoinHandle<()>>,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let b = [1u8];
        unsafe { libc::write(self.wake_wr, b.as_ptr().cast(), 1) };
        self.theme_bus.1.notify_all();
        if let Ok(mut map) = self.streams.lock() {
            for (_, s) in map.drain() {
                let _ = s.shutdown(Shutdown::Both);
            }
        }
        if let Some(h) = self.listener_thread.take() {
            let _ = h.join();
        }
        unsafe { libc::close(self.wake_wr) };
    }
}

impl ShareState {
    pub fn set_theme(&self, t: &Theme) {
        if let Ok(mut g) = self.theme_bus.0.lock() {
            g.0 = g.0.wrapping_add(1);
            g.1 = ShareTheme::from_theme(t);
            self.theme_bus.1.notify_all();
        }
    }

    pub fn start(&mut self, port: u16, theme: &Theme, tx: Sender<Msg>) {
        if self.active {
            return;
        }
        self.error.clear();
        self.set_theme(theme);
        let (ip, _) = match detect_lan_iface() {
            Ok(v) => v,
            Err(e) => {
                self.error = e;
                return;
            }
        };
        let listener = match TcpListener::bind((ip, port)) {
            Ok(l) => l,
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                self.error = crate::tf!("порт {} занят", "port {} is in use", "porta {} em uso", port);
                return;
            }
            Err(_) => {
                self.error = crate::tf!("не удалось открыть порт {}", "failed to bind port {}", "falha ao abrir porta {}", port);
                return;
            }
        };
        if listener.set_nonblocking(true).is_err() {
            self.error = crate::tf!("не удалось открыть порт {}", "failed to bind port {}", "falha ao abrir porta {}", port);
            return;
        }
        let mut fds = [0i32; 2];
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) } != 0 {
            self.error = crate::tf!("не удалось открыть порт {}", "failed to bind port {}", "falha ao abrir porta {}", port);
            return;
        }
        let (wake_rd, wake_wr) = (fds[0], fds[1]);

        let token = gen_token();
        let url = format!("http://{ip}:{port}/?t={token}");
        let qr = encode_qr(url.as_bytes()).ok();

        self.sync_files();
        let stop = Arc::new(AtomicBool::new(false));
        let streams: Arc<Mutex<HashMap<u64, TcpStream>>> = Arc::new(Mutex::new(HashMap::new()));
        let active_conns = Arc::new(AtomicUsize::new(0));
        let sse_conns = Arc::new(AtomicUsize::new(0));
        let next_id = Arc::new(AtomicU64::new(1));

        let ctx = Arc::new(ServerCtx {
            token: token.clone(),
            files: self.shared_list.clone(),
            upload_dir: downloads_dir(),
            stop: stop.clone(),
            streams: streams.clone(),
            theme_bus: self.theme_bus.clone(),
            active_conns,
            sse_conns,
            next_id,
            dl_groups: Mutex::new(HashMap::new()),
            tx,
        });

        let handle = std::thread::Builder::new()
            .name("yami-share".into())
            .spawn(move || accept_loop(listener, wake_rd, ctx))
            .ok();

        if handle.is_none() {
            unsafe {
                libc::close(wake_rd);
                libc::close(wake_wr);
            }
            self.error = crate::i18n::tr("не удалось запустить сервер").to_string();
            return;
        }

        self.active = true;
        self.url = url;
        self.token = token;
        self.qr = qr;
        self.server = Some(ServerHandle { stop, wake_wr, streams, theme_bus: self.theme_bus.clone(), listener_thread: handle });
    }

    pub fn stop(&mut self) {
        self.server.take();
        self.active = false;
        self.url.clear();
        self.token.clear();
        self.qr = None;
        for mut t in std::mem::take(&mut self.active_transfers) {
            t.status = TransferStatus::Cancelled;
            self.push_history(t);
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    fn push_history(&mut self, t: Transfer) {
        self.history.insert(0, t);
        self.history.truncate(MAX_HISTORY);
    }

    pub fn add_files(&mut self, paths: Vec<PathBuf>) {
        for p in paths {
            if p.is_file() && !self.files.iter().any(|f| f.path == p) {
                let id = self.next_file_id;
                self.next_file_id += 1;
                self.files.push(SharedFile { id, path: p });
            }
        }
        self.sync_files();
    }

    pub fn remove_file(&mut self, idx: usize) {
        if idx < self.files.len() {
            self.files.remove(idx);
            self.sync_files();
        }
    }

    pub fn clear_files(&mut self) {
        self.files.clear();
        self.scroll = 0;
        self.sync_files();
    }

    fn sync_files(&self) {
        if let Ok(mut g) = self.shared_list.lock() {
            *g = self.files.clone();
        }
    }

    pub fn apply_event(&mut self, t: Transfer) {
        if t.status == TransferStatus::Active {
            if let Some(ex) = self.active_transfers.iter_mut().find(|x| x.id == t.id) {
                *ex = t;
            } else if !self.history.iter().any(|x| x.id == t.id && x.status == TransferStatus::Done) {
                self.active_transfers.insert(0, t);
            }
        } else {
            self.active_transfers.retain(|x| x.id != t.id);
            if let Some(ex) = self.history.iter_mut().find(|x| x.id == t.id) {
                if !(ex.status == TransferStatus::Done && t.status == TransferStatus::Failed) {
                    *ex = t;
                }
            } else {
                self.push_history(t);
            }
        }
    }
}

struct DlGroup {
    id: u64,
    last_seen: Instant,
    active_reqs: usize,
    any_done: bool,
}

struct ServerCtx {
    token: String,
    files: Arc<Mutex<Vec<SharedFile>>>,
    upload_dir: PathBuf,
    stop: Arc<AtomicBool>,
    streams: Arc<Mutex<HashMap<u64, TcpStream>>>,
    theme_bus: Arc<(Mutex<(u64, ShareTheme)>, Condvar)>,
    active_conns: Arc<AtomicUsize>,
    sse_conns: Arc<AtomicUsize>,
    next_id: Arc<AtomicU64>,
    dl_groups: Mutex<HashMap<String, DlGroup>>,
    tx: Sender<Msg>,
}

impl ServerCtx {
    fn begin_dl(&self, key: &str, auto_window: bool) -> (u64, bool) {
        let now = Instant::now();
        let Ok(mut map) = self.dl_groups.lock() else {
            return (self.next_id.fetch_add(1, Ordering::Relaxed), true);
        };
        map.retain(|_, g| g.active_reqs > 0 || now.duration_since(g.last_seen) < Duration::from_secs(60));
        let reuse = map.get_mut(key).filter(|g| !auto_window || g.active_reqs > 0 || now.duration_since(g.last_seen) < Duration::from_secs(10));
        if let Some(g) = reuse {
            g.active_reqs += 1;
            g.last_seen = now;
            return (g.id, !g.any_done);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        map.insert(key.to_string(), DlGroup { id, last_seen: now, active_reqs: 1, any_done: false });
        (id, true)
    }

    fn finish_dl(&self, key: &str, id: u64, name: &str, done: u64, total: u64, status: TransferStatus) {
        let should_emit = if let Ok(mut map) = self.dl_groups.lock() {
            if let Some(g) = map.get_mut(key).filter(|g| g.id == id) {
                g.active_reqs = g.active_reqs.saturating_sub(1);
                g.last_seen = Instant::now();
                match status {
                    TransferStatus::Done => {
                        g.any_done = true;
                        true
                    }
                    TransferStatus::Cancelled => true,
                    TransferStatus::Failed => !g.any_done && g.active_reqs == 0,
                    TransferStatus::Active => true,
                }
            } else {
                true
            }
        } else {
            true
        };
        if should_emit {
            emit_transfer(&self.tx, id, name, false, done, total, status);
        }
    }
}

fn accept_loop(listener: TcpListener, wake_rd: RawFd, ctx: Arc<ServerCtx>) {
    let lfd = listener.as_raw_fd();
    let mut workers: Vec<JoinHandle<()>> = Vec::new();
    while !ctx.stop.load(Ordering::Relaxed) {
        workers.retain(|h| !h.is_finished());
        let mut pfds = [
            libc::pollfd { fd: lfd, events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: wake_rd, events: libc::POLLIN, revents: 0 },
        ];
        let rc = unsafe { libc::poll(pfds.as_mut_ptr(), 2, -1) };
        if rc < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if pfds[1].revents != 0 || ctx.stop.load(Ordering::Relaxed) {
            break;
        }
        if pfds[0].revents & libc::POLLIN != 0 {
            loop {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if ctx.active_conns.load(Ordering::Relaxed) >= MAX_CONNS {
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        }
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
                        let conn_id = ctx.next_id.fetch_add(1, Ordering::Relaxed);
                        if let (Ok(cloned), Ok(mut map)) = (stream.try_clone(), ctx.streams.lock()) {
                            map.insert(conn_id, cloned);
                        }
                        ctx.active_conns.fetch_add(1, Ordering::Relaxed);
                        let c = ctx.clone();
                        if let Ok(h) = std::thread::Builder::new().name("share-worker".into()).spawn(move || {
                            handle_conn(stream, &c);
                            if let Ok(mut map) = c.streams.lock() {
                                map.remove(&conn_id);
                            }
                            c.active_conns.fetch_sub(1, Ordering::Relaxed);
                        }) {
                            workers.push(h);
                        } else {
                            if let Ok(mut map) = ctx.streams.lock() {
                                map.remove(&conn_id);
                            }
                            ctx.active_conns.fetch_sub(1, Ordering::Relaxed);
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => break,
                }
            }
        }
    }
    drop(listener);
    unsafe { libc::close(wake_rd) };
    for h in workers {
        let _ = h.join();
    }
}

fn handle_conn(mut stream: TcpStream, ctx: &ServerCtx) {
    let mut reader = BufReader::with_capacity(BUF_SIZE, match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let Ok(req) = read_request_head(&mut reader) else {
        let _ = send_simple(&mut stream, "400 Bad Request", "Bad Request");
        return;
    };

    let (path, query) = split_target(&req.target);
    let params = parse_query(query);
    let token = params
        .get("t")
        .map(String::as_str)
        .or_else(|| req.headers.get("x-share-token").map(String::as_str))
        .or_else(|| req.headers.get("authorization").and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer "))))
        .unwrap_or("");

    if !ct_eq(token.as_bytes(), ctx.token.as_bytes()) {
        let _ = send_simple(&mut stream, "401 Unauthorized", "Unauthorized");
        return;
    }

    match (req.method.as_str(), path) {
        ("GET", "/") => {
            let vars = ctx.theme_bus.0.lock().map(|g| g.1.css_vars()).unwrap_or_default();
            let body = MOBILE_HTML.replace("__TOKEN__", &ctx.token).replace("__THEME_VARS__", &vars);
            let _ = send_bytes(&mut stream, "200 OK", "text/html; charset=utf-8", body.as_bytes());
        }
        ("GET", "/api/events") => {
            ctx.active_conns.fetch_sub(1, Ordering::Relaxed);
            serve_sse(&mut stream, ctx);
            ctx.active_conns.fetch_add(1, Ordering::Relaxed);
        }
        ("GET", "/api/files") => {
            let files = ctx.files.lock().map(|g| g.clone()).unwrap_or_default();
            let items: Vec<serde_json::Value> = files
                .iter()
                .filter_map(|f| {
                    let meta = std::fs::metadata(&f.path).ok()?;
                    let name = f.path.file_name()?.to_string_lossy().to_string();
                    Some(serde_json::json!({ "id": f.id, "name": name, "size": meta.len() }))
                })
                .collect();
            let json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".into());
            let _ = send_bytes(&mut stream, "200 OK", "application/json; charset=utf-8", json.as_bytes());
        }
        ("GET", p) | ("HEAD", p) if p.starts_with("/dl/") => {
            let id_str = p.strip_prefix("/dl/").unwrap_or("").split('/').next().unwrap_or("");
            let Ok(fid) = id_str.parse::<u64>() else {
                let _ = send_simple(&mut stream, "404 Not Found", "Not Found");
                return;
            };
            let path = ctx.files.lock().ok().and_then(|g| g.iter().find(|f| f.id == fid).map(|f| f.path.clone()));
            let Some(path) = path else {
                let _ = send_simple(&mut stream, "404 Not Found", "Not Found");
                return;
            };
            let (dl_key, auto_win) = match params.get("dl") {
                Some(dl) => (format!("dl:{fid}:{dl}"), false),
                None => {
                    let ip = stream.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();
                    (format!("auto:{fid}:{ip}"), true)
                }
            };
            serve_file(&mut stream, &path, req.method == "HEAD", req.headers.get("range").map(String::as_str), &dl_key, auto_win, ctx);
        }
        ("GET", "/dl-all") | ("HEAD", "/dl-all") => {
            let files = ctx.files.lock().map(|g| g.clone()).unwrap_or_default();
            let selected: Vec<PathBuf> = if let Some(ids) = params.get("ids") {
                let want: Vec<u64> = ids.split(',').filter_map(|s| s.parse::<u64>().ok()).collect();
                files.into_iter().filter(|f| want.contains(&f.id)).map(|f| f.path).collect()
            } else {
                files.into_iter().map(|f| f.path).collect()
            };
            if selected.is_empty() {
                let _ = send_simple(&mut stream, "404 Not Found", "No files selected");
                return;
            }
            if req.method == "HEAD" {
                let head = "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Disposition: attachment; filename=\"yami-share.zip\"\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(head.as_bytes());
                return;
            }
            serve_zip(&mut stream, &selected, ctx);
        }
        ("POST", "/upload") => {
            handle_upload(&mut reader, &mut stream, &req, &params, ctx);
        }
        _ => {
            let _ = send_simple(&mut stream, "404 Not Found", "Not Found");
        }
    }
}

struct ReqHead {
    method: String,
    target: String,
    headers: HashMap<String, String>,
}

fn read_request_head(reader: &mut BufReader<TcpStream>) -> io::Result<ReqHead> {
    let mut total = 0usize;
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "eof"));
    }
    total += n;
    let mut parts = line.trim_end_matches(&['\r', '\n'][..]).split_whitespace();
    let method = parts.next().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "method"))?.to_string();
    let target = parts.next().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "target"))?.to_string();

    let mut headers = HashMap::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        total += n;
        if total > MAX_HEADER_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "headers too large"));
        }
        let trimmed = line.trim_end_matches(&['\r', '\n'][..]);
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    Ok(ReqHead { method, target, headers })
}

fn send_simple(stream: &mut TcpStream, status: &str, msg: &str) -> io::Result<()> {
    send_bytes(stream, status, "text/plain; charset=utf-8", msg.as_bytes())
}

fn send_bytes(stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn parse_range(hdr: &str, total: u64) -> Option<(u64, u64)> {
    let spec = hdr.trim().strip_prefix("bytes=")?;
    if spec.contains(',') || total == 0 {
        return None;
    }
    let (a, b) = spec.split_once('-')?;
    if a.is_empty() {
        let suffix = b.parse::<u64>().ok().filter(|&n| n > 0)?;
        let start = total.saturating_sub(suffix);
        return (start < total).then_some((start, total - 1));
    }
    let start = a.parse::<u64>().ok()?;
    if start >= total {
        return None;
    }
    let end = if b.is_empty() { total - 1 } else { b.parse::<u64>().ok()?.min(total - 1) };
    (start <= end).then_some((start, end))
}

fn serve_file(stream: &mut TcpStream, path: &Path, head_only: bool, range_hdr: Option<&str>, dl_key: &str, auto_win: bool, ctx: &ServerCtx) {
    let Ok(mut file) = File::open(path) else {
        let _ = send_simple(stream, "404 Not Found", "Not Found");
        return;
    };
    let Ok(meta) = file.metadata() else {
        let _ = send_simple(stream, "404 Not Found", "Not Found");
        return;
    };
    let total = meta.len();
    let (status, start, len) = match range_hdr.and_then(|h| parse_range(h, total)) {
        Some((s, e)) => ("206 Partial Content", s, e - s + 1),
        None => ("200 OK", 0, total),
    };
    if start > 0 && io::Seek::seek(&mut file, io::SeekFrom::Start(start)).is_err() {
        let _ = send_simple(stream, "500 Internal Server Error", "Seek error");
        return;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let safe_hdr = sanitize_header_filename(&name);
    let range_line = if status == "206 Partial Content" {
        format!("Content-Range: bytes {start}-{}/{total}\r\n", start + len - 1)
    } else {
        String::new()
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\nContent-Length: {len}\r\nAccept-Ranges: bytes\r\n{range_line}Content-Disposition: attachment; filename=\"{safe_hdr}\"\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(head.as_bytes()).is_err() || head_only {
        return;
    }

    // Small partial probes (e.g. player/download-manager header probes) should not create history entries.
    let track = !(status == "206 Partial Content" && len < total && len <= 4096);
    if len == 0 {
        if track {
            let (tid, _) = ctx.begin_dl(dl_key, auto_win);
            ctx.finish_dl(dl_key, tid, &name, 0, 0, TransferStatus::Done);
        }
        return;
    }

    let mut id: Option<u64> = None;
    let mut buf = vec![0u8; BUF_SIZE];
    let mut done = 0u64;
    let mut last_emit = Instant::now();
    while done < len {
        if ctx.stop.load(Ordering::Relaxed) {
            if let Some(tid) = id {
                ctx.finish_dl(dl_key, tid, &name, done, total, TransferStatus::Cancelled);
            }
            return;
        }
        let to_read = ((len - done) as usize).min(BUF_SIZE);
        match file.read(&mut buf[..to_read]) {
            Ok(0) => break,
            Ok(n) => {
                if stream.write_all(&buf[..n]).is_err() {
                    if let Some(tid) = id {
                        ctx.finish_dl(dl_key, tid, &name, done, total, TransferStatus::Failed);
                    }
                    return;
                }
                done += n as u64;
                if track && id.is_none() {
                    let (tid, emit_active) = ctx.begin_dl(dl_key, auto_win);
                    id = Some(tid);
                    if emit_active {
                        emit_transfer(&ctx.tx, tid, &name, false, start + done, total, TransferStatus::Active);
                    }
                    last_emit = Instant::now();
                } else if let Some(tid) = id {
                    if last_emit.elapsed() >= PROGRESS_INTERVAL {
                        emit_transfer(&ctx.tx, tid, &name, false, start + done, total, TransferStatus::Active);
                        last_emit = Instant::now();
                    }
                }
            }
            Err(_) => {
                if track {
                    let tid = id.unwrap_or_else(|| ctx.begin_dl(dl_key, auto_win).0);
                    ctx.finish_dl(dl_key, tid, &name, done, total, TransferStatus::Failed);
                }
                return;
            }
        }
    }
    let _ = stream.flush();
    if let Some(tid) = id {
        let final_done = if status == "206 Partial Content" { total } else { done };
        ctx.finish_dl(dl_key, tid, &name, final_done, total, TransferStatus::Done);
    }
}

struct ChunkedWriter<'a> {
    inner: &'a mut TcpStream,
}

impl<'a> Write for ChunkedWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        write!(self.inner, "{:x}\r\n", buf.len())?;
        self.inner.write_all(buf)?;
        self.inner.write_all(b"\r\n")?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<'a> ChunkedWriter<'a> {
    fn finish(self) -> io::Result<()> {
        self.inner.write_all(b"0\r\n\r\n")?;
        self.inner.flush()
    }
}

/// Streams uncompressed STORE ZIP (method 0) using crc32fast without temporary files.
fn serve_zip(stream: &mut TcpStream, files: &[PathBuf], ctx: &ServerCtx) {
    let head = "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Disposition: attachment; filename=\"yami-share.zip\"\r\nTransfer-Encoding: chunked\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }

    let id = ctx.next_id.fetch_add(1, Ordering::Relaxed);
    let total: u64 = files
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()).filter(|&l| l <= u32::MAX as u64))
        .sum();
    emit_transfer(&ctx.tx, id, "yami-share.zip", false, 0, total, TransferStatus::Active);

    let mut cw = ChunkedWriter { inner: stream };
    let mut offset = 0u64;
    let mut done = 0u64;
    let mut last_emit = Instant::now();
    let mut buf = vec![0u8; BUF_SIZE];
    let mut cd: Vec<(Vec<u8>, u32, u32, u32)> = Vec::new();

    for path in files {
        let Ok(mut f) = File::open(path) else { continue };
        let Ok(meta) = f.metadata() else { continue };
        let size = meta.len();
        if size > u32::MAX as u64 {
            continue;
        }
        let name = sanitize_filename(&path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into()));
        let name_bytes = name.as_bytes();

        // Compute CRC32 in streaming pass before writing local header (keeps ZIP64-free compatibility).
        let mut hasher = crc32fast::Hasher::new();
        loop {
            if ctx.stop.load(Ordering::Relaxed) {
                emit_transfer(&ctx.tx, id, "yami-share.zip", false, done, total, TransferStatus::Cancelled);
                return;
            }
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => hasher.update(&buf[..n]),
                Err(_) => {
                    emit_transfer(&ctx.tx, id, "yami-share.zip", false, done, total, TransferStatus::Failed);
                    return;
                }
            }
        }
        let crc = hasher.finalize();
        if io::Seek::seek(&mut f, io::SeekFrom::Start(0)).is_err() {
            continue;
        }

        let local_off = offset as u32;
        let mut lfh = Vec::with_capacity(30 + name_bytes.len());
        lfh.extend_from_slice(&0x04034b50u32.to_le_bytes());
        lfh.extend_from_slice(&20u16.to_le_bytes());
        lfh.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8
        lfh.extend_from_slice(&0u16.to_le_bytes()); // STORE
        lfh.extend_from_slice(&0u16.to_le_bytes());
        lfh.extend_from_slice(&0u16.to_le_bytes());
        lfh.extend_from_slice(&crc.to_le_bytes());
        lfh.extend_from_slice(&(size as u32).to_le_bytes());
        lfh.extend_from_slice(&(size as u32).to_le_bytes());
        lfh.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        lfh.extend_from_slice(&0u16.to_le_bytes());
        lfh.extend_from_slice(name_bytes);
        if cw.write_all(&lfh).is_err() {
            emit_transfer(&ctx.tx, id, "yami-share.zip", false, done, total, TransferStatus::Failed);
            return;
        }
        offset += lfh.len() as u64;

        loop {
            if ctx.stop.load(Ordering::Relaxed) {
                emit_transfer(&ctx.tx, id, "yami-share.zip", false, done, total, TransferStatus::Cancelled);
                return;
            }
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if cw.write_all(&buf[..n]).is_err() {
                        emit_transfer(&ctx.tx, id, "yami-share.zip", false, done, total, TransferStatus::Failed);
                        return;
                    }
                    offset += n as u64;
                    done += n as u64;
                    if last_emit.elapsed() >= PROGRESS_INTERVAL {
                        emit_transfer(&ctx.tx, id, "yami-share.zip", false, done, total, TransferStatus::Active);
                        last_emit = Instant::now();
                    }
                }
                Err(_) => {
                    emit_transfer(&ctx.tx, id, "yami-share.zip", false, done, total, TransferStatus::Failed);
                    return;
                }
            }
        }
        cd.push((name_bytes.to_vec(), crc, size as u32, local_off));
    }

    let cd_start = offset as u32;
    let mut cd_size = 0u32;
    for (name_bytes, crc, size, local_off) in &cd {
        let mut cdf = Vec::with_capacity(46 + name_bytes.len());
        cdf.extend_from_slice(&0x02014b50u32.to_le_bytes());
        cdf.extend_from_slice(&20u16.to_le_bytes());
        cdf.extend_from_slice(&20u16.to_le_bytes());
        cdf.extend_from_slice(&0x0800u16.to_le_bytes());
        cdf.extend_from_slice(&0u16.to_le_bytes());
        cdf.extend_from_slice(&0u16.to_le_bytes());
        cdf.extend_from_slice(&0u16.to_le_bytes());
        cdf.extend_from_slice(&crc.to_le_bytes());
        cdf.extend_from_slice(&size.to_le_bytes());
        cdf.extend_from_slice(&size.to_le_bytes());
        cdf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        cdf.extend_from_slice(&0u16.to_le_bytes());
        cdf.extend_from_slice(&0u16.to_le_bytes());
        cdf.extend_from_slice(&0u16.to_le_bytes());
        cdf.extend_from_slice(&0u16.to_le_bytes());
        cdf.extend_from_slice(&0u32.to_le_bytes());
        cdf.extend_from_slice(&local_off.to_le_bytes());
        cdf.extend_from_slice(name_bytes);
        if cw.write_all(&cdf).is_err() {
            emit_transfer(&ctx.tx, id, "yami-share.zip", false, done, total, TransferStatus::Failed);
            return;
        }
        cd_size += cdf.len() as u32;
    }
    let mut eocd = Vec::with_capacity(22);
    eocd.extend_from_slice(&0x06054b50u32.to_le_bytes());
    eocd.extend_from_slice(&0u16.to_le_bytes());
    eocd.extend_from_slice(&0u16.to_le_bytes());
    eocd.extend_from_slice(&(cd.len() as u16).to_le_bytes());
    eocd.extend_from_slice(&(cd.len() as u16).to_le_bytes());
    eocd.extend_from_slice(&cd_size.to_le_bytes());
    eocd.extend_from_slice(&cd_start.to_le_bytes());
    eocd.extend_from_slice(&0u16.to_le_bytes());
    if cw.write_all(&eocd).is_err() || cw.finish().is_err() {
        emit_transfer(&ctx.tx, id, "yami-share.zip", false, done, total, TransferStatus::Failed);
        return;
    }
    emit_transfer(&ctx.tx, id, "yami-share.zip", false, done, total, TransferStatus::Done);
}

fn handle_upload(
    reader: &mut BufReader<TcpStream>,
    stream: &mut TcpStream,
    req: &ReqHead,
    params: &HashMap<String, String>,
    ctx: &ServerCtx,
) {
    let Some(len_str) = req.headers.get("content-length") else {
        let _ = send_simple(stream, "411 Length Required", "Content-Length required");
        return;
    };
    let Ok(total) = len_str.parse::<u64>() else {
        let _ = send_simple(stream, "400 Bad Request", "Invalid Content-Length");
        return;
    };
    if total > MAX_UPLOAD_BYTES {
        let _ = send_simple(stream, "413 Payload Too Large", "File exceeds 2 GiB limit");
        return;
    }

    let raw_name = params
        .get("name")
        .map(String::as_str)
        .or_else(|| req.headers.get("x-filename").map(String::as_str))
        .unwrap_or("upload");
    let safe_name = sanitize_filename(raw_name);
    let Ok((dest_path, file)) = create_unique_file(&ctx.upload_dir, &safe_name) else {
        let _ = send_simple(stream, "500 Internal Server Error", "Cannot create destination file");
        return;
    };

    let saved_name = dest_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(safe_name);
    let id = ctx.next_id.fetch_add(1, Ordering::Relaxed);
    emit_transfer(&ctx.tx, id, &saved_name, true, 0, total, TransferStatus::Active);

    let mut writer = BufWriter::with_capacity(BUF_SIZE, file);
    let mut buf = vec![0u8; BUF_SIZE];
    let mut done = 0u64;
    let mut last_emit = Instant::now();

    let res: Result<(), (TransferStatus, Option<(&str, &str)>)> = (|| {
        while done < total {
            if ctx.stop.load(Ordering::Relaxed) {
                return Err((TransferStatus::Cancelled, None));
            }
            let to_read = ((total - done) as usize).min(BUF_SIZE);
            match reader.read(&mut buf[..to_read]) {
                Ok(0) => return Err((TransferStatus::Failed, Some(("400 Bad Request", "Unexpected EOF")))),
                Ok(n) => {
                    if writer.write_all(&buf[..n]).is_err() {
                        return Err((TransferStatus::Failed, Some(("500 Internal Server Error", "Write error"))));
                    }
                    done += n as u64;
                    if last_emit.elapsed() >= PROGRESS_INTERVAL {
                        emit_transfer(&ctx.tx, id, &saved_name, true, done, total, TransferStatus::Active);
                        last_emit = Instant::now();
                    }
                }
                Err(_) => return Err((TransferStatus::Failed, None)),
            }
        }
        writer.flush().map_err(|_| (TransferStatus::Failed, Some(("500 Internal Server Error", "Flush error"))))
    })();

    if let Err((status, http_err)) = res {
        drop(writer);
        let _ = std::fs::remove_file(&dest_path);
        emit_transfer(&ctx.tx, id, &saved_name, true, done, total, status);
        if let Some((code, msg)) = http_err {
            let _ = send_simple(stream, code, msg);
        }
        return;
    }

    emit_transfer(&ctx.tx, id, &saved_name, true, done, total, TransferStatus::Done);
    let resp = serde_json::json!({ "ok": true, "name": saved_name }).to_string();
    let _ = send_bytes(stream, "200 OK", "application/json; charset=utf-8", resp.as_bytes());
}

fn emit_transfer(tx: &Sender<Msg>, id: u64, name: &str, upload: bool, done: u64, total: u64, status: TransferStatus) {
    let _ = tx.send(Msg::ShareUpdate(Transfer {
        id,
        name: name.to_string(),
        upload,
        done,
        total,
        status,
    }));
}

pub fn downloads_dir() -> PathBuf {
    yami_px::paths::xdg_dir("DOWNLOAD")
        .unwrap_or_else(|| yami_px::paths::home().join("Downloads"))
        .join("yami-share")
}

pub fn sanitize_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .filter(|&c| !c.is_control() && c != '/' && c != '\\' && c != '\0')
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return "file".into();
    }
    if trimmed.len() <= 200 {
        return trimmed.to_string();
    }
    let mut end = 200;
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    trimmed[..end].to_string()
}

fn sanitize_header_filename(name: &str) -> String {
    name.chars().filter(|&c| !c.is_control() && c != '"' && c != '\\').collect()
}

pub fn create_unique_file(dir: &Path, name: &str) -> io::Result<(PathBuf, File)> {
    std::fs::create_dir_all(dir)?;
    let canon_dir = std::fs::canonicalize(dir)?;
    let safe = sanitize_filename(name);
    let (stem, ext) = match safe.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && !e.is_empty() => (s, Some(e)),
        _ => (safe.as_str(), None),
    };
    for i in 0u32..10_000 {
        let candidate_name = if i == 0 {
            safe.clone()
        } else if let Some(e) = ext {
            format!("{stem} ({i}).{e}")
        } else {
            format!("{stem} ({i})")
        };
        let path = canon_dir.join(&candidate_name);
        if path.parent() != Some(canon_dir.as_path()) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "path escape"));
        }
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "too many collisions"))
}

pub fn gen_token() -> String {
    let mut buf = [0u8; 16];
    let mut off = 0;
    while off < buf.len() {
        let n = unsafe { libc::getrandom(buf[off..].as_mut_ptr().cast(), buf.len() - off, 0) };
        if n > 0 {
            off += n as usize;
        }
    }
    let mut out = String::with_capacity(32);
    for b in buf {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn split_target(t: &str) -> (&str, &str) {
    t.split_once('?').unwrap_or((t, ""))
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for pair in q.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        m.insert(percent_decode(k), percent_decode(v));
    }
    m
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_valid_lan_ip(ip: Ipv4Addr) -> bool {
    !ip.is_loopback() && !ip.is_unspecified() && !ip.is_link_local() && !ip.is_multicast()
}

fn detect_lan_iface() -> Result<(Ipv4Addr, Option<String>), String> {
    let mut probed = None;
    if let Ok(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
        if sock.connect(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 80)).is_ok() {
            if let Ok(SocketAddr::V4(addr)) = sock.local_addr() {
                if is_valid_lan_ip(*addr.ip()) {
                    probed = Some(*addr.ip());
                }
            }
        }
    }
    let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut ifap) } == 0 && !ifap.is_null() {
        let mut fallback: Option<(Ipv4Addr, Option<String>)> = None;
        let mut priv_hit: Option<(Ipv4Addr, Option<String>)> = None;
        let mut cur = ifap;
        while !cur.is_null() {
            let ifa = unsafe { &*cur };
            let up_running = (ifa.ifa_flags as i32 & (libc::IFF_UP | libc::IFF_RUNNING)) == (libc::IFF_UP | libc::IFF_RUNNING);
            let loopback = (ifa.ifa_flags as i32 & libc::IFF_LOOPBACK) != 0;
            if up_running && !loopback && !ifa.ifa_addr.is_null() {
                let family = unsafe { (*ifa.ifa_addr).sa_family as i32 };
                if family == libc::AF_INET {
                    let sin = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in) };
                    let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                    let name = if !ifa.ifa_name.is_null() {
                        Some(unsafe { CStr::from_ptr(ifa.ifa_name) }.to_string_lossy().into_owned())
                    } else {
                        None
                    };
                    if probed == Some(ip) {
                        unsafe { libc::freeifaddrs(ifap) };
                        return Ok((ip, name));
                    }
                    if is_valid_lan_ip(ip) {
                        if ip.is_private() && priv_hit.is_none() {
                            priv_hit = Some((ip, name));
                        } else if fallback.is_none() {
                            fallback = Some((ip, name));
                        }
                    }
                }
            }
            cur = ifa.ifa_next;
        }
        unsafe { libc::freeifaddrs(ifap) };
        if let Some(ip) = probed {
            return Ok((ip, None));
        }
        if let Some(hit) = priv_hit.or(fallback) {
            return Ok(hit);
        }
    } else if let Some(ip) = probed {
        return Ok((ip, None));
    }
    Err(crate::i18n::tr("локальная сеть не найдена").to_string())
}

fn valid_iface(name: &str) -> bool {
    !name.is_empty() && name.len() <= 32 && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn build_ufw_script(port: u16, iface: Option<&str>) -> String {
    let allow = match iface.filter(|s| valid_iface(s)) {
        Some(i) => format!("ufw allow in on {i} to any port {port} proto tcp comment 'yami-share'"),
        None => format!("ufw allow {port}/tcp comment 'yami-share'"),
    };
    format!(
        "export LC_ALL=C; \
         if ! ufw status 2>/dev/null | grep -q 'Status: active'; then echo inactive; exit 0; fi; \
         for n in $(ufw status numbered 2>/dev/null | sed -n 's/^\\[ *\\([0-9]\\+\\)\\].*# yami-share.*/\\1/p' | sort -rn); do \
           ufw --force delete \"$n\" >/dev/null || exit 1; \
         done; \
         {allow} >/dev/null"
    )
}

pub fn apply_ufw_rule(port: u16, tx: Sender<Msg>) {
    std::thread::Builder::new()
        .name("share-ufw".into())
        .spawn(move || {
            if !crate::proc::have("ufw") {
                let _ = tx.send(Msg::ShareFw(Ok(String::new())));
                return;
            }
            if !crate::proc::have("pkexec") {
                let _ = tx.send(Msg::ShareFw(Err(crate::i18n::tr("pkexec не найден").to_string())));
                return;
            }
            let iface = detect_lan_iface().ok().and_then(|(_, i)| i);
            let script = build_ufw_script(port, iface.as_deref());
            let out = std::process::Command::new("pkexec").args(["sh", "-c", &script]).output();
            let res = match out {
                Ok(o) if o.status.success() => {
                    let s = String::from_utf8_lossy(&o.stdout);
                    if s.trim() == "inactive" {
                        Ok(crate::i18n::tr("ufw выключен").to_string())
                    } else {
                        Ok(crate::tf!("правило ufw: {}", "ufw rule: {}", "regra ufw: {}", port))
                    }
                }
                Ok(o) if matches!(o.status.code(), Some(126 | 127)) => Err(crate::i18n::tr("авторизация отменена").to_string()),
                _ => Err(crate::i18n::tr("ошибка ufw").to_string()),
            };
            let _ = tx.send(Msg::ShareFw(res));
        })
        .ok();
}

fn serve_sse(stream: &mut TcpStream, ctx: &ServerCtx) {
    if ctx.sse_conns.fetch_add(1, Ordering::Relaxed) >= MAX_SSE {
        ctx.sse_conns.fetch_sub(1, Ordering::Relaxed);
        let _ = send_simple(stream, "503 Service Unavailable", "Too many listeners");
        return;
    }
    let mut last_ver = ctx.theme_bus.0.lock().map(|g| g.0).unwrap_or(0);
    let _ = stream.set_read_timeout(None);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
    if stream.write_all(head.as_bytes()).is_err() || stream.flush().is_err() {
        ctx.sse_conns.fetch_sub(1, Ordering::Relaxed);
        return;
    }
    loop {
        if ctx.stop.load(Ordering::Relaxed) {
            break;
        }
        let next_theme = {
            let Ok(guard) = ctx.theme_bus.0.lock() else { break };
            let (guard, timed_out) = match ctx.theme_bus.1.wait_timeout_while(guard, Duration::from_secs(15), |g| g.0 == last_ver && !ctx.stop.load(Ordering::Relaxed)) {
                Ok((g, to)) => (g, to.timed_out()),
                Err(_) => break,
            };
            if ctx.stop.load(Ordering::Relaxed) {
                break;
            }
            if guard.0 != last_ver {
                last_ver = guard.0;
                Some(guard.1.clone())
            } else if timed_out {
                None
            } else {
                continue;
            }
        };
        let msg = match next_theme {
            Some(th) => format!("event: theme\ndata: {}\n\n", serde_json::to_string(&th).unwrap_or_default()),
            None => ": ping\n\n".to_string(),
        };
        if stream.write_all(msg.as_bytes()).is_err() || stream.flush().is_err() {
            break;
        }
    }
    ctx.sse_conns.fetch_sub(1, Ordering::Relaxed);
}

// ── Minimal QR Code Encoder (Byte mode, ECC Level L, Versions 1..=8) ───────

const QR_CAP_L: [(usize, usize, usize); 8] = [
    (1, 17, 7),
    (2, 32, 10),
    (3, 53, 15),
    (4, 78, 20),
    (5, 106, 26),
    (6, 134, 18),
    (7, 154, 20),
    (8, 192, 24),
];

const QR_TOTAL_CW: [usize; 8] = [26, 44, 70, 100, 134, 172, 196, 242];
const QR_BLOCKS_L: [(usize, usize, usize, usize); 8] = [
    (1, 19, 0, 0),
    (1, 34, 0, 0),
    (1, 55, 0, 0),
    (1, 80, 0, 0),
    (1, 108, 0, 0),
    (2, 68, 0, 0),
    (2, 78, 0, 0),
    (2, 97, 0, 0),
];
const QR_ALIGN: [&[usize]; 8] = [&[], &[6, 18], &[6, 22], &[6, 26], &[6, 30], &[6, 34], &[6, 22, 38], &[6, 24, 42]];

pub fn encode_qr(data: &[u8]) -> Result<QrCode, ()> {
    let (ver, _, ecc_per_blk) = QR_CAP_L.iter().copied().find(|&(_, cap, _)| data.len() <= cap).ok_or(())?;
    let vi = ver - 1;
    let total_cw = QR_TOTAL_CW[vi];
    let (b1, d1, b2, d2) = QR_BLOCKS_L[vi];
    let data_cw = b1 * d1 + b2 * d2;

    let mut bits: Vec<u8> = Vec::with_capacity(data_cw * 8);
    let push = |bits: &mut Vec<u8>, val: usize, n: usize| {
        for i in (0..n).rev() {
            bits.push(((val >> i) & 1) as u8);
        }
    };
    push(&mut bits, 0b0100, 4); // Byte mode
    push(&mut bits, data.len(), 8);
    for &b in data {
        push(&mut bits, b as usize, 8);
    }
    let rem = (data_cw * 8).saturating_sub(bits.len()).min(4);
    push(&mut bits, 0, rem);
    while bits.len() % 8 != 0 {
        bits.push(0);
    }
    let mut cw = Vec::with_capacity(data_cw);
    for chunk in bits.chunks(8) {
        let mut v = 0u8;
        for &b in chunk {
            v = (v << 1) | b;
        }
        cw.push(v);
    }
    let mut pad = [0xec, 0x11].into_iter().cycle();
    while cw.len() < data_cw {
        cw.push(pad.next().unwrap());
    }

    let mut data_blocks = Vec::new();
    let mut ecc_blocks = Vec::new();
    let mut pos = 0;
    for _ in 0..b1 {
        let blk = &cw[pos..pos + d1];
        data_blocks.push(blk.to_vec());
        ecc_blocks.push(rs_ecc(blk, ecc_per_blk));
        pos += d1;
    }
    for _ in 0..b2 {
        let blk = &cw[pos..pos + d2];
        data_blocks.push(blk.to_vec());
        ecc_blocks.push(rs_ecc(blk, ecc_per_blk));
        pos += d2;
    }

    let mut final_cw = Vec::with_capacity(total_cw);
    let max_d = d1.max(d2);
    for i in 0..max_d {
        for blk in &data_blocks {
            if i < blk.len() {
                final_cw.push(blk[i]);
            }
        }
    }
    for i in 0..ecc_per_blk {
        for blk in &ecc_blocks {
            final_cw.push(blk[i]);
        }
    }

    let size = 17 + ver * 4;
    let mut grid = vec![false; size * size];
    let mut func = vec![false; size * size];
    let mut set_f = |x: usize, y: usize, v: bool| {
        grid[y * size + x] = v;
        func[y * size + x] = true;
    };

    for (fx, fy) in [(0, 0), (size - 7, 0), (0, size - 7)] {
        for dy in -1i32..=7 {
            for dx in -1i32..=7 {
                let xx = fx as i32 + dx;
                let yy = fy as i32 + dy;
                if xx >= 0 && yy >= 0 && (xx as usize) < size && (yy as usize) < size {
                    let on = (0..=6).contains(&dx)
                        && (0..=6).contains(&dy)
                        && (dx == 0 || dx == 6 || dy == 0 || dy == 6 || ((2..=4).contains(&dx) && (2..=4).contains(&dy)));
                    set_f(xx as usize, yy as usize, on);
                }
            }
        }
    }
    for i in 8..size - 8 {
        set_f(i, 6, i % 2 == 0);
        set_f(6, i, i % 2 == 0);
    }
    set_f(8, size - 8, true);

    let align = QR_ALIGN[vi];
    for &ay in align {
        for &ax in align {
            let near_finder = (ax < 9 && ay < 9) || (ax < 9 && ay >= size - 8) || (ax >= size - 8 && ay < 9);
            if near_finder {
                continue;
            }
            for dy in -2i32..=2 {
                for dx in -2i32..=2 {
                    let on = dx.abs().max(dy.abs()) != 1;
                    set_f((ax as i32 + dx) as usize, (ay as i32 + dy) as usize, on);
                }
            }
        }
    }
    // Reserve format info cells.
    for i in 0..9 {
        if i != 6 {
            func[8 * size + i] = true;
            func[i * size + 8] = true;
        }
    }
    for i in 0..8 {
        func[8 * size + (size - 1 - i)] = true;
        if i < 7 {
            func[(size - 1 - i) * size + 8] = true;
        }
    }
    if ver >= 7 {
        for i in 0..6 {
            for j in 0..3 {
                func[i * size + (size - 11 + j)] = true;
                func[(size - 11 + j) * size + i] = true;
            }
        }
    }

    // Place data bits in zig-zag.
    let mut bit_idx = 0usize;
    let total_bits = final_cw.len() * 8;
    let mut right = size as i32 - 1;
    let mut up = true;
    while right > 0 {
        if right == 6 {
            right -= 1;
        }
        for step in 0..size {
            let y = if up { size - 1 - step } else { step };
            for dx in [0i32, -1] {
                let x = (right + dx) as usize;
                if !func[y * size + x] {
                    let v = if bit_idx < total_bits {
                        (final_cw[bit_idx / 8] >> (7 - (bit_idx % 8))) & 1 == 1
                    } else {
                        false
                    };
                    grid[y * size + x] = v;
                    bit_idx += 1;
                }
            }
        }
        right -= 2;
        up = !up;
    }

    let mut best_score = u32::MAX;
    let mut best_grid = grid.clone();
    for mask in 0..8 {
        let mut g = grid.clone();
        for y in 0..size {
            for x in 0..size {
                if !func[y * size + x] && mask_bit(mask, x, y) {
                    g[y * size + x] = !g[y * size + x];
                }
            }
        }
        write_format(&mut g, size, mask);
        if ver >= 7 {
            write_version(&mut g, size, ver);
        }
        let s = qr_penalty(&g, size);
        if s < best_score {
            best_score = s;
            best_grid = g;
        }
    }
    Ok(QrCode { size, modules: best_grid })
}

fn mask_bit(m: usize, x: usize, y: usize) -> bool {
    match m {
        0 => (x + y) % 2 == 0,
        1 => y % 2 == 0,
        2 => x % 3 == 0,
        3 => (x + y) % 3 == 0,
        4 => ((y / 2) + (x / 3)) % 2 == 0,
        5 => ((x * y) % 2) + ((x * y) % 3) == 0,
        6 => (((x * y) % 2) + ((x * y) % 3)) % 2 == 0,
        _ => (((x + y) % 2) + ((x * y) % 3)) % 2 == 0,
    }
}

fn write_format(g: &mut [bool], size: usize, mask: usize) {
    // ECC Level L = 01
    let data = (0b01 << 3) | mask;
    let mut rem = data << 10;
    for i in (10..=14).rev() {
        if (rem >> i) & 1 != 0 {
            rem ^= 0b10100110111 << (i - 10);
        }
    }
    let fmt = ((data << 10) | rem) ^ 0b101010000010010;
    for i in 0..15 {
        let bit = (fmt >> i) & 1 == 1;
        if i < 6 {
            g[i * size + 8] = bit;
        } else if i < 8 {
            g[(i + 1) * size + 8] = bit;
        } else {
            g[(size - 15 + i) * size + 8] = bit;
        }
        if i < 8 {
            g[8 * size + (size - 1 - i)] = bit;
        } else if i < 9 {
            g[8 * size + 7] = bit;
        } else {
            g[8 * size + (14 - i)] = bit;
        }
    }
}

fn write_version(g: &mut [bool], size: usize, ver: usize) {
    let mut rem = ver << 12;
    for i in (12..=17).rev() {
        if (rem >> i) & 1 != 0 {
            rem ^= 0x1f25 << (i - 12);
        }
    }
    let bits = (ver << 12) | rem;
    for i in 0..18 {
        let bit = (bits >> i) & 1 == 1;
        let a = size - 11 + (i % 3);
        let b = i / 3;
        g[b * size + a] = bit;
        g[a * size + b] = bit;
    }
}

fn qr_penalty(g: &[bool], size: usize) -> u32 {
    let mut score = 0u32;
    for y in 0..size {
        let mut run = 1u32;
        for x in 1..size {
            if g[y * size + x] == g[y * size + x - 1] {
                run += 1;
            } else {
                if run >= 5 {
                    score += run - 2;
                }
                run = 1;
            }
        }
        if run >= 5 {
            score += run - 2;
        }
    }
    for x in 0..size {
        let mut run = 1u32;
        for y in 1..size {
            if g[y * size + x] == g[(y - 1) * size + x] {
                run += 1;
            } else {
                if run >= 5 {
                    score += run - 2;
                }
                run = 1;
            }
        }
        if run >= 5 {
            score += run - 2;
        }
    }
    score
}

fn rs_ecc(data: &[u8], nsym: usize) -> Vec<u8> {
    let mut gen = vec![1u8];
    for i in 0..nsym {
        let root = gf_pow2(i);
        let mut next = vec![0u8; gen.len() + 1];
        for (j, &c) in gen.iter().enumerate() {
            next[j] ^= c;
            next[j + 1] ^= gf_mul(c, root);
        }
        gen = next;
    }
    let mut rem = vec![0u8; nsym];
    for &b in data {
        let factor = b ^ rem[0];
        rem.rotate_left(1);
        rem[nsym - 1] = 0;
        for i in 0..nsym {
            rem[i] ^= gf_mul(gen[i + 1], factor);
        }
    }
    rem
}

fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut r = 0u8;
    while b != 0 {
        if b & 1 != 0 {
            r ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1d;
        }
        b >>= 1;
    }
    r
}

fn gf_pow2(n: usize) -> u8 {
    let mut v = 1u8;
    for _ in 0..n {
        v = gf_mul(v, 2);
    }
    v
}

const MOBILE_HTML: &str = r#"<!doctype html>
<html lang="en"><head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
<title>闇</title>
<style>
:root{__THEME_VARS__}
*{box-sizing:border-box;-webkit-tap-highlight-color:transparent}
[hidden]{display:none!important}
body{margin:0 auto;max-width:560px;padding:max(16px,env(safe-area-inset-top)) max(16px,env(safe-area-inset-right)) max(28px,env(safe-area-inset-bottom)) max(16px,env(safe-area-inset-left));background:var(--bg);color:var(--text);font:14px/1.45 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;-webkit-text-size-adjust:100%}
.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}

/* header */
.hdr{display:flex;justify-content:space-between;align-items:center;padding:10px 12px;background:var(--panel);border:1px solid var(--border);box-shadow:3px 3px 0 var(--shade);margin-bottom:16px}
.brand{font-weight:700;font-size:15px;letter-spacing:.06em;color:var(--text-hi)}
.brand span{color:var(--accent)}
.status{display:flex;align-items:center;gap:6px;font-size:11px;letter-spacing:.08em;color:var(--text-dim);text-transform:uppercase}
.dot{width:8px;height:8px;background:var(--ok);display:inline-block}
.dot.off{background:var(--warn)}

/* cards */
.card{background:var(--panel);border:1px solid var(--border);box-shadow:3px 3px 0 var(--shade);padding:14px;margin-bottom:16px}
.ctitle{display:flex;justify-content:space-between;align-items:center;gap:10px;flex-wrap:wrap;margin-bottom:12px;padding-bottom:8px;border-bottom:1px solid var(--border);font-size:12px;font-weight:700;letter-spacing:.08em;text-transform:uppercase;color:var(--text-hi)}
.ctitle .t{display:flex;align-items:center;gap:8px;margin:0;font:inherit;letter-spacing:inherit;text-transform:inherit}
.cnt{min-width:20px;padding:0 6px;text-align:center;font-size:11px;line-height:18px;letter-spacing:0;background:var(--surface);border:1px solid var(--border);color:var(--text-dim)}
.tools{display:flex;gap:8px;flex-wrap:wrap}

/* buttons */
.btn{display:inline-flex;align-items:center;justify-content:center;min-height:44px;min-width:44px;padding:0 14px;background:var(--accent);color:var(--on-accent);border:1px solid var(--accent-dim);box-shadow:2px 2px 0 var(--shade);font-family:inherit;font-weight:600;font-size:12px;line-height:1.2;letter-spacing:.04em;text-transform:uppercase;text-decoration:none;white-space:nowrap;cursor:pointer;touch-action:manipulation}
.btn:active{transform:translate(1px,1px);box-shadow:1px 1px 0 var(--shade)}
.btn.sec{background:var(--surface);color:var(--text);border-color:var(--border)}
.btn.ok{background:var(--ok);color:var(--on-accent);border-color:var(--ok)}
.btn:disabled{opacity:.45;box-shadow:none;cursor:not-allowed;transform:none}
.btn:focus-visible,.pick:focus-within{outline:2px solid var(--accent);outline-offset:2px}

/* file picker */
.pick{display:block;position:relative;padding:20px 12px;margin-bottom:12px;text-align:center;background:var(--surface);border:2px dashed var(--border);color:var(--text-hi);font-size:12px;font-weight:700;letter-spacing:.06em;text-transform:uppercase}
.pick:active{background:var(--surface-hi,var(--surface))}
.pick small{display:block;margin-top:4px;font-size:11px;font-weight:400;letter-spacing:0;text-transform:none;color:var(--text-dim)}
.pick input{position:absolute;inset:0;width:100%;height:100%;opacity:0;cursor:pointer;font-size:16px}

/* aggregate progress */
.agg{margin-bottom:12px;padding:10px;background:var(--surface);border:1px solid var(--border)}
.aggtop{display:flex;justify-content:space-between;gap:10px;font-size:12px;font-weight:700;color:var(--text-hi)}
.agg .bar{height:8px}
.agg .meta{margin-top:6px}
.agg[data-s=ok] .fill{background:var(--ok)}
.agg[data-s=fail] .fill{background:var(--warn)}

/* rows */
.row{display:flex;justify-content:space-between;align-items:center;gap:12px;padding:10px 0;border-bottom:1px solid var(--border)}
.row:first-child{padding-top:0}
.row:last-child{border-bottom:0;padding-bottom:0}
.info{min-width:0;flex:1}
.name{color:var(--text-hi);overflow-wrap:anywhere;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}
.meta{margin-top:2px;color:var(--text-dim);font-size:11px}
.empty{padding:22px 12px;text-align:center;border:1px dashed var(--border);color:var(--text-dim);font-size:12px}
.empty b{display:block;margin-bottom:4px;color:var(--text-hi);letter-spacing:.06em;text-transform:uppercase}

/* upload queue */
.q{padding:10px 0;border-bottom:1px solid var(--border)}
.q:first-child{padding-top:0}
.q:last-child{border-bottom:0}
.qtop{display:flex;align-items:center;gap:10px}
.acts{display:flex;gap:8px;flex-shrink:0}
.tag{display:inline-block;margin-right:6px;padding:0 5px;border:1px solid currentColor;font-size:10px;line-height:15px;letter-spacing:.08em;text-transform:uppercase;color:var(--text-dim)}
.q[data-s=uploading] .tag{color:var(--accent)}
.q[data-s=done] .tag{color:var(--ok)}
.q[data-s=error] .tag{color:var(--warn)}
.bar{height:6px;margin-top:8px;background:var(--surface);border:1px solid var(--border)}
.fill{height:100%;width:0;background:var(--accent)}
.q[data-s=done] .fill{background:var(--ok)}
.q[data-s=error] .fill{background:var(--warn)}
.q[data-s=queued] .fill,.q[data-s=cancelled] .fill{background:var(--text-faint,var(--text-dim))}
.ctl{display:flex;gap:8px;flex-wrap:wrap;margin-top:12px}
.ctl .btn{flex:1 1 auto}
.ctl #go{flex-grow:2}
.ctl:first-child{margin-top:0}
</style></head><body>
<div class="hdr">
  <div class="brand">闇</div>
  <div class="status"><i class="dot" id="dot"></i><span id="conn">LAN</span></div>
</div>

<section class="card" aria-labelledby="h-up">
  <div class="ctitle"><h2 class="t" id="h-up">Send to PC <span class="cnt" id="qcnt" hidden>0</span></h2></div>
  <label class="pick">Tap to choose files<small>Multiple files supported</small><input id="pick" type="file" multiple></label>
  <div class="agg" id="agg" data-s="run" hidden>
    <div class="aggtop"><span id="aggt"></span><span id="aggp">0%</span></div>
    <div class="bar" id="aggbar" role="progressbar" aria-label="Overall upload progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow="0"><div class="fill" id="aggf"></div></div>
    <div class="meta" id="aggs"></div>
  </div>
  <div id="queue"></div>
  <div class="ctl">
    <button type="button" class="btn" id="go" disabled>Upload</button>
    <button type="button" class="btn sec" id="cx" hidden>Cancel all</button>
    <button type="button" class="btn sec" id="clr" hidden>Clear finished</button>
  </div>
</section>

<section class="card" aria-labelledby="h-dl">
  <div class="ctitle">
    <h2 class="t" id="h-dl">Files from PC <span class="cnt" id="fcnt" hidden>0</span></h2>
    <div class="tools">
      <button type="button" class="btn sec" id="rf" aria-label="Refresh file list">Refresh</button>
      <a class="btn" id="dlall" href="/dl-all?t=__TOKEN__" download="yami-share.zip" hidden>Download all</a>
    </div>
  </div>
  <div id="files" class="meta">Loading…</div>
</section>
<div class="sr" id="live" role="status" aria-live="polite"></div>

<script>
const T="__TOKEN__";
const $=id=>document.getElementById(id);
function fmt(n){if(n<1024)return n+" B";if(n<1048576)return (n/1024).toFixed(1)+" KB";if(n<1073741824)return (n/1048576).toFixed(1)+" MB";return (n/1073741824).toFixed(2)+" GB";}
function esc(s){return s.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
function say(m){const l=$("live");l.textContent="";setTimeout(()=>{l.textContent=m;},50);}
function applyTheme(th){
  const r=document.documentElement.style;
  const m={bg:"--bg",panel:"--panel",surface:"--surface",surface_hi:"--surface-hi",border:"--border",accent:"--accent",accent_dim:"--accent-dim",ember:"--ember",text:"--text",text_dim:"--text-dim",text_faint:"--text-faint",on_accent:"--on-accent",text_hi:"--text-hi",shade:"--shade",ok:"--ok",warn:"--warn"};
  for(const k in m)if(th[k])r.setProperty(m[k],th[k]);
}

/* connection + theme */
let seen=false;
const es=new EventSource("/api/events?t="+encodeURIComponent(T));
es.addEventListener("theme",e=>{try{applyTheme(JSON.parse(e.data));}catch(_){}});
es.onopen=()=>{$("dot").className="dot";$("conn").textContent="CONNECTED";if(seen){load();pump();}seen=true;};
es.onerror=()=>{
  $("dot").className="dot off";$("conn").textContent="OFFLINE";
  if(es.readyState===EventSource.CLOSED)dropActive("Server disconnected");
};
addEventListener("offline",()=>{$("dot").className="dot off";$("conn").textContent="OFFLINE";dropActive("Network offline");});
addEventListener("online",()=>{load();pump();});

/* files from PC */
const ID_RE=/^\d+$/;
function empty(t,h){return '<div class="empty"><b>'+esc(t)+'</b>'+esc(h)+'</div>';}
let seq=0;
async function load(){
  const my=++seq,box=$("files"),all=$("dlall"),cnt=$("fcnt"),rf=$("rf");
  rf.disabled=true;
  try{
    const r=await fetch("/api/files?t="+encodeURIComponent(T),{cache:"no-store"});
    if(my!==seq)return;
    if(!r.ok){
      box.innerHTML=empty(r.status===401||r.status===403?"Unauthorized":"Couldn't load files","Error "+r.status+" · reopen the link from your PC if this persists.");
      all.hidden=true;cnt.hidden=true;return;
    }
    const raw=await r.json();
    if(my!==seq)return;
    const list=Array.isArray(raw)?raw.filter(f=>f&&ID_RE.test(String(f.id))):[];
    cnt.textContent=list.length;cnt.hidden=!list.length;
    if(!list.length){box.innerHTML=empty("No files shared yet","Files you share from your PC will show up here.");all.hidden=true;return;}
    all.hidden=list.length<2;
    box.innerHTML=list.map(f=>`<div class="row"><div class="info"><div class="name" title="${esc(f.name)}">${esc(f.name)}</div><div class="meta">${fmt(f.size)}</div></div><a class="btn sec dl" href="/dl/${f.id}?t=${encodeURIComponent(T)}" download="${esc(f.name)}" aria-label="Download ${esc(f.name)}">Download</a></div>`).join("");
  }catch(_){
    if(my!==seq)return;
    if(box.querySelector(".row"))say("Refresh failed");
    else{box.innerHTML=empty("Can't reach your PC","Check you're on the same network, then tap Refresh.");all.hidden=true;cnt.hidden=true;}
  }finally{
    if(my===seq)rf.disabled=false;
  }
}
function flash(a,msg){
  if(a._t)return;
  const old=a.textContent;
  a.textContent="Started ✓";a.classList.add("ok");
  a._t=setTimeout(()=>{a.textContent=old;a.classList.remove("ok");a._t=0;},2500);
  say(msg);
}
let dlSeq=1;
document.addEventListener("click",e=>{
  const a=e.target.closest("a.dl,#dlall");
  if(!a)return;
  const u=new URL(a.getAttribute("href"),location.href);
  u.searchParams.set("dl",String(dlSeq++));
  a.setAttribute("href",u.pathname+u.search);
  flash(a,a.id==="dlall"?"Downloading all files":"Download started");
});
$("rf").addEventListener("click",load);
document.addEventListener("visibilitychange",()=>{if(!document.hidden)load();});

/* upload queue */
const Q=[],MAX=2;let nid=1;
const LB={ready:"Ready",queued:"Queued",uploading:"Uploading",done:"Done",error:"Failed",cancelled:"Cancelled"};
const BL={cancel:"Cancel",retry:"Retry",remove:"Remove"};
const ACT={ready:["remove"],queued:["cancel"],uploading:["cancel"],done:["remove"],error:["retry","remove"],cancelled:["retry","remove"]};
const pct=it=>it.s==="done"?100:it.size?Math.min(100,Math.floor(it.loaded*100/it.size)):0;

function paint(it){
  const p=pct(it);
  it.el.dataset.s=it.s;
  it.tag.textContent=LB[it.s];
  it.mt.textContent=it.s==="uploading"?fmt(it.loaded)+" / "+fmt(it.size)+" · "+p+"%":it.s==="error"?it.err+" · "+fmt(it.size):fmt(it.size);
  it.bar.hidden=it.s==="ready";
  it.fl.style.width=p+"%";
  it.bar.setAttribute("aria-valuenow",p);
  if(it.as!==it.s){
    it.as=it.s;
    it.ac.innerHTML=ACT[it.s].map(a=>'<button type="button" class="btn sec" data-act="'+a+'" aria-label="'+BL[a]+' '+esc(it.name)+'">'+(a==="remove"?"✕":BL[a])+'</button>').join("");
  }
}
function mk(it){
  const el=document.createElement("div");
  el.className="q";el.dataset.id=it.id;
  el.innerHTML='<div class="qtop"><div class="info"><div class="name"></div><div class="meta"><span class="tag"></span><span class="mt"></span></div></div><div class="acts"></div></div><div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100"><div class="fill"></div></div>';
  it.el=el;it.n=el.querySelector(".name");it.tag=el.querySelector(".tag");it.mt=el.querySelector(".mt");
  it.ac=el.querySelector(".acts");it.bar=el.querySelector(".bar");it.fl=el.querySelector(".fill");
  it.n.textContent=it.name;it.n.title=it.name;
  it.bar.setAttribute("aria-label","Upload progress for "+it.name);
  $("queue").appendChild(el);
  paint(it);
}
function set(it,s){it.s=s;paint(it);}
function sync(){
  let n=0,done=0,tot=0,ld=0,act=0,fail=0,canc=0,ready=0,fin=0;
  for(const it of Q){
    if(it.s==="ready"){ready++;continue;}
    if(it.s==="error"){fail++;fin++;continue;}
    if(it.s==="cancelled"){canc++;fin++;continue;}
    n++;tot+=it.size;ld+=it.loaded;
    if(it.s==="done"){done++;fin++;}else act++;
  }
  const p=tot?Math.floor(ld*100/tot):(done&&!act?100:0);
  const parts=[];
  if(done)parts.push(done+" uploaded");
  if(fail)parts.push(fail+" failed");
  if(canc)parts.push(canc+" cancelled");
  $("agg").hidden=!(n||fail||canc);
  $("agg").dataset.s=act?"run":fail?"fail":done?"ok":"cancel";
  $("aggt").textContent=act?"Uploading · "+done+" of "+n+" done"+(fail?", "+fail+" failed":"")+(canc?", "+canc+" cancelled":""):parts.length===1&&done?"All "+done+" uploaded":parts.join(" · ");
  $("aggp").textContent=p+"%";
  $("aggf").style.width=p+"%";
  $("aggbar").setAttribute("aria-valuenow",p);
  $("aggs").textContent=fmt(ld)+" / "+fmt(tot)+(fail||canc?" ("+n+" of "+(n+fail+canc)+" files)":"");
  const go=$("go");
  go.disabled=!ready;
  go.textContent=ready?"Upload "+ready+(ready>1?" files":" file"):"Upload";
  $("cx").hidden=!act;
  $("clr").hidden=!fin;
  const c=$("qcnt");c.hidden=!Q.length;c.textContent=Q.length;
}
const IDLE_MS=45000,RESP_MS=120000;
function clearTimers(it){
  if(it._idle){clearTimeout(it._idle);it._idle=0;}
  if(it._resp){clearTimeout(it._resp);it._resp=0;}
  if(it._tot){clearTimeout(it._tot);it._tot=0;}
}
function dropActive(msg){
  for(const it of Q){
    if(it.s==="uploading"&&it.xhr){
      const x=it.xhr;
      clearTimers(it);
      it.xhr=null;it.err=msg;
      set(it,"error");
      x.abort();
    }
  }
  sync();
}
function pump(){
  if(navigator.onLine===false){sync();return;}
  let run=Q.filter(i=>i.s==="uploading").length;
  for(const it of Q){
    if(run>=MAX)break;
    if(it.s==="queued"){send(it);run++;}
  }
  sync();
}
function send(it){
  clearTimers(it);
  const x=new XMLHttpRequest();
  it.xhr=x;it.loaded=0;set(it,"uploading");
  const fail=(err)=>{
    if(it.xhr!==x)return;
    clearTimers(it);
    it.xhr=null;it.err=err;
    set(it,"error");
    say(it.name+" failed: "+err);
    x.abort();
    pump();
  };
  const armIdle=()=>{
    if(it._idle)clearTimeout(it._idle);
    it._idle=setTimeout(()=>fail("Upload stalled (inactivity timeout)"),IDLE_MS);
  };
  const totalMs=Math.min(21600000,Math.max(600000,Math.ceil(it.size/5120)*1000+300000));
  it._tot=setTimeout(()=>fail("Total request timeout"),totalMs);
  armIdle();
  x.open("POST","/upload?t="+encodeURIComponent(T)+"&name="+encodeURIComponent(it.name));
  x.upload.onprogress=e=>{
    if(it.xhr!==x)return;
    armIdle();
    if(e.lengthComputable){it.loaded=Math.min(e.loaded,it.size);paint(it);sync();}
  };
  x.upload.onload=()=>{
    if(it.xhr!==x)return;
    if(it._idle){clearTimeout(it._idle);it._idle=0;}
    it._resp=setTimeout(()=>fail("Server response timeout"),RESP_MS);
  };
  const end=(s,err)=>{
    if(it.xhr!==x)return;
    clearTimers(it);
    it.xhr=null;it.err=err||"";
    if(s==="done")it.loaded=it.size;
    set(it,s);
    say(it.name+" "+(s==="done"?"uploaded":s==="error"?"failed":"cancelled"));
    if(s==="done")load();
    pump();
  };
  x.onload=()=>{
    if(x.status===200)end("done");
    else end("error",x.status===401||x.status===403?"Unauthorized":"Error "+x.status);
  };
  x.onerror=()=>end("error",navigator.onLine===false?"Network offline":"Connection lost");
  x.onabort=()=>end("cancelled");
  x.send(it.f);
}
function add(files){
  for(const f of files){
    const it={id:nid++,f:f,name:f.name,size:f.size,s:"ready",loaded:0,xhr:null,err:"",_idle:0,_resp:0,_tot:0};
    Q.push(it);mk(it);
  }
  sync();
}
$("pick").addEventListener("change",e=>{add(e.target.files);e.target.value="";});
$("go").addEventListener("click",()=>{
  for(const it of Q)if(it.s==="ready")set(it,"queued");
  pump();
});
$("cx").addEventListener("click",()=>{
  for(const it of Q)if(it.s==="queued")set(it,"cancelled");
  for(const it of Q)if(it.s==="uploading"&&it.xhr)it.xhr.abort();
  sync();
});
$("clr").addEventListener("click",()=>{
  for(let i=Q.length-1;i>=0;i--){
    const it=Q[i];
    if(it.s==="done"||it.s==="error"||it.s==="cancelled"){it.el.remove();Q.splice(i,1);}
  }
  sync();
});
$("queue").addEventListener("click",e=>{
  const b=e.target.closest("button[data-act]");
  if(!b)return;
  const it=Q.find(i=>String(i.id)===b.closest(".q").dataset.id);
  if(!it)return;
  const a=b.dataset.act;
  if(a==="cancel"){
    if(it.xhr)it.xhr.abort();
    else{set(it,"cancelled");sync();}
  }else if(a==="retry"){
    clearTimers(it);
    if(it.xhr){const x=it.xhr;it.xhr=null;x.abort();}
    it.loaded=0;it.err="";set(it,"queued");pump();
  }else if(a==="remove"){
    clearTimers(it);
    Q.splice(Q.indexOf(it),1);it.el.remove();sync();
  }
});
addEventListener("beforeunload",e=>{
  if(Q.some(i=>i.s==="uploading"||i.s==="queued")){e.preventDefault();e.returnValue="";}
});

sync();
load();
</script></body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_encodes_url() {
        let q = encode_qr(b"http://192.168.1.10:45123/?t=0123456789abcdef0123456789abcdef").unwrap();
        assert!(q.size >= 29);
        assert_eq!(q.modules.len(), q.size * q.size);
        // Top-left finder corner is dark.
        assert!(q.modules[0]);
        assert!(!q.modules[1 * q.size + 1]);
    }

    #[test]
    fn token_and_ct_eq() {
        let t1 = gen_token();
        let t2 = gen_token();
        assert_eq!(t1.len(), 32);
        assert_ne!(t1, t2);
        assert!(ct_eq(t1.as_bytes(), t1.as_bytes()));
        assert!(!ct_eq(t1.as_bytes(), t2.as_bytes()));
        assert!(!ct_eq(b"abc", b"abcd"));
    }

    #[test]
    fn sanitize_blocks_traversal() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("..\\..\\secret.txt"), "secret.txt");
        assert_eq!(sanitize_filename(".."), "file");
        assert_eq!(sanitize_filename("."), "file");
        assert_eq!(sanitize_filename("a\0b\nc.txt"), "abc.txt");
    }

    #[test]
    fn unique_file_avoids_collisions() {
        let dir = std::env::temp_dir().join(format!("yami-share-test-{}", gen_token()));
        let (p1, _) = create_unique_file(&dir, "photo.jpg").unwrap();
        let (p2, _) = create_unique_file(&dir, "photo.jpg").unwrap();
        assert_eq!(p1.file_name().unwrap(), "photo.jpg");
        assert_eq!(p2.file_name().unwrap(), "photo (1).jpg");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_auth_upload_download_and_stop() {
        let dir = std::env::temp_dir().join(format!("yami-share-srv-{}", gen_token()));
        std::fs::create_dir_all(&dir).unwrap();
        let src_file = dir.join("hello.txt");
        std::fs::write(&src_file, b"hello from pc").unwrap();

        let (tx, _rx) = calloop::channel::channel();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();

        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) }, 0);
        let token = gen_token();
        let stop = Arc::new(AtomicBool::new(false));
        let streams = Arc::new(Mutex::new(HashMap::new()));
        let upload_dir = dir.join("uploads");
        let theme_bus = Arc::new((Mutex::new((1, ShareTheme::default())), Condvar::new()));
        let ctx = Arc::new(ServerCtx {
            token: token.clone(),
            files: Arc::new(Mutex::new(vec![SharedFile { id: 7, path: src_file }])),
            upload_dir: upload_dir.clone(),
            stop: stop.clone(),
            streams: streams.clone(),
            theme_bus: theme_bus.clone(),
            active_conns: Arc::new(AtomicUsize::new(0)),
            sse_conns: Arc::new(AtomicUsize::new(0)),
            next_id: Arc::new(AtomicU64::new(1)),
            dl_groups: Mutex::new(HashMap::new()),
            tx,
        });
        let handle = std::thread::spawn(move || accept_loop(listener, fds[0], ctx));
        let srv = ServerHandle { stop, wake_wr: fds[1], streams, theme_bus: theme_bus.clone(), listener_thread: Some(handle) };

        // Invalid token rejected.
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        s.write_all(b"GET /api/files?t=wrong HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        assert!(resp.starts_with("HTTP/1.1 401"));

        // Valid download works via stable ID.
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        write!(s, "GET /dl/7?t={token} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).unwrap();
        let body = String::from_utf8_lossy(&raw);
        assert!(body.starts_with("HTTP/1.1 200 OK"));
        assert!(body.ends_with("hello from pc"));

        // Valid multi-file ZIP download works.
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        write!(s, "GET /dl-all?t={token}&ids=7 HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut zip_raw = Vec::new();
        s.read_to_end(&mut zip_raw).unwrap();
        let zip_str = String::from_utf8_lossy(&zip_raw);
        assert!(zip_str.starts_with("HTTP/1.1 200 OK"));
        assert!(zip_raw.windows(4).any(|w| w == b"PK\x03\x04"));

        // Streaming POST /upload saves file to upload_dir.
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        let payload = b"uploaded bytes";
        write!(s, "POST /upload?t={token}&name=up.txt HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n", payload.len()).unwrap();
        s.write_all(payload).unwrap();
        let mut up_resp = String::new();
        s.read_to_string(&mut up_resp).unwrap();
        assert!(up_resp.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(std::fs::read(upload_dir.join("up.txt")).unwrap(), payload);

        // SSE /api/events pushes live theme update.
        let mut sse = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        sse.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        write!(sse, "GET /api/events?t={token} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut sse_buf = [0u8; 512];
        let n1 = sse.read(&mut sse_buf).unwrap();
        assert!(String::from_utf8_lossy(&sse_buf[..n1]).starts_with("HTTP/1.1 200 OK"));
        if let Ok(mut g) = theme_bus.0.lock() {
            g.0 += 1;
            g.1.accent = "#123456".into();
            theme_bus.1.notify_all();
        }
        let n2 = sse.read(&mut sse_buf).unwrap();
        let ev = String::from_utf8_lossy(&sse_buf[..n2]);
        assert!(ev.contains("event: theme") && ev.contains("#123456"));
        drop(sse);

        // Drop closes listener immediately.
        drop(srv);
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_separation_and_clear() {
        let mut st = ShareState::default();
        st.apply_event(Transfer { id: 1, name: "a.bin".into(), upload: true, done: 50, total: 100, status: TransferStatus::Active });
        st.apply_event(Transfer { id: 2, name: "b.txt".into(), upload: false, done: 10, total: 10, status: TransferStatus::Done });
        assert_eq!(st.active_transfers.len(), 1);
        assert_eq!(st.history.len(), 1);
        st.clear_history();
        assert_eq!(st.active_transfers.len(), 1);
        assert!(st.history.is_empty());
        st.stop();
        assert!(st.active_transfers.is_empty());
        assert_eq!(st.history.len(), 1);
        assert_eq!(st.history[0].status, TransferStatus::Cancelled);
        // Duplicate worker Cancelled event for id 1 updates in place instead of duplicating.
        st.apply_event(Transfer { id: 1, name: "a.bin".into(), upload: true, done: 50, total: 100, status: TransferStatus::Cancelled });
        assert_eq!(st.history.len(), 1);
    }

    #[test]
    fn ufw_script_targets_single_rule() {
        let s = build_ufw_script(45837, Some("wlan0"));
        assert!(s.contains("export LC_ALL=C;"));
        assert!(s.contains("ufw allow in on wlan0 to any port 45837 proto tcp comment 'yami-share'"));
        assert!(s.contains("sed -n"));
        let s_bad = build_ufw_script(45837, Some("wlan0; rm -rf /"));
        assert!(!s_bad.contains("rm -rf"));
    }

    #[test]
    fn theme_tokens_and_port_default() {
        let t = Theme::new(0xc4161c, 0.5, 0.9, 0.0, 0.5, false).apply();
        let st = ShareTheme::from_theme(&t);
        assert!(st.accent.starts_with('#') && st.accent.len() == 7);
        assert_eq!(crate::config::Settings::default().share_port, 45837);
    }

    #[test]
    fn download_probes_and_large_file_history() {
        let dir = std::env::temp_dir().join(format!("yami-share-dl-{}", gen_token()));
        std::fs::create_dir_all(&dir).unwrap();
        let img_path = dir.join("clip.png");
        std::fs::write(&img_path, vec![0x89u8; 4096]).unwrap();
        let mp4_path = dir.join("clip.mp4");
        std::fs::write(&mp4_path, vec![0x42u8; 300 * 1024]).unwrap();

        let (tx, rx) = calloop::channel::channel();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();

        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) }, 0);
        let token = gen_token();
        let stop = Arc::new(AtomicBool::new(false));
        let streams = Arc::new(Mutex::new(HashMap::new()));
        let theme_bus = Arc::new((Mutex::new((1, ShareTheme::default())), Condvar::new()));
        let ctx = Arc::new(ServerCtx {
            token: token.clone(),
            files: Arc::new(Mutex::new(vec![
                SharedFile { id: 1, path: img_path },
                SharedFile { id: 2, path: mp4_path },
            ])),
            upload_dir: dir.join("uploads"),
            stop: stop.clone(),
            streams: streams.clone(),
            theme_bus: theme_bus.clone(),
            active_conns: Arc::new(AtomicUsize::new(0)),
            sse_conns: Arc::new(AtomicUsize::new(0)),
            next_id: Arc::new(AtomicU64::new(1)),
            dl_groups: Mutex::new(HashMap::new()),
            tx,
        });
        let handle = std::thread::spawn(move || accept_loop(listener, fds[0], ctx));
        let srv = ServerHandle { stop, wake_wr: fds[1], streams, theme_bus, listener_thread: Some(handle) };

        // HEAD and small Range probes must not add history entries or Failed events.
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        write!(s, "HEAD /dl/2?t={token} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut head_resp = String::new();
        s.read_to_string(&mut head_resp).unwrap();
        assert!(head_resp.starts_with("HTTP/1.1 200 OK"));

        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        write!(s, "GET /dl/2?t={token} HTTP/1.1\r\nHost: x\r\nRange: bytes=0-1\r\n\r\n").unwrap();
        let mut probe_resp = Vec::new();
        s.read_to_end(&mut probe_resp).unwrap();
        assert!(String::from_utf8_lossy(&probe_resp).starts_with("HTTP/1.1 206 Partial Content"));

        // Real downloads of image and large MP4 (with &dl=1 + multi-range/aborted retry) produce 1 Done each.
        {
            // Simulated browser hand-off: reads headers + first bytes then drops, followed by Range/full retry with same dl=1.
            let mut s1 = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            write!(s1, "GET /dl/2?t={token}&dl=1 HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut tmp = [0u8; 256];
            let _ = s1.read(&mut tmp);
            drop(s1);

            let mut s2 = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            write!(s2, "GET /dl/2?t={token}&dl=1 HTTP/1.1\r\nHost: x\r\nRange: bytes=0-150000\r\n\r\n").unwrap();
            let mut r2 = Vec::new();
            s2.read_to_end(&mut r2).unwrap();

            let mut s3 = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            write!(s3, "GET /dl/2?t={token}&dl=1 HTTP/1.1\r\nHost: x\r\nRange: bytes=150001-\r\n\r\n").unwrap();
            let mut r3 = Vec::new();
            s3.read_to_end(&mut r3).unwrap();
        }
        {
            let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            write!(s, "GET /dl/1?t={token}&dl=2 HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut raw = Vec::new();
            s.read_to_end(&mut raw).unwrap();
            let hdr_end = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            assert_eq!(raw.len() - hdr_end, 4096);
        }

        drop(srv);
        let mut st = ShareState::default();
        while let Ok(Msg::ShareUpdate(ev)) = rx.try_recv() {
            st.apply_event(ev);
        }
        assert!(st.active_transfers.is_empty());
        assert_eq!(st.history.len(), 2);
        assert!(st.history.iter().all(|h| h.status == TransferStatus::Done));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
