//! MOBI6 (Mobipocket / PRC / AZW cũ): HTML kiểu Mobipocket → các chương HTML hoàn chỉnh (`text/html`).
//!
//! Hai lượt trên chữ gốc (byte, mã gốc — `filepos` là vị trí byte trong chữ gốc):
//! 1. Quét: thu các đích `filepos=` (liên kết, guide), vị trí ngắt chương (`<mbp:pagebreak/>`; phần quá ~300 KB cắt thêm ở
//!    đầu một khối), vùng `<head>`, CSS trong head.
//! 2. Ghi: chép từng mảnh sang UTF-8; neo `<a id="fp…"></a>` của mỗi đích đặt ngay trước nội dung nhìn thấy đầu tiên từ
//!    vị trí đó (không bao giờ giữa thẻ / thực thể / ký tự; đích ngay trước ngắt chương rơi sang chương sau);
//!    `<a filepos>` → `href="partNNNN.html#fp…"` (vá sau khi biết neo ở phần nào); `<img recindex>` →
//!    `src="../images/NNNNN.jpg"`; bỏ thẻ riêng của Mobipocket / Office; `width` / `height` của đoạn → CSS (thụt đầu dòng,
//!    cách đoạn); thẻ định dạng còn mở lúc ngắt được mở lại ở đầu phần sau.
//!
//! Mục lục: chỉ mục NCX (sách KindleGen) → trang «toc» của guide → tiêu đề h1–h3 → mỗi phần một mục.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use crate::header::{self, Header, NULL};
use crate::html::{
    attr, attrs, attrs_into, find, find_ci, is_void, is_ws, Attr, Kind, LName, Lexer, Tok,
};
use crate::meta;
use crate::pdb::Pdb;
use crate::resource::image_type;
use crate::text::{clean_label, decode_str, escape, looks_vietnamese, push_decoded, Encoding};
use crate::{decomp, index, Format, Item, MobiBook, MobiError, TocEntry};

/// Phần lớn hơn ngưỡng này được cắt thêm ở đầu một khối (lõi còn cắt tiếp chương > 150 KB — đây chỉ là lưới an toàn).
const SOFT_SPLIT: usize = 300 * 1024;
/// Quá ngưỡng này cắt ở thẻ mở bất kỳ ngoài bảng; quá gấp đôi thì cắt cả trong bảng / giữa chữ dài.
const HARD_SPLIT: usize = 1024 * 1024;
/// Khoảng cách chữ giữa hai liên kết chỉ cần đếm tới đây (ngưỡng trang mục lục ≤ 2000).
const GAP_MAX: usize = 2000;

/// Số thập phân (giá trị `filepos`, `recindex`).
fn parse_num(v: &[u8]) -> Option<u32> {
    let t = v.trim_ascii();
    if t.is_empty() || t.len() > 10 || !t.iter().all(u8::is_ascii_digit) {
        return None;
    }
    t.iter().try_fold(0u32, |a, &d| {
        a.checked_mul(10)?.checked_add((d - b'0') as u32)
    })
}

/// Chỉ số bản ghi tuyệt đối từ trường tương đối `v` (bỏ NULL, 0 = bản ghi đầu, ngoài tệp).
pub(crate) fn rec_index(v: u32, base: usize, len: usize) -> Option<usize> {
    if v == NULL || v == 0 {
        return None;
    }
    base.checked_add(v as usize).filter(|&i| i < len)
}

/// Chữ có ký tự nhìn thấy (không chỉ khoảng trắng / `&nbsp;` / NBSP).
fn visible(t: &[u8]) -> bool {
    let mut i = 0;
    'outer: while i < t.len() {
        let b = t[i];
        if is_ws(b) || b == 0 || b == 0xA0 {
            i += 1;
            continue;
        }
        if b == 0xC2 && t.get(i + 1) == Some(&0xA0) {
            i += 2;
            continue;
        }
        if b == b'&' {
            for e in [&b"&nbsp;"[..], b"&#160;", b"&#xa0;", b"&#xA0;"] {
                if t[i..].starts_with(e) {
                    i += e.len();
                    continue 'outer;
                }
            }
        }
        return true;
    }
    false
}

fn is_block(n: &[u8]) -> bool {
    matches!(
        n,
        b"p" | b"div"
            | b"h1"
            | b"h2"
            | b"h3"
            | b"h4"
            | b"h5"
            | b"h6"
            | b"table"
            | b"ul"
            | b"ol"
            | b"dl"
            | b"blockquote"
            | b"pre"
            | b"hr"
            | b"center"
            | b"section"
            | b"article"
    )
}

/// Thẻ mở khối đóng ngầm một `<p>` đang mở (luật HTML, rút gọn).
fn closes_p(n: &[u8]) -> bool {
    is_block(n)
        || matches!(
            n,
            b"form"
                | b"address"
                | b"fieldset"
                | b"menu"
                | b"nav"
                | b"header"
                | b"footer"
                | b"figure"
        )
}

/// Thẻ định dạng được mở lại ở đầu phần mới khi ngắt giữa chừng.
fn reopenable(n: &[u8]) -> bool {
    matches!(
        n,
        b"div"
            | b"blockquote"
            | b"center"
            | b"font"
            | b"span"
            | b"b"
            | b"i"
            | b"u"
            | b"em"
            | b"strong"
            | b"small"
            | b"big"
            | b"sub"
            | b"sup"
            | b"tt"
            | b"s"
            | b"strike"
            | b"code"
            | b"pre"
            | b"ul"
            | b"ol"
            | b"dl"
            | b"section"
    )
}

fn heading_level(n: &[u8]) -> u8 {
    match n {
        [b'h', d @ b'1'..=b'6'] => d - b'0',
        _ => 0,
    }
}

/// `id` an toàn để đặt trong href / mục lục.
pub(crate) fn safe_id(v: &[u8]) -> bool {
    !v.is_empty()
        && v.len() <= 200
        && v.iter()
            .all(|&c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b':'))
}

fn is_pagebreak(s: &[u8], t: &Tok, n: &[u8]) -> bool {
    n == b"mbp:pagebreak"
        || (n == b"mbp:"
            && attrs(s, t)
                .first()
                .is_some_and(|a| a.name(s).eq_ignore_ascii_case(b"pagebreak")))
}

