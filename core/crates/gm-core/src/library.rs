//! Thư viện sách: các nguồn (Tin nhắn đã lưu, nhóm, kênh, chat riêng) và chỉ mục sách của từng nguồn. Chép từ GramMusic
//! (bài hát → sách).
//!
//! - `sources.json`: danh sách nguồn (bật / tắt trong thư viện, lần quét cuối).
//! - `library/<peer>.json`: chỉ mục một nguồn — các sách (tin mới → cũ), đã quét hết chưa.
//!
//! Trong bộ nhớ: danh sách phẳng các sách của nguồn đang bật (lọc trùng theo mã tài liệu — cùng một tệp chuyển tiếp sang nhiều
//! chat chỉ hiện một lần), kèm chữ đã bỏ dấu để tìm / xếp. Thứ tự xếp được đệm theo (nguồn, cách xếp) tới khi thư viện đổi.
//! App lấy từng trang (`query`) chứ không lấy cả thư viện — máy A4 256 MB, kênh sách có hàng chục nghìn tệp.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::text;

/// Một cuốn sách = một tin có tệp sách.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Book {
    pub peer: String,
    pub id: i32,
    /// Tên theo Telegram / tên tệp.
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub mime: String,
    /// Tên tệp gốc.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub ext: String,
    /// Ngày gửi (Unix giây).
    #[serde(default)]
    pub date: i64,
    /// Mã tài liệu Telegram (lọc trùng).
    #[serde(default)]
    pub doc: i64,
    /// Có ảnh nhỏ đính kèm tệp.
    #[serde(default)]
    pub thumb: bool,
    /// Chủ đề (nhóm diễn đàn): id tin tạo chủ đề; 0 = không thuộc chủ đề nào.
    #[serde(default)]
    pub topic: i32,
    /// Chú thích của tin (≤ 500 ký tự) — kênh sách hay ghi tên, tác giả, giới thiệu.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub caption: String,
    /// Tên / tác giả thật đọc trong sách (OPF) sau lần mở đầu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ba: Option<String>,
    /// Tin «hạn chế lưu nội dung» (cờ `noforwards` của Telegram): không gửi tiếp, không chép chữ, không mở bằng app khác / xuất.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nf: bool,
}

impl Book {
    pub fn key(&self) -> String {
        format!("{}_{}", self.peer, self.id)
    }

    /// Tên tệp trong bộ nhớ đệm: `<khoá>.<đuôi>`.
    pub fn file_name(&self) -> String {
        format!("{}.{}", self.key(), if self.ext.is_empty() { "bin" } else { &self.ext })
    }

    pub fn readable(&self) -> bool {
        text::readable_ext(&self.ext)
    }

    /// Tên hiện ra: tên trong sách nếu đã đọc, không thì tên theo Telegram / tên tệp.
    pub fn shown_title(&self) -> &str {
        self.bt.as_deref().filter(|t| !t.is_empty()).unwrap_or(&self.title)
    }

    pub fn shown_author(&self) -> &str {
        self.ba.as_deref().filter(|t| !t.is_empty()).unwrap_or(&self.author)
    }
}

/// Tách khoá sách `<peer>_<id>`.
pub fn split_key(key: &str) -> Option<(String, i32)> {
    let (p, i) = key.rsplit_once('_')?;
    p.parse::<i64>().ok()?;
    Some((p.to_string(), i.parse().ok()?))
}

/// Một nguồn sách.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Source {
    pub peer: String,
    pub name: String,
    /// self (Tin nhắn đã lưu) | user | bot | group | channel
    pub kind: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub photo: Option<String>,
    #[serde(default)]
    pub photo_dc: Option<i32>,
    /// Hiện trong «Thư viện» (người dùng có thể ẩn một nguồn).
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Nguồn thêm tay (kênh công khai thêm bằng @tên / link — chưa vào kênh thì danh sách chat không có).
    #[serde(default)]
    pub manual: bool,
    #[serde(default)]
    pub added: i64,
    /// Lần quét riêng xong gần nhất (Unix giây, 0 = chưa).
    #[serde(default)]
    pub synced: i64,
    /// Nhóm có chủ đề (diễn đàn).
    #[serde(default)]
    pub forum: bool,
    /// Các chủ đề (lấy lần gần nhất).
    #[serde(default)]
    pub topics: Vec<Topic>,
}

