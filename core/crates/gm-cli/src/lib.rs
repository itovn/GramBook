//! gm-cli — chạy thử lõi GramBook trên PC, gọi thẳng lớp dịch vụ mà app dùng (`gm_core::Core`):
//!
//!   gm-cli probe [--dc 2] [--proxy "<tg://proxy…>"]   tạo auth key (không cần đăng nhập)
//!   gm-cli login <thư mục dữ liệu>                    đăng nhập (số điện thoại → mã → mật khẩu 2 lớp) — phiên riêng của PC
//!   gm-cli svc <thư mục> <lệnh> [json] [giây]         một lệnh (docs/FFI.md), in kết quả + sự kiện trong [giây]
//!   gm-cli repl <thư mục>                             nhiều lệnh: mỗi dòng «lệnh {json}»; «wait N» chờ sự kiện N giây
//!   gm-cli book <thư mục> <tệp .epub> [giây]          mở sách trên máy qua cổng nội bộ, thử lấy từng chương (không cần đăng nhập)
//!
//! Kết quả in ra đã che tên sách / tác giả / tên chat (chỉ giữ độ dài) — đặt GM_SVC_TEXT=1 để in đủ.
//! Một phiên chỉ dùng ở một nơi (GramMini D24): đừng chép phiên của máy iOS sang PC.

use std::io::BufRead;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gm_net::{parse_link, GmConnector, ProxyConfig};
use grammers_mtsender::{connect, ServerAddr};
use serde_json::{json, Value};

const DCS: [(i32, &str); 5] = [
    (1, "149.154.175.53:443"),
    (2, "149.154.167.41:443"),
    (3, "149.154.175.100:443"),
    (4, "149.154.167.92:443"),
    (5, "91.108.56.104:443"),
];

fn usage() -> i32 {
    eprintln!(
        "gm-cli probe [--dc 2] [--proxy \"tg://proxy?…\"]
gm-cli login <thư mục dữ liệu>
gm-cli svc <thư mục> <lệnh> [json] [giây]
gm-cli repl <thư mục>
gm-cli book <thư mục> <tệp .epub> [giây]"
    );
    2
}

pub fn run(args: Vec<String>) -> i32 {
    let Some(cmd) = args.first().cloned() else { return usage() };
    let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("LỖI: không tạo được runtime tokio: {e}");
            return 1;
        }
    };
    let rest = &args[1..];
    let r = rt.block_on(async {
        match cmd.as_str() {
            "probe" => {
                let mut dc = 2;
                let mut cfg = ProxyConfig::Direct;
                let mut i = 0;
                while i < rest.len() {
                    match (rest[i].as_str(), rest.get(i + 1)) {
                        ("--dc", Some(v)) => dc = v.parse().map_err(|_| "DC sai".to_string())?,
                        ("--proxy", Some(v)) => cfg = parse_link(v)?,
                        _ => return Err("đối số sai".into()),
                    }
                    i += 2;
                }
                probe(cfg, dc).await
            }
            "login" => login(rest.first().ok_or("thiếu thư mục dữ liệu")?).await,
            "svc" => svc(rest).await,
            "repl" => repl(rest.first().ok_or("thiếu thư mục dữ liệu")?).await,
            "book" => book(rest).await,
            _ => {
                usage();
                Err("lệnh sai".into())
            }
        }
    });
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("LỖI: {e}");
            1
        }
    }
}

async fn probe(cfg: ProxyConfig, dc: i32) -> Result<(), String> {
    let (_, addr) = DCS.iter().find(|(id, _)| *id == dc).ok_or("DC phải từ 1 tới 5")?;
    let addr: SocketAddr = addr.parse().unwrap();
    println!("đường: {cfg:?} → DC{dc} {addr}");
    let conn = GmConnector::new(cfg);
    let t0 = Instant::now();
    let server = ServerAddr::Custom { dc_id: dc, address: addr, connector: conn.clone() };
    tokio::time::timeout(Duration::from_secs(40), connect(conn.transport(), server))
        .await
        .map_err(|_| "tạo auth key quá 40 giây".to_string())?
        .map_err(|e| format!("tạo auth key: {e}"))?;
    println!("auth key: xong sau {} ms", t0.elapsed().as_millis());
    Ok(())
}

