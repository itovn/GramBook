//! Bộ tách thẻ HTML / XHTML tối giản («tag soup»): chữ, thẻ mở / đóng, chú thích, khai báo. Không dựng cây, không chép
//! dữ liệu — mỗi mảnh là một khoảng byte của chuỗi gốc. Dùng cho cả HTML Mobipocket (lỏng lẻo) lẫn XHTML của KF8.
//!
//! Giá trị thuộc tính theo luật HTML: dấu nháy chỉ có nghĩa ở đầu giá trị. Nháy không đóng (tệp hỏng) không được nuốt cả
//! sách: nếu trong «giá trị» có `>` rồi `<`, coi thẻ kết thúc ở `>` đầu tiên.

/// Loại mảnh.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Text,
    /// Thẻ mở (kể cả tự đóng `<br/>`).
    Start,
    End,
    /// `<!-- … -->`
    Comment,
    /// `<!DOCTYPE …>`, `<?xml …?>`, `<![CDATA[…]]>`
    Decl,
}

/// Một mảnh: khoảng `start..end` trong chuỗi gốc; tên thẻ ở `name`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tok {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub name: (usize, usize),
    pub self_closing: bool,
}

impl Tok {
    pub fn name<'a>(&self, s: &'a [u8]) -> &'a [u8] {
        &s[self.name.0..self.name.1]
    }

    /// Tên thẻ chữ thường (cắt ≤ 31 byte) — để so sánh nhanh.
    pub fn lname(&self, s: &[u8]) -> LName {
        LName::new(self.name(s))
    }
}

/// Tên thẻ chữ thường trong bộ đệm cố định.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct LName {
    buf: [u8; 31],
    len: u8,
}

impl LName {
    pub fn new(n: &[u8]) -> LName {
        let mut buf = [0u8; 31];
        let len = n.len().min(31);
        for (d, s) in buf.iter_mut().zip(&n[..len]) {
            *d = s.to_ascii_lowercase();
        }
        LName {
            buf,
            len: len as u8,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len as usize]
    }
}

impl PartialEq<&[u8]> for LName {
    fn eq(&self, o: &&[u8]) -> bool {
        self.as_bytes() == *o
    }
}

pub(crate) fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b':' | b'-' | b'_' | b'.') || b >= 0x80
}

/// Vị trí đầu tiên của `pat` trong `s` từ `from`.
pub(crate) fn find(s: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if pat.is_empty() || from >= s.len() {
        return None;
    }
    let first = pat[0];
    let mut i = from;
    while i + pat.len() <= s.len() {
        let p = i + s[i..s.len() - pat.len() + 1]
            .iter()
            .position(|&b| b == first)?;
        if &s[p..p + pat.len()] == pat {
            return Some(p);
        }
        i = p + 1;
    }
    None
}

/// Như [`find`] nhưng không phân biệt hoa thường (ASCII).
pub(crate) fn find_ci(s: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if pat.is_empty() {
        return None;
    }
    let (lo, up) = (pat[0].to_ascii_lowercase(), pat[0].to_ascii_uppercase());
    let mut i = from;
    while i + pat.len() <= s.len() {
        let k = s[i..s.len() - pat.len() + 1]
            .iter()
            .position(|&b| b == lo || b == up)?;
        let p = i + k;
        if s[p..p + pat.len()].eq_ignore_ascii_case(pat) {
            return Some(p);
        }
        i = p + 1;
    }
    None
}

fn find_byte(s: &[u8], from: usize, b: u8) -> Option<usize> {
    s.get(from..)?
        .iter()
        .position(|&c| c == b)
        .map(|k| from + k)
}

/// Vị trí thẻ đóng `</name` (không phân biệt hoa thường) từ `from`.
fn find_close(s: &[u8], from: usize, name: &[u8]) -> Option<usize> {
    let mut i = from;
    loop {
        let p = find(s, i, b"</")?;
        let after = p + 2 + name.len();
        if after <= s.len()
            && s[p + 2..after].eq_ignore_ascii_case(name)
            && s.get(after)
                .is_none_or(|&c| is_ws(c) || c == b'>' || c == b'/')
        {
            return Some(p);
        }
        i = p + 2;
    }
}

