//! Dịch vụ lõi cho app: một tài khoản, lệnh dạng JSON (`call`), sự kiện đẩy lên app (`Emit`). Chạy trong runtime tokio của
//! `gm-ffi`. Phần đăng nhập / proxy / mạng / quét chép từ GramMusic; phần sách viết mới.
//!
//! Dữ liệu ở `data_dir`: `session.json` (phiên), `settings.json` (proxy, chính sách đệm, cấu hình màn đọc), `sources.json` +
//! `library/` (thư viện), `sync.json` (quét), `collections.json`, `reading.json` (vị trí đọc, dấu trang), `offline.json` (sách đã
//! tải về máy), `core.log`; tệp đệm ở `books/`, `covers/`, `avatars/`. Hợp đồng lệnh / sự kiện: docs/FFI.md.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gm_net::{ClientInfo, GmConnector, ProxyConfig};
use grammers_client::client::{LoginToken, PasswordToken, SignInError};
use grammers_client::media::Media;
use grammers_client::message::Message;
use grammers_client::peer::Peer;
use grammers_client::tl;
use grammers_client::update::Update;
use grammers_client::{Client, InvocationError};
use grammers_mtsender::{SenderPool, SenderPoolFatHandle, ServerAddr, UpdatesConfiguration};
use grammers_session::types::{PeerId, PeerRef};
use grammers_session::updates::UpdatesLike;
use grammers_session::Session as _;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::files::{BoxFut, Fetch, FetchError, Files, BLOCK};
use crate::library::{split_key, Book, Library, Sort, Source};
use crate::media::{self, Loc, Origin};
use crate::playlists::{Item, Playlists};
use crate::reading::{Pos, Readings};
use crate::serve::{OpenBook, Server, Spine, Store};
use crate::sync::{Cursor, Overlap, SyncState, PAGE, PHASES};
use crate::{epub, text, FileSession};

/// Hàm đẩy sự kiện lên app (gọi từ luồng của lõi).
pub type Emit = Arc<dyn Fn(Value) + Send + Sync>;

/// Thông tin lúc mở lõi.
pub struct Config {
    pub data_dir: PathBuf,
    pub api_id: i32,
    pub api_hash: String,
    pub client: ClientInfo,
}

/// Lỗi trả cho app: `code` máy đọc (vd `PHONE_CODE_INVALID`, `FLOOD_WAIT`), `msg` câu báo tiếng Việt.
#[derive(Debug)]
pub struct CoreError {
    pub code: String,
    pub msg: String,
    pub extra: Value,
}

impl CoreError {
    pub fn new(code: &str, msg: impl Into<String>) -> Self {
        Self { code: code.into(), msg: msg.into(), extra: Value::Null }
    }

    pub fn to_json(&self) -> Value {
        let mut v = json!({"code": self.code, "msg": self.msg});
        if let (Value::Object(m), Value::Object(x)) = (&mut v, &self.extra) {
            m.extend(x.clone());
        }
        v
    }
}

impl From<InvocationError> for CoreError {
    fn from(e: InvocationError) -> Self {
        match &e {
            InvocationError::Rpc(r) => {
                let msg = match r.name.as_str() {
                    "FLOOD_WAIT" => format!("Telegram bảo chờ {} giây rồi thử lại.", r.value.unwrap_or(0)),
                    "PHONE_NUMBER_INVALID" => "Số điện thoại không đúng.".into(),
                    "PHONE_CODE_INVALID" => "Mã không đúng.".into(),
                    "PHONE_CODE_EXPIRED" => "Mã đã hết hạn — gửi mã mới.".into(),
                    "PHONE_NUMBER_BANNED" => "Số điện thoại này bị Telegram khoá.".into(),
                    "AUTH_KEY_UNREGISTERED" | "SESSION_REVOKED" => "Phiên đăng nhập đã hết — đăng nhập lại.".into(),
                    "SRP_ID_INVALID" => "Phiên kiểm tra mật khẩu đã hết — nhập lại mật khẩu.".into(),
                    "AUTH_KEY_DUPLICATED" => "Phiên đăng nhập bị Telegram huỷ (dùng cùng lúc ở hai nơi) — đăng nhập lại.".into(),
                    "USERNAME_NOT_OCCUPIED" | "USERNAME_INVALID" => "Không có kênh / nhóm nào dùng tên này.".into(),
                    "CHANNEL_PRIVATE" => "Kênh riêng tư — cần vào kênh bằng app Telegram trước.".into(),
                    "CHAT_FORWARDS_RESTRICTED" => "Chat này hạn chế lưu nội dung — không gửi tiếp được.".into(),
                    _ => format!("Telegram báo lỗi {} ({})", r.name, r.code),
                };
                let mut ce = CoreError::new(&r.name, msg);
                if let Some(v) = r.value {
                    ce.extra = json!({"value": v});
                }
                ce
            }
            _ => CoreError::new("NETWORK", format!("Lỗi mạng: {e}")),
        }
    }
}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::new("IO", format!("Lỗi ghi / đọc tệp: {e}"))
    }
}

impl From<FetchError> for CoreError {
    fn from(e: FetchError) -> Self {
        CoreError::new(&e.code, e.msg)
    }
}

type Res = Result<Value, CoreError>;

fn arg_str<'a>(a: &'a Value, k: &str) -> Result<&'a str, CoreError> {
    a.get(k).and_then(Value::as_str).ok_or_else(|| CoreError::new("BAD_ARGS", format!("thiếu «{k}»")))
}