/// Vị trí thẻ đóng `</name` đầu tiên sau `from`.
fn close_pos(s: &[u8], from: usize, name: &[u8]) -> Option<usize> {
    let mut i = from;
    while let Some(p) = find(s, i, b"</") {
        let e = p + 2 + name.len();
        if e <= s.len() && s[p + 2..e].eq_ignore_ascii_case(name) {
            return Some(p);
        }
        i = p + 2;
    }
    None
}

/// Vị trí thẻ mở `<body` đầu tiên.
fn find_body(s: &[u8]) -> Option<usize> {
    let mut i = 0;
    while let Some(p) = find_ci(s, i, b"<body") {
        if s.get(p + 5)
            .is_none_or(|&c| is_ws(c) || c == b'>' || c == b'/')
        {
            return Some(p);
        }
        i = p + 5;
    }
    None
}

struct GuideRef {
    kind: String,
    pos: u32,
}

/// Kết quả lượt quét.
#[derive(Default)]
struct Scan {
    fileposes: Vec<u32>,
    guide: Vec<GuideRef>,
    /// Vị trí ngắt chương (tăng dần).
    splits: Vec<usize>,
    /// Vùng đầu (`<head>`…) là `0..head_end` (0 = không có).
    head_end: usize,
    /// Khoảng `<style>…</style>` trong vùng đầu.
    styles: Vec<(usize, usize)>,
}

fn scan(s: &[u8]) -> Scan {
    let mut sc = Scan {
        head_end: match find_body(s) {
            Some(b) if find_ci(&s[..b], 0, b"<head").is_some() => b,
            _ => 0,
        },
        ..Scan::default()
    };
    let mut last = 0usize;
    let mut content = false;
    let mut table = 0u32;
    let mut in_guide = false;
    let mut style_start = None;
    for t in Lexer::new(s) {
        let in_head = t.start < sc.head_end;
        match t.kind {
            Kind::Start => {
                let ln = t.lname(s);
                let n = ln.as_bytes();
                if let Some(p) = attr(s, &t, b"filepos").and_then(parse_num) {
                    sc.fileposes.push(p);
                }
                if n == b"guide" {
                    in_guide = !t.self_closing;
                    continue;
                }
                if n == b"reference" {
                    if let Some(pos) = attr(s, &t, b"filepos").and_then(parse_num) {
                        let kind = attr(s, &t, b"type")
                            .map(|v| String::from_utf8_lossy(v).to_ascii_lowercase());
                        sc.guide.push(GuideRef {
                            kind: kind.unwrap_or_default(),
                            pos,
                        });
                    }
                    continue;
                }
                if in_head {
                    if n == b"style" && !t.self_closing {
                        style_start = Some(t.start);
                    }
                    continue;
                }
                if is_pagebreak(s, &t, n) {
                    if content {
                        sc.splits.push(t.start);
                        last = t.start;
                        content = false;
                    }
                    continue;
                }
                let since = t.start - last;
                if content
                    && !in_guide
                    && ((since > SOFT_SPLIT && table == 0 && is_block(n))
                        || (since > HARD_SPLIT && table == 0)
                        || since > 2 * HARD_SPLIT)
                {
                    sc.splits.push(t.start);
                    last = t.start;
                    content = false;
                }
                if n == b"table" && !t.self_closing {
                    table += 1;
                }
                if matches!(n, b"img" | b"hr" | b"svg" | b"object" | b"embed" | b"video")
                    && !in_guide
                {
                    content = true;
                }
            }
            Kind::End => match t.lname(s).as_bytes() {
                b"guide" => in_guide = false,
                b"table" => table = table.saturating_sub(1),
                b"style" => {
                    if let Some(st) = style_start.take() {
                        if in_head {
                            sc.styles.push((st, t.end));
                        }
                    }
                }
                _ => {}
            },
            Kind::Text => {
                if in_head || in_guide || !visible(&s[t.start..t.end]) {
                    continue;
                }
                content = true;
                // Chữ rất dài không có thẻ: cắt giữa chữ, sau một khoảng trắng.
                while t.end - last > 2 * HARD_SPLIT {
                    let from = (last + SOFT_SPLIT).max(t.start + 1);
                    let Some(q) = s[from.min(t.end)..t.end].iter().position(|&c| is_ws(c)) else {
                        break;
                    };
                    let at = from + q + 1;
                    if at >= t.end {
                        break;
                    }
                    sc.splits.push(at);
                    last = at;
                }
            }
            _ => {}
        }
    }
    // Sau lần ngắt cuối không còn nội dung → bỏ lần ngắt đó.
    if !content {
        sc.splits.pop();
    }
    sc
}

/// Đẩy `p` (vị trí đích trong chữ) ra khỏi thực thể `&…;` / giữa ký tự UTF-8, không quá `end`.
fn adjust(s: &[u8], mut p: usize, end: usize, enc: Encoding) -> usize {
    if p >= end {
        return end;
    }
    if let Some(amp) = (p.saturating_sub(12)..p).rev().find(|&i| s[i] == b'&') {
        if !s[amp..p]
            .iter()
            .any(|&c| c == b';' || is_ws(c) || c == b'<')
        {
            if let Some(k) = s[p..end.min(p + 12)].iter().position(|&c| c == b';') {
                p += k + 1;
            }
        }
    }
    if enc == Encoding::Utf8 {
        while p < end && s[p] & 0xC0 == 0x80 {
            p += 1;
        }
    }
    p.min(end)
}

struct Heading {
    level: u8,
    id: String,
    text: Vec<u8>,
}

struct Link {
    target: u32,
    text: Vec<u8>,
    /// Vị trí thẻ `<a>` trong bộ đệm của phần.
    offset: usize,
    /// Lượng chữ nhìn thấy ngoài liên kết kể từ liên kết trước.
    gap: usize,
}

struct Part {
    buf: Vec<u8>,
    /// (vị trí trong `buf`, đích filepos) — chỗ điền href.
    fixups: Vec<(usize, u32)>,
    headings: Vec<Heading>,
    links: Vec<Link>,
    /// Dòng chữ đầu tiên của phần (nhãn mục lục dự phòng); `snippet_done` khi hết dòng đó.
    snippet: Vec<u8>,
    snippet_done: bool,
    visible: bool,
    anchors: usize,
}

