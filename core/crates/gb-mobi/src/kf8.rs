//! KF8 (AZW3, phần KF8 của tệp kép MOBI6 + KF8): XHTML ghép từ khung (SKEL) + mảnh (FRAG), CSS / SVG từ các luồng FDST,
//! mục lục từ chỉ mục NCX.
//!
//! Chữ (luồng 0) gồm lần lượt từng tệp: khung rồi các mảnh của nó; mảnh chèn vào khung ở vị trí tuyệt đối (trừ vị trí đầu
//! khung) — thuật toán của Calibre `mobi8.py` / KindleUnpack. Đường dẫn trong sách:
//! - `kindle:pos:fid:XXXX:off:YYYYYYYYYY` (mảnh XXXX, lệch YYYY — cơ số 32) → `partNNNN.html#id`: id của phần tử ở vị trí
//!   đó nếu có, không thì chèn `id="gbposN"` vào phần tử đó;
//! - `kindle:embed:XXXX` (tài nguyên thứ XXXX, 1 = bản ghi tài nguyên đầu) → `../images/NNNNN.jpg` / `../fonts/NNNNN.ttf`;
//! - `kindle:flow:XXXX` → `../styles/flowNNNN.css` (SVG có `<image>` thì chép thẳng vào trang như Calibre).
//!
//! Tệp đúng cú pháp XML → `application/xhtml+xml`; hỏng (hiếm) → `text/html` để WebKit vẫn hiện được.

use std::collections::{BTreeMap, HashMap};

use crate::header::{Header, NULL};
use crate::html::{attr, find, find_ci, is_void, is_ws, Kind, Lexer};
use crate::meta;
use crate::mobi6::{rec_index, safe_id};
use crate::pdb::Pdb;
use crate::resource::{font, image_type};
use crate::text::{clean_bytes, clean_label, decode_str, looks_vietnamese, Encoding};
use crate::{decomp, index, xmlcheck, Format, Item, MobiBook, MobiError, TocEntry};

/// Số cơ số 32 của Kindle (0-9, A-V).
pub(crate) fn base32(s: &[u8]) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    s.iter().try_fold(0u32, |a, &c| {
        let d = match c {
            b'0'..=b'9' => c - b'0',
            b'A'..=b'V' => c - b'A' + 10,
            b'a'..=b'v' => c - b'a' + 10,
            _ => return None,
        };
        a.checked_mul(32)?.checked_add(d as u32)
    })
}