/// Bộ tách mảnh.
pub(crate) struct Lexer<'a> {
    s: &'a [u8],
    pos: usize,
    /// Đang trong phần tử chữ thô (`style`, `script`): tên thẻ đóng cần tìm.
    raw: Option<&'static [u8]>,
}

impl<'a> Lexer<'a> {
    pub fn new(s: &'a [u8]) -> Lexer<'a> {
        Lexer {
            s,
            pos: 0,
            raw: None,
        }
    }

    /// Bắt đầu tách từ vị trí `pos` (coi như ở ngoài thẻ).
    pub fn at(s: &'a [u8], pos: usize) -> Lexer<'a> {
        Lexer { s, pos, raw: None }
    }
}

impl Iterator for Lexer<'_> {
    type Item = Tok;

    fn next(&mut self) -> Option<Tok> {
        let s = self.s;
        let n = s.len();
        if self.pos >= n {
            return None;
        }
        if let Some(close) = self.raw.take() {
            let end = find_close(s, self.pos, close).unwrap_or(n);
            if end > self.pos {
                let t = Tok {
                    kind: Kind::Text,
                    start: self.pos,
                    end,
                    name: (0, 0),
                    self_closing: false,
                };
                self.pos = end;
                return Some(t);
            }
        }
        let start = self.pos;
        if s[start] == b'<' {
            if let Some(t) = lex_markup(s, start) {
                self.pos = t.end;
                if t.kind == Kind::Start && !t.self_closing {
                    let nm = t.name(s);
                    if nm.eq_ignore_ascii_case(b"style") {
                        self.raw = Some(b"style");
                    } else if nm.eq_ignore_ascii_case(b"script") {
                        self.raw = Some(b"script");
                    }
                }
                return Some(t);
            }
        }
        let end = find_byte(s, start + 1, b'<').unwrap_or(n);
        self.pos = end;
        Some(Tok {
            kind: Kind::Text,
            start,
            end,
            name: (0, 0),
            self_closing: false,
        })
    }
}

/// Đọc một thẻ / chú thích / khai báo bắt đầu ở `i` (`s[i] == '<'`). `None` = `<` chỉ là chữ.
fn lex_markup(s: &[u8], i: usize) -> Option<Tok> {
    let n = s.len();
    let rest = &s[i..];
    let mk = |kind, end: usize| Tok {
        kind,
        start: i,
        end,
        name: (i, i),
        self_closing: false,
    };
    if rest.starts_with(b"<!--") {
        return Some(mk(
            Kind::Comment,
            find(s, i + 4, b"-->").map_or(n, |p| p + 3),
        ));
    }
    if rest.starts_with(b"<![CDATA[") {
        return Some(mk(Kind::Decl, find(s, i + 9, b"]]>").map_or(n, |p| p + 3)));
    }
    if rest.starts_with(b"<!") || rest.starts_with(b"<?") {
        return Some(mk(
            Kind::Decl,
            find_byte(s, i + 2, b'>').map_or(n, |p| p + 1),
        ));
    }
    let is_end = rest.get(1) == Some(&b'/');
    let name_start = if is_end { i + 2 } else { i + 1 };
    if !s.get(name_start)?.is_ascii_alphabetic() {
        return None;
    }
    let mut j = name_start;
    while j < n && is_name_char(s[j]) {
        j += 1;
    }
    let name = (name_start, j);
    if is_end {
        let end = find_byte(s, j, b'>').map_or(n, |p| p + 1);
        return Some(Tok {
            kind: Kind::End,
            start: i,
            end,
            name,
            self_closing: false,
        });
    }
    let (end, self_closing) = scan_attrs(s, j, None);
    Some(Tok {
        kind: Kind::Start,
        start: i,
        end,
        name,
        self_closing,
    })
}

/// Một thuộc tính: cả khoảng `start..end` (kể cả nháy), tên, giá trị (không kèm nháy).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Attr {
    pub start: usize,
    pub end: usize,
    pub name: (usize, usize),
    pub value: Option<(usize, usize)>,
}

