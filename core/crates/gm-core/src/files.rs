//! Tải tệp sách từ Telegram: từng khúc 256 KB qua `upload.getFile`, ghi xuống đĩa. Chép từ `stream.rs` của GramMusic (bỏ cổng phát,
//! nhịp nghe, giải mã nhạc — cổng phục vụ sách nằm ở serve.rs).
//!
//! - Mỗi sách một tệp trong `books/`: đang dở là `<khoá>.<đuôi>.part` (tệp thưa, đủ cỡ) + `.map` (khúc nào đã có); đủ khúc thì
//!   đổi tên thành `<khoá>.<đuôi>`. Tắt app giữa chừng → lần sau tải tiếp từ `.map`.
//! - Khúc 256 KB: `upload.getFile` đòi offset / limit chia hết cho 4 KB, 1 MB chia hết cho limit và một lần đọc không vắt qua
//!   ranh giới 1 MB — khúc 256 KB bắt đầu ở bội 256 KB luôn thoả.
//! - Mỗi tệp tối đa 2 khúc tải song song, cả lõi tối đa 4. Lỗi tạm (mạng, FLOOD_WAIT) thì chờ rồi thử lại.
//! - `wait_block` (đọc một khúc khi tệp chưa đủ) để dành cho PDF lớn mở khi chưa tải xong (GĐ3).

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::sync::{Notify, Semaphore};

use crate::library::Book;

/// Cỡ một khúc tải.
pub const BLOCK: u64 = 256 * 1024;
/// Khúc tải song song của một tệp.
const TRACK_WORKERS: u32 = 2;
/// Không còn ai cần quá chừng này thì thôi tải (trừ tệp đang tải trọn).
pub(crate) const IDLE: Duration = Duration::from_secs(6);
/// Chờ một khúc quá chừng này thì báo lỗi.
#[allow(dead_code)] // PDF mở khi chưa tải xong (GĐ3)
const WAIT_BLOCK: Duration = Duration::from_secs(45);

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Lỗi lấy dữ liệu: `retry` = đáng thử lại (mạng, FLOOD_WAIT…), kèm thời gian chờ gợi ý.
#[derive(Debug, Clone)]
pub struct FetchError {
    pub code: String,
    pub msg: String,
    pub retry: Option<Duration>,
}

/// Nơi lấy dữ liệu: lõi thật dùng `upload.getFile` (service.rs); phép thử dùng dữ liệu trong bộ nhớ.
pub trait Fetch: Send + Sync + 'static {
    /// Đọc khúc bắt đầu ở `offset` (bội của BLOCK), tối đa BLOCK byte (khúc cuối ngắn hơn).
    fn fetch<'a>(&'a self, book: &'a Book, offset: u64) -> BoxFut<'a, Result<Vec<u8>, FetchError>>;
}

pub type Emit = Arc<dyn Fn(Value) + Send + Sync>;

struct TState {
    have: Vec<bool>,
    inflight: HashSet<u32>,
    /// Khúc đang cần (đọc khi chưa đủ).
    want: u32,
    /// Tải trọn tệp.
    full: bool,
    workers: u32,
    last_read: Instant,
    err: Option<FetchError>,
    done: bool,
    last_emit: Instant,
}

/// Một tệp đang tải.
pub struct Track {
    pub book: Book,
    n: u32,
    part: PathBuf,
    map: PathBuf,
    fin: PathBuf,
    st: Mutex<TState>,
    notify: Notify,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(unix)]
pub(crate) fn read_at(f: &std::fs::File, buf: &mut [u8], off: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(f, buf, off)
}
#[cfg(windows)]
pub(crate) fn read_at(f: &std::fs::File, buf: &mut [u8], off: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(f, buf, off)
}
#[cfg(unix)]
pub(crate) fn write_all_at(f: &std::fs::File, buf: &[u8], off: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::write_all_at(f, buf, off)
}
#[cfg(windows)]
pub(crate) fn write_all_at(f: &std::fs::File, mut buf: &[u8], mut off: u64) -> io::Result<()> {
    while !buf.is_empty() {
        let n = std::os::windows::fs::FileExt::seek_write(f, buf, off)?;
        buf = &buf[n..];
        off += n as u64;
    }
    Ok(())
}