fn open(dir: &str, print_events: bool) -> Result<Arc<gm_core::Core>, String> {
    let cfg = gm_core::Config {
        data_dir: dir.into(),
        api_id: option_env!("GM_API_ID").and_then(|v| v.parse().ok()).ok_or("bản dựng chưa có api_id (keys/telegram.txt)")?,
        api_hash: option_env!("GM_API_HASH").unwrap_or("").into(),
        client: gm_net::ClientInfo {
            device_model: "PC".into(),
            system_version: std::env::consts::OS.into(),
            app_version: format!("GramBook {}", env!("CARGO_PKG_VERSION")),
            lang_code: "vi".into(),
        },
    };
    let emit: gm_core::Emit = Arc::new(move |mut v| {
        if print_events {
            mask(&mut v);
            println!("  · sự kiện: {}", short(&v));
        }
    });
    gm_core::Core::start(cfg, emit).map_err(|e| e.msg)
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if std::env::var_os("GM_SVC_FULL").is_some() {
        s
    } else {
        s.chars().take(900).collect()
    }
}

/// Che tên bài, ca sĩ, tên chat, số điện thoại khi in (chỉ giữ độ dài).
fn mask(v: &mut Value) {
    if std::env::var_os("GM_SVC_TEXT").is_some() {
        return;
    }
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                if matches!(k.as_str(), "title" | "artist" | "name" | "src" | "username" | "phone" | "hint") {
                    if let Some(s) = x.as_str() {
                        *x = Value::String(format!("<{} ký tự>", s.chars().count()));
                    }
                } else {
                    mask(x);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(mask),
        _ => {}
    }
}

async fn call(core: &Arc<gm_core::Core>, method: &str, args: Value) {
    let t = Instant::now();
    match core.call(method, args).await {
        Ok(mut v) => {
            mask(&mut v);
            let n = v.as_array().map(|a| format!(" ({} mục)", a.len())).unwrap_or_default();
            println!("{method}: xong sau {} ms{n} → {}", t.elapsed().as_millis(), short(&v));
        }
        Err(e) => println!("{method}: LỖI {} sau {} ms", e.to_json(), t.elapsed().as_millis()),
    }
}

fn read_line(prompt: &str) -> String {
    eprint!("{prompt}");
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
    s.trim().to_string()
}

async fn login(dir: &str) -> Result<(), String> {
    let core = open(dir, false)?;
    if let Ok(v) = core.call("auth.state", json!({})).await {
        if v["authorized"] == json!(true) {
            println!("đã đăng nhập");
            return Ok(());
        }
    }
    let phone = read_line("Số điện thoại (+84…): ");
    core.call("auth.sendCode", json!({"phone": phone})).await.map_err(|e| e.msg)?;
    let code = read_line("Mã Telegram gửi: ");
    match core.call("auth.signIn", json!({"code": code})).await {
        Ok(_) => {}
        Err(e) if e.code == "PASSWORD_NEEDED" => {
            #[cfg(not(target_os = "ios"))]
            let pw = rpassword::prompt_password("Mật khẩu 2 lớp: ").map_err(|e| e.to_string())?;
            #[cfg(target_os = "ios")]
            let pw = read_line("Mật khẩu 2 lớp: ");
            core.call("auth.checkPassword", json!({"password": pw})).await.map_err(|e| e.msg)?;
        }
        Err(e) => return Err(e.msg),
    }
    println!("đăng nhập xong — lõi bắt đầu quét thư viện (chạy «repl» rồi «sync.state {{}}» để xem)");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _ = core.call("app.flush", json!({})).await;
    Ok(())
}

async fn svc(rest: &[String]) -> Result<(), String> {
    let [dir, method, more @ ..] = rest else {
        return Err("svc <thư mục dữ liệu> <lệnh> [json] [giây]".into());
    };
    let args: Value = more.first().map(|s| serde_json::from_str(s).map_err(|e| e.to_string())).transpose()?.unwrap_or(json!({}));
    let wait: u64 = more.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let core = open(dir, true)?;
    call(&core, method, args).await;
    if wait > 0 {
        tokio::time::sleep(Duration::from_secs(wait)).await;
    }
    let _ = core.call("app.flush", json!({})).await;
    Ok(())
}