impl Attr {
    pub fn name<'a>(&self, s: &'a [u8]) -> &'a [u8] {
        &s[self.name.0..self.name.1]
    }

    pub fn value<'a>(&self, s: &'a [u8]) -> &'a [u8] {
        self.value.map_or(&[][..], |(a, b)| &s[a..b])
    }
}

/// Chặn tìm nháy đóng (nháy hỏng không làm chậm cả sách).
const QUOTE_LIMIT: usize = 64 * 1024;

/// Đọc thuộc tính từ `i` (ngay sau tên thẻ) tới `>`. Trả (vị trí sau `>`, tự đóng). Ghi thuộc tính vào `out` nếu có.
fn scan_attrs(s: &[u8], mut i: usize, mut out: Option<&mut Vec<Attr>>) -> (usize, bool) {
    let n = s.len();
    let mut slash = false;
    loop {
        while i < n && is_ws(s[i]) {
            i += 1;
        }
        if i >= n {
            return (n, false);
        }
        match s[i] {
            b'>' => return (i + 1, slash),
            b'/' => {
                slash = true;
                i += 1;
                continue;
            }
            _ => {}
        }
        slash = false;
        let a0 = i;
        while i < n && !is_ws(s[i]) && !matches!(s[i], b'>' | b'=' | b'/') {
            i += 1;
        }
        if i == a0 {
            // '=' đứng đầu — bỏ qua.
            i += 1;
            continue;
        }
        let name = (a0, i);
        let mut k = i;
        while k < n && is_ws(s[k]) {
            k += 1;
        }
        let mut value = None;
        if k < n && s[k] == b'=' {
            k += 1;
            while k < n && is_ws(s[k]) {
                k += 1;
            }
            if k >= n {
                return (n, false);
            }
            let q = s[k];
            if q == b'"' || q == b'\'' {
                let lim = n.min(k + 1 + QUOTE_LIMIT);
                let close = s[k + 1..lim]
                    .iter()
                    .position(|&c| c == q)
                    .map(|p| k + 1 + p);
                let gt = find_byte(&s[..lim], k + 1, b'>');
                // Nháy hỏng: trong «giá trị» có '>' rồi '<' (hoặc không có nháy đóng) → thẻ kết thúc ở '>' đó.
                let broken = match (close, gt) {
                    (Some(c), Some(g)) => g < c && find_byte(&s[..c], g, b'<').is_some(),
                    (None, _) => true,
                    _ => false,
                };
                if broken {
                    let g = gt.or_else(|| find_byte(s, k + 1, b'>')).unwrap_or(n);
                    value = Some((k + 1, g));
                    i = g;
                } else {
                    let c = close.unwrap_or(n);
                    value = Some((k + 1, c));
                    i = (c + 1).min(n);
                }
            } else {
                let v0 = k;
                while k < n && !is_ws(s[k]) && s[k] != b'>' {
                    k += 1;
                }
                value = Some((v0, k));
                i = k;
            }
        }
        if let Some(o) = out.as_deref_mut() {
            o.push(Attr {
                start: a0,
                end: i,
                name,
                value,
            });
        }
    }
}

/// Các thuộc tính của thẻ mở `t`.
pub(crate) fn attrs(s: &[u8], t: &Tok) -> Vec<Attr> {
    let mut v = Vec::new();
    attrs_into(s, t, &mut v);
    v
}

/// Như [`attrs`] nhưng ghi vào `out` (xoá trước) — dùng lại bộ nhớ khi gọi cho từng thẻ.
pub(crate) fn attrs_into(s: &[u8], t: &Tok, out: &mut Vec<Attr>) {
    out.clear();
    if t.kind == Kind::Start {
        scan_attrs(&s[..t.end], t.name.1, Some(out));
    }
}