/// Một chủ đề của nhóm diễn đàn.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Topic {
    pub id: i32,
    pub title: String,
    #[serde(default)]
    pub color: i32,
}

fn yes() -> bool {
    true
}

/// Chỉ mục một nguồn: các sách (tin mới → cũ).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SourceIndex {
    pub peer: String,
    #[serde(default)]
    pub complete: bool,
    #[serde(default)]
    pub books: Vec<Book>,
}

impl SourceIndex {
    /// Gộp sách (đã có thì cập nhật — tin có thể đã bị sửa, giữ tên / tác giả đã đọc trong sách); giữ thứ tự tin mới → cũ. Trả số
    /// sách mới.
    pub fn merge(&mut self, books: Vec<Book>) -> usize {
        let mut pos: HashMap<i32, usize> = self.books.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        let mut n = 0;
        let mut sorted = true;
        for mut b in books {
            match pos.get(&b.id) {
                Some(&i) => {
                    let old = &self.books[i];
                    if b.doc == old.doc {
                        b.bt = old.bt.clone();
                        b.ba = old.ba.clone();
                    }
                    self.books[i] = b;
                }
                None => {
                    if self.books.last().is_some_and(|l| l.id < b.id) {
                        sorted = false;
                    }
                    pos.insert(b.id, self.books.len());
                    self.books.push(b);
                    n += 1;
                }
            }
        }
        if !sorted {
            self.books.sort_by_key(|b| std::cmp::Reverse(b.id));
        }
        n
    }

    pub fn has(&self, id: i32) -> bool {
        self.books.iter().any(|s| s.id == id)
    }
}

/// Cách xếp.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sort {
    /// Mới nhất trước.
    Date,
    /// Tên A–Z.
    Title,
    /// Tác giả A–Z (rồi tên).
    Author,
}

impl Sort {
    pub fn parse(s: &str) -> Sort {
        match s {
            "title" => Sort::Title,
            "author" => Sort::Author,
            _ => Sort::Date,
        }
    }
}

/// Sách trong bộ nhớ: sách + chữ bỏ dấu để tìm, khoá xếp.
struct Entry {
    book: Book,
    /// Nguồn đang bật và không trùng tệp với sách mới hơn của nguồn đang bật khác → có trong «mọi nguồn».
    shown: bool,
    /// tên + tác giả + tên tệp + đầu chú thích, bỏ dấu (tìm).
    hay: String,
    tkey: String,
    akey: String,
}

fn rank(c: char) -> u8 {
    if c == '#' { 27 } else { c as u8 - b'A' + 1 }
}

pub struct Library {
    dir: PathBuf,
    sources: Vec<Source>,
    idx: HashMap<String, SourceIndex>,
    /// Sách của mọi nguồn (mới → cũ) — dựng lại khi `dirty`.
    all: Vec<Entry>,
    dirty: bool,
    /// Thứ tự xếp đã tính: (nguồn hoặc "", cách xếp) → chỉ số trong `all`.
    orders: HashMap<(String, Sort), Vec<u32>>,
    /// Đổi mỗi khi thư viện đổi (app so để biết trang đã lấy còn đúng không).
    pub generation: u64,
    /// Nguồn có sách mới chưa ghi xuống đĩa.
    unsaved: HashSet<String>,
}