fn be32(d: &[u8], o: usize) -> Option<u32> {
    let b = d.get(o..o.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Bảng luồng FDST: «FDST», vị trí bảng, số luồng, rồi các cặp (đầu, cuối). Không có → một luồng là cả chữ.
fn read_fdst(rec: &[u8], text_len: usize) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    if rec.starts_with(b"FDST") {
        let start = be32(rec, 4).unwrap_or(12) as usize;
        let n = (be32(rec, 8).unwrap_or(0) as usize).min(4096);
        for i in 0..n {
            let at = start.saturating_add(i * 8);
            let (Some(a), Some(b)) = (be32(rec, at), be32(rec, at.saturating_add(4))) else {
                break;
            };
            let (a, b) = ((a as usize).min(text_len), (b as usize).min(text_len));
            v.push((a, b.max(a)));
        }
    }
    if v.is_empty() {
        v.push((0, text_len));
    }
    v
}

/// Khung một tệp: số mảnh, vị trí + độ dài trong luồng 0.
struct Skel {
    count: usize,
    start: usize,
    len: usize,
}

/// Mảnh: vị trí chèn (tuyệt đối), bộ chọn (`P-//*[@aid='…']`), lệch trong phần tử, độ dài.
struct Frag {
    insert: usize,
    selector: Vec<u8>,
    offset: usize,
    len: usize,
}

fn read_skel(pdb: &Pdb, idx: Option<usize>) -> Vec<Skel> {
    let Some(ix) = idx.and_then(|i| index::read(pdb, i).ok()) else {
        return Vec::new();
    };
    ix.entries
        .iter()
        .map(|e| Skel {
            count: e.val(1, 0).unwrap_or(0) as usize,
            start: e.val(6, 0).unwrap_or(0) as usize,
            len: e.val(6, 1).unwrap_or(0) as usize,
        })
        .collect()
}

fn read_frag(pdb: &Pdb, idx: Option<usize>) -> Vec<Frag> {
    let Some(ix) = idx.and_then(|i| index::read(pdb, i).ok()) else {
        return Vec::new();
    };
    ix.entries
        .iter()
        .map(|e| {
            let insert = std::str::from_utf8(&e.ident)
                .ok()
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(usize::MAX);
            let selector = e
                .val(2, 0)
                .and_then(|o| ix.cncx.get(&o))
                .cloned()
                .unwrap_or_default();
            Frag {
                insert,
                selector,
                offset: e.val(6, 0).unwrap_or(0) as usize,
                len: e.val(6, 1).unwrap_or(0) as usize,
            }
        })
        .collect()
}

/// Cửa sổ dò ngược / xuôi quanh một vị trí (thẻ dài hơn thế là vô lý) — giữ mọi phép dò ở O(1) theo cỡ tệp.
const NEAR: usize = 64 * 1024;

/// Cuối `d` nằm giữa một thẻ (gần nhất phía trước là `<` chưa có `>`), chỉ dò ngược [`NEAR`] byte.
fn ends_inside_tag(d: &[u8]) -> bool {
    for &c in d[d.len().saturating_sub(NEAR)..].iter().rev() {
        match c {
            b'>' => return false,
            b'<' => return true,
            _ => {}
        }
    }
    false
}

/// Vị trí chèn theo bộ chọn `P-//*[@aid='X']`: sau thẻ mở có `aid="X"`, cộng lệch (cách Calibre sửa vị trí chèn hỏng).
fn aid_insert(doc: &[u8], selector: &[u8], offset: usize) -> Option<usize> {
    let p = find(selector, 0, b"@aid='")? + 6;
    let q = p + selector.get(p..)?.iter().position(|&c| c == b'\'')?;
    let aid = &selector[p..q];
    let mut pat = b"aid=\"".to_vec();
    pat.extend_from_slice(aid);
    pat.push(b'"');
    let at = find(doc, 0, &pat).or_else(|| {
        pat[4] = b'\'';
        let l = pat.len();
        pat[l - 1] = b'\'';
        find(doc, 0, &pat)
    })?;
    let gt = at + doc[at..].iter().take(NEAR).position(|&c| c == b'>')?;
    Some((gt + 1).saturating_add(offset).min(doc.len()))
}

/// Tệp đang ghép dạng «gap buffer»: `head` + đảo của `tail_rev`, chèn ở chỗ hở (cuối `head`). Dời chỗ hở k byte tốn O(k):
/// mảnh chèn tăng dần (thường gặp) không dời byte nào, mảnh lồng trong mảnh trước chỉ dời ít.
struct Gap {
    head: Vec<u8>,
    tail_rev: Vec<u8>,
}

impl Gap {
    fn new(s: &[u8]) -> Gap {
        Gap {
            head: Vec::with_capacity(s.len()),
            tail_rev: s.iter().rev().copied().collect(),
        }
    }

    fn len(&self) -> usize {
        self.head.len() + self.tail_rev.len()
    }

    /// Đưa chỗ hở tới `p` (≤ độ dài); trả số byte đã dời.
    fn move_to(&mut self, p: usize) -> usize {
        let h = self.head.len();
        if p < h {
            self.tail_rev.extend(self.head[p..].iter().rev());
            self.head.truncate(p);
            h - p
        } else {
            let k = (p - h).min(self.tail_rev.len());
            let t = self.tail_rev.len();
            self.head.extend(self.tail_rev[t - k..].iter().rev());
            self.tail_rev.truncate(t - k);
            k
        }
    }

    fn to_vec(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.len());
        v.extend_from_slice(&self.head);
        v.extend(self.tail_rev.iter().rev());
        v
    }

    fn into_vec(mut self) -> Vec<u8> {
        self.head.extend(self.tail_rev.iter().rev());
        self.head
    }
}

struct Assembled {
    parts: Vec<Vec<u8>>,
    /// Mảnh thứ i → (tệp, vị trí đã chèn trong tệp).
    frag_loc: Vec<Option<(usize, usize)>>,
    /// Khoảng của mỗi tệp trong luồng 0 (đầu khung, hết mảnh cuối).
    ranges: Vec<(usize, usize)>,
}

/// Chặn công ghép (tệp độc: vị trí chèn nhảy qua lại): số lần sửa vị trí theo aid; tổng byte dời chỗ hở chặn ở 2 × cỡ
/// chữ + 16 MB (sách thật chỉ dời xuôi ≈ 1 × cỡ chữ).
const AID_BUDGET: usize = 32;

fn assemble(f0: &[u8], skels: &[Skel], frags: &[Frag]) -> Assembled {
    let mut a = Assembled {
        parts: Vec::new(),
        frag_loc: Vec::new(),
        ranges: Vec::new(),
    };
    let mut fi = 0usize;
    let mut moved = 0usize;
    let budget = f0.len().saturating_mul(2).saturating_add(16 << 20);
    let mut aid_left = AID_BUDGET;
    for (si, sk) in skels.iter().enumerate() {
        let s0 = sk.start.min(f0.len());
        let s1 = sk.start.saturating_add(sk.len).min(f0.len());
        let mut g = Gap::new(&f0[s0..s1]);
        let mut base = s1;
        for _ in 0..sk.count.min(frags.len().saturating_sub(fi)) {
            let fr = &frags[fi];
            fi += 1;
            let p0 = base.min(f0.len());
            let p1 = base.saturating_add(fr.len).min(f0.len());
            base = p1;
            // Vị trí chèn tuyệt đối − đầu khung; hỏng → cuối tệp. Hết ngân sách dời → chèn tại chỗ.
            let want = fr.insert.checked_sub(sk.start).filter(|&p| p <= g.len());
            let at = want.unwrap_or(g.len());
            if moved <= budget {
                moved += g.move_to(at);
            }
            if want.is_none() || ends_inside_tag(&g.head) {
                // Vị trí chèn hỏng (ngoài tệp / cắt giữa thẻ): tìm theo aid, không được thì sau dấu `>` kế.
                let fixed = if aid_left > 0 && moved <= budget {
                    aid_left -= 1;
                    aid_insert(&g.to_vec(), &fr.selector, fr.offset)
                } else {
                    None
                };
                let to = fixed.unwrap_or_else(|| {
                    let t = &g.tail_rev;
                    let k = t
                        .iter()
                        .rev()
                        .take(NEAR)
                        .position(|&c| c == b'>')
                        .map_or(0, |k| k + 1);
                    g.head.len() + k
                });
                if moved <= budget {
                    moved += g.move_to(to);
                }
            }
            a.frag_loc.push(Some((si, g.head.len())));
            g.head.extend_from_slice(&f0[p0..p1]);
        }
        a.ranges.push((sk.start, base));
        a.parts.push(g.into_vec());
    }
    a
}