impl Part {
    fn new(prefix: &[u8]) -> Part {
        let mut buf = Vec::with_capacity(64 * 1024);
        buf.extend_from_slice(prefix);
        Part {
            buf,
            fixups: Vec::new(),
            headings: Vec::new(),
            links: Vec::new(),
            snippet: Vec::new(),
            snippet_done: false,
            visible: false,
            anchors: 0,
        }
    }

    /// Ranh giới khối / xuống dòng: dòng chữ đầu (nếu đã có) kết thúc.
    fn line_break(&mut self) {
        if !self.snippet.is_empty() {
            self.snippet_done = true;
        }
    }
}

/// Một phần tử đang mở: tên + nơi thẻ mở đã ghi (phần, khoảng byte) để mở lại sau ngắt.
struct Open {
    name: LName,
    part: usize,
    range: (usize, usize),
}

/// Ảnh theo `recindex` (1 = bản ghi tài nguyên đầu).
struct Images<'a, 'p> {
    pdb: &'p Pdb<'a>,
    first: Option<usize>,
}

impl Images<'_, '_> {
    fn get(&self, idx: u32) -> Option<(usize, &'static str, &'static str)> {
        let rec = self.first?.checked_add((idx as usize).checked_sub(1)?)?;
        if rec >= self.pdb.len() {
            return None;
        }
        let (ext, mime) = image_type(self.pdb.record(rec))?;
        Some((rec, ext, mime))
    }
}

struct Emitter<'s> {
    s: &'s [u8],
    enc: Encoding,
    prefix: Vec<u8>,
    head_end: usize,
    splits: &'s [usize],
    targets: &'s [u32],
    parts: Vec<Part>,
    cur: Part,
    stack: Vec<Open>,
    ti: usize,
    si: usize,
    pending: Vec<u32>,
    /// Đích → (phần, vị trí neo trong bộ đệm).
    anchors: HashMap<u32, (usize, usize)>,
    open_a: usize,
    link: Option<Link>,
    heading: Option<Heading>,
    gap: usize,
    hcount: usize,
    skip: Option<LName>,
    in_style: bool,
    /// recindex → (bản ghi, đuôi, mime) của ảnh đã dùng.
    used: BTreeMap<u32, (usize, &'static str, &'static str)>,
    /// Bộ nhớ dùng lại khi viết lại thẻ.
    attrs: Vec<Attr>,
    style: String,
    /// Kết quả tìm thẻ đóng gần nhất theo tên (xem `closes_later`).
    close_cache: Vec<(LName, Option<usize>)>,
}

impl<'s> Emitter<'s> {
    fn run(&mut self, images: &Images) {
        let s = self.s;
        for t in Lexer::new(s) {
            if t.kind == Kind::Text {
                self.text(t.start, t.end);
                continue;
            }
            self.advance(t.start);
            match t.kind {
                Kind::Start => self.start_tag(&t, images),
                Kind::End => self.end_tag(&t),
                _ => {}
            }
        }
        self.advance(s.len());
        while self.ti < self.targets.len() {
            self.pending.push(self.targets[self.ti]);
            self.ti += 1;
        }
        self.flush_pending();
        self.end_captures();
        let mut last = std::mem::replace(&mut self.cur, Part::new(&[]));
        if last.visible || last.anchors > 0 || self.parts.is_empty() {
            last.buf.extend_from_slice(b"</body></html>");
            self.parts.push(last);
        }
    }

    /// Nhận các đích ≤ `pos` vào hàng chờ, thực hiện các lần ngắt ≤ `pos`.
    fn advance(&mut self, pos: usize) {
        while self.ti < self.targets.len() && self.targets[self.ti] as usize <= pos {
            self.pending.push(self.targets[self.ti]);
            self.ti += 1;
        }
        while self.si < self.splits.len() && self.splits[self.si] <= pos {
            self.si += 1;
            self.split();
        }
    }