fn arg_keys(a: &Value) -> Vec<String> {
    match a.get("keys") {
        Some(Value::Array(v)) => v.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => a.get("key").and_then(Value::as_str).map(|k| vec![k.to_string()]).unwrap_or_default(),
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Khoá sách trên máy: `file:<đường dẫn>`.
pub const LOCAL_PREFIX: &str = "file:";

/// Mã hoá đường dẫn trong sách thành phần URL (giữ «/», mã %XX ký tự ngoài bộ an toàn).
pub fn url_path(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    for b in p.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Lấy dữ liệu sách qua `upload.getFile` (grammers tự chuyển DC giữ tệp). `file_reference` hết hạn sau vài giờ → gặp
/// FILE_REFERENCE_* thì đọc lại tin để lấy vị trí mới.
struct TgFetch {
    client: Client,
    session: Arc<FileSession>,
    locs: Mutex<HashMap<String, Loc>>,
}

impl TgFetch {
    async fn locate(&self, book: &Book) -> Result<Loc, FetchError> {
        if let Some(l) = lock(&self.locs).get(&book.key()) {
            return Ok(l.clone());
        }
        let fatal = |code: &str, msg: &str| FetchError { code: code.into(), msg: msg.into(), retry: None };
        let pr = peer_ref_of_key(&self.session, &book.peer).await.ok_or_else(|| fatal("PEER_UNKNOWN", "Chưa biết chat chứa sách — quét lại thư viện."))?;
        let msg = match self.client.get_messages_by_id(pr, &[book.id]).await {
            Ok(v) => v.into_iter().flatten().next(),
            Err(e) => return Err(fetch_err(e)),
        };
        let Some(Media::Document(d)) = msg.as_ref().and_then(|m| m.media()) else {
            return Err(fatal("NO_MEDIA", "Tin chứa sách này đã bị xoá."));
        };
        let loc = media::document_loc(&d).ok_or_else(|| fatal("NO_MEDIA", "Tệp sách không tải được."))?;
        lock(&self.locs).insert(book.key(), loc.clone());
        Ok(loc)
    }
}

/// Lỗi Telegram → lỗi lấy dữ liệu (mạng / FLOOD_WAIT thì thử lại).
fn fetch_err(e: InvocationError) -> FetchError {
    match &e {
        InvocationError::Rpc(r) if r.name == "FLOOD_WAIT" => FetchError {
            code: "FLOOD_WAIT".into(),
            msg: format!("Telegram bảo chờ {} giây.", r.value.unwrap_or(5)),
            retry: Some(Duration::from_secs(r.value.unwrap_or(5) as u64)),
        },
        InvocationError::Rpc(r) => FetchError { code: r.name.clone(), msg: CoreError::from(e).msg, retry: None },
        _ => FetchError { code: "NETWORK".into(), msg: format!("Lỗi mạng: {e}"), retry: Some(Duration::from_secs(2)) },
    }
}

impl Fetch for TgFetch {
    fn fetch<'a>(&'a self, book: &'a Book, offset: u64) -> BoxFut<'a, Result<Vec<u8>, FetchError>> {
        Box::pin(async move {
            for attempt in 0..2 {
                let loc = self.locate(book).await?;
                let mut it = self.client.iter_download(&loc).chunk_size(BLOCK as i32).skip_chunks((offset / BLOCK) as i32);
                match tokio::time::timeout(Duration::from_secs(40), it.next()).await {
                    Ok(Ok(Some(d))) => return Ok(d),
                    Ok(Ok(None)) => return Ok(Vec::new()),
                    Ok(Err(InvocationError::Rpc(r))) if r.name.starts_with("FILE_REFERENCE") && attempt == 0 => {
                        lock(&self.locs).remove(&book.key());
                        continue;
                    }
                    Ok(Err(e)) => return Err(fetch_err(e)),
                    Err(_) => {
                        return Err(FetchError { code: "TIMEOUT".into(), msg: "Tải sách quá lâu.".into(), retry: Some(Duration::from_secs(1)) })
                    }
                }
            }
            Err(FetchError { code: "FILE_REFERENCE".into(), msg: "Không lấy được tệp sách.".into(), retry: None })
        })
    }
}

async fn peer_ref_of_key(session: &FileSession, key: &str) -> Option<PeerRef> {
    let id = key.parse::<i64>().ok().and_then(PeerId::from_bot_api_dialog_id)?;
    session.peer_ref(id).await.ok().flatten()
}

pub struct Core {
    cfg: Config,
    client: Client,
    session: Arc<FileSession>,
    conn: Arc<GmConnector>,
    emit: Emit,
    handle: SenderPoolFatHandle,
    login: Mutex<Option<LoginToken>>,
    password: Mutex<Option<PasswordToken>>,
    updates: Mutex<Option<mpsc::Receiver<UpdatesLike>>>,
    log: Mutex<Option<std::fs::File>>,
    /// Mạng: 0 chưa biết, 1 kết nối được, 2 mất kết nối (sự kiện `net.state` khi đổi).
    net: AtomicU8,
    /// Phiên đã bị Telegram huỷ — tệp phiên đã dời đi, chờ app mở lại để đăng nhập mới.
    dead: AtomicBool,
    /// Tải bìa / ảnh nguồn: tối đa 3 tệp cùng lúc.
    dl: tokio::sync::Semaphore,
    cache: crate::cache::Cache,
    lib: Arc<Mutex<Library>>,
    pls: Mutex<Playlists>,
    reads: Mutex<Readings>,
    files: Arc<Files>,
    server: Arc<Server>,
    open_seq: AtomicU64,
    /// Sách đã tải về máy (`offline.json`): không tự xoá.
    offline: Mutex<HashSet<String>>,
    /// Hàng tải về máy (tải lần lượt từng cuốn).
    dlq: Mutex<VecDeque<String>>,
    dl_running: AtomicBool,
    syncing: AtomicBool,
    sync_stop: AtomicBool,
    /// Nguồn thêm tay trong lúc đang quét: quét riêng ngay sau lượt đang chạy.
    sync_pending: Mutex<Vec<String>>,
    sync_info: Mutex<Value>,
    /// Khoá peer của tài khoản (Tin nhắn đã lưu).
    me: Mutex<Option<String>>,
}

impl Core {
    /// Mở lõi (không đụng mạng). Phải gọi trong runtime tokio.
    pub fn start(cfg: Config, emit: Emit) -> Result<Arc<Core>, CoreError> {
        std::fs::create_dir_all(&cfg.data_dir).map_err(|e| CoreError::new("IO", format!("tạo thư mục dữ liệu: {e}")))?;
        let session = Arc::new(
            FileSession::open(cfg.data_dir.join("session.json")).map_err(|e| CoreError::new("IO", e.to_string()))?,
        );
        let settings = Self::settings(&cfg.data_dir);
        let proxy = settings.get("proxy").and_then(Value::as_str).and_then(|l| gm_net::parse_link(l).ok()).unwrap_or_default();
        gm_net::dns::set_enabled(settings.get("secure_dns").and_then(Value::as_bool).unwrap_or(true));
        let policy = {
            let d = crate::cache::Policy::default();
            crate::cache::Policy {
                limit_mb: settings.get("cache_limit_mb").and_then(Value::as_u64).unwrap_or(d.limit_mb),
                keep_days: settings.get("cache_keep_days").and_then(Value::as_u64).unwrap_or(d.keep_days),
            }
        };
        let conn = GmConnector::new(proxy);
        let params = conn.connection_params(cfg.client.clone());
        let SenderPool { runner, updates, handle } = SenderPool::with_configuration(Arc::clone(&session), cfg.api_id, params);
        let client = Client::new(handle.clone());
        tokio::spawn(runner.run());
        let log_path = cfg.data_dir.join("core.log");
        if std::fs::metadata(&log_path).map(|m| m.len() > 1 << 20).unwrap_or(false) {
            let _ = std::fs::remove_file(&log_path);
        }
        let log = std::fs::OpenOptions::new().create(true).append(true).open(&log_path).ok();
        let lib = Arc::new(Mutex::new(Library::open(&cfg.data_dir)));
        let fetch = Arc::new(TgFetch { client: client.clone(), session: Arc::clone(&session), locs: Mutex::new(HashMap::new()) });
        let files = Files::new(cfg.data_dir.join("books"), fetch, Arc::clone(&emit));
        let server = Server::new();
        if let Some(c) = settings.get("reader") {
            server.set_config(&c.to_string());
        }
        let offline: HashSet<String> = std::fs::read(cfg.data_dir.join("offline.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .collect();
        let me = settings.get("me").and_then(Value::as_str).map(str::to_string);
        let pls = Playlists::open(&cfg.data_dir);
        let reads = Readings::open(&cfg.data_dir);
        let core = Arc::new(Core {
            cfg,
            client,
            session,
            conn,
            emit,
            handle,
            login: Mutex::new(None),
            password: Mutex::new(None),
            updates: Mutex::new(Some(updates)),
            log: Mutex::new(log),
            net: AtomicU8::new(0),
            dead: AtomicBool::new(false),
            dl: tokio::sync::Semaphore::new(3),
            cache: crate::cache::Cache::new(policy),
            lib,
            pls: Mutex::new(pls),
            reads: Mutex::new(reads),
            files,
            server,
            open_seq: AtomicU64::new(1),
            offline: Mutex::new(offline),
            dlq: Mutex::new(VecDeque::new()),
            dl_running: AtomicBool::new(false),
            syncing: AtomicBool::new(false),
            sync_stop: AtomicBool::new(false),
            sync_pending: Mutex::new(Vec::new()),
            sync_info: Mutex::new(json!({"running": false})),
            me: Mutex::new(me),
        });
        core.log(&format!("mở lõi, proxy: {:?}", core.conn.config()));
        let weak = Arc::downgrade(&core);
        let weak2 = weak.clone();
        gm_net::dns::set_log(Arc::new(move |m: &str| {
            if let Some(c) = weak2.upgrade() {
                c.log(m);
            }
        }));
        crate::set_logger(Arc::new(move |m: &str| {
            if let Some(c) = weak.upgrade() {
                c.log(m);
            }
        }));
        // Mất kết nối thì 10 giây hỏi lại một lần (updates.getState — rẻ) tới khi được
        let h = Arc::clone(&core);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                if h.net.load(Ordering::Relaxed) != 1 {
                    h.check_net().await;
                }
                let _ = lock(&h.reads).flush();
            }
        });
        // Tự dọn bộ nhớ đệm: 20 giây sau khi mở, rồi 30 phút một lần
        let h = Arc::clone(&core);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            loop {
                h.trim_soon(true);
                tokio::time::sleep(Duration::from_secs(30 * 60)).await;
            }
        });
        // Đã đăng nhập thì nghe cập nhật (sách mới gửi vào các nguồn), rồi tải tiếp hàng tải về máy còn dở
        let c = Arc::clone(&core);
        tokio::spawn(async move {
            match c.client.is_authorized().await {
                Ok(true) => {
                    (c.emit)(json!({"e": "auth.state", "authorized": true}));
                    c.after_login().await;
                }
                Ok(false) => (c.emit)(json!({"e": "auth.state", "authorized": false})),
                Err(e) => {
                    c.log(&format!("is_authorized: {e}"));
                    let ce = CoreError::from(e);
                    c.check_dead(&ce.code);
                }
            }
        });
        Ok(core)
    }

    /// Sau khi đăng nhập / mở lõi đã đăng nhập: nhớ mã tài khoản, nghe cập nhật, tải tiếp sách chờ tải về máy.
    async fn after_login(self: &Arc<Self>) {
        if lock(&self.me).is_none() {
            if let Ok(me) = self.client.get_me().await {
                let key = peer_key(me.id());
                let _ = self.save_settings(&[("me", json!(key))]);
                *lock(&self.me) = Some(key);
            }
        }
        self.start_updates();
        let c = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            c.start_sync(false, None);
        });
        let pending: Vec<String> = {
            let off = lock(&self.offline);
            let lib = lock(&self.lib);
            off.iter().filter(|k| lib.book(k).is_some_and(|b| self.files.complete_path(b).is_none())).cloned().collect()
        };
        if !pending.is_empty() {
            lock(&self.dlq).extend(pending);
            self.run_downloads();
        }
    }

    async fn password_token(&self) -> Result<PasswordToken, CoreError> {
        let p: tl::types::account::Password = self.client.invoke(&tl::functions::account::GetPassword {}).await?.into();
        Ok(PasswordToken::new(p))
    }

    /// Phiên chết (AUTH_KEY_DUPLICATED, SESSION_REVOKED, tài khoản khoá) → dời `session.json`, báo app (GramMini D24).
    fn check_dead(&self, code: &str) -> bool {
        if !matches!(code, "AUTH_KEY_DUPLICATED" | "SESSION_REVOKED" | "USER_DEACTIVATED" | "USER_DEACTIVATED_BAN") {
            return false;
        }
        if self.dead.swap(true, Ordering::SeqCst) {
            return true;
        }
        let from = self.cfg.data_dir.join("session.json");
        let to = self.cfg.data_dir.join(format!("session.dead-{}.json", now()));
        let moved = std::fs::rename(&from, &to).is_ok();
        self.log(&format!("phiên chết ({code}) — dời tệp phiên: {moved}"));
        (self.emit)(json!({"e": "auth.dead", "code": code}));
        (self.emit)(json!({"e": "auth.state", "authorized": false, "dead": true}));
        true
    }

    /// Tải tệp nhỏ (bìa, ảnh nguồn) về `path` qua tệp tạm riêng của lần tải.
    async fn download(self: &Arc<Self>, loc: &Loc, path: &std::path::Path) -> Result<(), CoreError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = part_path(path);
        let fetch = async {
            let mut buf = Vec::new();
            let mut it = self.client.iter_download(loc);
            while let Some(chunk) = it.next().await? {
                buf.extend_from_slice(&chunk);
            }
            std::fs::write(&tmp, &buf)?;
            Ok::<(), CoreError>(())
        };
        match tokio::time::timeout(Duration::from_secs(60), fetch).await {
            Ok(Ok(())) => {
                std::fs::rename(&tmp, path)?;
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
            Err(_) => {
                let _ = std::fs::remove_file(&tmp);
                Err(CoreError::new("TIMEOUT", "Tải quá lâu."))
            }
        }
    }

    // ───────────────────────── Quét thư viện (như GramMusic) ─────────────────────────

    fn me_key(&self) -> Option<String> {
        lock(&self.me).clone()
    }

    /// Nguồn mới cho sách lấy từ tin `m`.
    fn source_for(&self, m: &Message, key: &str) -> Source {
        let me = self.me_key();
        let mut s = match m.peer() {
            Some(p) => source_from_peer(p),
            None => Source { name: format!("Chat {key}"), kind: "user".into(), enabled: true, ..Default::default() },
        };
        if me.as_deref() == Some(key) {
            s.kind = "self".into();
            s.name = "Tin nhắn đã lưu".into();
        }
        s.peer = key.to_string();
        s.added = now();
        s
    }

    /// Tin nằm ở chat riêng với người (không phải Tin nhắn đã lưu, bot, nhóm, kênh) — DECISIONS B4.
    fn origin_of(&self, m: &Message, key: &str) -> Origin {
        if key.starts_with('-') || self.me_key().as_deref() == Some(key) {
            return Origin::Other;
        }
        match m.peer() {
            Some(Peer::User(u)) if u.is_bot() => Origin::Other,
            _ => Origin::Private,
        }
    }

    /// Một trang kết quả tìm trong một chat (lọc tài liệu) → (tin, next_rate, tổng máy chủ báo, đã trả hết trong một lần).
    async fn search_page(&self, peer: Option<&PeerRef>, _filter: &str, cur: &Cursor) -> Result<(Vec<Message>, Option<i32>, Option<i32>, bool), CoreError> {
        use tl::enums::messages::Messages;
        let filter = tl::enums::MessagesFilter::InputMessagesFilterDocument;
        let resp = match peer {
            Some(p) => {
                self.client
                    .invoke(&tl::functions::messages::Search {
                        peer: p.clone().into(),
                        q: String::new(),
                        from_id: None,
                        saved_peer_id: None,
                        saved_reaction: None,
                        top_msg_id: None,
                        filter,
                        min_date: 0,
                        max_date: 0,
                        offset_id: cur.offset_id,
                        add_offset: 0,
                        limit: PAGE,
                        max_id: 0,
                        min_id: 0,
                        hash: 0,
                    })
                    .await?
            }
            None => return Ok((Vec::new(), None, None, true)),
        };
        let (msgs, users, chats, rate, total, all) = match resp {
            Messages::Messages(m) => (m.messages, m.users, m.chats, None, None, true),
            Messages::Slice(m) => (m.messages, m.users, m.chats, m.next_rate, Some(m.count), false),
            Messages::ChannelMessages(m) => (m.messages, m.users, m.chats, None, Some(m.count), false),
            Messages::NotModified(_) => (Vec::new(), Vec::new(), Vec::new(), None, None, true),
        };
        let peers = self.client.build_peer_map(users, chats).await;
        let out = msgs.into_iter().map(|m| Message::from_raw(&self.client, m, peer.cloned(), peers.clone())).collect();
        Ok((out, rate, total, all))
    }

    /// Gộp sách của một trang vào thư viện. Trả (số sách mới, các sách theo thứ tự: đã có hay chưa).
    fn absorb(&self, msgs: &[Message]) -> Result<(usize, Vec<bool>), CoreError> {
        let mut groups: HashMap<String, (Vec<Book>, usize)> = HashMap::new();
        let mut known = Vec::new();
        let mut lib = lock(&self.lib);
        for (i, m) in msgs.iter().enumerate() {
            let key = peer_key(m.peer_id());
            if let Some(b) = media::book_from(m, &key, self.origin_of(m, &key)) {
                known.push(lib.index(&key).is_some_and(|ix| ix.has(b.id)));
                groups.entry(key).or_insert_with(|| (Vec::new(), i)).0.push(b);
            }
        }
        let mut added = 0;
        for (key, (books, i)) in groups {
            let m = &msgs[i];
            added += lib.add_books(&key, books, || self.source_for(m, &key))?;
        }
        Ok((added, known))
    }

    fn set_sync_info(&self, v: Value) {
        *lock(&self.sync_info) = v.clone();
        let mut ev = v;
        ev["e"] = json!("sync.state");
        (self.emit)(ev);
    }

    fn library_changed(&self) {
        let (gen, total) = {
            let mut lib = lock(&self.lib);
            let t = lib.total();
            (lib.generation, t)
        };
        (self.emit)(json!({"e": "library.changed", "gen": gen, "total": total}));
    }

    /// Chạy một lượt tìm trong một chat tới hết / bắt kịp. `full`: quét hết, không dừng sớm.
    async fn scan_phase(
        self: &Arc<Self>,
        peer: Option<&PeerRef>,
        phase: &str,
        mut cur: Cursor,
        full: bool,
        since: i64,
        st: &mut SyncState,
        found: &mut usize,
    ) -> Result<(), CoreError> {
        let since = if full { 0 } else { since };
        let mut overlap = Overlap::default();
        let mut fetched = 0usize;
        let mut pages_since_flush = 0;
        let mut flood_tries = 0;
        loop {
            if self.sync_stop.load(Ordering::Relaxed) {
                return Err(CoreError::new("STOPPED", "Đã dừng quét."));
            }
            let (msgs, rate, total, all) = match self.search_page(peer, phase, &cur).await {
                Ok(r) => {
                    flood_tries = 0;
                    r
                }
                Err(e) if e.code == "FLOOD_WAIT" && flood_tries < 3 => {
                    flood_tries += 1;
                    let wait = e.extra.get("value").and_then(Value::as_u64).unwrap_or(3).max(1);
                    if wait > 600 {
                        return Err(e);
                    }
                    self.set_sync_info(json!({"running": true, "scope": "chat", "phase": phase, "pages": st.pages, "found": *found,
                                              "chat": st.chat_i, "chats": st.chat_n, "wait": wait, "try": flood_tries}));
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                    continue;
                }
                Err(e) => return Err(e),
            };
            if msgs.is_empty() {
                break;
            }
            let (added, known) = self.absorb(&msgs)?;
            *found += added;
            st.pages += 1;
            fetched += msgs.len();
            self.log(&format!("quét {phase}: trang {} — {} tin, {} sách, {added} sách mới, máy chủ báo tổng {:?}{}", st.pages, msgs.len(),
                              known.len(), total, if all { ", trả hết một lần" } else { "" }));
            let mut caught_up = false;
            if !full {
                for k in known {
                    if overlap.see(k) {
                        caught_up = true;
                        break;
                    }
                }
            }
            let last = msgs.last().expect("trang có tin");
            if since > 0 && last.date().as_second() < since {
                caught_up = true;
            }
            let next = Cursor { offset_rate: rate.unwrap_or(0), offset_peer: String::new(), offset_id: last.id() };
            if next == cur {
                break;
            }
            cur = next;
            pages_since_flush += 1;
            if pages_since_flush >= 5 {
                pages_since_flush = 0;
                lock(&self.lib).flush()?;
                self.library_changed();
            }
            self.set_sync_info(json!({"running": true, "scope": "chat", "phase": phase, "pages": st.pages, "found": *found,
                                      "chat": st.chat_i, "chats": st.chat_n}));
            let exhausted = all || total.is_some_and(|t| fetched >= t.max(0) as usize);
            if caught_up || exhausted || cur.offset_id <= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
        lock(&self.lib).flush()?;
        Ok(())
    }

    /// Quét cả tài khoản (`full` = quét toàn bộ / tiếp lần dở; không thì lấy sách mới), rồi các nguồn thêm tay. Chạy nền, báo
    /// `sync.state`; xong báo `library.changed`.
    fn start_sync(self: &Arc<Self>, full: bool, only: Option<String>) -> bool {
        if self.syncing.swap(true, Ordering::AcqRel) {
            if let Some(p) = only {
                let mut q = lock(&self.sync_pending);
                if !q.contains(&p) {
                    q.push(p);
                }
                return true;
            }
            return false;
        }
        self.sync_stop.store(false, Ordering::Relaxed);
        let c = Arc::clone(self);
        tokio::spawn(async move {
            let t = Instant::now();
            let mut st = SyncState::load(&c.cfg.data_dir);
            let mut found = 0usize;
            let r = Box::pin(c.sync_run(full, only.as_deref(), &mut st, &mut found)).await;
            let err = r.as_ref().err().map(|e| e.to_json());
            if let Err(e) = &r {
                c.log(&format!("quét lỗi {} sau {} ms", e.code, t.elapsed().as_millis()));
                st.last_error = Some(e.msg.clone());
            } else {
                c.log(&format!("quét xong: {found} sách mới, {} trang, {} ms", st.pages, t.elapsed().as_millis()));
                st.last_error = None;
            }
            let _ = st.save(&c.cfg.data_dir);
            c.syncing.store(false, Ordering::Release);
            c.set_sync_info(json!({"running": false, "found": found, "error": err, "last_sync": st.last_sync, "full_done": st.full_done}));
            c.library_changed();
            let next = {
                let mut q = lock(&c.sync_pending);
                (!q.is_empty()).then(|| q.remove(0))
            };
            if let Some(p) = next {
                if !c.sync_stop.load(Ordering::Relaxed) {
                    c.start_sync(true, Some(p));
                }
            }
        });
        true
    }

    /// Các chat của tài khoản (cả Lưu trữ): (khoá peer, ngày tin mới nhất). grammers lưu luôn access_hash vào phiên.
    async fn dialog_list(&self) -> Result<Vec<(String, i64)>, CoreError> {
        let mut it = self.client.iter_dialogs();
        let mut out = Vec::new();
        let mut fresh = Vec::new();
        while let Some(d) = it.next().await? {
            let date = d.last_message.as_ref().map(|m| m.date().as_second()).unwrap_or(0);
            let key = peer_key(d.peer().id());
            out.push((key.clone(), date));
            fresh.push((key, source_from_peer(d.peer())));
            if out.len() >= 3000 {
                break;
            }
        }
        self.save_session();
        let mut lib = lock(&self.lib);
        for (key, src) in fresh {
            if let Some(old) = lib.source(&key) {
                if old.name != src.name || old.photo != src.photo || old.forum != src.forum || old.username != src.username {
                    let mut s = src;
                    s.kind = if old.kind == "self" { old.kind.clone() } else { s.kind };
                    s.name = if old.kind == "self" { old.name.clone() } else { s.name };
                    let _ = lib.upsert_source(s);
                }
            }
        }
        Ok(out)
    }

    /// Quét thư viện: lần đầu (hoặc `full`, hoặc lần quét toàn bộ trước còn dở) quét hết từng chat; các lần sau chỉ chat có tin mới
    /// từ lần trước (và chat chưa quét bao giờ). Rồi các nguồn thêm tay (kênh công khai chưa vào). (GramMusic M7)
    async fn sync_run(self: &Arc<Self>, full: bool, only: Option<&str>, st: &mut SyncState, found: &mut usize) -> Result<(), CoreError> {
        if let Some(peer) = only {
            return Box::pin(self.sync_chat(peer, full, st, found)).await;
        }
        let t = Instant::now();
        let dialogs = self.dialog_list().await?;
        self.log(&format!("quét: {} chat (đọc danh sách {} ms)", dialogs.len(), t.elapsed().as_millis()));
        let discovery = full || !st.full_done || st.phase == "chats" || st.version < crate::sync::VERSION;
        if discovery {
            if st.phase != "chats" || full {
                st.phase = "chats".into();
                st.done.clear();
                st.pages = 0;
            }
            let done: HashSet<String> = st.done.iter().cloned().collect();
            st.chat_n = dialogs.len();
            for (i, (peer, _)) in dialogs.iter().enumerate() {
                st.chat_i = i + 1;
                if done.contains(peer) {
                    continue;
                }
                if let Err(e) = Box::pin(self.sync_chat(peer, true, st, found)).await {
                    if matches!(e.code.as_str(), "STOPPED" | "NETWORK" | "TIMEOUT" | "FLOOD_WAIT") {
                        return Err(e);
                    }
                    self.log(&format!("quét chat {}: lỗi {} — bỏ qua", i + 1, e.code));
                }
                st.done.push(peer.clone());
                if i % 5 == 4 {
                    let _ = st.save(&self.cfg.data_dir);
                    self.library_changed();
                }
                tokio::time::sleep(Duration::from_millis(80)).await;
            }
            st.phase.clear();
            st.done.clear();
            st.full_done = true;
            st.version = crate::sync::VERSION;
        } else {
            let since = st.last_sync - 86_400;
            let todo: Vec<String> =
                dialogs.iter().filter(|(p, date)| *date >= since || !st.chats.contains_key(p.as_str())).map(|(p, _)| p.clone()).collect();
            st.chat_n = todo.len();
            for (i, peer) in todo.iter().enumerate() {
                st.chat_i = i + 1;
                if let Err(e) = Box::pin(self.sync_chat(peer, false, st, found)).await {
                    if matches!(e.code.as_str(), "STOPPED" | "NETWORK" | "TIMEOUT") {
                        return Err(e);
                    }
                    self.log(&format!("quét chat: lỗi {} — bỏ qua", e.code));
                }
            }
        }
        let in_dialogs: HashSet<&String> = dialogs.iter().map(|(p, _)| p).collect();
        let manual: Vec<String> =
            lock(&self.lib).sources().iter().filter(|s| s.manual && !in_dialogs.contains(&s.peer)).map(|s| s.peer.clone()).collect();
        for p in manual {
            if let Err(e) = Box::pin(self.sync_chat(&p, false, st, found)).await {
                if e.code == "STOPPED" {
                    return Err(e);
                }
                self.log(&format!("quét nguồn thêm tay: lỗi {}", e.code));
            }
        }
        st.chat_i = 0;
        st.chat_n = 0;
        st.last_sync = now();
        Ok(())
    }

    /// Quét riêng một chat. Chưa quét bao giờ (hoặc `full`) thì quét hết; không thì chỉ lấy tin mới hơn lần quét trước (lùi 1 ngày).
    async fn sync_chat(self: &Arc<Self>, peer: &str, full: bool, st: &mut SyncState, found: &mut usize) -> Result<(), CoreError> {
        let pr = peer_ref_of_key(&self.session, peer).await.ok_or_else(|| CoreError::new("PEER_UNKNOWN", "Chưa biết chat này."))?;
        let last = st.chats.get(peer).copied();
        let deep = full || last.is_none();
        let since = last.map(|t| t - 86_400).unwrap_or(0);
        for phase in PHASES {
            Box::pin(self.scan_phase(Some(&pr), phase, Cursor::default(), deep, since, st, found)).await?;
        }
        st.chats.insert(peer.to_string(), now());
        let mut lib = lock(&self.lib);
        lib.set_complete(peer, true);
        let _ = lib.update_source(peer, |s| s.synced = now());
        lib.flush()?;
        Ok(())
    }

    /// Thêm nguồn bằng @tên / link t.me (kênh / nhóm công khai, kể cả chưa vào) rồi quét riêng.
    async fn add_source(self: &Arc<Self>, link: &str) -> Res {
        let name = username_of(link).ok_or_else(|| CoreError::new("BAD_LINK", "Nhập @tên hoặc link t.me/… của kênh / nhóm."))?;
        let p = self.client.resolve_username(&name).await?.ok_or_else(|| CoreError::new("USERNAME_NOT_OCCUPIED", "Không có kênh / nhóm nào dùng tên này."))?;
        let key = peer_key(p.id());
        let mut s = source_from_peer(&p);
        s.peer = key.clone();
        s.manual = true;
        s.added = now();
        lock(&self.lib).upsert_source(s)?;
        self.save_session();
        let started = self.start_sync(true, Some(key.clone()));
        let src = lock(&self.lib).source(&key).cloned();
        Ok(json!({"source": src.map(|s| self.source_json(&s)), "syncing": started}))
    }

    // ───────────────────────── Sách → JSON ─────────────────────────

    fn source_json(&self, s: &Source) -> Value {
        let n = lock(&self.lib).indexed(&s.peer);
        json!({"peer": s.peer, "name": s.name, "kind": s.kind, "username": s.username, "photo": s.photo, "photo_dc": s.photo_dc,
               "enabled": s.enabled, "manual": s.manual, "count": n, "synced": s.synced, "forum": s.forum})
    }

    /// Trạng thái tệp: "offline" (đã tải về máy), "queued" (chờ / đang tải về máy), "cached" (đã mở — có trong đệm), null.
    fn dl_state(&self, b: &Book, offline: &HashSet<String>) -> Value {
        let done = self.files.complete_path(b).is_some();
        match (offline.contains(&b.key()), done) {
            (true, true) => json!("offline"),
            (true, false) => json!("queued"),
            (false, true) => json!("cached"),
            _ => Value::Null,
        }
    }

    fn books_json(&self, books: &[Book]) -> Vec<Value> {
        let offline = lock(&self.offline).clone();
        let names: HashMap<String, String> = lock(&self.lib).sources().iter().map(|s| (s.peer.clone(), s.name.clone())).collect();
        let pls = lock(&self.pls);
        let reads = lock(&self.reads);
        books
            .iter()
            .map(|b| {
                let key = b.key();
                let r = reads.get(&key);
                json!({
                    "key": key, "peer": b.peer, "id": b.id, "title": b.shown_title(), "author": b.shown_author(), "ext": b.ext,
                    "fmt": text::format_group(&b.ext), "size": b.size, "date": b.date, "thumb": b.thumb, "ok": b.readable(),
                    "src": names.get(&b.peer), "caption": text::truncate_chars(&b.caption, 200), "topic": b.topic,
                    "fav": pls.is_fav(&key), "want": pls.is_want(&key), "dl": self.dl_state(b, &offline),
                    "progress": r.map(|r| r.progress), "finished": r.map(|r| r.finished).unwrap_or(false), "nf": b.nf,
                })
            })
            .collect()
    }

    /// Bộ lọc của `library.query`: `filter` all | offline (đã tải về máy) | cached (có trên máy) | readable (mở được) | unread |
    /// reading (đang đọc dở) | done (đã đọc xong); `fmt` epub | kindle | pdf | text | other; `topic`.
    fn keep_fn(&self, a: &Value) -> Box<dyn Fn(&Book) -> bool + '_> {
        let topic = a.get("topic").and_then(Value::as_i64).map(|t| t as i32);
        let fmt = a.get("fmt").and_then(Value::as_str).filter(|f| !f.is_empty()).map(str::to_string);
        let base = move |b: &Book| {
            topic.is_none_or(|t| b.topic == t || (t == 1 && b.topic == 0)) && fmt.as_deref().is_none_or(|f| text::format_group(&b.ext) == f)
        };
        let filter = a.get("filter").and_then(Value::as_str).unwrap_or("all").to_string();
        match filter.as_str() {
            "offline" => {
                let off = lock(&self.offline).clone();
                Box::new(move |b: &Book| base(b) && off.contains(&b.key()))
            }
            "cached" => Box::new(move |b: &Book| base(b) && self.files.complete_path(b).is_some()),
            "readable" => Box::new(move |b: &Book| base(b) && b.readable()),
            "unread" | "reading" | "done" => {
                let reads = lock(&self.reads);
                let state: HashMap<String, bool> = reads.recent(100_000, None).into_iter().map(|r| (r.key.clone(), r.finished)).collect();
                drop(reads);
                Box::new(move |b: &Book| {
                    base(b)
                        && match (filter.as_str(), state.get(&b.key())) {
                            ("unread", s) => s.is_none(),
                            ("reading", Some(false)) => true,
                            ("done", Some(true)) => true,
                            _ => false,
                        }
                })
            }
            _ => Box::new(base),
        }
    }

    /// Chủ đề của nhóm diễn đàn (`messages.getForumTopics`, tối đa 500); mất mạng thì dùng bản đã lưu.
    async fn topics(&self, peer: &str) -> Res {
        let pr = peer_ref_of_key(&self.session, peer).await.ok_or_else(|| CoreError::new("PEER_UNKNOWN", "Chưa biết nhóm này."))?;
        let mut fetched: Vec<crate::library::Topic> = Vec::new();
        let (mut od, mut oid, mut otop) = (0, 0, 0);
        let mut err = None;
        while fetched.len() < 500 {
            let r = self
                .client
                .invoke(&tl::functions::messages::GetForumTopics { peer: pr.into(), q: None, offset_date: od, offset_id: oid, offset_topic: otop, limit: 100 })
                .await;
            let tl::enums::messages::ForumTopics::Topics(r) = match r {
                Ok(r) => r,
                Err(e) => {
                    err = Some(CoreError::from(e));
                    break;
                }
            };
            let n = r.topics.len();
            let total = r.count as usize;
            for t in r.topics {
                if let tl::enums::ForumTopic::Topic(t) = t {
                    od = t.date;
                    oid = t.top_message;
                    otop = t.id;
                    fetched.push(crate::library::Topic { id: t.id, title: t.title, color: t.icon_color });
                }
            }
            if n < 100 || fetched.len() >= total {
                break;
            }
        }
        let mut lib = lock(&self.lib);
        if !fetched.is_empty() {
            let _ = lib.update_source(peer, |s| {
                s.forum = true;
                s.topics = fetched.clone();
            });
        } else if let Some(e) = err.filter(|_| lib.source(peer).is_none_or(|s| s.topics.is_empty())) {
            return Err(e);
        }
        let counts = lib.topic_counts(peer);
        let src = lib.source(peer).cloned().unwrap_or_default();
        let mut out: Vec<Value> =
            src.topics.iter().filter_map(|t| counts.get(&t.id).map(|n| json!({"id": t.id, "title": t.title, "color": t.color, "count": n}))).collect();
        let named: HashSet<i32> = src.topics.iter().map(|t| t.id).collect();
        for (id, n) in &counts {
            if !named.contains(id) {
                let title = if *id == 1 { "General".to_string() } else { format!("Chủ đề {id}") };
                out.push(json!({"id": id, "title": title, "color": 0, "count": n}));
            }
        }
        let total: usize = counts.values().sum();
        Ok(json!({"forum": src.forum, "name": src.name, "total": total, "topics": out}))
    }

    /// Sách theo khoá (sách không còn trong thư viện thì dùng bản chép của bộ sưu tập, `missing: true`).
    fn items_json(&self, items: &[Item]) -> Vec<Value> {
        let found: Vec<Option<Book>> = {
            let lib = lock(&self.lib);
            items.iter().map(|i| lib.book(&i.key).cloned()).collect()
        };
        let mut out = Vec::with_capacity(items.len());
        for (b, it) in found.iter().zip(items) {
            match b {
                Some(b) => out.extend(self.books_json(std::slice::from_ref(b))),
                None if it.key.starts_with(LOCAL_PREFIX) => out.push(json!({"key": it.key, "title": it.title, "author": it.author,
                    "ext": it.ext, "fmt": text::format_group(&it.ext), "ok": text::readable_ext(&it.ext), "local": true})),
                None => out.push(json!({"key": it.key, "title": it.title, "author": it.author, "ext": it.ext, "missing": true, "ok": false})),
            }
        }
        out
    }

    fn save_offline(&self) {
        let v: Vec<String> = lock(&self.offline).iter().cloned().collect();
        if let Err(e) = crate::library::write_atomic(&self.cfg.data_dir.join("offline.json"), &serde_json::to_vec(&v).unwrap_or_default()) {
            self.log(&format!("ghi offline.json: {e}"));
        }
    }

    /// Hàng tải về máy: lần lượt từng cuốn, báo `dl.progress` / `dl.done` / `dl.error`.
    fn run_downloads(self: &Arc<Self>) {
        if self.dl_running.swap(true, Ordering::AcqRel) {
            return;
        }
        let c = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                let Some(key) = lock(&c.dlq).pop_front() else { break };
                if !lock(&c.offline).contains(&key) {
                    continue;
                }
                let Some(book) = lock(&c.lib).book(&key).cloned() else { continue };
                if c.files.complete_path(&book).is_some() {
                    (c.emit)(json!({"e": "dl.done", "key": key}));
                    continue;
                }
                let t = match c.files.set_full(&book, true) {
                    Ok(t) => t,
                    Err(e) => {
                        (c.emit)(json!({"e": "dl.error", "key": key, "code": "IO", "msg": e.to_string()}));
                        continue;
                    }
                };
                loop {
                    tokio::time::sleep(Duration::from_millis(700)).await;
                    let (h, n) = t.progress();
                    if t.is_done() {
                        (c.emit)(json!({"e": "dl.done", "key": key}));
                        break;
                    }
                    if !lock(&c.offline).contains(&key) {
                        let _ = c.files.set_full(&book, false);
                        break;
                    }
                    if let Some(e) = t.failed() {
                        (c.emit)(json!({"e": "dl.error", "key": key, "code": e.code, "msg": e.msg}));
                        break;
                    }
                    if !t.is_full() {
                        let _ = c.files.set_full(&book, true);
                    }
                    (c.emit)(json!({"e": "dl.progress", "key": key, "have": h, "total": n}));
                }
            }
            c.dl_running.store(false, Ordering::Release);
            if !lock(&c.dlq).is_empty() {
                c.run_downloads();
            }
        });
    }

    // ───────────────────────── Mở sách ─────────────────────────

    /// Tệp sách trên máy (tải trọn nếu chưa có — báo `file.progress`). Trả (đường dẫn, sách trong thư viện nếu có).
    async fn book_file(self: &Arc<Self>, key: &str) -> Result<(PathBuf, Option<Book>), CoreError> {
        if let Some(p) = key.strip_prefix(LOCAL_PREFIX) {
            let path = PathBuf::from(p);
            if !path.is_file() {
                return Err(CoreError::new("NO_BOOK", "Không còn tệp này trên máy."));
            }
            return Ok((path, None));
        }
        let book = lock(&self.lib).book(key).cloned().ok_or_else(|| CoreError::new("NO_BOOK", "Sách này không còn trong thư viện."))?;
        let t = Instant::now();
        let path = self.files.fetch_all(&book, Duration::from_secs(900)).await?;
        if t.elapsed() > Duration::from_millis(50) {
            self.log(&format!("sách {key}: tải {} KB trong {} ms", book.size >> 10, t.elapsed().as_millis()));
        }
        crate::cache::touch(&path);
        Ok((path, Some(book)))
    }

    /// Mở sách: tải (nếu cần), đọc, đưa lên cổng sách. Trả thông tin cho màn đọc.
    async fn open_book(self: &Arc<Self>, key: &str) -> Res {
        let (path, book) = Box::pin(self.book_file(key)).await?;
        let ext = match &book {
            Some(b) => b.ext.clone(),
            None => text::ext_of(&path.to_string_lossy(), ""),
        };
        if !text::readable_ext(&ext) {
            return Err(CoreError::new("FORMAT", format!("Chưa đọc được định dạng {}.", ext.to_uppercase())));
        }
        let id = format!("b{}", self.open_seq.fetch_add(1, Ordering::Relaxed));
        let (key2, id2) = (key.to_string(), id.clone());
        let t = Instant::now();
        let kindle = text::kindle_ext(&ext);
        let (ob, ep) = tokio::task::spawn_blocking(move || if kindle { open_kindle(&path, id2, key2) } else { open_epub(&path, id2, key2) })
            .await
            .map_err(|e| CoreError::new("IO", e.to_string()))??;
        let ob = Arc::new(ob);
        self.server.add(Arc::clone(&ob));
        let base = self.server.base_url(&id).await?;
        // Bìa trong sách Kindle (đã có trong bộ nhớ sau khi chuyển) → covers/ cho Thư viện / Đang đọc
        if let Some((data, cext)) = &ep.cover_data {
            let dir = self.cfg.data_dir.join("covers");
            let out = dir.join(format!("{}.book.{cext}", cover_stem(key)));
            if !out.exists() {
                let _ = std::fs::create_dir_all(&dir);
                if let Err(e) = std::fs::write(&out, data.as_slice()) {
                    self.log(&format!("ghi bìa sách Kindle: {e}"));
                }
            }
        }
        // Tên / tác giả thật (OPF / EXTH) → thư viện; tên trong sách không dấu mà tên tệp có dấu → lấy bản có dấu
        let author = ep.authors.join(", ");
        let file_title = match &book {
            Some(b) => b.title.clone(),
            None => text::title_from_file(key.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(key)).0,
        };
        let meta_title = text::prefer_accented(&ep.title, &file_title);
        if book.is_some() {
            let changed = lock(&self.lib).set_book_meta(key, Some(&meta_title), Some(&author));
            if changed {
                let _ = lock(&self.lib).flush();
                self.library_changed();
            }
        }
        let title = if meta_title.is_empty() {
            book.as_ref().map(|b| b.shown_title().to_string()).unwrap_or(file_title)
        } else {
            meta_title
        };
        let reading = {
            let mut r = lock(&self.reads);
            r.opened(key, &title, &author, &ext).clone()
        };
        if let Some(b) = &book {
            let _ = lock(&self.pls).played(Item::from(b));
        }
        let spine: Vec<Value> = ob
            .spine
            .iter()
            .map(|s| json!({"path": s.path, "href": url_path(&s.path), "linear": s.linear, "size": s.size}))
            .collect();
        let toc: Vec<Value> = ep
            .toc
            .iter()
            .map(|t| json!({"title": t.title, "s": ob.spine_index(&t.path), "frag": t.frag, "depth": t.depth}))
            .collect();
        self.log(&format!("mở sách {key} ({}): {} chương, {} mục lục, {} ms", ep.format, ob.spine.len(), toc.len(), t.elapsed().as_millis()));
        Ok(json!({
            "id": id, "key": key, "base": base, "title": title, "author": author, "ext": ext, "lang": ep.language,
            "rtl": ep.rtl, "fixed": ep.fixed, "series": ep.series.as_ref().map(|(s, i)| json!({"name": s, "index": i})),
            "spine": spine, "toc": toc, "reading": reading_json(&reading), "cover": ep.has_cover, "format": ep.format,
            "nf": book.as_ref().is_some_and(|b| b.nf),
        }))
    }

    /// Bìa: bìa trong sách (khi tệp đã có trên máy) → ảnh nhỏ Telegram → không có.
    async fn cover(self: &Arc<Self>, key: &str) -> Res {
        let dir = self.cfg.data_dir.join("covers");
        let stem = cover_stem(key);
        for ext in ["jpg", "png", "gif", "jpeg"] {
            let p = dir.join(format!("{stem}.book.{ext}"));
            if self.cache.hit(&p) {
                return Ok(json!({"path": p, "book": true}));
            }
        }
        // Tệp sách đã có trên máy → lấy bìa trong sách
        let local: Option<(PathBuf, String)> = match key.strip_prefix(LOCAL_PREFIX) {
            Some(p) => Some((PathBuf::from(p), text::ext_of(p, ""))),
            None => {
                let b = lock(&self.lib).book(key).cloned();
                b.and_then(|b| self.files.complete_path(&b).map(|p| (p, b.ext.clone())))
            }
        };
        if let Some((path, ext)) = local.filter(|(_, e)| e == "epub") {
            let dir2 = dir.clone();
            let stem2 = stem.clone();
            let _ = ext;
            let r = tokio::task::spawn_blocking(move || -> Option<PathBuf> {
                let z = crate::zip::Zip::open(&path).ok()?;
                let ep = epub::parse(&z).ok()?;
                let c = ep.cover?;
                let data = z.read(&c).ok()?;
                let ext = image_ext(&data);
                std::fs::create_dir_all(&dir2).ok()?;
                let out = dir2.join(format!("{stem2}.book.{ext}"));
                std::fs::write(&out, data).ok()?;
                Some(out)
            })
            .await
            .ok()
            .flatten();
            if let Some(p) = r {
                return Ok(json!({"path": p, "book": true}));
            }
        }
        if key.starts_with(LOCAL_PREFIX) {
            return Err(CoreError::new("NO_COVER", "Sách không có bìa."));
        }
        let path = dir.join(format!("{stem}.jpg"));
        if self.cache.hit(&path) {
            return Ok(json!({"path": path, "book": false}));
        }
        let book = lock(&self.lib).book(key).cloned().ok_or_else(|| CoreError::new("NO_BOOK", "Sách này không còn trong thư viện."))?;
        if !book.thumb {
            return Err(CoreError::new("NO_COVER", "Sách không có bìa."));
        }
        let _permit = self.dl.acquire().await.map_err(|_| CoreError::new("BUSY", "hàng tải đã đóng"))?;
        let pr = peer_ref_of_key(&self.session, &book.peer).await.ok_or_else(|| CoreError::new("PEER_UNKNOWN", "Chưa biết chat chứa sách."))?;
        let msg = self.client.get_messages_by_id(pr, &[book.id]).await?.into_iter().flatten().next();
        let Some(Media::Document(d)) = msg.as_ref().and_then(|m| m.media()) else {
            return Err(CoreError::new("NO_MEDIA", "Tin chứa sách này đã bị xoá."));
        };
        let loc = media::doc_thumb_loc(&d).ok_or_else(|| CoreError::new("NO_COVER", "Sách không có bìa."))?;
        self.download(&loc, &path).await?;
        Ok(json!({"path": path, "book": false}))
    }

    // ───────────────────────── Bộ nhớ đệm, mạng, cài đặt (như GramMusic) ─────────────────────────

    fn trim_soon(self: &Arc<Self>, force: bool) {
        if !self.cache.begin(force) {
            return;
        }
        let c = Arc::clone(self);
        let offline = lock(&self.offline).clone();
        let mut busy = self.files.busy();
        // Sách đang đọc dở: không dọn
        busy.extend(lock(&self.reads).recent(10_000, Some(false)).into_iter().map(|r| r.key.clone()));
        tokio::task::spawn_blocking(move || {
            let t = Instant::now();
            let r = crate::cache::trim(&c.cfg.data_dir, c.cache.policy(), &offline, &busy);
            c.cache.end();
            if r.files > 0 {
                c.log(&format!("dọn đệm: xoá {} tệp ({} KB), còn {} KB, {} ms", r.files, r.freed >> 10, r.total >> 10, t.elapsed().as_millis()));
                (c.emit)(json!({"e": "cache.cleared"}));
            }
        });
    }

    fn set_net(&self, ok: bool) {
        let new = if ok { 1 } else { 2 };
        if self.net.swap(new, Ordering::Relaxed) != new {
            self.log(if ok { "mạng: kết nối được" } else { "mạng: mất kết nối" });
            (self.emit)(json!({"e": "net.state", "ok": ok}));
        }
    }

    async fn check_net(&self) -> bool {
        let r = tokio::time::timeout(Duration::from_secs(8), self.client.invoke(&tl::functions::updates::GetState {})).await;
        let ok = matches!(r, Ok(Ok(_)) | Ok(Err(InvocationError::Rpc(_))));
        self.set_net(ok);
        ok
    }

    /// Ghi nhật ký lõi (không ghi tên sách / số điện thoại).
    pub fn log(&self, line: &str) {
        if let Some(f) = lock(&self.log).as_mut() {
            let _ = writeln!(f, "{} {line}", now());
        }
    }

    fn settings(dir: &std::path::Path) -> serde_json::Map<String, Value> {
        std::fs::read(dir.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| match v {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn save_settings(&self, kv: &[(&str, Value)]) -> Result<(), CoreError> {
        let mut m = Self::settings(&self.cfg.data_dir);
        for (k, v) in kv {
            m.insert(k.to_string(), v.clone());
        }
        crate::library::write_atomic(&self.cfg.data_dir.join("settings.json"), Value::Object(m).to_string().as_bytes())
            .map_err(|e| CoreError::new("IO", format!("ghi cài đặt: {e}")))
    }

    /// Chat để gửi sách: «Tin nhắn đã lưu» trước, rồi các chat theo tin mới nhất (bỏ kênh không có quyền đăng, tài khoản đã xoá).
    async fn recent_chats(&self, limit: usize) -> Result<Value, CoreError> {
        let mut out = vec![json!({"peer": "self", "name": "Tin nhắn đã lưu", "kind": "self"})];
        let me = self.me_key();
        let mut it = self.client.iter_dialogs();
        let mut seen = 0;
        while let Some(d) = it.next().await? {
            seen += 1;
            let p = d.peer();
            let key = peer_key(p.id());
            if me.as_deref() != Some(key.as_str()) && can_post(p) {
                let (photo, photo_dc) = peer_photo(p);
                out.push(json!({"peer": key, "name": peer_name(p), "kind": peer_kind(p), "username": p.username(),
                                "photo": photo, "photo_dc": photo_dc}));
            }
            if out.len() > limit || seen >= limit * 4 {
                break;
            }
        }
        self.save_session();
        Ok(Value::Array(out))
    }

    /// Gửi sách cho bạn: sách từ Telegram → chuyển tiếp đúng tin chứa tệp (không tải lên lại; tin «hạn chế lưu nội dung» →
    /// NOFORWARDS); sách trên máy (`file:`) → tải tệp lên rồi gửi như tài liệu (tên tệp giữ nguyên).
    async fn forward_book(&self, key: &str, to: &str) -> Result<Value, CoreError> {
        if let Some(p) = key.strip_prefix(LOCAL_PREFIX) {
            return Box::pin(self.send_local_book(std::path::PathBuf::from(p), to)).await;
        }
        let book = lock(&self.lib).book(key).cloned().ok_or_else(|| CoreError::new("NO_BOOK", "Sách này không còn trong thư viện."))?;
        if book.nf {
            return Err(CoreError::new("NOFORWARDS", "Chat chứa sách này hạn chế lưu nội dung — không gửi tiếp được."));
        }
        let from = peer_ref_of_key(&self.session, &book.peer).await.ok_or_else(|| CoreError::new("PEER_UNKNOWN", "Chưa biết chat chứa sách."))?;
        let dest = self.dest_peer(to).await?;
        match self.client.forward_messages(dest, &[book.id], from).await {
            Ok(v) if v.iter().any(Option::is_some) => Ok(json!({"sent": true})),
            Ok(_) => Err(CoreError::new("NOT_SENT", "Telegram không gửi được (tin gốc có thể đã bị xoá).")),
            Err(InvocationError::Rpc(r)) if r.name == "CHAT_FORWARDS_RESTRICTED" => {
                Err(CoreError::new("NOFORWARDS", "Chat chứa sách này hạn chế lưu nội dung — không gửi tiếp được."))
            }
            Err(InvocationError::Rpc(r)) if r.name.starts_with("CHAT_WRITE_FORBIDDEN") || r.name.starts_with("CHAT_SEND_") => {
                Err(CoreError::new("FORBIDDEN", "Không có quyền gửi tệp vào chat này."))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Chat nhận: "self" = Tin nhắn đã lưu, còn lại là khoá peer (đã có trong phiên — `chats.recent` đọc danh sách chat).
    async fn dest_peer(&self, to: &str) -> Result<PeerRef, CoreError> {
        if to == "self" {
            return Ok(PeerId::self_user().to_ambient_ref());
        }
        peer_ref_of_key(&self.session, to).await.ok_or_else(|| CoreError::new("PEER_UNKNOWN", "Chưa biết chat nhận — mở lại danh sách chat."))
    }

    /// Gửi tệp sách trên máy vào chat (tải lên từng phần qua grammers, mime đoán theo đuôi tệp: EPUB / MOBI…).
    async fn send_local_book(&self, path: PathBuf, to: &str) -> Result<Value, CoreError> {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "book".into());
        let p2 = path.clone();
        let data = tokio::task::spawn_blocking(move || std::fs::read(&p2))
            .await
            .map_err(|e| CoreError::new("IO", e.to_string()))?
            .map_err(|e| CoreError::new("IO", format!("Không đọc được tệp sách: {e}")))?;
        if data.len() > 1536 << 20 {
            return Err(CoreError::new("TOO_BIG", "Tệp lớn quá — Telegram nhận tối đa 2 GB."));
        }
        let dest = self.dest_peer(to).await?;
        let t = Instant::now();
        let size = data.len();
        let mut cur = std::io::Cursor::new(data);
        let up = self.client.upload_stream(&mut cur, size, name).await.map_err(|e| CoreError::new("NETWORK", format!("Tải tệp lên lỗi: {e}")))?;
        match self.client.send_message(dest, grammers_client::message::InputMessage::new().document(up)).await {
            Ok(_) => {
                self.log(&format!("gửi sách trên máy: {} KB, {} ms", size >> 10, t.elapsed().as_millis()));
                Ok(json!({"sent": true, "uploaded": true}))
            }
            Err(InvocationError::Rpc(r)) if r.name.starts_with("CHAT_WRITE_FORBIDDEN") || r.name.starts_with("CHAT_SEND_") => {
                Err(CoreError::new("FORBIDDEN", "Không có quyền gửi tệp vào chat này."))
            }
            Err(e) => Err(e.into()),
        }
    }

    fn save_session(&self) {
        if let Err(e) = self.session.save() {
            self.log(&format!("ghi phiên: {e}"));
        }
    }

    /// Nghe cập nhật (một lần): sách mới gửi vào chat nào cũng vào thư viện; tin bị xoá thì bỏ khỏi thư viện.
    fn start_updates(self: &Arc<Self>) {
        let Some(rx) = lock(&self.updates).take() else { return };
        let c = Arc::clone(self);
        tokio::spawn(async move {
            let mut stream = match c.client.stream_updates(rx, UpdatesConfiguration { catch_up: true }).await {
                Ok(s) => s,
                Err(e) => {
                    c.log(&format!("stream_updates: {e}"));
                    return;
                }
            };
            let mut last_save = Instant::now();
            loop {
                match tokio::time::timeout(Duration::from_secs(30), stream.next()).await {
                    Ok(Ok(Update::NewMessage(m))) | Ok(Ok(Update::MessageEdited(m))) => {
                        let key = peer_key(m.peer_id());
                        if let Some(book) = media::book_from(&m, &key, c.origin_of(&m, &key)) {
                            let n = {
                                let mut lib = lock(&c.lib);
                                let n = lib.add_books(&key, vec![book], || c.source_for(&m, &key)).unwrap_or(0);
                                let _ = lib.flush();
                                n
                            };
                            if n > 0 {
                                c.library_changed();
                            }
                        }
                    }
                    Ok(Ok(Update::MessageDeleted(d))) => {
                        let ch = d.channel_id().and_then(PeerId::channel).map(peer_key);
                        let n = lock(&c.lib).remove_books(ch.as_deref(), d.messages());
                        if n > 0 {
                            c.library_changed();
                        }
                    }
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => {
                        c.log(&format!("cập nhật lỗi: {e}"));
                        if c.check_dead(&CoreError::from(e).code) {
                            break;
                        }
                        c.set_net(false);
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                    Err(_) => {}
                }
                if last_save.elapsed() > Duration::from_secs(30) {
                    let _ = stream.sync_update_state().await;
                    c.save_session();
                    last_save = Instant::now();
                }
            }
        });
    }

    // ───────────────────────── Lệnh ─────────────────────────

    /// Gọi một lệnh. Xem docs/FFI.md.
    pub async fn call(self: &Arc<Self>, method: &str, a: Value) -> Res {
        let t = Instant::now();
        let r = Box::pin(self.dispatch(method, &a)).await;
        let ms = t.elapsed().as_millis();
        match &r {
            Ok(_) if method.starts_with("auth.") || method.starts_with("sync.") || method == "net.check" => self.set_net(true),
            Err(e) if e.code == "NETWORK" || e.code == "TIMEOUT" => self.set_net(false),
            _ => {}
        }
        match &r {
            Ok(_) if ms > 200 => self.log(&format!("{method}: xong {ms} ms")),
            Ok(_) => {}
            Err(e) => self.log(&format!("{method}: lỗi {} sau {ms} ms", e.code)),
        }
        r
    }

    fn book_arg(&self, a: &Value) -> Result<Book, CoreError> {
        let key = arg_str(a, "key")?;
        lock(&self.lib).book(key).cloned().ok_or_else(|| CoreError::new("NO_BOOK", "Sách này không còn trong thư viện."))
    }

    fn open_arg(&self, a: &Value) -> Result<Arc<OpenBook>, CoreError> {
        let id = arg_str(a, "id")?;
        self.server.get(id).ok_or_else(|| CoreError::new("NOT_OPEN", "Sách đã đóng — mở lại."))
    }

    async fn dispatch(self: &Arc<Self>, method: &str, a: &Value) -> Res {
        match method {
            "auth.state" => {
                if self.dead.load(Ordering::Relaxed) {
                    return Ok(json!({"authorized": false, "dead": true}));
                }
                let ok = match self.client.is_authorized().await {
                    Ok(v) => v,
                    Err(e) => {
                        let ce = CoreError::from(e);
                        if self.check_dead(&ce.code) {
                            return Ok(json!({"authorized": false, "dead": true}));
                        }
                        return Err(ce);
                    }
                };
                if !ok {
                    return Ok(json!({"authorized": false}));
                }
                let me = self.client.get_me().await?;
                Ok(json!({"authorized": true, "me": user_json(&me)}))
            }
            "auth.sendCode" => {
                let phone = arg_str(a, "phone")?;
                let token = self.client.request_login_code(phone, &self.cfg.api_hash).await?;
                *lock(&self.login) = Some(token);
                Ok(json!({}))
            }
            "auth.signIn" => {
                let code = arg_str(a, "code")?;
                let token = lock(&self.login).take().ok_or_else(|| CoreError::new("NO_CODE", "Chưa gửi mã."))?;
                match self.client.sign_in(&token, code).await {
                    Ok(me) => self.signed_in(&me).await,
                    Err(SignInError::PasswordRequired(p)) => {
                        let hint = p.hint().unwrap_or("").to_string();
                        *lock(&self.password) = Some(p);
                        let mut e = CoreError::new("PASSWORD_NEEDED", "Tài khoản có mật khẩu 2 lớp.");
                        e.extra = json!({"hint": hint});
                        Err(e)
                    }
                    Err(SignInError::InvalidCode) => {
                        *lock(&self.login) = Some(token);
                        Err(CoreError::new("PHONE_CODE_INVALID", "Mã không đúng."))
                    }
                    Err(SignInError::SignUpRequired) => Err(CoreError::new(
                        "SIGN_UP_REQUIRED",
                        "Số này chưa có tài khoản Telegram — đăng ký bằng app Telegram chính thức trước.",
                    )),
                    Err(SignInError::Other(e)) => Err(e.into()),
                    Err(e) => Err(CoreError::new("SIGN_IN", e.to_string())),
                }
            }
            "auth.checkPassword" => {
                let pw = arg_str(a, "password")?;
                let token = lock(&self.password).take().ok_or_else(|| CoreError::new("NO_PASSWORD", "Không cần mật khẩu."))?;
                let mut r = self.client.check_password(token, pw).await;
                if matches!(&r, Err(SignInError::Other(e)) if e.is("SRP_ID_INVALID")) {
                    let fresh = self.password_token().await?;
                    r = self.client.check_password(fresh, pw).await;
                }
                match r {
                    Ok(me) => self.signed_in(&me).await,
                    Err(SignInError::InvalidPassword(t)) => {
                        *lock(&self.password) = Some(self.password_token().await.unwrap_or(t));
                        Err(CoreError::new("PASSWORD_INVALID", "Mật khẩu không đúng."))
                    }
                    Err(SignInError::Other(e)) => {
                        if let Ok(t) = self.password_token().await {
                            *lock(&self.password) = Some(t);
                        }
                        Err(e.into())
                    }
                    Err(e) => Err(CoreError::new("SIGN_IN", e.to_string())),
                }
            }
            "auth.logout" => {
                let _ = self.client.sign_out().await;
                self.save_session();
                (self.emit)(json!({"e": "auth.state", "authorized": false}));
                Ok(json!({}))
            }
            "proxy.get" => {
                let link = Self::settings(&self.cfg.data_dir).get("proxy").cloned().unwrap_or(Value::Null);
                Ok(json!({"link": link}))
            }
            "proxy.set" => {
                let link = a.get("link").and_then(Value::as_str).filter(|s| !s.trim().is_empty());
                let cfg = match link {
                    Some(l) => gm_net::parse_link(l).map_err(|e| CoreError::new("PROXY_LINK", e))?,
                    None => ProxyConfig::Direct,
                };
                self.save_settings(&[("proxy", json!(link))])?;
                self.conn.set(cfg);
                for dc in 1..=5 {
                    self.handle.disconnect_from_dc(dc);
                }
                Ok(json!({}))
            }
            "proxy.test" => {
                let cfg = match a.get("link").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
                    Some(l) => gm_net::parse_link(l).map_err(|e| CoreError::new("PROXY_LINK", e))?,
                    None => ProxyConfig::Direct,
                };
                let conn = GmConnector::new(cfg);
                let t = Instant::now();
                let addr = ServerAddr::Custom { dc_id: 2, address: "149.154.167.41:443".parse().unwrap(), connector: conn.clone() };
                tokio::time::timeout(Duration::from_secs(25), grammers_mtsender::connect(conn.transport(), addr))
                    .await
                    .map_err(|_| CoreError::new("TIMEOUT", "Quá 25 giây không kết nối được."))?
                    .map_err(|e| CoreError::new("PROXY_FAIL", e.to_string()))?;
                Ok(json!({"ms": t.elapsed().as_millis() as u64}))
            }
            "net.state" => Ok(json!({"ok": self.net.load(Ordering::Relaxed) != 2})),
            "net.check" => Ok(json!({"ok": self.check_net().await})),
            "app.flush" => {
                self.save_session();
                lock(&self.lib).flush()?;
                lock(&self.reads).flush()?;
                Ok(json!({}))
            }

            // ── Thư viện ──
            "library.query" => {
                let q = a.get("q").and_then(Value::as_str).unwrap_or("");
                let source = a.get("source").and_then(Value::as_str).unwrap_or("");
                let sort = Sort::parse(a.get("sort").and_then(Value::as_str).unwrap_or("date"));
                let offset = a.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
                let limit = a.get("limit").and_then(Value::as_u64).unwrap_or(100).min(500) as usize;
                let keep = self.keep_fn(a);
                let (total, page, sections, gen) = {
                    let mut lib = lock(&self.lib);
                    let (t, p, s) = lib.query(q, source, sort, offset, limit, &*keep);
                    (t, p, s, lib.generation)
                };
                drop(keep);
                Ok(json!({"total": total, "gen": gen, "offset": offset, "items": self.books_json(&page), "sections": sections}))
            }
            "library.keys" => {
                let q = a.get("q").and_then(Value::as_str).unwrap_or("");
                let source = a.get("source").and_then(Value::as_str).unwrap_or("");
                let sort = Sort::parse(a.get("sort").and_then(Value::as_str).unwrap_or("date"));
                let keep = self.keep_fn(a);
                let mut keys = lock(&self.lib).keys(q, source, sort, &*keep);
                keys.truncate(5000);
                Ok(json!(keys))
            }
            "library.books" => {
                let items: Vec<Item> = arg_keys(a).into_iter().map(|k| Item { key: k, ..Default::default() }).collect();
                Ok(Value::Array(self.items_json(&items)))
            }
            "library.stats" => {
                let (total, sources, gen) = {
                    let mut lib = lock(&self.lib);
                    let t = lib.total();
                    (t, lib.sources().len(), lib.generation)
                };
                let st = SyncState::load(&self.cfg.data_dir);
                Ok(json!({"total": total, "sources": sources, "gen": gen, "offline": lock(&self.offline).len(),
                          "full_done": st.full_done, "last_sync": st.last_sync, "partial": !st.phase.is_empty(),
                          "sync": lock(&self.sync_info).clone()}))
            }
            "sources.list" => {
                let me = self.me_key();
                let mut v: Vec<Source> = lock(&self.lib).sources().to_vec();
                let counts: HashMap<String, usize> = {
                    let lib = lock(&self.lib);
                    v.iter().map(|s| (s.peer.clone(), lib.indexed(&s.peer))).collect()
                };
                v.sort_by(|x, y| {
                    let sx = me.as_deref() == Some(&x.peer);
                    let sy = me.as_deref() == Some(&y.peer);
                    sy.cmp(&sx).then(counts[&y.peer].cmp(&counts[&x.peer])).then(x.name.cmp(&y.name))
                });
                Ok(Value::Array(v.iter().map(|s| self.source_json(s)).collect()))
            }
            "sources.set" => {
                let peer = arg_str(a, "peer")?;
                let on = a.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                lock(&self.lib).update_source(peer, |s| s.enabled = on)?;
                self.library_changed();
                Ok(json!({"enabled": on}))
            }
            "sources.add" => Box::pin(self.add_source(arg_str(a, "link")?)).await,
            "sources.topics" => Box::pin(self.topics(arg_str(a, "peer")?)).await,
            "sources.remove" => {
                let peer = arg_str(a, "peer")?;
                lock(&self.lib).remove_source(peer)?;
                self.library_changed();
                Ok(json!({}))
            }
            "sync.start" => {
                let full = a.get("full").and_then(Value::as_bool).unwrap_or(false);
                let peer = a.get("peer").and_then(Value::as_str).map(str::to_string);
                Ok(json!({"started": self.start_sync(full, peer)}))
            }
            "sync.stop" => {
                self.sync_stop.store(true, Ordering::Relaxed);
                Ok(json!({}))
            }
            "sync.state" => Ok(lock(&self.sync_info).clone()),

            // ── Mở sách, màn đọc ──
            "book.open" => Box::pin(self.open_book(arg_str(a, "key")?)).await,
            "book.cancel" => {
                // Thôi tải sách đang mở dở (người dùng đóng màn chờ)
                if let Ok(b) = self.book_arg(a) {
                    if !lock(&self.offline).contains(&b.key()) {
                        let _ = self.files.set_full(&b, false);
                    }
                }
                Ok(json!({}))
            }
            "book.close" => {
                self.server.close(arg_str(a, "id")?);
                Ok(json!({}))
            }
            "book.chapter" => {
                let ob = self.open_arg(a)?;
                let s = a.get("s").and_then(Value::as_u64).unwrap_or(0) as usize;
                let c = ob.chapter(s).await?;
                Ok(json!({"parts": c.part_count(), "xhtml": c.xhtml, "sizes": c.part_sizes()}))
            }
            "book.locate" => {
                // Link trong sách (URL đầy đủ của cổng, hoặc đường dẫn trong sách + #id) → chương + phần
                let ob = self.open_arg(a)?;
                let (path, frag) = match a.get("href").and_then(Value::as_str) {
                    Some(h) => {
                        let rest = h.split_once(&format!("/{}/", ob.id)).map(|(_, r)| r).unwrap_or(h);
                        let (p, f) = rest.split_once('#').map(|(p, f)| (p, Some(f))).unwrap_or((rest, None));
                        (epub::percent_decode(p.split('?').next().unwrap_or(p)), f.map(epub::percent_decode))
                    }
                    None => (arg_str(a, "path")?.to_string(), a.get("frag").and_then(Value::as_str).map(str::to_string)),
                };
                let s = ob.spine_index(&path).ok_or_else(|| CoreError::new("NO_TARGET", "Link trỏ ra ngoài chương của sách."))?;
                let part = match frag.as_deref().filter(|f| !f.is_empty()) {
                    Some(f) => ob.chapter(s).await?.part_of(f).unwrap_or(0),
                    None => 0,
                };
                Ok(json!({"s": s, "p": part, "frag": frag}))
            }
            "reader.config" => {
                // Cấu hình màn đọc (khung, cỡ chữ, font, nền…) — chèn vào mọi chương; lưu để lần sau dùng ngay
                let cfg = a.get("config").cloned().unwrap_or(json!({}));
                self.server.set_config(&cfg.to_string());
                self.save_settings(&[("reader", cfg)])?;
                Ok(json!({}))
            }
            "reader.get" => Ok(Self::settings(&self.cfg.data_dir).get("reader").cloned().unwrap_or(json!({}))),
            "serve.revive" => match self.server.revive().await? {
                Some((port, revived)) => Ok(json!({"port": port, "revived": revived})),
                None => Ok(json!({"revived": false})),
            },

            // ── Vị trí đọc, dấu trang ──
            "reading.get" => {
                let key = arg_str(a, "key")?;
                Ok(lock(&self.reads).get(key).map(reading_json).unwrap_or(Value::Null))
            }
            "reading.save" => {
                let key = arg_str(a, "key")?;
                let pos: Pos = a.get("pos").and_then(|p| serde_json::from_value(p.clone()).ok()).ok_or_else(|| CoreError::new("BAD_ARGS", "thiếu «pos»"))?;
                let progress = a.get("progress").and_then(Value::as_f64).map(|v| v as f32);
                let chapter = a.get("chapter").and_then(Value::as_str);
                let secs = a.get("secs").and_then(Value::as_u64).unwrap_or(0);
                lock(&self.reads).save_pos(key, pos, progress, chapter, secs);
                Ok(json!({}))
            }
            "reading.recent" => {
                let limit = a.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
                let finished = a.get("finished").and_then(Value::as_bool);
                let list: Vec<Value> = lock(&self.reads).recent(limit, finished).into_iter().map(reading_json).collect();
                Ok(Value::Array(list))
            }
            "reading.finish" => {
                lock(&self.reads).set_finished(arg_str(a, "key")?, a.get("on").and_then(Value::as_bool).unwrap_or(true));
                Ok(json!({}))
            }
            "reading.forget" => {
                lock(&self.reads).forget(arg_str(a, "key")?);
                Ok(json!({}))
            }
            "chats.recent" => {
                let limit = a.get("limit").and_then(Value::as_u64).unwrap_or(40).clamp(1, 200) as usize;
                Box::pin(self.recent_chats(limit)).await
            }
            "book.forward" => Box::pin(self.forward_book(arg_str(a, "key")?, arg_str(a, "to")?)).await,
            "mark.add" => {
                let key = arg_str(a, "key")?;
                let pos: Pos = a.get("pos").and_then(|p| serde_json::from_value(p.clone()).ok()).ok_or_else(|| CoreError::new("BAD_ARGS", "thiếu «pos»"))?;
                let chapter = a.get("chapter").and_then(Value::as_str).unwrap_or("");
                let id = lock(&self.reads).add_mark(key, pos, chapter).ok_or_else(|| CoreError::new("NOT_OPEN", "Mở sách trước."))?;
                Ok(json!({"id": id}))
            }
            "mark.remove" => {
                let key = arg_str(a, "key")?;
                let id = a.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
                Ok(json!({"removed": lock(&self.reads).remove_mark(key, id)}))
            }

            // ── Tải về máy ──
            "dl.add" => {
                let keys = arg_keys(a);
                let mut n = 0;
                {
                    let mut off = lock(&self.offline);
                    let mut q = lock(&self.dlq);
                    for k in keys {
                        if lock(&self.lib).book(&k).is_some() && off.insert(k.clone()) {
                            q.push_back(k);
                            n += 1;
                        }
                    }
                }
                self.save_offline();
                self.run_downloads();
                Ok(json!({"added": n}))
            }
            "dl.remove" => {
                let keys: HashSet<String> = arg_keys(a).into_iter().collect();
                {
                    let mut off = lock(&self.offline);
                    for k in &keys {
                        off.remove(k);
                    }
                }
                self.save_offline();
                lock(&self.dlq).retain(|k| !keys.contains(k));
                for k in &keys {
                    self.files.forget(k);
                }
                let dir = self.cfg.data_dir.clone();
                let busy = self.files.busy();
                let off = lock(&self.offline).clone();
                let kinds = vec!["offline".to_string(), "books".to_string()];
                let (freed, files) =
                    tokio::task::spawn_blocking(move || crate::cache::clear(&dir, &off, Some(&kinds), None, Some(&keys), &busy))
                        .await
                        .map_err(|e| CoreError::new("IO", e.to_string()))?;
                Ok(json!({"freed": freed, "files": files}))
            }
            "dl.state" => {
                let q: Vec<String> = lock(&self.dlq).iter().cloned().collect();
                Ok(json!({"queue": q, "running": self.dl_running.load(Ordering::Relaxed)}))
            }
            "file.progress" => {
                let b = self.book_arg(a)?;
                Ok(json!({"cached": self.files.cached_fraction(&b)}))
            }

            // ── Ảnh ──
            "media.cover" => Box::pin(self.cover(arg_str(a, "key")?)).await,
            "media.avatar" => {
                let key = arg_str(a, "peer")?;
                let peer = peer_ref_of_key(&self.session, key).await.ok_or_else(|| CoreError::new("PEER_UNKNOWN", "Chưa biết chat này."))?;
                let photo: i64 = arg_str(a, "photo")?.parse().map_err(|_| CoreError::new("BAD_ARGS", "«photo» sai"))?;
                let dc = a.get("dc").and_then(Value::as_i64).unwrap_or(0) as i32;
                let path = self.cfg.data_dir.join("avatars").join(format!("{key}_{photo}.jpg"));
                if self.cache.hit(&path) {
                    return Ok(json!({"path": path}));
                }
                let _permit = self.dl.acquire().await.map_err(|_| CoreError::new("BUSY", "hàng tải đã đóng"))?;
                self.download(&media::avatar_loc(&peer, photo, dc), &path).await?;
                Ok(json!({"path": path}))
            }

            // ── Bộ sưu tập: bộ tự tạo, Yêu thích, Muốn đọc, Mở gần đây ──
            "col.list" => {
                let pls = lock(&self.pls);
                let lists: Vec<Value> = pls
                    .lists()
                    .iter()
                    .map(|p| json!({"id": p.id, "name": p.name, "count": p.items.len(), "updated": p.updated,
                                    "cover": p.items.first().map(|i| i.key.clone())}))
                    .collect();
                Ok(json!({"lists": lists, "fav": pls.favs().len(), "want": pls.wants().len(), "recent": pls.history().len(), "offline": lock(&self.offline).len()}))
            }
            "col.create" => {
                let name = arg_str(a, "name")?.trim().to_string();
                if name.is_empty() {
                    return Err(CoreError::new("BAD_ARGS", "Chưa đặt tên bộ sưu tập."));
                }
                let items = self.items_of(&arg_keys(a));
                let mut pls = lock(&self.pls);
                let id = pls.create(&name)?;
                if !items.is_empty() {
                    pls.add(id, items)?;
                }
                Ok(json!({"id": id}))
            }
            "col.rename" => {
                let id = pl_id(a)?;
                lock(&self.pls).rename(id, arg_str(a, "name")?)?;
                Ok(json!({}))
            }
            "col.delete" => {
                lock(&self.pls).delete(pl_id(a)?)?;
                Ok(json!({}))
            }
            "col.items" => {
                let items: Vec<Item> = {
                    let pls = lock(&self.pls);
                    match a.get("id") {
                        Some(Value::String(s)) if s == "fav" => pls.favs().to_vec(),
                        Some(Value::String(s)) if s == "want" => pls.wants().to_vec(),
                        Some(Value::String(s)) if s == "recent" => pls.history().to_vec(),
                        _ => pls.get(pl_id(a)?).map(|p| p.items.clone()).ok_or_else(|| CoreError::new("NO_LIST", "Bộ sưu tập đã bị xoá."))?,
                    }
                };
                Ok(Value::Array(self.items_json(&items)))
            }
            "col.add" => {
                let id = pl_id(a)?;
                let items = self.items_of(&arg_keys(a));
                let n = lock(&self.pls).add(id, items)?;
                Ok(json!({"added": n}))
            }
            "col.remove" => {
                let keys = arg_keys(a);
                match a.get("id") {
                    Some(Value::String(s)) if s == "recent" => lock(&self.pls).clear_history()?,
                    Some(Value::String(s)) if s == "fav" || s == "want" => {
                        let mut pls = lock(&self.pls);
                        for k in keys {
                            let it = Item { key: k, ..Default::default() };
                            if s == "fav" {
                                pls.set_fav(it, false)?;
                            } else {
                                pls.set_want(it, false)?;
                            }
                        }
                    }
                    _ => {
                        lock(&self.pls).remove(pl_id(a)?, &keys)?;
                    }
                }
                Ok(json!({}))
            }
            "col.move" => {
                let id = pl_id(a)?;
                let from = a.get("from").and_then(Value::as_u64).unwrap_or(0) as usize;
                let to = a.get("to").and_then(Value::as_u64).unwrap_or(0) as usize;
                lock(&self.pls).move_item(id, from, to)?;
                Ok(json!({}))
            }
            "fav.set" | "want.set" => {
                let key = arg_str(a, "key")?;
                let item = match lock(&self.lib).book(key) {
                    Some(b) => Item::from(b),
                    None => {
                        let r = lock(&self.reads).get(key).cloned();
                        Item { key: key.to_string(), title: r.as_ref().map(|r| r.title.clone()).unwrap_or_default(),
                               author: r.as_ref().map(|r| r.author.clone()).unwrap_or_default(), ext: r.map(|r| r.ext).unwrap_or_default() }
                    }
                };
                let on = a.get("on").and_then(Value::as_bool).unwrap_or(true);
                let mut pls = lock(&self.pls);
                let v = if method == "fav.set" { pls.set_fav(item, on)? } else { pls.set_want(item, on)? };
                Ok(json!({"on": v}))
            }

            // ── Bộ nhớ đệm ──
            "cache.stats" => {
                let dir = self.cfg.data_dir.clone();
                let off = lock(&self.offline).clone();
                let entries = tokio::task::spawn_blocking(move || crate::cache::scan(&dir, &off))
                    .await
                    .map_err(|e| CoreError::new("IO", e.to_string()))?;
                let mut kinds: std::collections::BTreeMap<&str, u64> = crate::cache::KINDS.iter().map(|k| (*k, 0)).collect();
                let mut by_src: HashMap<String, u64> = HashMap::new();
                let mut total = 0u64;
                for e in &entries {
                    total += e.size;
                    *kinds.entry(e.kind).or_default() += e.size;
                    if let (Some(p), true) = (&e.peer, e.kind == "books" || e.kind == "offline") {
                        *by_src.entry(p.clone()).or_default() += e.size;
                    }
                }
                let mut srcs: Vec<(String, u64)> = by_src.into_iter().collect();
                srcs.sort_by(|a, b| b.1.cmp(&a.1));
                srcs.truncate(50);
                let names: HashMap<String, String> = lock(&self.lib).sources().iter().map(|s| (s.peer.clone(), s.name.clone())).collect();
                let p = self.cache.policy();
                let disk = crate::cache::disk_space(&self.cfg.data_dir);
                Ok(json!({
                    "total": total, "files": entries.len(), "kinds": kinds,
                    "sources": srcs.iter().map(|(p, b)| json!({"peer": p, "name": names.get(p), "bytes": b})).collect::<Vec<_>>(),
                    "limit_mb": p.limit_mb, "keep_days": p.keep_days,
                    "free": disk.map(|d| d.0), "disk": disk.map(|d| d.1),
                }))
            }
            "cache.clear" => {
                let kinds: Option<Vec<String>> =
                    a.get("kinds").and_then(Value::as_array).map(|v| v.iter().filter_map(Value::as_str).map(str::to_string).collect());
                let peer = a.get("peer").and_then(Value::as_str).map(str::to_string);
                let dir = self.cfg.data_dir.clone();
                let mut busy = self.files.busy();
                busy.extend(lock(&self.reads).recent(10_000, Some(false)).into_iter().map(|r| r.key.clone()));
                let off = lock(&self.offline).clone();
                let clears_offline = kinds.as_ref().is_some_and(|k| k.iter().any(|x| x == "offline"));
                let (freed, n) = tokio::task::spawn_blocking(move || crate::cache::clear(&dir, &off, kinds.as_deref(), peer.as_deref(), None, &busy))
                    .await
                    .map_err(|e| CoreError::new("IO", e.to_string()))?;
                if clears_offline {
                    let lib = lock(&self.lib);
                    lock(&self.offline).retain(|k| lib.book(k).is_some_and(|b| self.files.complete_path(b).is_some()));
                    drop(lib);
                    self.save_offline();
                }
                self.log(&format!("xoá đệm: {n} tệp ({} KB)", freed >> 10));
                (self.emit)(json!({"e": "cache.cleared"}));
                Ok(json!({"freed": freed, "files": n}))
            }
            "cache.set" => {
                let mut p = self.cache.policy();
                if let Some(v) = a.get("limit_mb").and_then(Value::as_u64) {
                    p.limit_mb = v;
                }
                if let Some(v) = a.get("keep_days").and_then(Value::as_u64) {
                    p.keep_days = v;
                }
                self.save_settings(&[("cache_limit_mb", json!(p.limit_mb)), ("cache_keep_days", json!(p.keep_days))])?;
                self.cache.set_policy(p);
                self.trim_soon(true);
                Ok(json!({}))
            }
            "net.dns" => {
                let host = arg_str(a, "host")?.to_string();
                let fail: HashSet<String> = a
                    .get("fail")
                    .and_then(Value::as_array)
                    .map(|l| l.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                if !fail.is_empty() || a.get("fresh").and_then(Value::as_bool).unwrap_or(false) {
                    gm_net::dns::clear_cache();
                }
                gm_net::dns::set_debug_fail(fail);
                let t = Instant::now();
                let r = gm_net::dns::for_connect(&host).await;
                gm_net::dns::set_debug_fail(Default::default());
                let ms = t.elapsed().as_millis() as u64;
                match r {
                    Ok(Some(ip)) => Ok(json!({"ip": ip.to_string(), "via": gm_net::dns::via(&host), "ms": ms, "secure": true})),
                    Ok(None) => {
                        let sys = tokio::time::timeout(Duration::from_secs(10), tokio::net::lookup_host((host.as_str(), 0))).await;
                        let ip = sys.ok().and_then(|r| r.ok()).and_then(|mut it| it.next()).map(|s| s.ip().to_string());
                        Ok(json!({"ip": ip, "via": "DNS của máy", "ms": t.elapsed().as_millis() as u64, "secure": gm_net::dns::enabled()}))
                    }
                    Err(e) => Err(CoreError::new("NO_DOMAIN", e.to_string())),
                }
            }
            "settings.get" => Ok(json!({"secure_dns": gm_net::dns::enabled(), "dns_providers": gm_net::dns::providers()})),
            "settings.set" => {
                if let Some(v) = a.get("secure_dns").and_then(Value::as_bool) {
                    gm_net::dns::set_enabled(v);
                    self.log(&format!("DNS mã hoá: {}", if v { "bật" } else { "tắt" }));
                    self.save_settings(&[("secure_dns", json!(v))])?;
                }
                Ok(json!({}))
            }
            _ => Err(CoreError::new("NO_METHOD", format!("không có lệnh {method}"))),
        }
    }

    /// Mục bộ sưu tập (bản chép tên…) cho các khoá còn trong thư viện.
    fn items_of(&self, keys: &[String]) -> Vec<Item> {
        let lib = lock(&self.lib);
        keys.iter().filter_map(|k| lib.book(k)).map(Item::from).collect()
    }

    async fn signed_in(self: &Arc<Self>, me: &grammers_client::peer::User) -> Res {
        self.save_session();
        let key = peer_key(me.id());
        let _ = self.save_settings(&[("me", json!(key))]);
        *lock(&self.me) = Some(key);
        (self.emit)(json!({"e": "auth.state", "authorized": true}));
        self.after_login().await;
        // Lần đầu: quét toàn bộ tài khoản
        self.start_sync(true, None);
        Ok(json!({"me": user_json(me)}))
    }
}

fn reading_json(r: &crate::reading::Reading) -> Value {
    json!({"key": r.key, "title": r.title, "author": r.author, "ext": r.ext, "pos": r.pos, "progress": r.progress,
           "chapter": r.chapter, "opened": r.opened, "started": r.started, "finished": r.finished, "seconds": r.seconds,
           "marks": r.marks.iter().map(|m| json!({"id": m.id, "pos": m.pos, "date": m.date, "chapter": m.chapter})).collect::<Vec<_>>()})
}

/// Tên tệp bìa: khoá sách; sách trên máy → `local_<băm đường dẫn>`.
/// Sách đã mở (EPUB, hoặc Kindle đã chuyển): phần thông tin `book.open` dùng chung.
struct Opened {
    title: String,
    authors: Vec<String>,
    language: Option<String>,
    rtl: bool,
    fixed: bool,
    series: Option<(String, f32)>,
    toc: Vec<epub::TocEntry>,
    has_cover: bool,
    /// Ảnh bìa lấy sẵn (Kindle): (dữ liệu, đuôi tệp).
    cover_data: Option<(Arc<Vec<u8>>, &'static str)>,
    /// epub | mobi6 | kf8
    format: &'static str,
}

fn open_epub(path: &std::path::Path, id: String, key: String) -> Result<(OpenBook, Opened), CoreError> {
    let z = crate::zip::Zip::open(path).map_err(|e| CoreError::new("BAD_BOOK", format!("Tệp sách hỏng: {e}")))?;
    let ep = epub::parse(&z).map_err(|e| CoreError::new("BAD_BOOK", e))?;
    if ep.drm {
        return Err(CoreError::new("DRM", "Sách có DRM — không mở được."));
    }
    let spine = ep.spine.iter().map(|s| Spine { path: s.path.clone(), linear: s.linear, size: s.size }).collect();
    let ob = OpenBook::new(id, key, Store::Zip(z), spine, ep.obfuscated.clone());
    let o = Opened {
        has_cover: ep.cover.is_some(),
        title: ep.title,
        authors: ep.authors,
        language: ep.language,
        rtl: ep.rtl,
        fixed: ep.fixed,
        series: ep.series,
        toc: ep.toc,
        cover_data: None,
        format: "epub",
    };
    Ok((ob, o))
}

/// MOBI / AZW3 / AZW / PRC: gb-mobi chuyển cả sách thành tệp chương + ảnh / CSS / font trong bộ nhớ, lõi phục vụ y như mục của
/// EPUB (THIET-KE 4.5). Chuyển một lần lúc mở (sách 1 – 5 MB: dưới 1 giây trên máy cũ).
fn open_kindle(path: &std::path::Path, id: String, key: String) -> Result<(OpenBook, Opened), CoreError> {
    let data = std::fs::read(path).map_err(|e| CoreError::new("IO", format!("Đọc tệp sách: {e}")))?;
    let mb = gb_mobi::convert(&data).map_err(|e| match e {
        gb_mobi::MobiError::Drm => CoreError::new("DRM", e.to_string()),
        gb_mobi::MobiError::Unsupported(_) => CoreError::new("FORMAT", e.to_string()),
        gb_mobi::MobiError::Invalid(_) => CoreError::new("BAD_BOOK", e.to_string()),
    })?;
    drop(data);
    let format = match mb.format {
        gb_mobi::Format::Mobi6 => "mobi6",
        gb_mobi::Format::Kf8 => "kf8",
    };
    let mut files: HashMap<String, (String, Arc<Vec<u8>>)> = HashMap::with_capacity(mb.spine.len() + mb.resources.len());
    let mut spine = Vec::with_capacity(mb.spine.len());
    for it in mb.spine {
        spine.push(Spine { path: it.path.clone(), linear: true, size: it.data.len() as u64 });
        files.insert(it.path, (it.mime, Arc::new(it.data)));
    }
    let mut cover_data = None;
    for it in mb.resources {
        let d = Arc::new(it.data);
        if mb.cover.as_deref() == Some(it.path.as_str()) {
            cover_data = Some((Arc::clone(&d), image_ext(&d)));
        }
        files.insert(it.path, (it.mime, d));
    }
    let toc = mb
        .toc
        .into_iter()
        .map(|t| {
            let (path, frag) = match t.href.split_once('#') {
                Some((p, f)) => (p.to_string(), Some(f.to_string()).filter(|f| !f.is_empty())),
                None => (t.href, None),
            };
            epub::TocEntry { title: t.title, path, frag, depth: t.depth }
        })
        .collect();
    let ob = OpenBook::new(id, key, Store::Mem(files), spine, HashMap::new());
    let o = Opened {
        title: mb.title,
        authors: mb.authors,
        language: mb.language,
        rtl: false,
        fixed: false,
        series: None,
        toc,
        has_cover: cover_data.is_some(),
        cover_data,
        format,
    };
    Ok((ob, o))
}

/// Đuôi tệp ảnh theo vài byte đầu (bìa).
fn image_ext(data: &[u8]) -> &'static str {
    match data.get(..4) {
        Some([0x89, b'P', b'N', b'G']) => "png",
        Some([b'G', b'I', b'F', _]) => "gif",
        _ => "jpg",
    }
}

fn cover_stem(key: &str) -> String {
    match key.strip_prefix(LOCAL_PREFIX) {
        Some(p) => {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            p.hash(&mut h);
            format!("local_{:016x}", h.finish())
        }
        None => key.to_string(),
    }
}

fn pl_id(a: &Value) -> Result<u32, CoreError> {
    a.get("id")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .map(|v| v as u32)
        .ok_or_else(|| CoreError::new("BAD_ARGS", "thiếu «id»"))
}

/// Tên người dùng từ «@tên», «tên», «t.me/tên», «https://t.me/tên/123», «tg://resolve?domain=tên».
pub fn username_of(link: &str) -> Option<String> {
    let l = link.trim();
    let rest = if let Some(i) = l.find("domain=") {
        l[i + 7..].split('&').next().unwrap_or("")
    } else if let Some(i) = l.find("t.me/").or_else(|| l.find("telegram.me/")) {
        let start = i + if l[i..].starts_with("t.me/") { 5 } else { 12 };
        l[start..].trim_start_matches("s/").split(['/', '?']).next().unwrap_or("")
    } else {
        l.trim_start_matches('@')
    };
    let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    (name.len() >= 4 && name.len() <= 32 && !name.starts_with('+') && name.chars().next()?.is_ascii_alphabetic()).then_some(name)
}

fn peer_key(id: PeerId) -> String {
    id.bot_api_dialog_id().map(|i| i.to_string()).unwrap_or_default()
}

fn peer_name(p: &Peer) -> String {
    match p {
        Peer::User(u) => {
            let n = u.full_name();
            if n.trim().is_empty() { "Tài khoản đã xoá".into() } else { n }
        }
        _ => p.name().unwrap_or("").to_string(),
    }
}

/// Gửi được tin vào chat này không: kênh (một chiều) chỉ khi là chủ / quản trị có quyền đăng; tài khoản đã xoá thì không.
fn can_post(p: &Peer) -> bool {
    match p {
        Peer::Channel(c) => c.admin_rights().is_some_and(|r| r.post_messages),
        Peer::User(u) => !u.deleted(),
        _ => true,
    }
}

fn peer_kind(p: &Peer) -> &'static str {
    match p {
        Peer::User(u) if u.is_bot() => "bot",
        Peer::User(_) => "user",
        Peer::Group(_) | Peer::Community(_) => "group",
        Peer::Channel(_) => "channel",
    }
}

/// Ảnh đại diện của peer: (photo_id dạng chuỗi, DC).
fn peer_photo(p: &Peer) -> (Option<String>, Option<i32>) {
    let x = match p {
        Peer::User(u) => u.photo().map(|x| (x.photo_id, x.dc_id)),
        Peer::Group(g) => g.photo().map(|x| (x.photo_id, x.dc_id)),
        Peer::Channel(c) => c.photo().map(|x| (x.photo_id, x.dc_id)),
        Peer::Community(c) => c.photo().map(|x| (x.photo_id, x.dc_id)),
    };
    match x {
        Some((id, dc)) => (Some(id.to_string()), Some(dc)),
        None => (None, None),
    }
}

fn source_from_peer(p: &Peer) -> Source {
    let (photo, photo_dc) = peer_photo(p);
    Source {
        peer: peer_key(p.id()),
        name: peer_name(p),
        kind: peer_kind(p).into(),
        username: p.username().map(str::to_string),
        photo,
        photo_dc,
        enabled: true,
        forum: matches!(p, Peer::Group(g) if matches!(&g.raw, tl::enums::Chat::Channel(c) if c.forum)),
        ..Default::default()
    }
}

fn user_json(u: &grammers_client::peer::User) -> Value {
    let (photo, photo_dc) = peer_photo(&Peer::User(u.clone()));
    json!({"id": peer_key(u.id()), "name": u.full_name(), "username": u.username(), "phone": u.phone(), "photo": photo, "photo_dc": photo_dc})
}

/// Tệp tạm riêng cho mỗi lần tải (`<tên>.<số>.part`) — hai lệnh cùng tải một tệp không ghi chồng (GramMini D27).
fn part_path(path: &std::path::Path) -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    path.with_extension(format!("{}.part", N.fetch_add(1, Ordering::Relaxed)))
}

/// Tách khoá sách cho app.
pub fn key_parts(key: &str) -> Option<(String, i32)> {
    split_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert_eq!(username_of("@sachhay_vn").as_deref(), Some("sachhay_vn"));
        assert_eq!(username_of("https://t.me/chiasesachhay/123").as_deref(), Some("chiasesachhay"));
        assert_eq!(username_of("t.me/s/chiasesachhay").as_deref(), Some("chiasesachhay"));
        assert_eq!(username_of("https://t.me/+AbCdEf"), None);
    }

    #[test]
    fn url_paths() {
        assert_eq!(url_path("OEBPS/Text/c 2.xhtml"), "OEBPS/Text/c%202.xhtml");
        assert_eq!(url_path("OEBPS/Chương 1.html"), "OEBPS/Ch%C6%B0%C6%A1ng%201.html");
    }
}
