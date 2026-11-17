//! Cổng nội bộ `127.0.0.1` phục vụ sách cho UIWebView — như giao thức riêng `x-ibooks-th:` của iBooks (THIET-KE 4.4):
//!
//! - `GET /b/<mã phiên>/<mã sách>/<đường dẫn trong sách>[?p=N]`: chương (phần N — chapter.rs) có chèn viewport + cấu hình màn đọc
//!   (`window.GBC`, app đặt bằng `reader.config`) + vị trí chương (`window.GBD`) + `reader.js`; ảnh / CSS / font lấy thẳng trong
//!   sách (font bị làm rối được gỡ);
//! - `GET /gb/reader.js` — mã màn đọc (đóng trong tệp chạy);
//! - mã phiên ngẫu nhiên theo lần mở lõi: app khác trên máy không đọc ké được;
//! - iOS thu hồi cổng nghe của app nằm nền lâu → `revive` mở lại, giữ số cổng nếu được (GramMusic M23).

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::chapter::Chapter;
use crate::epub::{self, Obf};
use crate::zip::Zip;

/// Mã màn đọc (chia cột, vị trí, chạm, tô màu…) — phục vụ ở `/gb/reader.js?v=…`.
pub const READER_JS: &str = include_str!("assets/reader.js");
/// Đổi khi reader.js đổi (WebView giữ bản cũ trong đệm theo URL).
pub const READER_V: u32 = 2;
/// Giữ chừng này chương đã chuẩn bị của mỗi sách (WebView đang đọc + WebView đếm trang).
const KEEP_CHAPTERS: usize = 6;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Nơi chứa tệp của sách: EPUB (zip) hoặc sách chuyển từ MOBI / AZW3 (trong bộ nhớ).
pub enum Store {
    Zip(Zip),
    Mem(HashMap<String, (String, Arc<Vec<u8>>)>),
}

impl Store {
    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        match self {
            Store::Zip(z) => z.read(path),
            Store::Mem(m) => m
                .get(path)
                .map(|(_, d)| d.as_ref().clone())
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("không có «{path}» trong sách"))),
        }
    }

    pub fn has(&self, path: &str) -> bool {
        match self {
            Store::Zip(z) => z.has(path),
            Store::Mem(m) => m.contains_key(path),
        }
    }

    fn mime(&self, path: &str) -> Option<String> {
        match self {
            Store::Mem(m) => m.get(path).map(|(mt, _)| mt.clone()),
            Store::Zip(_) => None,
        }
    }
}

/// Một chương (mục spine) của sách đang mở.
#[derive(Clone, Debug, PartialEq)]
pub struct Spine {
    pub path: String,
    pub linear: bool,
    pub size: u64,
}

/// Sách đang mở.
pub struct OpenBook {
    /// Mã ngắn trong URL.
    pub id: String,
    /// Khoá sách trong thư viện (`<peer>_<id>`) hoặc `file:<đường dẫn>`.
    pub key: String,
    pub store: Store,
    pub spine: Vec<Spine>,
    pub obfuscated: HashMap<String, Obf>,
    chapters: Mutex<Vec<(usize, Arc<Chapter>)>>,
}

impl OpenBook {
    pub fn new(id: String, key: String, store: Store, spine: Vec<Spine>, obfuscated: HashMap<String, Obf>) -> OpenBook {
        OpenBook { id, key, store, spine, obfuscated, chapters: Mutex::new(Vec::new()) }
    }

    pub fn spine_index(&self, path: &str) -> Option<usize> {
        self.spine.iter().position(|s| s.path == path).or_else(|| self.spine.iter().position(|s| s.path.eq_ignore_ascii_case(path)))
    }

    /// Chương đã chuẩn bị (đệm vài chương gần nhất). Việc nặng (tách thẻ cả chương) chạy ở luồng blocking.
    pub async fn chapter(self: &Arc<Self>, s: usize) -> io::Result<Arc<Chapter>> {
        if let Some(c) = lock(&self.chapters).iter().find(|(i, _)| *i == s).map(|(_, c)| Arc::clone(c)) {
            return Ok(c);
        }
        let path = self.spine.get(s).map(|x| x.path.clone()).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "không có chương này"))?;
        let me = Arc::clone(self);
        let c = tokio::task::spawn_blocking(move || -> io::Result<Chapter> {
            let t = std::time::Instant::now();
            let data = me.store.read(&path)?;
            let c = Chapter::build(&data);
            let ms = t.elapsed().as_millis();
            if ms > 300 || c.part_count() > 1 || !c.xhtml {
                crate::dlog(&format!(
                    "sách: chương {s} ({} KB) → {} phần, {}, {ms} ms",
                    data.len() >> 10,
                    c.part_count(),
                    if c.xhtml { "XHTML".to_string() } else { format!("HTML ({})", c.why_html.as_deref().unwrap_or("?")) }
                ));
            }
            Ok(c)
        })
        .await
        .map_err(|e| io::Error::other(e.to_string()))??;
        let c = Arc::new(c);
        let mut v = lock(&self.chapters);
        v.retain(|(i, _)| *i != s);
        v.push((s, Arc::clone(&c)));
        if v.len() > KEEP_CHAPTERS {
            v.remove(0);
        }
        Ok(c)
    }

    /// Đọc tệp phụ (ảnh, CSS, font) — gỡ làm rối font.
    fn resource(&self, path: &str) -> io::Result<Vec<u8>> {
        let mut d = self.store.read(path)?;
        if let Some(o) = self.obfuscated.get(path) {
            epub::deobfuscate(&mut d, o);
        }
        Ok(d)
    }
}