    /// Có thẻ đóng `</name` sau `from`. Nhớ kết quả lần tìm trước của từng tên: tệp độc có vô số `<script>` / `<title>`
    /// không đóng không bắt tìm lại cả sách mỗi lần.
    fn closes_later(&mut self, from: usize, name: LName) -> bool {
        let cached = self
            .close_cache
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, r)| r);
        match cached {
            Some(Some(p)) if p >= from => return true,
            Some(None) => return false,
            _ => {}
        }
        let r = close_pos(self.s, from, name.as_bytes());
        self.close_cache.retain(|(n, _)| *n != name);
        self.close_cache.push((name, r));
        r.is_some()
    }

    fn skipping(&self, pos: usize) -> bool {
        self.skip.is_some() || pos < self.head_end
    }

    fn text(&mut self, a: usize, b: usize) {
        let skipped = self.skipping(a);
        let mut cur = a;
        loop {
            let na = self
                .targets
                .get(self.ti)
                .map(|&p| p as usize)
                .filter(|&p| p < b)
                .map(|p| adjust(self.s, p.max(cur), b, self.enc));
            let ns = self.splits.get(self.si).copied().filter(|&p| p < b);
            let ev = match (na, ns) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            };
            let Some(e) = ev else { break };
            let e = e.max(cur);
            if !skipped {
                self.emit_text(cur, e);
            }
            cur = e;
            self.advance(e);
        }
        if !skipped {
            self.emit_text(cur, b);
        }
    }

    fn emit_text(&mut self, a: usize, b: usize) {
        if a >= b {
            return;
        }
        let src = &self.s[a..b];
        if self.in_style {
            push_decoded(&mut self.cur.buf, src, self.enc);
            return;
        }
        if !self.pending.is_empty() {
            if let Some(k) = src.iter().position(|&c| !is_ws(c)) {
                self.push_text(&src[..k]);
                self.flush_pending();
                self.push_text(&src[k..]);
                return;
            }
        }
        self.push_text(src);
    }

    fn push_text(&mut self, src: &[u8]) {
        if src.is_empty() {
            return;
        }
        let from = self.cur.buf.len();
        push_decoded(&mut self.cur.buf, src, self.enc);
        let new = &self.cur.buf[from..];
        if !visible(new) {
            return;
        }
        self.cur.visible = true;
        match &mut self.link {
            Some(l) => cap_extend(&mut l.text, new, 600),
            // Khoảng cách giữa hai liên kết chỉ cần so với ngưỡng nhỏ — đếm tới đó là đủ.
            None if self.gap <= GAP_MAX => {
                self.gap += new.iter().take(GAP_MAX + 1).filter(|&&c| !is_ws(c)).count()
            }
            None => {}
        }
        if let Some(h) = &mut self.heading {
            cap_extend(&mut h.text, new, 600);
        }
        if !self.cur.snippet_done && self.cur.snippet.len() < 200 {
            cap_extend(&mut self.cur.snippet, new, 200);
            self.cur.snippet.push(b' ');
        }
    }

    fn flush_pending(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let part = self.parts.len();
        for p in std::mem::take(&mut self.pending) {
            let off = self.cur.buf.len();
            let tag = if self.open_a > 0 {
                format!("<span id=\"fp{p:010}\"></span>")
            } else {
                format!("<a id=\"fp{p:010}\"></a>")
            };
            self.cur.buf.extend_from_slice(tag.as_bytes());
            self.anchors.insert(p, (part, off));
            self.cur.anchors += 1;
        }
    }

    /// Kết thúc thu chữ liên kết / tiêu đề đang dở (lúc ngắt phần / hết sách).
    fn end_captures(&mut self) {
        if let Some(l) = self.link.take() {
            self.cur.links.push(l);
        }
        if let Some(h) = self.heading.take() {
            self.cur.headings.push(h);
        }
    }

    fn split(&mut self) {
        if !self.cur.visible {
            // Phần chưa có gì nhìn thấy: gộp với phần sau.
            return;
        }
        self.end_captures();
        let mut next = Part::new(&self.prefix);
        let new_index = self.parts.len() + 1;
        let mut reopened = 0;
        for o in self.stack.iter_mut() {
            if !reopenable(o.name.as_bytes()) || reopened >= 24 {
                continue;
            }
            let src = if o.part == self.parts.len() {
                &self.cur.buf
            } else {
                &self.parts[o.part].buf
            };
            let Some(tag) = src.get(o.range.0..o.range.1) else {
                continue;
            };
            let start = next.buf.len();
            next.buf.extend_from_slice(&strip_id(tag));
            o.part = new_index;
            o.range = (start, next.buf.len());
            reopened += 1;
        }
        let mut done = std::mem::replace(&mut self.cur, next);
        done.buf.extend_from_slice(b"</body></html>");
        self.parts.push(done);
    }

    fn push_open(&mut self, name: LName, start: usize) {
        if self.stack.len() < 256 {
            self.stack.push(Open {
                name,
                part: self.parts.len(),
                range: (start, self.cur.buf.len()),
            });
        }
    }

    fn pop_open(&mut self, n: &[u8]) {
        if let Some(i) = self.stack.iter().rposition(|o| o.name.as_bytes() == n) {
            self.stack.truncate(i);
        }
    }

    /// Đóng ngầm theo luật HTML (rút gọn): `<li>` đóng `<li>` trước, `<td>` đóng ô trước, khối đóng `<p>`.
    fn close_implied(&mut self, n: &[u8]) {
        // Phần tử `targets` gần nhất còn mở, không vượt qua phần tử `stops`.
        let scope = |stops: &[&[u8]], targets: &[&[u8]]| -> Option<usize> {
            for (i, o) in self.stack.iter().enumerate().rev() {
                let on = o.name.as_bytes();
                if targets.contains(&on) {
                    return Some(i);
                }
                if stops.contains(&on) {
                    return None;
                }
            }
            None
        };
        let cut = match n {
            b"li" => scope(&[b"ul", b"ol"], &[b"li"]),
            b"dt" | b"dd" => scope(&[b"dl"], &[b"dt", b"dd"]),
            b"tr" => scope(&[b"table"], &[b"tr"]),
            b"td" | b"th" => scope(&[b"tr", b"table"], &[b"td", b"th"]),
            _ => None,
        };
        if let Some(i) = cut {
            self.stack.truncate(i);
        }
        if closes_p(n) && self.stack.last().is_some_and(|o| o.name.as_bytes() == b"p") {
            self.stack.pop();
        }
    }

    fn start_tag(&mut self, t: &Tok, images: &Images) {
        let s = self.s;
        if self.skipping(t.start) {
            return;
        }
        let ln = t.lname(s);
        let n = ln.as_bytes();
        match n {
            b"html" | b"body" | b"meta" | b"link" | b"base" | b"basefont" | b"reference"
            | b"guide" => return,
            b"head" | b"title" | b"metadata" | b"dc-metadata" | b"x-metadata" | b"script"
            | b"noscript" => {
                if !t.self_closing && self.closes_later(t.end, ln) {
                    self.skip = Some(ln);
                }
                return;
            }
            _ => {}
        }
        // mbp:pagebreak (đã thành điểm ngắt), mbp:nu / section / frameset / slave-frame / soundplayer, o:p, v:shape…: bỏ thẻ,
        // giữ nội dung.
        if n.contains(&b':') {
            return;
        }
        match n {
            b"a" => return self.start_a(t),
            b"img" => return self.start_img(t, images),
            _ => {}
        }
        if matches!(
            n,
            b"hr" | b"svg" | b"object" | b"embed" | b"video" | b"audio" | b"iframe"
        ) {
            self.flush_pending();
            self.cur.visible = true;
        }
        if n == b"br" || closes_p(n) || n == b"li" || n == b"tr" {
            self.cur.line_break();
        }
        self.close_implied(n);
        let start = self.cur.buf.len();
        let level = heading_level(n);
        if !self.rewrite(t, n, level) {
            self.push_tag(t, n);
        }
        if n == b"style" && !t.self_closing {
            self.in_style = true;
        }
        if !t.self_closing && !is_void(n) {
            self.push_open(ln, start);
        }
    }

    /// Chép thẻ mở nguyên văn; `<x/>` của phần tử không rỗng → `<x></x>` (HTML bỏ qua dấu `/`).
    fn push_tag(&mut self, t: &Tok, n: &[u8]) {
        let tag = &self.s[t.start..t.end];
        if t.self_closing && !is_void(n) {
            if let Some(sl) = tag.iter().rposition(|&c| c == b'/') {
                push_decoded(&mut self.cur.buf, &tag[..sl], self.enc);
                self.cur.buf.push(b'>');
                self.cur.buf.extend_from_slice(b"</");
                self.cur.buf.extend_from_slice(n);
                self.cur.buf.push(b'>');
                return;
            }
        }
        push_decoded(&mut self.cur.buf, tag, self.enc);
    }

    /// Viết lại thẻ khi cần, ghi thẳng vào phần: `width` / `height` → CSS, id cho tiêu đề h1–h3. `false` = không cần (người
    /// gọi chép nguyên thẻ).
    fn rewrite(&mut self, t: &Tok, n: &[u8], level: u8) -> bool {
        let s = self.s;
        let dims_ok = !matches!(
            n,
            b"table"
                | b"td"
                | b"th"
                | b"tr"
                | b"col"
                | b"colgroup"
                | b"hr"
                | b"iframe"
                | b"object"
                | b"embed"
                | b"canvas"
                | b"video"
                | b"input"
                | b"textarea"
                | b"select"
                | b"applet"
                | b"svg"
        );
        let wants_id = (1..=3).contains(&level);
        // Lọc nhanh trên byte của thẻ trước khi tách thuộc tính (đa số thẻ không cần viết lại).
        let tag = &s[t.start..t.end];
        let maybe_dims = dims_ok
            && (find_ci(tag, 0, b"width").is_some() || find_ci(tag, 0, b"height").is_some());
        if !maybe_dims && !wants_id {
            return false;
        }
        let mut list = std::mem::take(&mut self.attrs);
        attrs_into(s, t, &mut list);
        let is_dim = |a: &Attr| {
            a.name(s).eq_ignore_ascii_case(b"width") || a.name(s).eq_ignore_ascii_case(b"height")
        };
        let has_dims = maybe_dims && list.iter().any(is_dim);
        if !has_dims && !wants_id {
            self.attrs = list;
            return false;
        }
        let mut out = std::mem::take(&mut self.cur.buf);
        let mut style = std::mem::take(&mut self.style);
        style.clear();
        out.push(b'<');
        out.extend_from_slice(t.name(s));
        let mut old_style: Option<&[u8]> = None;
        let mut id: Option<&[u8]> = None;
        for a in &list {
            let an = a.name(s);
            if has_dims && is_dim(a) {
                dim_css(&mut style, an.eq_ignore_ascii_case(b"height"), a.value(s));
                continue;
            }
            if an.eq_ignore_ascii_case(b"style") {
                old_style = Some(a.value(s));
                continue;
            }
            if an.eq_ignore_ascii_case(b"id") {
                id = Some(a.value(s));
            }
            out.push(b' ');
            push_decoded(&mut out, &s[a.start..a.end], self.enc);
        }
        if wants_id {
            let hid = match id {
                Some(v) if safe_id(v) => Some(String::from_utf8_lossy(v).into_owned()),
                Some(_) => None,
                None => {
                    self.hcount += 1;
                    let v = format!("gbh{}", self.hcount);
                    out.extend_from_slice(b" id=\"");
                    out.extend_from_slice(v.as_bytes());
                    out.push(b'"');
                    Some(v)
                }
            };
            self.end_heading();
            if let Some(id) = hid {
                self.heading = Some(Heading {
                    level,
                    id,
                    text: Vec::new(),
                });
            }
        }
        if !style.is_empty() || old_style.is_some() {
            out.extend_from_slice(b" style=\"");
            out.extend_from_slice(style.as_bytes());
            if let Some(os) = old_style {
                let mut v = Vec::new();
                push_decoded(&mut v, os, self.enc);
                out.extend_from_slice(escape(&String::from_utf8_lossy(&v)).as_bytes());
            }
            out.push(b'"');
        }
        if t.self_closing && is_void(n) {
            out.extend_from_slice(b"/>");
        } else if t.self_closing {
            out.extend_from_slice(b"></");
            out.extend_from_slice(t.name(s));
            out.push(b'>');
        } else {
            out.push(b'>');
        }
        self.cur.buf = out;
        self.style = style;
        self.attrs = list;
        true
    }

    fn end_heading(&mut self) {
        if let Some(h) = self.heading.take() {
            self.cur.headings.push(h);
        }
    }

    fn start_a(&mut self, t: &Tok) {
        let s = self.s;
        // Neo chờ đặt trước thẻ <a> (không lồng <a> trong <a>).
        self.flush_pending();
        let start = self.cur.buf.len();
        match attr(s, t, b"filepos").and_then(parse_num) {
            Some(target) => {
                let mut out = b"<a".to_vec();
                for a in attrs(s, t) {
                    let an = a.name(s);
                    if an.eq_ignore_ascii_case(b"filepos") || an.eq_ignore_ascii_case(b"href") {
                        continue;
                    }
                    out.push(b' ');
                    push_decoded(&mut out, &s[a.start..a.end], self.enc);
                }
                out.extend_from_slice(b" href=\"");
                let slot = start + out.len();
                out.extend_from_slice(b"\">");
                self.cur.buf.extend_from_slice(&out);
                self.cur.fixups.push((slot, target));
                if t.self_closing {
                    self.cur.buf.extend_from_slice(b"</a>");
                    return;
                }
                if let Some(l) = self.link.take() {
                    self.cur.links.push(l);
                }
                self.link = Some(Link {
                    target,
                    text: Vec::new(),
                    offset: start,
                    gap: self.gap,
                });
                self.gap = 0;
            }
            None => {
                self.push_tag(t, b"a");
                if t.self_closing {
                    return;
                }
            }
        }
        self.open_a += 1;
        self.push_open(LName::new(b"a"), start);
    }

    fn start_img(&mut self, t: &Tok, images: &Images) {
        let s = self.s;
        let found = [&b"hirecindex"[..], b"recindex", b"lorecindex"]
            .iter()
            .filter_map(|k| attr(s, t, k).and_then(parse_num))
            .find_map(|i| images.get(i).map(|g| (i, g)));
        // Ảnh không có / hỏng → bỏ thẻ (thẻ img không src hiện khung vỡ).
        let Some((idx, (rec, ext, mime))) = found else {
            return;
        };
        self.used.insert(idx, (rec, ext, mime));
        self.flush_pending();
        self.cur.visible = true;
        let mut out = b"<img".to_vec();
        for a in attrs(s, t) {
            let an = a.name(s);
            if [&b"recindex"[..], b"hirecindex", b"lorecindex", b"src"]
                .iter()
                .any(|k| an.eq_ignore_ascii_case(k))
            {
                continue;
            }
            out.push(b' ');
            push_decoded(&mut out, &s[a.start..a.end], self.enc);
        }
        out.extend_from_slice(format!(" src=\"../images/{idx:05}.{ext}\"/>").as_bytes());
        self.cur.buf.extend_from_slice(&out);
    }

    fn end_tag(&mut self, t: &Tok) {
        let s = self.s;
        let ln = t.lname(s);
        let n = ln.as_bytes();
        if let Some(sk) = self.skip {
            if sk == ln {
                self.skip = None;
            }
            return;
        }
        if t.start < self.head_end {
            return;
        }
        if n == b"style" {
            self.in_style = false;
        }
        // `</br>`: HTML đọc thành một `<br>` nữa (`<br></br>` của KindleGen → hai dòng trống) — bỏ.
        if matches!(
            n,
            b"html"
                | b"body"
                | b"head"
                | b"title"
                | b"guide"
                | b"reference"
                | b"metadata"
                | b"dc-metadata"
                | b"x-metadata"
                | b"script"
                | b"noscript"
                | b"meta"
                | b"link"
                | b"br"
        ) || n.contains(&b':')
        {
            return;
        }
        if closes_p(n) || matches!(n, b"li" | b"tr" | b"td" | b"th" | b"dd" | b"dt") {
            self.cur.line_break();
        }
        if n == b"a" {
            if let Some(l) = self.link.take() {
                self.cur.links.push(l);
            }
            self.open_a = self.open_a.saturating_sub(1);
        }
        if heading_level(n) > 0 {
            self.end_heading();
        }
        push_decoded(&mut self.cur.buf, &s[t.start..t.end], self.enc);
        self.pop_open(n);
    }
}

