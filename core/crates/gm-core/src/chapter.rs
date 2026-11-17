//! Chuẩn bị một chương cho UIWebView (DECISIONS B7):
//!
//! - giải mã byte (UTF-8 / UTF-16 / Windows-1252), chữ về **NFC**, thực thể có tên → tham chiếu số (`&nbsp;` → `&#160;`), `&` lạc
//!   → `&amp;` — để bộ đọc XML của WebKit không báo lỗi;
//! - đúng cú pháp XML và có không gian tên XHTML → trả `application/xhtml+xml` (sách vnthuquan ngắt đoạn bằng `<br></br>`: đọc như
//!   HTML thì xuống dòng gấp đôi); không thì `text/html` (bỏ thẻ đóng của thẻ rỗng như `</br>`);
//! - **cắt chương lớn** (> `PART_LIMIT`) thành nhiều phần ở ranh giới đoạn: đóng các thẻ đang mở rồi mở lại ở đầu phần sau — chương
//!   1,3 MB mở mất 2,8 s và đổi cỡ chữ 1,4 s trên iPad mini 1; phần ≤ 150 KB thì ≤ 0,4 s;
//! - chỗ chèn phần đầu của màn đọc (viewport, cấu hình, reader.js) ngay sau `<head>`;
//! - `id` nào nằm ở phần nào (mục lục / link có `#id` → đúng phần).

use std::collections::HashMap;

use unicode_normalization::{is_nfc_quick, IsNormalized, UnicodeNormalization};

use crate::xml::{self, Tok};

/// Phần chương lớn tối đa (byte, tính cả thẻ).
pub const PART_LIMIT: usize = 150 * 1024;
/// Không gặp chỗ cắt an toàn tới chừng này thì cắt ở thẻ bất kỳ.
const HARD_LIMIT: usize = 3 * PART_LIMIT;

pub struct Chapter {
    /// Trả `application/xhtml+xml` (không thì `text/html`).
    pub xhtml: bool,
    /// Lý do không phải XHTML (nhật ký).
    pub why_html: Option<String>,
    /// Phần đầu trang tới chỗ chèn / sau chỗ chèn tới hết `<body …>`.
    head_a: String,
    head_b: String,
    parts: Vec<String>,
    tail: String,
    /// id → phần chứa nó.
    pub ids: HashMap<String, u32>,
}

/// Chuẩn hoá chữ: NFC (chỉ khi cần) + sửa tham chiếu.
fn clean_text(s: &str) -> String {
    let s = xml::fix_refs(s);
    match is_nfc_quick(s.chars()) {
        IsNormalized::Yes => s.into_owned(),
        _ => s.nfc().collect(),
    }
}

fn lower(name: &str) -> String {
    xml::local(name).to_ascii_lowercase()
}

/// Kết thúc một khối: cắt sau đây được.
fn block_end(name: &str) -> bool {
    matches!(
        lower(name).as_str(),
        "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote" | "section" | "article" | "center" | "figure" | "dd"
            | "dt" | "pre" | "table" | "ul" | "ol" | "dl" | "aside" | "header" | "footer" | "nav" | "br" | "hr"
    )
}

/// Không cắt bên trong các phần tử này (bảng, danh sách đánh số, khối giữ nguyên định dạng, hình vẽ).
fn no_split_inside(name: &str) -> bool {
    matches!(
        lower(name).as_str(),
        "table" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th" | "ul" | "ol" | "dl" | "pre" | "svg" | "math" | "ruby" | "select" | "figure"
    )
}

impl Chapter {
    pub fn build(bytes: &[u8]) -> Chapter {
        let (text, _) = xml::decode_bytes(bytes);
        // Khai báo XML ghi mã khác UTF-8 → đã đổi sang UTF-8, sửa lại khai báo
        let text = fix_xml_decl(&text);
        // Chuẩn hoá toàn văn một lượt: thẻ (giá trị thuộc tính) chỉ sửa tham chiếu, chữ thì thêm NFC
        let mut fixed = String::with_capacity(text.len() + 64);
        for (t, r) in xml::tokens(&text) {
            match t {
                Tok::Text(x) => fixed.push_str(&clean_text(x)),
                Tok::Start { .. } => fixed.push_str(&xml::fix_refs(&text[r])),
                _ => fixed.push_str(&text[r]),
            }
        }
        let mut why = xml::check_xml(&fixed).err();
        if why.is_none() && !has_xhtml_ns(&fixed) {
            why = Some("thiếu không gian tên XHTML".into());
        }
        let xhtml = why.is_none();
        let mut c = split(&fixed, xhtml);
        c.why_html = why;
        c
    }