/// Cổng phục vụ sách.
pub struct Server {
    token: String,
    port: Mutex<Option<(u16, tokio::task::AbortHandle)>>,
    port_gate: tokio::sync::Mutex<()>,
    books: Mutex<HashMap<String, Arc<OpenBook>>>,
    /// Cấu hình màn đọc (JSON, app đặt) — chèn vào mọi chương.
    config: Mutex<String>,
}

impl Server {
    pub fn new() -> Arc<Server> {
        let mut seed = [0u8; 8];
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let mix = t.as_nanos() as u64 ^ (std::process::id() as u64).rotate_left(32) ^ (&seed as *const _ as u64);
        seed.copy_from_slice(&mix.to_le_bytes());
        let token: String = seed.iter().map(|b| format!("{b:02x}")).collect();
        Arc::new(Server {
            token,
            port: Mutex::new(None),
            port_gate: tokio::sync::Mutex::new(()),
            books: Mutex::new(HashMap::new()),
            config: Mutex::new("{}".into()),
        })
    }

    pub fn set_config(&self, json: &str) {
        *lock(&self.config) = json.to_string();
    }

    pub fn add(&self, b: Arc<OpenBook>) {
        lock(&self.books).insert(b.id.clone(), b);
    }

    pub fn get(&self, id: &str) -> Option<Arc<OpenBook>> {
        lock(&self.books).get(id).cloned()
    }

    /// Sách đang mở theo khoá thư viện.
    pub fn find_key(&self, key: &str) -> Option<Arc<OpenBook>> {
        lock(&self.books).values().find(|b| b.key == key).cloned()
    }

    pub fn close(&self, id: &str) {
        lock(&self.books).remove(id);
    }

    /// URL gốc của sách (mở cổng nếu chưa).
    pub async fn base_url(self: &Arc<Self>, id: &str) -> io::Result<String> {
        let port = self.ensure_port(true).await?.map(|(p, _)| p).unwrap_or(0);
        Ok(format!("http://127.0.0.1:{port}/b/{}/{id}/", self.token))
    }

    /// Quay lại app: cổng bị iOS thu hồi lúc nằm nền thì mở lại (M23). Trả (cổng, vừa mở lại?); chưa mở lần nào thì None.
    pub async fn revive(self: &Arc<Self>) -> io::Result<Option<(u16, bool)>> {
        self.ensure_port(false).await
    }

    /// Thử: giả iOS thu hồi cổng.
    pub fn drop_port(&self) -> Option<u16> {
        let mut g = lock(&self.port);
        let (port, task) = g.take()?;
        task.abort();
        *g = Some((port, tokio::spawn(async {}).abort_handle()));
        Some(port)
    }