impl Track {
    fn open(dir: &Path, book: Book) -> io::Result<Track> {
        let name = book.file_name();
        let fin = dir.join(&name);
        let part = dir.join(format!("{name}.part"));
        let map = dir.join(format!("{name}.map"));
        let n = book.size.div_ceil(BLOCK).max(1) as u32;
        let mut have = vec![false; n as usize];
        let mut done = false;
        if std::fs::metadata(&fin).map(|m| m.len() == book.size).unwrap_or(false) {
            have.iter_mut().for_each(|h| *h = true);
            done = true;
        } else {
            let ok = std::fs::metadata(&part).map(|m| m.len() == book.size).unwrap_or(false);
            match std::fs::read(&map) {
                Ok(m) if ok && m.len() == n as usize => {
                    for (h, b) in have.iter_mut().zip(m) {
                        *h = b == 1;
                    }
                }
                _ => {
                    std::fs::create_dir_all(dir)?;
                    let f = std::fs::File::create(&part)?;
                    f.set_len(book.size)?; // tệp thưa: chưa tốn chỗ thật
                    std::fs::write(&map, vec![0u8; n as usize])?;
                }
            }
        }
        let now = Instant::now();
        Ok(Track {
            book,
            n,
            part,
            map,
            fin,
            st: Mutex::new(TState {
                have,
                inflight: HashSet::new(),
                want: 0,
                full: false,
                workers: 0,
                last_read: now - IDLE * 2,
                err: None,
                done,
                last_emit: now,
            }),
            notify: Notify::new(),
        })
    }

    pub fn key(&self) -> String {
        self.book.key()
    }

    /// (khúc đã có, tổng số khúc).
    pub fn progress(&self) -> (u32, u32) {
        let st = lock(&self.st);
        (st.have.iter().filter(|h| **h).count() as u32, self.n)
    }

    pub fn is_done(&self) -> bool {
        lock(&self.st).done
    }

    /// Lỗi hẳn (không tự thử lại nữa) của lần tải gần nhất, khi không còn lượt tải nào chạy.
    pub fn failed(&self) -> Option<FetchError> {
        let st = lock(&self.st);
        if st.done || st.workers > 0 {
            return None;
        }
        st.err.clone().filter(|e| e.retry.is_none())
    }

    pub fn is_full(&self) -> bool {
        lock(&self.st).full
    }

    /// Còn việc (vừa đọc, đang tải trọn, đang có khúc bay).
    fn active(st: &TState) -> bool {
        st.last_read.elapsed() < IDLE || st.full || !st.inflight.is_empty()
    }

    /// Khúc nên tải tiếp: tải trọn thì từ chỗ đang cần tới hết rồi quay lại phần đầu; đang đọc dở (PDF) thì khúc cần + vài khúc sau.
    fn next_block(&self, st: &TState) -> Option<u32> {
        if st.done || st.err.as_ref().is_some_and(|e| e.retry.is_none()) {
            return None;
        }
        let free = |b: u32| !st.have[b as usize] && !st.inflight.contains(&b);
        let want = st.want.min(self.n);
        if st.full {
            if let Some(b) = (want..self.n).find(|&b| free(b)) {
                return Some(b);
            }
            return (0..want).find(|&b| free(b));
        }
        if st.last_read.elapsed() < IDLE {
            return (want..(want + 4).min(self.n)).find(|&b| free(b));
        }
        None
    }
}

/// Bộ tải tệp của lõi.
pub struct Files {
    dir: PathBuf,
    fetch: Arc<dyn Fetch>,
    emit: Emit,
    tracks: Mutex<HashMap<String, Arc<Track>>>,
    sem: Arc<Semaphore>,
}