/// Thẻ mở «ở» vị trí `npos`: thẻ chứa / bắt đầu tại đó, hoặc thẻ mở kế (chỉ cách khoảng trắng / thẻ đóng), không thì thẻ
/// mở gần nhất phía trước. Trả vị trí `<`. Mọi phép dò giới hạn trong [`NEAR`] byte quanh `npos`.
fn target_tag(d: &[u8], npos: usize) -> Option<usize> {
    let npos = npos.min(d.len());
    let lo = npos.saturating_sub(NEAR);
    let hi = d.len().min(npos.saturating_add(NEAR));
    let lt = d[lo..(npos + 1).min(d.len())]
        .iter()
        .rposition(|&c| c == b'<')
        .map(|k| lo + k);
    let gt = d[lo..npos].iter().rposition(|&c| c == b'>').map(|k| lo + k);
    let mut cand = match (lt, gt) {
        (Some(l), Some(g)) if l > g => Some(l),
        (Some(l), None) => Some(l),
        _ => None,
    };
    if cand.is_none() {
        let q = npos + d[npos..hi].iter().take_while(|&&c| is_ws(c)).count();
        if d.get(q) == Some(&b'<') {
            cand = Some(q);
        }
    }
    let win = &d[..hi];
    if let Some(c) = cand {
        for t in Lexer::at(win, c).take(16) {
            match t.kind {
                Kind::Start => return Some(t.start),
                Kind::Text if !win[t.start..t.end].iter().all(|&b| is_ws(b)) => break,
                _ => {}
            }
        }
    }
    let mut i = npos;
    while i > lo {
        let l = lo + d[lo..i].iter().rposition(|&c| c == b'<')?;
        if d.get(l + 1).is_some_and(|c| c.is_ascii_alphabetic())
            && Lexer::at(win, l)
                .next()
                .is_some_and(|t| t.kind == Kind::Start)
        {
            return Some(l);
        }
        i = l;
    }
    None
}

/// Đích đã giải của một liên kết: (tệp, id — None = đầu tệp).
type Target = Option<(usize, Option<String>)>;

/// Giải `kindle:pos:fid` → (tệp, id), gom các id cần chèn.
struct Linker<'p> {
    parts: &'p [Vec<u8>],
    frag_loc: &'p [Option<(usize, usize)>],
    tag_ids: HashMap<(usize, usize), Option<String>>,
    inserts: Vec<Vec<(usize, Vec<u8>)>>,
    resolved: HashMap<(u32, u32), Target>,
    next: u32,
}

impl<'p> Linker<'p> {
    fn new(parts: &'p [Vec<u8>], frag_loc: &'p [Option<(usize, usize)>]) -> Linker<'p> {
        Linker {
            parts,
            frag_loc,
            tag_ids: HashMap::new(),
            inserts: vec![Vec::new(); parts.len()],
            resolved: HashMap::new(),
            next: 0,
        }
    }

    fn resolve(&mut self, fid: u32, off: u32) -> Target {
        if let Some(r) = self.resolved.get(&(fid, off)) {
            return r.clone();
        }
        let r = self.compute(fid, off);
        self.resolved.insert((fid, off), r.clone());
        r
    }

    fn compute(&mut self, fid: u32, off: u32) -> Target {
        let (part, at) = (*self.frag_loc.get(fid as usize)?)?;
        let parts = self.parts;
        let doc = parts.get(part)?;
        let npos = at.saturating_add(off as usize).min(doc.len());
        let id = target_tag(doc, npos).and_then(|ts| self.id_for(part, ts));
        Some((part, id))
    }

    fn id_for(&mut self, part: usize, ts: usize) -> Option<String> {
        if let Some(v) = self.tag_ids.get(&(part, ts)) {
            return v.clone();
        }
        let parts = self.parts;
        let doc = &parts[part];
        let t = Lexer::at(doc, ts)
            .next()
            .filter(|t| t.kind == Kind::Start)?;
        let ln = t.lname(doc);
        let n = ln.as_bytes();
        let r = if matches!(
            n,
            b"html" | b"head" | b"body" | b"title" | b"meta" | b"link" | b"style" | b"script"
        ) {
            None
        } else {
            match attr(doc, &t, b"id") {
                Some(v) if safe_id(v) => Some(String::from_utf8_lossy(v).into_owned()),
                Some(_) => {
                    // id có ký tự lạ: chèn neo rỗng ngay sau thẻ mở (trước nó nếu là thẻ rỗng).
                    let id = self.new_id();
                    let at = if t.self_closing || is_void(n) {
                        t.start
                    } else {
                        t.end
                    };
                    self.inserts[part].push((at, format!("<a id=\"{id}\"></a>").into_bytes()));
                    Some(id)
                }
                None => {
                    let id = self.new_id();
                    self.inserts[part].push((t.name.1, format!(" id=\"{id}\"").into_bytes()));
                    Some(id)
                }
            }
        };
        self.tag_ids.insert((part, ts), r.clone());
        r
    }

    fn new_id(&mut self) -> String {
        self.next += 1;
        format!("gbpos{}", self.next)
    }
}