/// Nối `src` vào `dst` tới tối đa `cap` byte, không cắt giữa ký tự UTF-8 / thực thể.
fn cap_extend(dst: &mut Vec<u8>, src: &[u8], cap: usize) {
    if dst.len() >= cap {
        return;
    }
    let mut take = src.len().min(cap - dst.len());
    while take < src.len() && take > 0 && src[take] & 0xC0 == 0x80 {
        take -= 1;
    }
    if take < src.len() {
        if let Some(a) = src[..take].iter().rposition(|&c| c == b'&') {
            if !src[a..take].contains(&b';') {
                take = a;
            }
        }
    }
    dst.extend_from_slice(&src[..take]);
}

/// Bỏ thuộc tính `id` khỏi một thẻ mở (thẻ mở lại ở phần sau không được trùng id).
fn strip_id(tag: &[u8]) -> Vec<u8> {
    let Some(t) = Lexer::new(tag).next().filter(|t| t.kind == Kind::Start) else {
        return tag.to_vec();
    };
    let list = attrs(tag, &t);
    let Some(a) = list
        .iter()
        .find(|a| a.name(tag).eq_ignore_ascii_case(b"id"))
    else {
        return tag.to_vec();
    };
    let mut v = tag[..a.start].to_vec();
    while v.last().is_some_and(|&c| is_ws(c)) {
        v.pop();
    }
    v.extend_from_slice(&tag[a.end..]);
    v
}