    pub fn part_count(&self) -> usize {
        self.parts.len()
    }

    /// Tài liệu đầy đủ của phần `p`, chèn `inject` sau `<head>`.
    pub fn render(&self, p: usize, inject: &str) -> Vec<u8> {
        let body = self.parts.get(p).map(String::as_str).unwrap_or("");
        let mut out = String::with_capacity(self.head_a.len() + inject.len() + self.head_b.len() + body.len() + self.tail.len());
        out.push_str(&self.head_a);
        out.push_str(inject);
        out.push_str(&self.head_b);
        out.push_str(body);
        out.push_str(&self.tail);
        out.into_bytes()
    }

    /// Phần chứa `id`.
    pub fn part_of(&self, id: &str) -> Option<u32> {
        self.ids.get(id).copied()
    }

    /// Cỡ (byte) nội dung từng phần — để app ước tiến độ khi chưa đếm trang.
    pub fn part_sizes(&self) -> Vec<usize> {
        self.parts.iter().map(String::len).collect()
    }
}

fn has_xhtml_ns(s: &str) -> bool {
    for (t, _) in xml::tokens(s) {
        if let Tok::Start { name, attrs, .. } = t {
            return lower(name) == "html" && xml::attr_exact(attrs, "xmlns").is_some_and(|v| v == "http://www.w3.org/1999/xhtml");
        }
    }
    false
}

fn fix_xml_decl(s: &str) -> std::borrow::Cow<'_, str> {
    if let Some(rest) = s.strip_prefix("<?xml") {
        if let Some(end) = rest.find("?>") {
            let decl = &rest[..end];
            if let Some(enc) = xml::attr(decl, "encoding") {
                if !enc.eq_ignore_ascii_case("utf-8") {
                    return std::borrow::Cow::Owned(format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>{}", &rest[end + 2..]));
                }
            }
        }
    }
    std::borrow::Cow::Borrowed(s)
}