fn apply_inserts(doc: &[u8], mut ins: Vec<(usize, Vec<u8>)>) -> Vec<u8> {
    if ins.is_empty() {
        return doc.to_vec();
    }
    ins.sort_by_key(|x| x.0);
    let mut out = Vec::with_capacity(doc.len() + ins.iter().map(|x| x.1.len()).sum::<usize>());
    let mut last = 0;
    for (at, b) in ins {
        let at = at.min(doc.len()).max(last);
        out.extend_from_slice(&doc[last..at]);
        out.extend_from_slice(&b);
        last = at;
    }
    out.extend_from_slice(&doc[last..]);
    out
}

/// Đường dẫn `kindle:…`.
#[derive(Debug, PartialEq)]
enum KUrl {
    Pos(u32, u32),
    Embed(u32),
    Flow(u32),
    Unknown,
}

fn b32_prefix(s: &[u8]) -> Option<u32> {
    let n = s.iter().take_while(|c| c.is_ascii_alphanumeric()).count();
    base32(&s[..n])
}

fn parse_kurl(u: &[u8]) -> KUrl {
    let parsed = if let Some(r) = u.strip_prefix(b"kindle:pos:fid:") {
        (|| {
            let c = r.iter().position(|&c| c == b':')?;
            let off = r[c..].strip_prefix(b":off:")?;
            Some(KUrl::Pos(base32(&r[..c])?, b32_prefix(off)?))
        })()
    } else if let Some(r) = u.strip_prefix(b"kindle:embed:") {
        b32_prefix(r).map(KUrl::Embed)
    } else if let Some(r) = u.strip_prefix(b"kindle:flow:") {
        b32_prefix(r).map(KUrl::Flow)
    } else {
        None
    };
    parsed.unwrap_or(KUrl::Unknown)
}

/// Cuối đường dẫn bắt đầu ở `k`.
fn url_end(s: &[u8], k: usize) -> usize {
    s[k..]
        .iter()
        .position(|&c| {
            matches!(
                c,
                b'"' | b'\'' | b')' | b'(' | b'<' | b'>' | b' ' | b'\t' | b'\r' | b'\n'
            )
        })
        .map_or(s.len(), |p| k + p)
}

/// Mọi `kindle:pos:fid` trong `s`.
fn pos_refs(s: &[u8]) -> Vec<(u32, u32)> {
    let mut v = Vec::new();
    let mut i = 0;
    while let Some(k) = find(s, i, b"kindle:pos:fid:") {
        let e = url_end(s, k);
        if let KUrl::Pos(f, o) = parse_kurl(&s[k..e]) {
            v.push((f, o));
        }
        i = e.max(k + 1);
    }
    v
}

/// Mọi `kindle:embed` trong `s`: (số thứ tự, muốn ảnh — theo `?mime=image/…`).
fn embed_refs(s: &[u8], out: &mut Vec<(u32, bool)>) {
    let mut i = 0;
    while let Some(k) = find(s, i, b"kindle:embed:") {
        let e = url_end(s, k);
        if let KUrl::Embed(n) = parse_kurl(&s[k..e]) {
            out.push((n, find(&s[k..e], 0, b"mime=image").is_some()));
        }
        i = e.max(k + 1);
    }
}

/// Luồng phụ (CSS / SVG).
struct Flow {
    /// Đường dẫn trong sách (rỗng: không xuất — luồng 0 / rỗng / SVG chép thẳng vào trang).
    path: String,
    mime: &'static str,
    data: Vec<u8>,
    /// SVG có `<image>`: chép thẳng vào trang thay cho thẻ tham chiếu.
    inline: bool,
}

fn classify_flows(text: &[u8], flows: &[(usize, usize)]) -> Vec<Flow> {
    let mut out = vec![Flow {
        path: String::new(),
        mime: "",
        data: Vec::new(),
        inline: false,
    }];
    for (j, &(a, b)) in flows.iter().enumerate().skip(1) {
        let d = &text[a..b];
        let f = if let Some(p) = find_ci(d, 0, b"<svg") {
            if find_ci(d, p, b"<image").is_some() {
                Flow {
                    path: String::new(),
                    mime: "image/svg+xml",
                    data: d[p..].to_vec(),
                    inline: true,
                }
            } else {
                Flow {
                    path: format!("images/flow{j:04}.svg"),
                    mime: "image/svg+xml",
                    data: d.to_vec(),
                    inline: false,
                }
            }
        } else if d.iter().all(|&c| is_ws(c)) {
            Flow {
                path: String::new(),
                mime: "",
                data: Vec::new(),
                inline: false,
            }
        } else {
            // CSS (đoạn CDATA thì bỏ dấu CDATA).
            let mut v = d.to_vec();
            if find(&v, 0, b"<![CDATA[").is_some() {
                v = String::from_utf8_lossy(&v)
                    .replace("<![CDATA[", "")
                    .replace("]]>", "")
                    .into_bytes();
            }
            Flow {
                path: format!("styles/flow{j:04}.css"),
                mime: "text/css",
                data: v,
                inline: false,
            }
        };
        out.push(f);
    }
    out
}