/// `width` / `height` kiểu Mobipocket của đoạn → CSS (như Calibre), nối vào `out`: height → cách trên, width → thụt đầu
/// dòng (âm: treo). Giá trị lạ → bỏ.
fn dim_css(out: &mut String, height: bool, v: &[u8]) {
    let Ok(v) = std::str::from_utf8(v) else {
        return;
    };
    let v = v.trim();
    if v.is_empty()
        || !v.bytes().any(|b| b.is_ascii_digit())
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'%')
    {
        return;
    }
    let unit = if v.ends_with(|c: char| c.is_ascii_digit()) {
        "px"
    } else {
        ""
    };
    if height {
        let _ = write!(out, "margin-top:{v}{unit};");
    } else if let Some(pos) = v.strip_prefix('-') {
        let _ = write!(out, "text-indent:{v}{unit};margin-left:{pos}{unit};");
    } else {
        let _ = write!(out, "text-indent:{v}{unit};");
    }
}

/// Điền href vào các liên kết filepos.
fn apply_fixups(
    buf: Vec<u8>,
    fixups: &[(usize, u32)],
    anchors: &HashMap<u32, (usize, usize)>,
) -> Vec<u8> {
    if fixups.is_empty() {
        return buf;
    }
    let mut out = Vec::with_capacity(buf.len() + fixups.len() * 32);
    let mut last = 0;
    for &(off, target) in fixups {
        let off = off.min(buf.len());
        out.extend_from_slice(&buf[last..off]);
        match anchors.get(&target) {
            Some(&(part, _)) => {
                out.extend_from_slice(format!("part{part:04}.html#fp{target:010}").as_bytes())
            }
            None => out.push(b'#'),
        }
        last = off;
    }
    out.extend_from_slice(&buf[last..]);
    out
}

struct NcxItem {
    pos: u32,
    label: String,
    depth: u32,
}

/// Chỉ mục NCX của MOBI6 (KindleGen): thẻ 1 = filepos, 3 = nhãn (CNCX), 4 = cấp.
fn read_ncx(pdb: &Pdb, ncx: u32, enc: Encoding) -> Vec<NcxItem> {
    let Some(idx) = rec_index(ncx, 0, pdb.len()) else {
        return Vec::new();
    };
    let Ok(ix) = index::read(pdb, idx) else {
        return Vec::new();
    };
    ix.entries
        .iter()
        .filter_map(|e| {
            let pos = e.val(1, 0)?;
            let label = e
                .val(3, 0)
                .and_then(|o| ix.cncx.get(&o))
                .map(|b| clean_label(&decode_str(b, enc), 200))?;
            Some(NcxItem {
                pos,
                label,
                depth: e.val(4, 0).unwrap_or(0).min(16),
            })
        })
        .collect()
}