    async fn ensure_port(self: &Arc<Self>, open: bool) -> io::Result<Option<(u16, bool)>> {
        let _gate = self.port_gate.lock().await;
        let old = lock(&self.port).as_ref().map(|(p, _)| *p);
        match old {
            Some(p) if port_alive(p).await => return Ok(Some((p, false))),
            None if !open => return Ok(None),
            _ => {}
        }
        if let Some((_, task)) = lock(&self.port).take() {
            task.abort();
        }
        let (l, port) = bind_local(old.unwrap_or(0)).await?;
        let me = Arc::clone(self);
        let task = tokio::spawn(async move {
            let mut errs = 0u32;
            loop {
                match l.accept().await {
                    Ok((s, _)) => {
                        errs = 0;
                        let me = Arc::clone(&me);
                        tokio::spawn(async move {
                            let _ = me.handle(s).await;
                        });
                    }
                    Err(e) => {
                        errs += 1;
                        if errs == 1 {
                            crate::dlog(&format!("sách: cổng {port} nhận kết nối lỗi: {e}"));
                        }
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        });
        *lock(&self.port) = Some((port, task.abort_handle()));
        if let Some(o) = old {
            crate::dlog(&format!("sách: cổng {o} không nhận kết nối (iOS thu hồi lúc app nằm nền) — mở lại cổng {port}"));
        }
        Ok(Some((port, old.is_some())))
    }

    /// Phần chèn sau `<head>` của chương.
    fn inject(&self, b: &OpenBook, s: usize, p: usize, n: usize, xhtml: bool) -> String {
        // JSON trong <script> của XHTML: không để «<», «&» trần
        let safe = |v: &str| v.replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026");
        let cfg = safe(&lock(&self.config));
        let id = safe(&serde_json::to_string(&b.id).unwrap_or_default());
        let (close, _) = if xhtml { ("/>", ()) } else { (">", ()) };
        format!(
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no\"{close}\
             <script type=\"text/javascript\">window.GBC={cfg};window.GBD={{\"b\":{id},\"s\":{s},\"p\":{p},\"n\":{n},\"x\":{x}}};</script>\
             <script type=\"text/javascript\" src=\"/gb/reader.js?v={READER_V}\"></script>",
            x = xhtml as u8
        )
    }

    async fn handle(self: Arc<Self>, mut s: TcpStream) -> io::Result<()> {
        let _ = s.set_nodelay(true);
        let head = match tokio::time::timeout(Duration::from_secs(15), read_head(&mut s)).await {
            Ok(Ok(h)) => h,
            _ => return Ok(()),
        };
        let Some((method, target)) = request_line(&head) else { return respond(&mut s, "400 Bad Request", "text/plain", b"", 0).await };
        let head_only = method == "HEAD";
        let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
        if path == "/gb/reader.js" {
            return respond_full(&mut s, "200 OK", "text/javascript; charset=utf-8", READER_JS.as_bytes(), 86_400, head_only).await;
        }
        let Some(rest) = path.strip_prefix("/b/") else { return respond(&mut s, "404 Not Found", "text/plain", b"", 0).await };
        let mut it = rest.splitn(3, '/');
        let (tok, bid, file) = (it.next().unwrap_or(""), it.next().unwrap_or(""), it.next().unwrap_or(""));
        if tok != self.token {
            return respond(&mut s, "404 Not Found", "text/plain", b"", 0).await;
        }
        let Some(book) = self.get(bid) else { return respond(&mut s, "404 Not Found", "text/plain", b"", 0).await };
        let file = epub::percent_decode(file);
        if let Some(si) = book.spine_index(&file) {
            let part: usize = query.split('&').find_map(|kv| kv.strip_prefix("p=")).and_then(|v| v.parse().ok()).unwrap_or(0);
            let c = match book.chapter(si).await {
                Ok(c) => c,
                Err(e) => {
                    crate::dlog(&format!("sách: không mở được chương {si}: {e}"));
                    return respond(&mut s, "500 Internal Server Error", "text/plain", e.to_string().as_bytes(), 0).await;
                }
            };
            let n = c.part_count();
            let p = part.min(n.saturating_sub(1));
            let body = c.render(p, &self.inject(&book, si, p, n, c.xhtml));
            let ctype = if c.xhtml { "application/xhtml+xml; charset=utf-8" } else { "text/html; charset=utf-8" };
            return respond_full(&mut s, "200 OK", ctype, &body, 0, head_only).await;
        }
        let me = Arc::clone(&book);
        let f2 = file.clone();
        let data = tokio::task::spawn_blocking(move || me.resource(&f2)).await.map_err(|e| io::Error::other(e.to_string()))?;
        match data {
            Ok(d) => {
                let mt = book.store.mime(&file).unwrap_or_else(|| crate::text::mime_for_path(&file).to_string());
                respond_full(&mut s, "200 OK", &mt, &d, 86_400, head_only).await
            }
            Err(_) => respond(&mut s, "404 Not Found", "text/plain", b"", 0).await,
        }
    }
}

/// Cổng nghe 127.0.0.1:port còn nhận kết nối không (thử kết nối vào chính nó).
async fn port_alive(port: u16) -> bool {
    matches!(tokio::time::timeout(Duration::from_millis(500), TcpStream::connect(("127.0.0.1", port))).await, Ok(Ok(_)))
}

/// Mở cổng nghe 127.0.0.1:want (0 = cổng bất kỳ). Cổng cũ vừa bỏ có thể chưa nhả ngay — thử lại một lúc rồi mới lấy cổng khác.
async fn bind_local(want: u16) -> io::Result<(TcpListener, u16)> {
    if want != 0 {
        for _ in 0..10 {
            match TcpListener::bind(("127.0.0.1", want)).await {
                Ok(l) => return Ok((l, want)),
                Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    }
    let l = TcpListener::bind("127.0.0.1:0").await?;
    let port = l.local_addr()?.port();
    Ok((l, port))
}

/// Đọc phần đầu yêu cầu HTTP (tới dòng trống), tối đa 16 KB.
async fn read_head(s: &mut TcpStream) -> io::Result<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut tmp = [0u8; 1024];
    loop {
        let n = s.read(&mut tmp).await?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "đóng giữa chừng"));
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.windows(2).any(|w| w == b"\n\n") {
            return Ok(String::from_utf8_lossy(&buf).into_owned());
        }
        if buf.len() > 16 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "đầu yêu cầu quá dài"));
        }
    }
}