/// Thay thẻ tham chiếu luồng SVG «chép thẳng» bằng chính SVG đó.
fn inline_flows(doc: &[u8], flows: &[Flow]) -> Vec<u8> {
    let mut out = Vec::with_capacity(doc.len());
    let mut last = 0;
    for t in Lexer::new(doc) {
        if t.kind != Kind::Start {
            continue;
        }
        let tag = &doc[t.start..t.end];
        let Some(k) = find(tag, 0, b"kindle:flow:") else {
            continue;
        };
        let KUrl::Flow(n) = parse_kurl(&tag[k..url_end(tag, k)]) else {
            continue;
        };
        let Some(f) = flows.get(n as usize).filter(|f| f.inline) else {
            continue;
        };
        out.extend_from_slice(&doc[last..t.start]);
        out.extend_from_slice(&f.data);
        last = t.end;
    }
    out.extend_from_slice(&doc[last..]);
    out
}

/// Tài nguyên đã dùng: số thứ tự → (đường dẫn, mime, bản ghi | dữ liệu đã giải).
struct Res<'a, 'p> {
    pdb: &'p Pdb<'a>,
    base: Option<usize>,
    map: BTreeMap<u32, Option<(String, &'static str, ResData)>>,
}

enum ResData {
    Rec(usize),
    Owned(Vec<u8>),
}

impl Res<'_, '_> {
    fn load(&self, n: u32) -> Option<(String, &'static str, ResData)> {
        let rec = self.base?.checked_add((n as usize).checked_sub(1)?)?;
        if rec >= self.pdb.len() {
            return None;
        }
        let d = self.pdb.record(rec);
        if let Some((ext, mime)) = image_type(d) {
            return Some((format!("images/{n:05}.{ext}"), mime, ResData::Rec(rec)));
        }
        let (data, ext, mime) = font(d)?;
        Some((format!("fonts/{n:05}.{ext}"), mime, ResData::Owned(data)))
    }

    /// Đường dẫn tài nguyên `n` (đánh dấu đã dùng).
    fn path(&mut self, n: u32) -> Option<String> {
        if !self.map.contains_key(&n) {
            let v = self.load(n);
            self.map.insert(n, v);
        }
        self.map.get(&n)?.as_ref().map(|r| r.0.clone())
    }
}

/// Viết lại mọi `kindle:` trong một tệp / luồng.
fn rewrite_urls(
    src: &[u8],
    resolved: &HashMap<(u32, u32), Target>,
    res: &mut Res,
    flows: &[Flow],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() + 256);
    let mut last = 0;
    let mut i = 0;
    while let Some(k) = find(src, i, b"kindle:") {
        let e = url_end(src, k);
        let rep = match parse_kurl(&src[k..e]) {
            KUrl::Pos(fid, off) => Some(match resolved.get(&(fid, off)) {
                Some(Some((p, id))) => format!(
                    "part{p:04}.html{}",
                    id.as_ref().map(|i| format!("#{i}")).unwrap_or_default()
                ),
                _ => "#".into(),
            }),
            KUrl::Embed(n) => Some(
                res.path(n)
                    .map_or_else(|| "data:,".into(), |p| format!("../{p}")),
            ),
            KUrl::Flow(n) => Some(
                flows
                    .get(n as usize)
                    .filter(|f| !f.path.is_empty())
                    .map_or_else(|| "data:,".into(), |f| format!("../{}", f.path)),
            ),
            KUrl::Unknown => None,
        };
        if let Some(r) = rep {
            out.extend_from_slice(&src[last..k]);
            out.extend_from_slice(r.as_bytes());
            last = e;
        }
        i = e.max(k + 7);
    }
    out.extend_from_slice(&src[last..]);
    out
}

/// Một mục NCX của KF8: nhãn (CNCX), cấp, `pos:fid` (thẻ 6) hoặc vị trí trong chữ (thẻ 1).
struct NcxItem {
    label: String,
    depth: u32,
    pos_fid: Option<(u32, u32)>,
    pos: Option<u32>,
}

fn read_ncx(pdb: &Pdb, idx: Option<usize>, enc: Encoding) -> Vec<NcxItem> {
    let Some(ix) = idx.and_then(|i| index::read(pdb, i).ok()) else {
        return Vec::new();
    };
    ix.entries
        .iter()
        .map(|e| NcxItem {
            label: e
                .val(3, 0)
                .and_then(|o| ix.cncx.get(&o))
                .map(|b| clean_label(&decode_str(b, enc), 200))
                .unwrap_or_default(),
            depth: e.val(4, 0).unwrap_or(0).min(16),
            pos_fid: e.val(6, 0).zip(e.val(6, 1)),
            pos: e.val(1, 0),
        })
        .collect()
}