async fn repl(dir: &str) -> Result<(), String> {
    let core = open(dir, true)?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    eprintln!("mỗi dòng: «lệnh {{json}}», «wait N», «quit»");
    while let Some(line) = rx.recv().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "quit" {
            break;
        }
        if let Some(n) = line.strip_prefix("wait ") {
            tokio::time::sleep(Duration::from_secs(n.trim().parse().unwrap_or(5))).await;
            continue;
        }
        let (m, a) = line.split_once(' ').unwrap_or((line, "{}"));
        match serde_json::from_str::<Value>(a) {
            Ok(a) => call(&core, m, a).await,
            Err(e) => println!("json sai: {e}"),
        }
    }
    let _ = core.call("app.flush", json!({})).await;
    Ok(())
}

/// Mở một EPUB trên máy qua đúng đường của app (book.open → cổng 127.0.0.1), lấy thử từng chương / phần, in số liệu; giữ cổng mở
/// thêm [giây] để thử bằng trình duyệt trên PC.
async fn book(rest: &[String]) -> Result<(), String> {
    let dir = rest.first().ok_or("thiếu thư mục dữ liệu")?;
    let path = rest.get(1).ok_or("thiếu tệp sách")?;
    let hold: u64 = rest.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let core = open(dir, false)?;
    let abs = std::fs::canonicalize(path).map_err(|e| format!("{path}: {e}"))?;
    // canonicalize trên Windows trả «\\?\C:\…» — bỏ tiền tố đó
    let key = format!("file:{}", abs.to_string_lossy().trim_start_matches(VERBATIM));
    let t = Instant::now();
    let info = core.call("book.open", json!({"key": key})).await.map_err(|e| format!("{}: {}", e.code, e.msg))?;
    let base = info["base"].as_str().unwrap_or("").to_string();
    let id = info["id"].as_str().unwrap_or("").to_string();
    println!("mở: {} ms — «{}» / {} — {} chương, {} mục lục", t.elapsed().as_millis(), info["title"].as_str().unwrap_or(""),
             info["author"].as_str().unwrap_or(""), info["spine"].as_array().map(|v| v.len()).unwrap_or(0),
             info["toc"].as_array().map(|v| v.len()).unwrap_or(0));
    println!("gốc: {base}");
    let (mut parts, mut bytes, mut html) = (0usize, 0usize, 0usize);
    let t = Instant::now();
    for (s, sp) in info["spine"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let ch = core.call("book.chapter", json!({"id": id, "s": s})).await.map_err(|e| e.msg)?;
        let n = ch["parts"].as_u64().unwrap_or(1) as usize;
        if ch["xhtml"] != json!(true) {
            html += 1;
        }
        for p in 0..n {
            let url = format!("{base}{}?p={p}", sp["href"].as_str().unwrap_or(""));
            let body = http_get(&url).await?;
            bytes += body.len();
            parts += 1;
        }
    }
    println!("lấy {parts} phần ({} KB, {html} chương HTML) trong {} ms", bytes >> 10, t.elapsed().as_millis());
    if hold > 0 {
        println!("giữ cổng mở {hold} giây…");
        tokio::time::sleep(Duration::from_secs(hold)).await;
    }
    Ok(())
}

const CRLF: &str = "\u{d}\u{a}";
/// Tiền tố đường dẫn «verbatim» của Windows: hai gạch chéo ngược, dấu hỏi, gạch chéo ngược.
const VERBATIM: &str = concat!('\u{5c}', '\u{5c}', '?', '\u{5c}');

async fn http_get(url: &str) -> Result<Vec<u8>, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let rest = url.strip_prefix("http://").ok_or("URL sai")?;
    let (host, path) = rest.split_once('/').ok_or("URL sai")?;
    let mut s = tokio::net::TcpStream::connect(host).await.map_err(|e| e.to_string())?;
    let req = format!("GET /{path} HTTP/1.1{CRLF}Host: {host}{CRLF}{CRLF}");
    s.write_all(req.as_bytes()).await.map_err(|e| e.to_string())?;
    let mut all = Vec::new();
    s.read_to_end(&mut all).await.map_err(|e| e.to_string())?;
    let split = all.windows(4).position(|w| w == [13u8, 10, 13, 10]).ok_or("trả lời sai")?;
    if !all.starts_with(b"HTTP/1.1 200") {
        return Err(format!("{url}: {}", String::from_utf8_lossy(&all[..split])));
    }
    Ok(all[split + 4..].to_vec())
}