/// Giá trị thuộc tính `name` (không phân biệt hoa thường) của thẻ mở `t`.
pub(crate) fn attr<'a>(s: &'a [u8], t: &Tok, name: &[u8]) -> Option<&'a [u8]> {
    if t.kind != Kind::Start || t.end - t.name.1 < name.len() {
        return None;
    }
    // Lọc nhanh: thẻ không chứa tên thuộc tính thì khỏi tách.
    find_ci(&s[..t.end], t.name.1, name)?;
    attrs(s, t)
        .into_iter()
        .find(|a| a.name(s).eq_ignore_ascii_case(name))
        .map(|a| a.value(s))
}

/// Phần tử rỗng (không có thẻ đóng) của HTML.
pub(crate) fn is_void(name: &[u8]) -> bool {
    matches!(
        name,
        b"area"
            | b"base"
            | b"br"
            | b"col"
            | b"embed"
            | b"hr"
            | b"img"
            | b"input"
            | b"link"
            | b"meta"
            | b"param"
            | b"source"
            | b"track"
            | b"wbr"
            | b"basefont"
            | b"frame"
            | b"keygen"
            | b"spacer"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<(Kind, String)> {
        Lexer::new(s.as_bytes())
            .map(|t| (t.kind, s[t.start..t.end].to_string()))
            .collect()
    }

    #[test]
    fn tach_manh() {
        let v = kinds(
            "<p a=1 b='x>y' c=\"q\">Chữ <br/>a < b<!-- c --><?xml x?></P><style>p<b{}</style>z",
        );
        let want = [
            (Kind::Start, "<p a=1 b='x>y' c=\"q\">"),
            (Kind::Text, "Chữ "),
            (Kind::Start, "<br/>"),
            (Kind::Text, "a "),
            (Kind::Text, "< b"),
            (Kind::Comment, "<!-- c -->"),
            (Kind::Decl, "<?xml x?>"),
            (Kind::End, "</P>"),
            (Kind::Start, "<style>"),
            (Kind::Text, "p<b{}"),
            (Kind::End, "</style>"),
            (Kind::Text, "z"),
        ];
        assert_eq!(v.len(), want.len(), "{v:?}");
        for (a, b) in v.iter().zip(want) {
            assert_eq!((a.0, a.1.as_str()), b);
        }
        let s = b"<br/>";
        assert!(Lexer::new(s).next().unwrap().self_closing);
        // Giá trị không nháy có '/' không phải tự đóng.
        let s = b"<a href=x/>";
        assert!(!Lexer::new(s).next().unwrap().self_closing);
    }

    #[test]
    fn nhay_hong_khong_nuot_sach() {
        let s = "<font face=\"\"\\.VnTime\"\" color=\"black\">A</font><p title=\"x>B</p><p>C</p>";
        let v = kinds(s);
        assert_eq!(v[0].1, "<font face=\"\"\\.VnTime\"\" color=\"black\">");
        assert_eq!(v[1].1, "A");
        assert_eq!(v[3].1, "<p title=\"x>");
        assert_eq!(v[4].1, "B");
        // Thẻ không đóng ở cuối: lấy tới hết, không panic.
        let v = kinds("abc <p class=\"x");
        assert_eq!(v.last().unwrap().1, "<p class=\"x");
        let v = kinds("<p a=");
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn thuoc_tinh() {
        let s = b"<a FilePos=0000012345 href = 'x' checked id=\"k\">";
        let t = Lexer::new(s).next().unwrap();
        assert_eq!(attr(s, &t, b"filepos"), Some(&b"0000012345"[..]));
        assert_eq!(attr(s, &t, b"href"), Some(&b"x"[..]));
        assert_eq!(attr(s, &t, b"checked"), Some(&b""[..]));
        assert_eq!(attr(s, &t, b"id"), Some(&b"k"[..]));
        assert_eq!(attr(s, &t, b"name"), None);
        let a = attrs(s, &t);
        assert_eq!(a.len(), 4);
        assert_eq!(&s[a[1].start..a[1].end], b"href = 'x'");
        assert_eq!(t.lname(s).as_bytes(), b"a");
        assert_eq!(find_ci(b"xx<BODY>", 0, b"<body"), Some(2));
        assert_eq!(find(b"abcabc", 1, b"abc"), Some(3));
        assert_eq!(find(b"ab", 0, b"abc"), None);
    }
}