impl Library {
    /// Đọc `sources.json` + `library/*.json` (hỏng thì bỏ qua — quét lại được).
    pub fn open(data_dir: &Path) -> Library {
        let dir = data_dir.join("library");
        let _ = std::fs::create_dir_all(&dir);
        let sources: Vec<Source> = std::fs::read(data_dir.join("sources.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut idx = HashMap::new();
        for s in &sources {
            if let Some(i) = std::fs::read(dir.join(format!("{}.json", s.peer))).ok().and_then(|b| serde_json::from_slice::<SourceIndex>(&b).ok()) {
                idx.insert(s.peer.clone(), i);
            }
        }
        Library { dir, sources, idx, all: Vec::new(), dirty: true, orders: HashMap::new(), generation: 1, unsaved: HashSet::new() }
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.orders.clear();
        self.generation += 1;
    }

    pub fn sources(&self) -> &[Source] {
        &self.sources
    }

    pub fn source(&self, peer: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.peer == peer)
    }

    pub fn index(&self, peer: &str) -> Option<&SourceIndex> {
        self.idx.get(peer)
    }

    /// Số sách đã quét của một nguồn.
    pub fn indexed(&self, peer: &str) -> usize {
        self.idx.get(peer).map(|i| i.books.len()).unwrap_or(0)
    }

    fn save_sources(&self) -> std::io::Result<()> {
        let path = self.dir.parent().unwrap_or(&self.dir).join("sources.json");
        write_atomic(&path, &serde_json::to_vec(&self.sources).unwrap_or_default())
    }

    /// Thêm / cập nhật nguồn (tên, ảnh mới; giữ `enabled`, `manual`, `added`, `synced` cũ nếu đã có).
    pub fn upsert_source(&mut self, mut s: Source) -> std::io::Result<()> {
        if let Some(old) = self.sources.iter_mut().find(|x| x.peer == s.peer) {
            s.enabled = old.enabled;
            s.manual = old.manual || s.manual;
            s.added = old.added;
            s.synced = old.synced;
            if s.topics.is_empty() {
                s.topics = std::mem::take(&mut old.topics);
            }
            *old = s;
        } else {
            self.sources.push(s);
        }
        self.changed();
        self.save_sources()
    }

    /// Gộp sách vào chỉ mục của nguồn (tạo nguồn nếu chưa có, bằng `make`). Chưa ghi xuống đĩa — gọi `flush`. Trả số sách mới.
    pub fn add_books(&mut self, peer: &str, books: Vec<Book>, make: impl FnOnce() -> Source) -> std::io::Result<usize> {
        if books.is_empty() {
            return Ok(0);
        }
        if self.source(peer).is_none() {
            let mut s = make();
            s.peer = peer.to_string();
            self.sources.push(s);
            self.save_sources()?;
        }
        let i = self.idx.entry(peer.to_string()).or_insert_with(|| SourceIndex { peer: peer.to_string(), ..Default::default() });
        let n = i.merge(books);
        self.unsaved.insert(peer.to_string());
        self.changed();
        Ok(n)
    }

    /// Ghi các chỉ mục còn chưa ghi.
    pub fn flush(&mut self) -> std::io::Result<()> {
        for p in std::mem::take(&mut self.unsaved) {
            if let Some(i) = self.idx.get(&p) {
                write_atomic(&self.dir.join(format!("{p}.json")), &serde_json::to_vec(i).unwrap_or_default())?;
            }
        }
        Ok(())
    }

    /// Đánh dấu nguồn đã quét riêng xong.
    pub fn set_complete(&mut self, peer: &str, complete: bool) {
        if let Some(i) = self.idx.get_mut(peer) {
            i.complete = complete;
            self.unsaved.insert(peer.to_string());
        }
    }

    pub fn update_source(&mut self, peer: &str, f: impl FnOnce(&mut Source)) -> std::io::Result<()> {
        if let Some(s) = self.sources.iter_mut().find(|x| x.peer == peer) {
            let was = s.enabled;
            f(s);
            if was != s.enabled {
                self.changed();
            }
            return self.save_sources();
        }
        Ok(())
    }

    /// Gỡ nguồn khỏi thư viện (xoá cả chỉ mục).
    pub fn remove_source(&mut self, peer: &str) -> std::io::Result<()> {
        self.sources.retain(|s| s.peer != peer);
        self.idx.remove(peer);
        let _ = std::fs::remove_file(self.dir.join(format!("{peer}.json")));
        self.changed();
        self.save_sources()
    }

    /// Thay chỉ mục của nguồn và ghi xuống đĩa.
    pub fn put_index(&mut self, i: SourceIndex) -> std::io::Result<()> {
        self.unsaved.remove(&i.peer);
        let path = self.dir.join(format!("{}.json", i.peer));
        let bytes = serde_json::to_vec(&i).unwrap_or_default();
        self.idx.insert(i.peer.clone(), i);
        self.changed();
        write_atomic(&path, &bytes)
    }

    /// Xoá sách khỏi chỉ mục (tin đã bị xoá trên Telegram). `peer` None = tìm ở mọi nguồn không phải kênh. Trả số sách đã xoá.
    pub fn remove_books(&mut self, peer: Option<&str>, ids: &[i32]) -> usize {
        let ids: HashSet<i32> = ids.iter().copied().collect();
        let mut n = 0;
        let mut touched = Vec::new();
        for (p, i) in self.idx.iter_mut() {
            let hit = match peer {
                Some(x) => x == p,
                None => !p.starts_with("-100"),
            };
            if !hit {
                continue;
            }
            let before = i.books.len();
            i.books.retain(|s| !ids.contains(&s.id));
            if i.books.len() != before {
                n += before - i.books.len();
                touched.push(p.clone());
            }
        }
        for p in touched {
            if let Some(i) = self.idx.get(&p) {
                let _ = write_atomic(&self.dir.join(format!("{p}.json")), &serde_json::to_vec(i).unwrap_or_default());
            }
        }
        if n > 0 {
            self.changed();
        }
        n
    }

    /// Ghi tên / tác giả thật đọc trong sách (OPF) cho mọi bản của cùng tệp (cùng mã tài liệu). Trả có đổi gì không.
    pub fn set_book_meta(&mut self, key: &str, title: Option<&str>, author: Option<&str>) -> bool {
        let Some(doc) = self.book(key).map(|b| b.doc) else { return false };
        let clean = |s: Option<&str>| s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        let (t, a) = (clean(title), clean(author));
        let mut touched = Vec::new();
        for (p, i) in self.idx.iter_mut() {
            for b in i.books.iter_mut() {
                let same = if doc != 0 { b.doc == doc } else { b.key() == key };
                if same && (b.bt != t || b.ba != a) {
                    b.bt = t.clone();
                    b.ba = a.clone();
                    touched.push(p.clone());
                }
            }
        }
        touched.dedup();
        for p in &touched {
            self.unsaved.insert(p.clone());
        }
        if !touched.is_empty() {
            self.changed();
        }
        !touched.is_empty()
    }

    fn rebuild(&mut self) {
        if !self.dirty {
            return;
        }
        let on: HashSet<&str> = self.sources.iter().filter(|s| s.enabled).map(|s| s.peer.as_str()).collect();
        let mut books: Vec<&Book> = Vec::new();
        for s in &self.sources {
            if let Some(i) = self.idx.get(&s.peer) {
                books.extend(i.books.iter());
            }
        }
        // Mới trước: sách trùng tệp (chuyển tiếp sang nhiều chat) chỉ hiện bản mới nhất trong «mọi nguồn»
        books.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
        let mut seen: HashSet<i64> = HashSet::new();
        let mut all = Vec::with_capacity(books.len());
        for b in books {
            let shown = on.contains(b.peer.as_str()) && (b.doc == 0 || seen.insert(b.doc));
            let cap = text::truncate_chars(&b.caption, 300);
            let hay = text::fold(&format!("{} {} {} {} {} {}", b.title, b.author, b.bt.as_deref().unwrap_or(""),
                                          b.ba.as_deref().unwrap_or(""), b.name, cap));
            all.push(Entry { tkey: text::sort_key(b.shown_title()), akey: text::sort_key(b.shown_author()), book: b.clone(), shown, hay });
        }
        self.all = all;
        self.dirty = false;
    }

    /// Thứ tự xếp (đệm) của các sách thuộc `source` ("" = mọi nguồn đang bật).
    fn order(&mut self, source: &str, sort: Sort) -> &Vec<u32> {
        self.rebuild();
        let k = (source.to_string(), sort);
        if !self.orders.contains_key(&k) {
            let all = &self.all;
            let mut v: Vec<u32> = (0..all.len() as u32)
                .filter(|&i| if source.is_empty() { all[i as usize].shown } else { all[i as usize].book.peer == source })
                .collect();
            match sort {
                Sort::Date => {}
                Sort::Title => v.sort_by(|&a, &b| {
                    let (x, y) = (&all[a as usize], &all[b as usize]);
                    (rank(text::letter(&x.tkey)), &x.tkey, &x.akey).cmp(&(rank(text::letter(&y.tkey)), &y.tkey, &y.akey))
                }),
                Sort::Author => v.sort_by(|&a, &b| {
                    let (x, y) = (&all[a as usize], &all[b as usize]);
                    (rank(text::letter(&x.akey)), &x.akey, &x.tkey).cmp(&(rank(text::letter(&y.akey)), &y.akey, &y.tkey))
                }),
            }
            self.orders.insert(k.clone(), v);
        }
        &self.orders[&k]
    }

    /// Tìm / lọc / xếp rồi lấy một trang. `keep`: chỉ giữ sách thoả (đã tải, định dạng…). Trả (tổng, trang, mục chữ cái
    /// [{t, i (vị trí đầu), n}] — chỉ khi xếp theo tên / tác giả).
    pub fn query(
        &mut self,
        q: &str,
        source: &str,
        sort: Sort,
        offset: usize,
        limit: usize,
        keep: &dyn Fn(&Book) -> bool,
    ) -> (usize, Vec<Book>, Vec<Value>) {
        let words = text::query_words(q);
        let order = self.order(source, sort).clone();
        let all = &self.all;
        let hits: Vec<u32> =
            order.into_iter().filter(|&i| text::matches(&all[i as usize].hay, &words) && keep(&all[i as usize].book)).collect();
        let page: Vec<Book> = hits.iter().skip(offset).take(limit).map(|&i| all[i as usize].book.clone()).collect();
        let mut sections = Vec::new();
        if sort != Sort::Date {
            let mut cur: Option<(char, usize, usize)> = None;
            for (pos, &i) in hits.iter().enumerate() {
                let e = &all[i as usize];
                let l = text::letter(if sort == Sort::Title { &e.tkey } else { &e.akey });
                match &mut cur {
                    Some((c, _, n)) if *c == l => *n += 1,
                    _ => {
                        if let Some((c, start, n)) = cur.take() {
                            sections.push(json!({"t": c.to_string(), "i": start, "n": n}));
                        }
                        cur = Some((l, pos, 1));
                    }
                }
            }
            if let Some((c, start, n)) = cur {
                sections.push(json!({"t": c.to_string(), "i": start, "n": n}));
            }
        }
        (hits.len(), page, sections)
    }

    /// Khoá của mọi sách thoả điều kiện.
    pub fn keys(&mut self, q: &str, source: &str, sort: Sort, keep: &dyn Fn(&Book) -> bool) -> Vec<String> {
        let words = text::query_words(q);
        let order = self.order(source, sort).clone();
        let all = &self.all;
        order
            .into_iter()
            .map(|i| &all[i as usize])
            .filter(|e| text::matches(&e.hay, &words) && keep(&e.book))
            .map(|e| e.book.key())
            .collect()
    }

    /// Số sách theo chủ đề của một nguồn (0 — không thuộc chủ đề nào — tính vào General, id 1).
    pub fn topic_counts(&self, peer: &str) -> HashMap<i32, usize> {
        let mut m = HashMap::new();
        if let Some(i) = self.idx.get(peer) {
            for s in &i.books {
                *m.entry(if s.topic == 0 { 1 } else { s.topic }).or_insert(0) += 1;
            }
        }
        m
    }

    /// Tìm sách theo khoá (ở mọi nguồn, kể cả nguồn đang tắt).
    pub fn book(&self, key: &str) -> Option<&Book> {
        let (peer, id) = split_key(key)?;
        self.idx.get(&peer)?.books.iter().find(|s| s.id == id)
    }

    /// Tổng số sách (đã lọc trùng) của các nguồn đang bật.
    pub fn total(&mut self) -> usize {
        self.rebuild();
        self.all.iter().filter(|e| e.shown).count()
    }
}

/// Ghi tệp tạm rồi đổi tên (không để tệp dở khi tắt app giữa chừng).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("json.part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(peer: &str, id: i32, title: &str, author: &str, doc: i64) -> Book {
        Book {
            peer: peer.into(),
            id,
            title: title.into(),
            author: author.into(),
            size: 300 << 10,
            mime: "application/epub+zip".into(),
            name: format!("{title} - {author}.epub"),
            ext: "epub".into(),
            date: 1_700_000_000 + id as i64,
            doc,
            topic: if id % 2 == 0 { 7 } else { 0 },
            ..Default::default()
        }
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gb-lib-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn lib_with_two_sources(d: &Path) -> Library {
        let mut lib = Library::open(d);
        for p in ["1", "-1001"] {
            lib.upsert_source(Source { peer: p.into(), name: format!("N{p}"), kind: "channel".into(), enabled: true, ..Default::default() })
                .unwrap();
        }
        let mut a = SourceIndex { peer: "1".into(), ..Default::default() };
        a.merge(vec![book("1", 1, "Ai Cập Huyền Bí", "Paul Brunton", 11), book("1", 2, "Đời tôi", "Bill Clinton", 12)]);
        let mut b = SourceIndex { peer: "-1001".into(), ..Default::default() };
        // sách 5 trùng tệp với sách 1 của nguồn kia (chuyển tiếp)
        b.merge(vec![book("-1001", 5, "Ai Cập Huyền Bí", "Paul Brunton", 11), book("-1001", 6, "Bê Trọc", "Phạm Việt Long", 13),
                     book("-1001", 7, "28 Ngày Việt Nam", "Dũng Vũ", 14)]);
        lib.put_index(a).unwrap();
        lib.put_index(b).unwrap();
        lib
    }