/// Tách đầu trang / thân / cuối và cắt thân thành các phần.
fn split(s: &str, xhtml: bool) -> Chapter {
    let toks: Vec<(Tok<'_>, std::ops::Range<usize>)> = xml::tokens(s).collect();
    // Chỗ chèn: sau <head …>; không có head thì sau <html …> (tự thêm <head></head>); không có cả hai thì đầu tệp
    let mut inject_at: Option<(usize, bool)> = None; // (vị trí, cần bọc <head>)
    let mut body_open: Option<usize> = None; // chỉ số token <body>
    for (i, (t, r)) in toks.iter().enumerate() {
        if let Tok::Start { name, empty, .. } = t {
            match lower(name).as_str() {
                "head" if inject_at.is_none_or(|(_, wrap)| wrap) => inject_at = Some((r.end, *empty)),
                "html" if inject_at.is_none() => inject_at = Some((r.end, true)),
                "body" if !*empty => {
                    body_open = Some(i);
                    break;
                }
                _ => {}
            }
        }
    }
    let (at, wrap) = inject_at.unwrap_or((0, false));
    let head_a_raw = &s[..at];
    let (head_a, mut head_b_prefix) = if wrap {
        (format!("{head_a_raw}<head>"), String::from("</head>"))
    } else {
        (head_a_raw.to_string(), String::new())
    };
    let Some(bi) = body_open else {
        // Không có <body>: một phần, không cắt
        head_b_prefix.push_str(&s[at..]);
        let mut ids = HashMap::new();
        collect_ids(&toks, 0, &mut ids);
        return Chapter { xhtml, why_html: None, head_a, head_b: head_b_prefix, parts: vec![String::new()], tail: String::new(), ids };
    };
    let body_end_tag = toks[bi].1.end;
    let head_b = format!("{head_b_prefix}{}", &s[at..body_end_tag]);

    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut stack: Vec<(String, String)> = Vec::new(); // (tên viết thường, thẻ mở thô)
    let mut ids: HashMap<String, u32> = HashMap::new();
    let mut tail = String::from("</body></html>");
    // Chữ cuối cùng của thân: sau nó không cắt nữa (khỏi phần rỗng)
    let last_text = toks.iter().rposition(|(t, _)| matches!(t, Tok::Text(x) if !x.trim().is_empty())).unwrap_or(0);
    let mut i = bi + 1;
    while i < toks.len() {
        let (t, r) = &toks[i];
        let raw = &s[r.clone()];
        let mut safe = false;
        match t {
            Tok::Start { name, attrs, empty } => {
                let n = lower(name);
                if let Some(id) = xml::attr(attrs, "id").or_else(|| if n == "a" { xml::attr(attrs, "name") } else { None }) {
                    ids.entry(id.into_owned()).or_insert(parts.len() as u32);
                }
                if !xhtml && xml::void_element(&n) {
                    // HTML: thẻ rỗng không vào ngăn xếp
                    cur.push_str(raw);
                    safe = n == "br" || n == "hr";
                } else if *empty {
                    cur.push_str(raw);
                    safe = n == "br" || n == "hr";
                } else {
                    cur.push_str(raw);
                    stack.push((n, raw.to_string()));
                }
            }
            Tok::End { name } => {
                let n = lower(name);
                if n == "body" {
                    // phần sau </body> (thường chỉ còn </html>) giữ nguyên
                    tail = s[r.start..].to_string();
                    break;
                }
                if !xhtml && xml::void_element(&n) {
                    // HTML: «</br>» là thêm một lần xuống dòng — bỏ
                    i += 1;
                    continue;
                }
                if let Some(pos) = stack.iter().rposition(|(m, _)| *m == n) {
                    stack.truncate(pos);
                    cur.push_str(raw);
                    safe = block_end(&n);
                } else if xhtml {
                    cur.push_str(raw);
                }
            }
            _ => cur.push_str(raw),
        }
        let limit_hit = cur.len() >= PART_LIMIT;
        let can = !stack.iter().any(|(m, _)| no_split_inside(m));
        let force = cur.len() >= HARD_LIMIT && matches!(t, Tok::End { .. } | Tok::Start { empty: true, .. } | Tok::Text(_));
        let more = i < last_text;
        if limit_hit && more && ((safe && can) || force) {
            // Đóng các thẻ đang mở, mở lại ở phần sau
            for (m, _) in stack.iter().rev() {
                cur.push_str(&format!("</{}>", original_name(&stack, m)));
            }
            parts.push(std::mem::take(&mut cur));
            for (_, open) in &stack {
                cur.push_str(&reopen(open));
            }
        }
        i += 1;
    }
    parts.push(cur);
    Chapter { xhtml, why_html: None, head_a, head_b, parts, tail, ids }
}

/// Tên thẻ đúng như trong thẻ mở (giữ hoa thường / tiền tố — XML phân biệt).
fn original_name(stack: &[(String, String)], lower_name: &str) -> String {
    stack
        .iter()
        .rev()
        .find(|(m, _)| m == lower_name)
        .map(|(_, open)| open[1..].split(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/').next().unwrap_or(lower_name).to_string())
        .unwrap_or_else(|| lower_name.to_string())
}

/// Thẻ mở lại ở đầu phần sau: bỏ `id` (khỏi trùng neo — neo thuộc phần trước).
fn reopen(open: &str) -> String {
    let Some(sp) = open.find(|c: char| c.is_ascii_whitespace()) else { return open.to_string() };
    let name = &open[..sp];
    let body = open[sp..].trim_end_matches('>').trim_end_matches('/');
    let kept: Vec<String> = xml::attrs(body)
        .into_iter()
        .filter(|(n, _, _)| !n.eq_ignore_ascii_case("id") && !n.eq_ignore_ascii_case("name"))
        .map(|(n, v, q)| if q { format!("{n}=\"{}\"", v.replace('"', "&quot;")) } else if v.is_empty() { n.to_string() } else { format!("{n}=\"{v}\"") })
        .collect();
    if kept.is_empty() { format!("{name}>") } else { format!("{name} {}>", kept.join(" ")) }
}

fn collect_ids(toks: &[(Tok<'_>, std::ops::Range<usize>)], part: u32, ids: &mut HashMap<String, u32>) {
    for (t, _) in toks {
        if let Tok::Start { name, attrs, .. } = t {
            if let Some(id) = xml::attr(attrs, "id").or_else(|| if lower(name) == "a" { xml::attr(attrs, "name") } else { None }) {
                ids.entry(id.into_owned()).or_insert(part);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INJ: &str = "<script>x</script>";

    #[test]
    fn small_xhtml_chapter_one_part() {
        let src = r#"<?xml version="1.0" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head><meta http-equiv="Content-Type" content="text/html; charset=utf-8"></meta><title>T</title></head>
  <body><p id="a">Người&nbsp;du khách</p><div>một<br clear="none"></br>hai</div></body>
</html>"#;
        let c = Chapter::build(src.as_bytes());
        assert!(c.xhtml, "{:?}", c.why_html);
        assert_eq!(c.part_count(), 1);
        let out = String::from_utf8(c.render(0, INJ)).unwrap();
        assert!(out.contains("<head><script>x</script><meta"), "{out}");
        assert!(out.contains("Người&#160;du"));
        assert!(out.ends_with("</body>\n</html>"));
        assert_eq!(xml::check_xml(&out), Ok(()));
        assert_eq!(c.part_of("a"), Some(0));
    }

    #[test]
    fn nfc_and_html_fallback() {
        // dấu tổ hợp → dựng sẵn; thiếu thẻ đóng → HTML, bỏ </br>
        let src = "<html><head><title>x</title></head><body><p>Vie\u{0302}\u{0323}t Nam<br></br><p>hết</body></html>";
        let c = Chapter::build(src.as_bytes());
        assert!(!c.xhtml);
        let out = String::from_utf8(c.render(0, "")).unwrap();
        assert!(out.contains("Việt Nam"), "{out}");
        assert!(!out.contains("</br>"));
    }

    #[test]
    fn splits_big_chapter_and_reopens_wrappers() {
        let mut body = String::from(r#"<div id="fontchu" style="LINE-HEIGHT: 150%">"#);
        let line = "Tôi còn ngồi lại một mình trên bãi cát vàng, đối diện với pho tượng Sphinx hùng vĩ.";
        for k in 0..6000 {
            body.push_str(line);
            if k == 4500 {
                body.push_str(r#"<a id="neo"></a>"#);
            }
            body.push_str(r#"<br clear="none"></br>"#);
        }
        body.push_str("</div>");
        let src = format!(r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>T</title></head><body>{body}</body></html>"#);
        let c = Chapter::build(src.as_bytes());
        assert!(c.xhtml);
        assert!(c.part_count() >= 4, "{}", c.part_count());
        for p in 0..c.part_count() {
            let out = String::from_utf8(c.render(p, INJ)).unwrap();
            assert_eq!(xml::check_xml(&out), Ok(()), "phần {p}");
            assert!(out.len() < PART_LIMIT + 4096, "phần {p}: {}", out.len());
            // phần sau mở lại khung <div style=…> (bỏ id)
            if p > 0 {
                assert!(out.contains(r#"<div style="LINE-HEIGHT: 150%">"#), "phần {p}");
            }
        }
        let n = c.part_of("neo").unwrap() as usize;
        assert!(n > 0 && String::from_utf8(c.render(n, "")).unwrap().contains(r#"id="neo""#));
        // không mất chữ
        let total: usize = c.part_sizes().iter().sum();
        assert!(total > 6000 * line.len());
    }

    #[test]
    fn no_head_gets_one() {
        let c = Chapter::build(br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>x</p></body></html>"#);
        let out = String::from_utf8(c.render(0, INJ)).unwrap();
        assert!(out.starts_with(r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><script>x</script></head><body>"#), "{out}");
        assert!(c.xhtml);
    }

    #[test]
    fn real_big_chapter_if_present() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../Tham_Khao/ebook/Bê Trọc - Phạm Việt Long.epub");
        let Ok(z) = crate::zip::Zip::open(&p) else { return };
        let data = z.read("OEBPS/3.html").unwrap();
        let t = std::time::Instant::now();
        let c = Chapter::build(&data);
        let ms = t.elapsed().as_millis();
        assert!(c.xhtml, "{:?}", c.why_html);
        assert!(c.part_count() >= 8, "{}", c.part_count());
        for p in 0..c.part_count() {
            assert_eq!(xml::check_xml(&String::from_utf8(c.render(p, INJ)).unwrap()), Ok(()), "phần {p}");
        }
        eprintln!("Bê Trọc chương 3: {} byte → {} phần, {ms} ms (máy PC, bản thử)", data.len(), c.part_count());
    }
}