impl Files {
    pub fn new(dir: PathBuf, fetch: Arc<dyn Fetch>, emit: Emit) -> Arc<Files> {
        Arc::new(Files { dir, fetch, emit, tracks: Mutex::new(HashMap::new()), sem: Arc::new(Semaphore::new(4)) })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Mở (hoặc lấy lại) tệp đang theo dõi; dọn các tệp đã thôi dùng.
    pub fn track(&self, book: &Book) -> io::Result<Arc<Track>> {
        let key = book.key();
        let mut m = lock(&self.tracks);
        m.retain(|k, t| *k == key || Track::active(&lock(&t.st)));
        if let Some(t) = m.get(&key) {
            if t.book.size == book.size {
                return Ok(Arc::clone(t));
            }
        }
        let t = Arc::new(Track::open(&self.dir, book.clone())?);
        m.insert(key, Arc::clone(&t));
        Ok(t)
    }

    /// Khoá các tệp đang dùng (bộ nhớ đệm không xoá).
    pub fn busy(&self) -> HashSet<String> {
        lock(&self.tracks).iter().filter(|(_, t)| Track::active(&lock(&t.st))).map(|(k, _)| k.clone()).collect()
    }

    /// Bỏ theo dõi (tệp vừa bị xoá khỏi bộ nhớ đệm).
    pub fn forget(&self, key: &str) {
        lock(&self.tracks).remove(key);
    }

    /// Đường dẫn tệp đầy đủ nếu đã tải đủ.
    pub fn complete_path(&self, book: &Book) -> Option<PathBuf> {
        let p = self.dir.join(book.file_name());
        std::fs::metadata(&p).ok().filter(|m| m.len() == book.size).map(|_| p)
    }

    /// Tiến độ đã có trên đĩa: 1 = đủ, 0…1 = dở, 0 = chưa có.
    pub fn cached_fraction(&self, book: &Book) -> f64 {
        if let Some(t) = lock(&self.tracks).get(&book.key()) {
            let (h, n) = t.progress();
            return h as f64 / n.max(1) as f64;
        }
        if self.complete_path(book).is_some() {
            return 1.0;
        }
        match std::fs::read(self.dir.join(format!("{}.map", book.file_name()))) {
            Ok(m) if !m.is_empty() => m.iter().filter(|b| **b == 1).count() as f64 / m.len() as f64,
            _ => 0.0,
        }
    }

    /// Tải trọn tệp (`on`) hoặc thôi (phần đã có giữ làm đệm).
    pub fn set_full(self: &Arc<Self>, book: &Book, on: bool) -> io::Result<Arc<Track>> {
        let t = self.track(book)?;
        {
            let mut st = lock(&t.st);
            st.full = on;
            if on {
                st.err = None;
            }
        }
        if on {
            self.pump(&t);
        }
        Ok(t)
    }

    /// Tải trọn rồi chờ xong (mở sách). Báo `file.progress` khoảng 1 lần / 0,8 giây. Lỗi hẳn → Err.
    pub async fn fetch_all(self: &Arc<Self>, book: &Book, max: Duration) -> Result<PathBuf, FetchError> {
        if let Some(p) = self.complete_path(book) {
            return Ok(p);
        }
        let io_err = |e: io::Error| FetchError { code: "IO".into(), msg: format!("Ghi tệp sách: {e}"), retry: None };
        let t = self.set_full(book, true).map_err(io_err)?;
        let deadline = Instant::now() + max;
        loop {
            let notified = t.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if t.is_done() {
                return self.complete_path(book).ok_or(FetchError { code: "IO".into(), msg: "Tệp sách tải xong nhưng không thấy trên đĩa.".into(), retry: None });
            }
            if let Some(e) = t.failed() {
                return Err(e);
            }
            if !t.is_full() {
                // lỗi tạm đã bỏ cờ tải trọn → bật lại
                let _ = self.set_full(book, true);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(FetchError { code: "TIMEOUT".into(), msg: "Tải sách quá lâu.".into(), retry: None });
            }
            let _ = tokio::time::timeout(left.min(Duration::from_secs(2)), notified).await;
        }
    }

    /// Chạy thêm lượt tải nếu còn khúc cần và chưa đủ lượt.
    fn pump(self: &Arc<Self>, t: &Arc<Track>) {
        loop {
            let b = {
                let mut st = lock(&t.st);
                if st.workers >= TRACK_WORKERS {
                    return;
                }
                let Some(b) = t.next_block(&st) else { return };
                st.inflight.insert(b);
                st.workers += 1;
                b
            };
            let (me, t) = (Arc::clone(self), Arc::clone(t));
            tokio::spawn(async move { me.worker(t, b).await });
        }
    }

    async fn worker(self: Arc<Self>, t: Arc<Track>, mut b: u32) {
        let mut fails = 0u32;
        loop {
            let permit = Arc::clone(&self.sem).acquire_owned().await;
            let r = self.fetch.fetch(&t.book, b as u64 * BLOCK).await;
            drop(permit);
            match r {
                Ok(data) => {
                    fails = 0;
                    let want = ((t.book.size - b as u64 * BLOCK).min(BLOCK)) as usize;
                    if data.len() < want {
                        self.fail(&t, b, FetchError { code: "SHORT".into(), msg: "Máy chủ trả thiếu dữ liệu.".into(), retry: None });
                        break;
                    }
                    if let Err(e) = self.store(&t, b, &data[..want]) {
                        self.fail(&t, b, FetchError { code: "IO".into(), msg: format!("Ghi tệp sách: {e}"), retry: None });
                        break;
                    }
                }
                Err(e) => {
                    fails += 1;
                    match e.retry {
                        Some(wait) if fails <= 6 => {
                            lock(&t.st).err = Some(e.clone());
                            t.notify.notify_waiters();
                            tokio::time::sleep(wait.min(Duration::from_secs(30))).await;
                            let st = lock(&t.st);
                            if !Track::active(&st) {
                                drop(st);
                                self.release(&t, b);
                                break;
                            }
                            continue;
                        }
                        _ => {
                            self.fail(&t, b, e);
                            break;
                        }
                    }
                }
            }
            let next = {
                let mut st = lock(&t.st);
                st.inflight.remove(&b);
                st.err = None;
                match t.next_block(&st) {
                    Some(n) => {
                        st.inflight.insert(n);
                        Some(n)
                    }
                    None => {
                        st.workers -= 1;
                        None
                    }
                }
            };
            match next {
                Some(n) => b = n,
                None => return,
            }
        }
    }

    fn release(&self, t: &Track, b: u32) {
        let mut st = lock(&t.st);
        st.inflight.remove(&b);
        st.workers -= 1;
    }

    fn fail(&self, t: &Track, b: u32, e: FetchError) {
        {
            let mut st = lock(&t.st);
            st.inflight.remove(&b);
            st.workers -= 1;
            st.full = false;
            st.err = Some(e.clone());
        }
        t.notify.notify_waiters();
        (self.emit)(json!({"e": "file.error", "key": t.key(), "code": e.code, "msg": e.msg}));
    }

    /// Ghi khúc `b`, đánh dấu đã có; đủ khúc thì đổi tên thành tệp đầy đủ.
    fn store(&self, t: &Track, b: u32, data: &[u8]) -> io::Result<()> {
        {
            let f = std::fs::OpenOptions::new().write(true).open(&t.part)?;
            write_all_at(&f, data, b as u64 * BLOCK)?;
        }
        let (all, emit) = {
            let mut st = lock(&t.st);
            st.have[b as usize] = true;
            let map: Vec<u8> = st.have.iter().map(|h| *h as u8).collect();
            std::fs::write(&t.map, map)?;
            let all = st.have.iter().all(|h| *h);
            let emit = all || st.last_emit.elapsed() > Duration::from_millis(800);
            if emit {
                st.last_emit = Instant::now();
            }
            (all, emit)
        };
        if all {
            std::fs::rename(&t.part, &t.fin)?;
            let _ = std::fs::remove_file(&t.map);
            lock(&t.st).done = true;
            (self.emit)(json!({"e": "file.done", "key": t.key()}));
        } else if emit {
            let (h, n) = t.progress();
            (self.emit)(json!({"e": "file.progress", "key": t.key(), "have": h, "total": n}));
        }
        t.notify.notify_waiters();
        Ok(())
    }

    /// Chờ tới khi khúc `b` có trên đĩa (đặt vị trí cần cho bộ tải) — PDF mở khi chưa tải xong (GĐ3).
    #[allow(dead_code)]
    pub(crate) async fn wait_block(self: &Arc<Self>, t: &Arc<Track>, b: u32) -> Result<(), FetchError> {
        let deadline = Instant::now() + WAIT_BLOCK;
        loop {
            let notified = t.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut st = lock(&t.st);
                if st.have[b as usize] {
                    return Ok(());
                }
                if let Some(e) = st.err.as_ref().filter(|e| e.retry.is_none()) {
                    let e = e.clone();
                    st.err = None;
                    if st.workers > 0 {
                        return Err(e);
                    }
                }
                st.want = b;
                st.last_read = Instant::now();
            }
            self.pump(t);
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                let e = lock(&t.st).err.clone();
                return Err(e.unwrap_or(FetchError { code: "TIMEOUT".into(), msg: "Tải sách quá lâu.".into(), retry: None }));
            }
            let _ = tokio::time::timeout(left.min(Duration::from_secs(1)), notified).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Dữ liệu giả trong bộ nhớ, chậm một chút, lỗi tạm vài lần đầu.
    struct Fake {
        data: Vec<u8>,
        calls: AtomicUsize,
        fail_first: AtomicUsize,
    }

    impl Fetch for Fake {
        fn fetch<'a>(&'a self, _book: &'a Book, offset: u64) -> BoxFut<'a, Result<Vec<u8>, FetchError>> {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(3)).await;
                self.calls.fetch_add(1, Ordering::SeqCst);
                #[allow(deprecated)] // try_update chỉ có ở Rust mới — giữ fetch_update cho bộ dựng iOS
                if self.fail_first.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() {
                    return Err(FetchError { code: "NETWORK".into(), msg: "thử".into(), retry: Some(Duration::from_millis(20)) });
                }
                let a = offset as usize;
                let b = (a + BLOCK as usize).min(self.data.len());
                Ok(self.data[a..b].to_vec())
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fetch_all_with_retries_and_resume() {
        let dir = std::env::temp_dir().join(format!("gb-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let size = (BLOCK * 5 + 777) as usize;
        let data: Vec<u8> = (0..size).map(|i| (i * 7 % 251) as u8).collect();
        let fake = Arc::new(Fake { data: data.clone(), calls: AtomicUsize::new(0), fail_first: AtomicUsize::new(2) });
        let book = Book { peer: "-1001".into(), id: 7, title: "t".into(), size: size as u64, ext: "epub".into(), ..Default::default() };
        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let ev2 = Arc::clone(&events);
        let emit: Emit = Arc::new(move |v| lock(&ev2).push(v["e"].as_str().unwrap_or("").to_string()));
        let files = Files::new(dir.join("books"), fake.clone(), emit);
        let p = files.fetch_all(&book, Duration::from_secs(20)).await.unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), data);
        assert_eq!(files.cached_fraction(&book), 1.0);
        assert!(lock(&events).contains(&"file.done".to_string()));
        // Lần sau: có sẵn, không tải
        let calls = fake.calls.load(Ordering::SeqCst);
        files.fetch_all(&book, Duration::from_secs(5)).await.unwrap();
        assert_eq!(fake.calls.load(Ordering::SeqCst), calls);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