/// Nhãn dự phòng của một tệp: chữ của tiêu đề đầu tiên, không thì `<title>`, không thì đoạn chữ đầu.
fn part_label(doc: &[u8]) -> Option<String> {
    let mut title = None;
    let mut first_text: Vec<u8> = Vec::new();
    let mut in_body = false;
    let mut cap: Option<(Vec<u8>, bool)> = None; // (chữ, là tiêu đề h1–h6)
    for t in Lexer::new(doc) {
        match t.kind {
            Kind::Start => {
                let ln = t.lname(doc);
                let n = ln.as_bytes();
                if n == b"body" {
                    in_body = true;
                } else if n == b"title" && !t.self_closing {
                    cap = Some((Vec::new(), false));
                } else if matches!(n, [b'h', b'1'..=b'6']) && !t.self_closing {
                    cap = Some((Vec::new(), true));
                }
            }
            Kind::End => {
                let ln = t.lname(doc);
                let n = ln.as_bytes();
                if n == b"title" || matches!(n, [b'h', b'1'..=b'6']) {
                    if let Some((txt, heading)) = cap.take() {
                        let l = clean_label(&String::from_utf8_lossy(&txt), 200);
                        if heading && !l.is_empty() {
                            return Some(l);
                        }
                        if !heading && !l.is_empty() {
                            title = Some(l);
                        }
                    }
                }
            }
            Kind::Text => {
                let txt = &doc[t.start..t.end];
                if let Some((c, _)) = cap.as_mut() {
                    if c.len() < 600 {
                        c.extend_from_slice(txt);
                    }
                }
                if in_body && first_text.len() < 300 {
                    first_text.extend_from_slice(txt);
                    first_text.push(b' ');
                }
            }
            _ => {}
        }
    }
    title.or_else(|| {
        Some(clean_label(&String::from_utf8_lossy(&first_text), 60)).filter(|s| !s.is_empty())
    })
}

/// Chọn bản ghi tài nguyên đầu: theo đầu KF8 (tương đối) hay theo đầu MOBI6 (tệp kép dùng chung ảnh) — lấy cái khớp
/// nhiều tham chiếu `kindle:embed` hơn.
fn choose_base(pdb: &Pdb, cands: &[usize], refs: &[(u32, bool)]) -> Option<usize> {
    let score = |b: usize| {
        refs.iter()
            .filter(|&&(n, img)| {
                let Some(rec) = (n as usize).checked_sub(1).and_then(|k| b.checked_add(k)) else {
                    return false;
                };
                let d = pdb.record(rec);
                image_type(d).is_some() || (!img && d.starts_with(b"FONT"))
            })
            .count()
    };
    let mut best: Option<(usize, usize)> = None;
    for &c in cands {
        let s = score(c);
        if best.is_none_or(|(_, bs)| s > bs) {
            best = Some((c, s));
        }
    }
    best.map(|b| b.0)
}