fn href_for(anchors: &HashMap<u32, (usize, usize)>, pos: u32) -> Option<String> {
    let &(part, _) = anchors.get(&pos)?;
    Some(format!("text/part{part:04}.html#fp{pos:010}"))
}

fn label_of(b: &[u8]) -> String {
    clean_label(&String::from_utf8_lossy(b), 200)
}

/// Chuỗi liên kết filepos liền nhau (giữa hai liên kết ≤ ~400 byte chữ khác) dài nhất trong một phần, chỉ tính liên kết có
/// chữ (bỏ «[1]», ảnh…).
fn link_cluster(parts: &[Part], anchors: &HashMap<u32, (usize, usize)>) -> Vec<TocEntry> {
    let mut best: Vec<TocEntry> = Vec::new();
    for p in parts {
        let mut run: Vec<TocEntry> = Vec::new();
        for l in &p.links {
            if l.gap > 400 && !run.is_empty() {
                if run.len() > best.len() {
                    best = std::mem::take(&mut run);
                } else {
                    run.clear();
                }
            }
            let title = label_of(&l.text);
            if title.chars().filter(|c| c.is_alphabetic()).count() >= 2 {
                if let Some(href) = href_for(anchors, l.target) {
                    run.push(TocEntry {
                        title,
                        href,
                        depth: 0,
                    });
                }
            }
        }
        if run.len() > best.len() {
            best = run;
        }
    }
    best
}

fn build_toc(
    parts: &[Part],
    anchors: &HashMap<u32, (usize, usize)>,
    ncx: &[NcxItem],
    guide: &[GuideRef],
) -> Vec<TocEntry> {
    // 1. NCX.
    let toc: Vec<TocEntry> = ncx
        .iter()
        .filter(|e| !e.label.is_empty())
        .filter_map(|e| {
            Some(TocEntry {
                title: e.label.clone(),
                href: href_for(anchors, e.pos)?,
                depth: e.depth,
            })
        })
        .collect();
    if !toc.is_empty() {
        return toc;
    }
    // 2. Trang mục lục trong guide: các liên kết filepos liền nhau từ neo «toc».
    if let Some(&(pi, off)) = guide
        .iter()
        .find(|g| g.kind == "toc")
        .and_then(|g| anchors.get(&g.pos))
    {
        let mut toc = Vec::new();
        for (k, l) in parts[pi]
            .links
            .iter()
            .filter(|l| l.offset >= off)
            .enumerate()
        {
            if k > 0 && l.gap > 2000 {
                break;
            }
            let title = label_of(&l.text);
            if let (false, Some(href)) = (title.is_empty(), href_for(anchors, l.target)) {
                toc.push(TocEntry {
                    title,
                    href,
                    depth: 0,
                });
            }
        }
        if toc.len() >= 2 {
            return toc;
        }
    }
    // 3. Tiêu đề h1–h3, hoặc trang mục lục không khai trong guide (chuỗi liên kết filepos dày nhất — sách làm bằng
    //    Mobipocket Creator hay có trang «MỤC LỤC» như vậy). Có cả hai: lấy tiêu đề (có cấp) trừ khi trang mục lục nhiều
    //    mục hơn hẳn.
    let heads: Vec<(usize, &Heading, String)> = parts
        .iter()
        .enumerate()
        .flat_map(|(i, p)| p.headings.iter().map(move |h| (i, h, label_of(&h.text))))
        .filter(|(_, _, t)| !t.is_empty())
        .collect();
    let cluster = link_cluster(parts, anchors);
    // Vài tiêu đề lẻ cho sách nhiều phần (vd 2 tiêu đề / 18 phần) → mục lục theo phần tốt hơn.
    let use_heads =
        heads.len() >= 2 && heads.len() * 2 >= cluster.len() && heads.len() * 3 >= parts.len();
    if !use_heads && cluster.len() >= 3 {
        return cluster;
    }
    if use_heads || (heads.len() == 1 && parts.len() == 1) {
        let min = heads.iter().map(|(_, h, _)| h.level).min().unwrap_or(1);
        return heads
            .into_iter()
            .map(|(i, h, title)| TocEntry {
                title,
                href: format!("text/part{i:04}.html#{}", h.id),
                depth: (h.level - min) as u32,
            })
            .collect();
    }
    // 4. Mỗi phần một mục.
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let title = p
                .headings
                .iter()
                .map(|h| label_of(&h.text))
                .find(|t| !t.is_empty())
                .or_else(|| {
                    Some(clean_label(&String::from_utf8_lossy(&p.snippet), 60))
                        .filter(|t| !t.is_empty())
                })
                .unwrap_or_else(|| format!("Phần {}", i + 1));
            TocEntry {
                title,
                href: format!("text/part{i:04}.html"),
                depth: 0,
            }
        })
        .collect()
}

fn doc_prefix(title: &str, s: &[u8], styles: &[(usize, usize)], enc: Encoding) -> Vec<u8> {
    let mut p = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"/><title>{}</title>",
        escape(title)
    )
    .into_bytes();
    for &(a, b) in styles {
        push_decoded(&mut p, &s[a..b.min(s.len())], enc);
    }
    p.extend_from_slice(b"</head><body>");
    p
}

/// Ảnh bìa: EXTH 201 (dự phòng 202 — ảnh nhỏ) → (recindex, bản ghi, đuôi, mime).
fn cover_image(
    images: &Images,
    m: &meta::Meta,
) -> Option<(u32, (usize, &'static str, &'static str))> {
    [m.cover, m.thumb].into_iter().flatten().find_map(|off| {
        let idx = off.checked_add(1)?;
        images.get(idx).map(|g| (idx, g))
    })
}