fn request_line(h: &str) -> Option<(String, String)> {
    let mut first = h.lines().next()?.split_whitespace();
    let method = first.next()?.to_string();
    let target = first.next()?.to_string();
    (method == "GET" || method == "HEAD").then_some((method, target))
}

async fn respond(s: &mut TcpStream, status: &str, ctype: &str, body: &[u8], max_age: u32) -> io::Result<()> {
    respond_full(s, status, ctype, body, max_age, false).await
}

async fn respond_full(s: &mut TcpStream, status: &str, ctype: &str, body: &[u8], max_age: u32, head_only: bool) -> io::Result<()> {
    let cache = if max_age > 0 { format!("max-age={max_age}") } else { "no-store".to_string() };
    let h = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: {cache}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(h.as_bytes()).await?;
    if !head_only {
        s.write_all(body).await?;
    }
    s.shutdown().await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn get(url: &str) -> (String, Vec<u8>) {
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_once('/').unwrap();
        let mut s = TcpStream::connect(host).await.unwrap();
        s.write_all(format!("GET /{path} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes()).await.unwrap();
        let mut all = Vec::new();
        s.read_to_end(&mut all).await.unwrap();
        let split = all.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (String::from_utf8_lossy(&all[..split]).into_owned(), all[split + 4..].to_vec())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn serves_chapter_parts_resources_and_revives() {
        let mut m = HashMap::new();
        let mut big = String::from(r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body><div>"#);
        for _ in 0..4000 {
            big.push_str("Người du khách cuối cùng đã về; người hướng dẫn viên cuối cùng đã lặp lại.<br/>");
        }
        big.push_str("</div></body></html>");
        m.insert("text/c1.xhtml".to_string(), ("application/xhtml+xml".to_string(), Arc::new(big.into_bytes())));
        m.insert("images/a.png".to_string(), ("image/png".to_string(), Arc::new(b"\x89PNG".to_vec())));
        let spine = vec![Spine { path: "text/c1.xhtml".into(), linear: true, size: 0 }];
        let srv = Server::new();
        srv.set_config(r#"{"fs":20,"note":"<b>&"}"#);
        srv.add(Arc::new(OpenBook::new("bk1".into(), "1_1".into(), Store::Mem(m), spine, HashMap::new())));
        let base = srv.base_url("bk1").await.unwrap();
        let (h, body) = get(&format!("{base}text/c1.xhtml?p=1")).await;
        assert!(h.starts_with("HTTP/1.1 200"), "{h}");
        assert!(h.contains("application/xhtml+xml"));
        let body = String::from_utf8(body).unwrap();
        assert!(body.contains(r#""s":0,"p":1"#), "{}", &body[..300]);
        // «<b>&» trong cấu hình phải thành <b>& (XHTML không cho «<», «&» trần trong <script>)
        assert!(body.contains("\\u003cb\\u003e\\u0026"), "{}", &body[..400]);
        assert_eq!(crate::xml::check_xml(&body), Ok(()));
        let (h, body) = get(&format!("{base}images/a.png")).await;
        assert!(h.contains("image/png") && h.contains("max-age"));
        assert_eq!(body, b"\x89PNG");
        let (h, _) = get(&format!("{base}nope.png")).await;
        assert!(h.starts_with("HTTP/1.1 404"));
        let bad = base.replace(&srv.token, "zzzz");
        assert!(get(&format!("{bad}images/a.png")).await.0.starts_with("HTTP/1.1 404"));
        let (h, js) = get(&format!("{}gb/reader.js?v=1", &base[..base.find("/b/").unwrap() + 1])).await;
        assert!(h.contains("text/javascript") && js.len() > 1000);
        // iOS thu hồi cổng → mở lại đúng số cổng
        let port = srv.drop_port().unwrap();
        assert!(!port_alive(port).await);
        assert_eq!(srv.revive().await.unwrap(), Some((port, true)));
        assert!(get(&format!("{base}images/a.png")).await.0.starts_with("HTTP/1.1 200"));
    }
}