pub(crate) fn convert(
    pdb: &Pdb,
    base: usize,
    h: &Header,
    outer: Option<&Header>,
) -> Result<MobiBook, MobiError> {
    let m = h
        .mobi
        .as_ref()
        .ok_or_else(|| MobiError::Invalid("KF8: thiếu đầu MOBI".into()))?;
    let len = pdb.len();
    let text = decomp::extract_text(pdb, base, h)?;
    let enc = Encoding::from_codepage(m.encoding);
    let flows = read_fdst(
        rec_index(m.fdst, base, len).map_or(&[][..], |i| pdb.record(i)),
        text.len(),
    );
    let (f0a, f0b) = flows[0];
    let f0 = &text[f0a..f0b];
    let skels = read_skel(pdb, rec_index(m.skel, base, len));
    let frags = read_frag(pdb, rec_index(m.frag, base, len));
    let asm = if skels.is_empty() {
        Assembled {
            parts: vec![f0.to_vec()],
            frag_loc: Vec::new(),
            ranges: vec![(0, f0.len())],
        }
    } else {
        assemble(f0, &skels, &frags)
    };
    if asm.parts.iter().all(|p| p.iter().all(|&c| is_ws(c))) {
        return Err(MobiError::Invalid("KF8: không có tệp chữ".into()));
    }
    let vietnamese = looks_vietnamese(f0);
    let ncx = read_ncx(pdb, rec_index(m.ncx, base, len), enc);

    // Liên kết vị trí: giải hết trên bản gốc rồi mới chèn id.
    let mut lk = Linker::new(&asm.parts, &asm.frag_loc);
    for p in &asm.parts {
        for (fid, off) in pos_refs(p) {
            lk.resolve(fid, off);
        }
    }
    for e in &ncx {
        if let Some((fid, off)) = e.pos_fid {
            lk.resolve(fid, off);
        }
    }
    let Linker {
        inserts, resolved, ..
    } = lk;
    let parts: Vec<Vec<u8>> = asm
        .parts
        .iter()
        .zip(inserts)
        .map(|(p, ins)| apply_inserts(p, ins))
        .collect();

    let mut flow_out = classify_flows(&text, &flows);
    drop(text);
    let any_inline = flow_out.iter().any(|f| f.inline);
    let parts: Vec<Vec<u8>> = if any_inline {
        parts.iter().map(|p| inline_flows(p, &flow_out)).collect()
    } else {
        parts
    };

    // Tài nguyên: bản ghi đầu theo đầu KF8 (tương đối, cho phép «âm» — tệp kép dùng chung ảnh với phần MOBI6) hoặc MOBI6.
    let mut meta = meta::read(pdb, h, outer);
    // Không ghi ngôn ngữ / ghi «en» mà chữ rõ là tiếng Việt → «vi» (như MOBI6).
    if vietnamese && meta.language.as_deref().is_none_or(|l| l.starts_with("en")) {
        meta.language = Some("vi".into());
    }
    let mut refs = Vec::new();
    for p in &parts {
        embed_refs(p, &mut refs);
    }
    for f in &flow_out {
        embed_refs(&f.data, &mut refs);
    }
    for c in [
        meta.cover,
        meta.cover_embed.and_then(|n| n.checked_sub(1)),
        meta.thumb,
    ]
    .into_iter()
    .flatten()
    {
        if let Some(n) = c.checked_add(1) {
            refs.push((n, true));
        }
    }
    let mut cands = Vec::new();
    if m.first_resource != NULL {
        let w = (base as u32).wrapping_add(m.first_resource) as usize;
        if w < len && w != base {
            cands.push(w);
        }
    }
    if let Some(o) = outer
        .and_then(|o| o.mobi.as_ref())
        .and_then(|om| rec_index(om.first_resource, 0, len))
    {
        if !cands.contains(&o) {
            cands.push(o);
        }
    }
    let mut res = Res {
        pdb,
        base: choose_base(pdb, &cands, &refs),
        map: BTreeMap::new(),
    };

    let parts: Vec<Vec<u8>> = parts
        .iter()
        .map(|p| clean_bytes(&rewrite_urls(p, &resolved, &mut res, &flow_out), enc))
        .collect();
    for j in 1..flow_out.len() {
        if flow_out[j].path.is_empty() && !flow_out[j].inline {
            continue;
        }
        let d = clean_bytes(
            &rewrite_urls(&flow_out[j].data, &resolved, &mut res, &flow_out),
            enc,
        );
        flow_out[j].data = d;
    }

    // Mục lục.
    let href = |t: &Target| -> Option<String> {
        let (p, id) = t.as_ref()?;
        Some(format!(
            "text/part{p:04}.html{}",
            id.as_ref().map(|i| format!("#{i}")).unwrap_or_default()
        ))
    };
    let mut toc: Vec<TocEntry> = ncx
        .iter()
        .filter(|e| !e.label.is_empty())
        .filter_map(|e| {
            let h = match (e.pos_fid, e.pos) {
                (Some(pf), _) => href(resolved.get(&pf)?)?,
                (None, Some(pos)) => {
                    let p = asm
                        .ranges
                        .iter()
                        .position(|&(a, b)| (a..b).contains(&(pos as usize)))?;
                    format!("text/part{p:04}.html")
                }
                _ => return None,
            };
            Some(TocEntry {
                title: e.label.clone(),
                href: h,
                depth: e.depth,
            })
        })
        .collect();
    if toc.is_empty() {
        toc = parts
            .iter()
            .enumerate()
            .map(|(i, p)| TocEntry {
                title: part_label(p).unwrap_or_else(|| format!("Phần {}", i + 1)),
                href: format!("text/part{i:04}.html"),
                depth: 0,
            })
            .collect();
    }

    // Ảnh bìa: EXTH 201, dự phòng 129 (kindle:embed), 202 (ảnh nhỏ).
    let cover = [
        meta.cover.and_then(|c| c.checked_add(1)),
        meta.cover_embed,
        meta.thumb.and_then(|c| c.checked_add(1)),
    ]
    .into_iter()
    .flatten()
    .find_map(|n| res.path(n).filter(|p| p.starts_with("images/")));

    let spine = parts
        .into_iter()
        .enumerate()
        .map(|(i, data)| {
            let mime = if xmlcheck::check(&data).is_ok() {
                "application/xhtml+xml"
            } else {
                "text/html"
            };
            Item {
                path: format!("text/part{i:04}.html"),
                mime: mime.into(),
                data,
            }
        })
        .collect();
    let mut resources: Vec<Item> = flow_out
        .into_iter()
        .filter(|f| !f.path.is_empty())
        .map(|f| Item {
            path: f.path,
            mime: f.mime.into(),
            data: f.data,
        })
        .collect();
    for (_, v) in res.map {
        if let Some((path, mime, data)) = v {
            let data = match data {
                ResData::Rec(r) => pdb.record(r).to_vec(),
                ResData::Owned(d) => d,
            };
            resources.push(Item {
                path,
                mime: mime.into(),
                data,
            });
        }
    }
    Ok(MobiBook {
        format: Format::Kf8,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn co_so_32_va_duong_dan() {
        assert_eq!(base32(b"0001"), Some(1));
        assert_eq!(base32(b"000V"), Some(31));
        assert_eq!(base32(b"0010"), Some(32));
        assert_eq!(base32(b"000a"), Some(10));
        assert_eq!(base32(b"00W"), None);
        assert_eq!(base32(b""), None);
        assert_eq!(base32(b"VVVVVVVVVV"), None);
        assert_eq!(
            parse_kurl(b"kindle:pos:fid:0002:off:000000000A"),
            KUrl::Pos(2, 10)
        );
        assert_eq!(
            parse_kurl(b"kindle:embed:0003?mime=image/jpeg"),
            KUrl::Embed(3)
        );
        assert_eq!(parse_kurl(b"kindle:flow:0001?mime=text/css"), KUrl::Flow(1));
        assert_eq!(parse_kurl(b"kindle:pos:fid:0002"), KUrl::Unknown);
        assert_eq!(parse_kurl(b"kindle:xyz"), KUrl::Unknown);
        assert_eq!(
            url_end(b"href=\"kindle:embed:0001?mime=image/png\" x", 6),
            38
        );
    }

    #[test]
    fn ghep_khung_va_manh() {
        // Luồng 0: khung tệp 0 (có <body aid="0"></body>), rồi 2 mảnh; mảnh 2 chèn vào trong mảnh 1.
        let skel = b"<html><body aid=\"0\"></body></html>";
        let f1 = b"<div aid=\"1\"></div>";
        let f2 = b"<p>Hi</p>";
        let mut f0 = skel.to_vec();
        f0.extend_from_slice(f1);
        f0.extend_from_slice(f2);
        let ins1 = skel.iter().position(|&c| c == b'<').unwrap() + "<html><body aid=\"0\">".len();
        let ins2 = ins1 + "<div aid=\"1\">".len();
        let skels = [Skel {
            count: 2,
            start: 0,
            len: skel.len(),
        }];
        let frags = [
            Frag {
                insert: ins1,
                selector: b"P-//*[@aid='0']".to_vec(),
                offset: 0,
                len: f1.len(),
            },
            Frag {
                insert: ins2,
                selector: b"P-//*[@aid='1']".to_vec(),
                offset: 0,
                len: f2.len(),
            },
        ];
        let a = assemble(&f0, &skels, &frags);
        assert_eq!(
            String::from_utf8_lossy(&a.parts[0]),
            "<html><body aid=\"0\"><div aid=\"1\"><p>Hi</p></div></body></html>"
        );
        assert_eq!(a.frag_loc, vec![Some((0, ins1)), Some((0, ins2))]);
        // Vị trí chèn hỏng (giữa thẻ) → sửa theo aid.
        let bad = [
            Frag {
                insert: 3,
                selector: b"P-//*[@aid='0']".to_vec(),
                offset: 0,
                len: f1.len(),
            },
            Frag {
                insert: ins2,
                selector: b"P-//*[@aid='1']".to_vec(),
                offset: 0,
                len: f2.len(),
            },
        ];
        let a = assemble(&f0, &skels, &bad);
        assert_eq!(
            String::from_utf8_lossy(&a.parts[0]),
            "<html><body aid=\"0\"><div aid=\"1\"><p>Hi</p></div></body></html>"
        );
        // Số liệu vô lý → không panic.
        let crazy = [Skel {
            count: 99,
            start: usize::MAX - 2,
            len: usize::MAX,
        }];
        let _ = assemble(&f0, &crazy, &bad);
    }

    #[test]
    fn tim_the_dich() {
        let d = b"<html><body><p id=\"a\">One</p>\n  <p class=\"x\">Two <b>bold</b> end</p></body></html>";
        let p2 = find(d, 0, b"<p class").unwrap();
        // Đúng tại thẻ, giữa thẻ, khoảng trắng trước thẻ.
        assert_eq!(target_tag(d, p2), Some(p2));
        assert_eq!(target_tag(d, p2 + 4), Some(p2));
        assert_eq!(target_tag(d, p2 - 2), Some(p2));
        // Giữa chữ → thẻ mở gần nhất phía trước.
        let b = find(d, 0, b"<b>").unwrap();
        assert_eq!(target_tag(d, find(d, 0, b"end").unwrap()), Some(b));
        // Thẻ đóng → thẻ mở kế.
        assert_eq!(target_tag(d, find(d, 0, b"</p>").unwrap()), Some(p2));
        let parts = vec![d.to_vec()];
        let loc = vec![Some((0usize, 0usize))];
        let mut lk = Linker::new(&parts, &loc);
        assert_eq!(
            lk.resolve(0, find(d, 0, b"<p id").unwrap() as u32),
            Some((0, Some("a".to_string())))
        );
        assert_eq!(
            lk.resolve(0, p2 as u32),
            Some((0, Some("gbpos1".to_string())))
        );
        assert_eq!(
            lk.resolve(0, p2 as u32 + 1),
            Some((0, Some("gbpos1".to_string())))
        );
        assert_eq!(lk.resolve(0, 0), Some((0, None)));
        assert_eq!(lk.resolve(5, 0), None);
        let out = apply_inserts(d, std::mem::take(&mut lk.inserts[0]));
        assert!(find(&out, 0, b"<p id=\"gbpos1\" class=\"x\">").is_some());
        assert!(xmlcheck::check(&out).is_ok());
    }
}