pub(crate) fn convert(pdb: &Pdb, h: &Header) -> Result<MobiBook, MobiError> {
    let m = h
        .mobi
        .as_ref()
        .ok_or_else(|| MobiError::Invalid("thiếu đầu MOBI".into()))?;
    let text = decomp::extract_text(pdb, 0, h)?;
    if text.starts_with(b"%MOP") {
        return Err(MobiError::Unsupported("Print Replica (azw4)".into()));
    }
    let enc = Encoding::from_codepage(m.encoding);
    let mut meta = meta::read(pdb, h, None);
    // Không ghi ngôn ngữ, hoặc ghi «en» (mặc định của Mobipocket Creator / KindleGen) mà chữ rõ là tiếng Việt → «vi».
    if meta
        .language
        .as_deref()
        .is_none_or(|l| m.exth(524).is_none() || l.starts_with("en"))
        && looks_vietnamese(&text)
    {
        meta.language = Some("vi".into());
    }
    let images = Images {
        pdb,
        first: rec_index(m.first_resource, 0, pdb.len()),
    };
    let ncx = read_ncx(pdb, m.ncx, enc);
    let sc = scan(&text);
    let mut targets: Vec<u32> = sc.fileposes.clone();
    targets.extend(ncx.iter().map(|e| e.pos));
    targets.extend(sc.guide.iter().map(|g| g.pos));
    targets.sort_unstable();
    targets.dedup();
    let prefix = doc_prefix(&meta.title, &text, &sc.styles, enc);
    let mut em = Emitter {
        s: &text,
        enc,
        cur: Part::new(&prefix),
        prefix,
        head_end: sc.head_end,
        splits: &sc.splits,
        targets: &targets,
        parts: Vec::new(),
        stack: Vec::new(),
        ti: 0,
        si: 0,
        pending: Vec::new(),
        anchors: HashMap::new(),
        open_a: 0,
        link: None,
        heading: None,
        gap: 0,
        hcount: 0,
        skip: None,
        in_style: false,
        used: BTreeMap::new(),
        attrs: Vec::new(),
        style: String::new(),
        close_cache: Vec::new(),
    };
    em.run(&images);
    let Emitter {
        parts,
        anchors,
        mut used,
        ..
    } = em;
    drop(text);
    let toc = build_toc(&parts, &anchors, &ncx, &sc.guide);
    let cover = cover_image(&images, &meta).map(|(idx, g)| {
        used.insert(idx, g);
        format!("images/{idx:05}.{}", g.1)
    });
    let spine = parts
        .into_iter()
        .enumerate()
        .map(|(i, p)| Item {
            path: format!("text/part{i:04}.html"),
            mime: "text/html".into(),
            data: apply_fixups(p.buf, &p.fixups, &anchors),
        })
        .collect();
    let resources = used
        .into_iter()
        .map(|(idx, (rec, ext, mime))| Item {
            path: format!("images/{idx:05}.{ext}"),
            mime: mime.into(),
            data: pdb.record(rec).to_vec(),
        })
        .collect();
    Ok(MobiBook {
        format: Format::Mobi6,
        title: meta.title,
        authors: meta.authors,
        language: meta.language,
        publisher: meta.publisher,
        description: meta.description,
        spine,
        resources,
        toc,
        cover,
    })
}

/// PalmDOC thuần («TEXtREAd»): chữ trơn → đoạn `<p>`, phần ≈ 100 KB.
pub(crate) fn convert_palmdoc(pdb: &Pdb) -> Result<MobiBook, MobiError> {
    let h = header::parse(pdb.record(0), false)?;
    if h.encryption != 0 {
        return Err(MobiError::Drm);
    }
    let text = decomp::extract_text(pdb, 0, &h)?;
    let title = clean_label(
        &decode_str(pdb.name, Encoding::Cp1252).replace('_', " "),
        200,
    );
    let s = decode_str(&text, Encoding::Utf8);
    drop(text);
    let prefix = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"/><title>{}</title></head><body>",
        escape(&title)
    );
    let mut bodies: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in s.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        cur.push_str("<p>");
        cur.push_str(&escape(l));
        cur.push_str("</p>\n");
        if cur.len() > 100 * 1024 {
            bodies.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() || bodies.is_empty() {
        bodies.push(cur);
    }
    let n = bodies.len();
    let spine = bodies
        .into_iter()
        .enumerate()
        .map(|(i, b)| Item {
            path: format!("text/part{i:04}.html"),
            mime: "text/html".into(),
            data: format!("{prefix}{b}</body></html>").into_bytes(),
        })
        .collect();
    let toc = (0..n)
        .map(|i| TocEntry {
            title: format!("Phần {}", i + 1),
            href: format!("text/part{i:04}.html"),
            depth: 0,
        })
        .collect();
    Ok(MobiBook {
        format: Format::Mobi6,
        title,
        authors: Vec::new(),
        language: None,
        publisher: None,
        description: None,
        spine,
        resources: Vec::new(),
        toc,
        cover: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_va_do_dai() {
        assert_eq!(parse_num(b"0000012345"), Some(12345));
        assert_eq!(parse_num(b" 7 "), Some(7));
        assert_eq!(parse_num(b"99999999999"), None);
        assert_eq!(parse_num(b"4294967296"), None);
        assert_eq!(parse_num(b"12a"), None);
        let css = |h: bool, v: &[u8]| {
            let mut s = String::new();
            dim_css(&mut s, h, v);
            s
        };
        assert_eq!(css(false, b"2em"), "text-indent:2em;");
        assert_eq!(css(true, b"0"), "margin-top:0px;");
        assert_eq!(css(false, b"-1em"), "text-indent:-1em;margin-left:1em;");
        assert_eq!(css(true, b"x\"y"), "");
        assert!(visible(b"  a"));
        assert!(!visible(b" &nbsp;\xC2\xA0 \n"));
        assert_eq!(
            strip_id(b"<div id=\"x\" class=\"c\">"),
            b"<div class=\"c\">"
        );
        assert_eq!(strip_id(b"<font size=3>"), b"<font size=3>");
    }

    #[test]
    fn day_vi_tri_ra_khoi_thuc_the() {
        let s = b"ab&#7841;cd \xe1\xbb\x87x";
        assert_eq!(adjust(s, 4, s.len(), Encoding::Utf8), 9);
        assert_eq!(adjust(s, 2, s.len(), Encoding::Utf8), 2);
        assert_eq!(adjust(s, 13, s.len(), Encoding::Utf8), 15);
        assert_eq!(adjust(s, 13, s.len(), Encoding::Cp1252), 13);
    }
}