    #[test]
    fn index_merge_keeps_book_meta() {
        let mut i = SourceIndex { peer: "1".into(), ..Default::default() };
        assert_eq!(i.merge(vec![book("1", 12, "b", "", 2), book("1", 10, "a", "", 1)]), 2);
        i.books[1].bt = Some("Tên thật".into());
        assert_eq!(i.merge(vec![book("1", 8, "c", "", 3), book("1", 10, "a2", "", 1)]), 1);
        assert_eq!(i.books.iter().map(|s| s.id).collect::<Vec<_>>(), vec![12, 10, 8]);
        assert_eq!((i.books[1].title.as_str(), i.books[1].shown_title()), ("a2", "Tên thật"));
    }

    #[test]
    fn query_dedupe_sort_sections_search() {
        let d = tmpdir("q");
        let mut lib = lib_with_two_sources(&d);
        assert_eq!(lib.total(), 4);
        let all = |_: &Book| true;
        let (n, page, _) = lib.query("", "", Sort::Date, 0, 10, &all);
        assert_eq!(n, 4);
        assert_eq!(page[0].id, 7);
        assert!(page.iter().any(|s| s.key() == "-1001_5") && !page.iter().any(|s| s.key() == "1_1"));
        let (_, page, secs) = lib.query("", "", Sort::Title, 0, 10, &all);
        let titles: Vec<&str> = page.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Ai Cập Huyền Bí", "Bê Trọc", "Đời tôi", "28 Ngày Việt Nam"]);
        assert_eq!(secs[0], json!({"t": "A", "i": 0, "n": 1}));
        let (n, page, _) = lib.query("brunton", "", Sort::Date, 0, 10, &all);
        assert_eq!((n, page[0].title.as_str()), (1, "Ai Cập Huyền Bí"));
        let (_, page, _) = lib.query("", "", Sort::Author, 0, 10, &all);
        assert_eq!(page[0].author, "Bill Clinton");
        lib.update_source("-1001", |s| s.enabled = false).unwrap();
        assert_eq!(lib.total(), 2);
        let mut again = Library::open(&d);
        assert_eq!(again.total(), 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn book_meta_applies_to_all_copies() {
        let d = tmpdir("meta");
        let mut lib = lib_with_two_sources(&d);
        assert!(lib.set_book_meta("1_1", Some("Ai Cập Huyền Bí (A Search in Secret Egypt)"), Some("Paul Brunton")));
        assert_eq!(lib.book("-1001_5").unwrap().shown_title(), "Ai Cập Huyền Bí (A Search in Secret Egypt)");
        assert!(!lib.set_book_meta("1_1", Some("Ai Cập Huyền Bí (A Search in Secret Egypt)"), Some("Paul Brunton")));
        let all = |_: &Book| true;
        let (n, _, _) = lib.query("secret egypt", "", Sort::Date, 0, 10, &all);
        assert_eq!(n, 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn remove_books_by_peer_or_any() {
        let d = tmpdir("rm");
        let mut lib = lib_with_two_sources(&d);
        assert_eq!(lib.remove_books(Some("-1001"), &[6]), 1);
        assert_eq!(lib.remove_books(None, &[2, 7]), 1);
        assert_eq!(lib.total(), 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn keys_split() {
        assert_eq!(split_key("-1001_5"), Some(("-1001".into(), 5)));
        assert_eq!(split_key("x_5"), None);
    }
}
