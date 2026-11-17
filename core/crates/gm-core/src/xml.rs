//! Bộ tách thẻ XML / HTML nhỏ, chịu lỗi — đủ cho EPUB: đọc OPF / NCX / nav, viết lại chương (chèn đầu trang, NFC, đổi thực thể,
//! cắt phần) mà giữ nguyên từng byte phần không đụng tới. Không dùng quick-xml: cần giữ nguyên văn bản gốc và chịu được HTML
//! hỏng (sách tự làm), API quick-xml lại đổi nhiều giữa các bản.

use std::borrow::Cow;
use std::ops::Range;

/// Một thẻ / đoạn chữ, kèm vị trí byte trong chuỗi gốc.
#[derive(Debug, Clone, PartialEq)]
pub enum Tok<'a> {
    Text(&'a str),
    /// `<tên thuộc_tính…>` — `attrs` là phần thô sau tên (chưa tách), `empty` = tự đóng `/>`.
    Start { name: &'a str, attrs: &'a str, empty: bool },
    End { name: &'a str },
    Comment(&'a str),
    CData(&'a str),
    /// `<?xml …?>` và các chỉ thị xử lý khác.
    Pi(&'a str),
    Doctype(&'a str),
    /// `<` lạc (không mở thẻ nào) — HTML hỏng.
    Stray,
}

pub struct Tokens<'a> {
    s: &'a str,
    pos: usize,
    /// Đang trong `<script>` / `<style>`: đọc thô tới thẻ đóng tương ứng.
    raw_until: Option<&'static str>,
}

pub fn tokens(s: &str) -> Tokens<'_> {
    Tokens { s, pos: 0, raw_until: None }
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b':' || c >= 0x80
}

/// Tìm `pat` (ASCII, không phân biệt hoa thường) từ `from`.
fn find_ci(s: &str, from: usize, pat: &str) -> Option<usize> {
    let b = s.as_bytes();
    let p = pat.as_bytes();
    if p.is_empty() || b.len() < p.len() {
        return None;
    }
    (from..=b.len() - p.len()).find(|&i| b[i..i + p.len()].eq_ignore_ascii_case(p))
}

impl<'a> Iterator for Tokens<'a> {
    type Item = (Tok<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        let s = self.s;
        let b = s.as_bytes();
        let start = self.pos;
        if start >= b.len() {
            return None;
        }
        if let Some(end_tag) = self.raw_until.take() {
            let stop = find_ci(s, start, end_tag).unwrap_or(b.len());
            if stop > start {
                self.pos = stop;
                return Some((Tok::Text(&s[start..stop]), start..stop));
            }
        }
        if b[start] != b'<' {
            let stop = memchr(b'<', b, start).unwrap_or(b.len());
            self.pos = stop;
            return Some((Tok::Text(&s[start..stop]), start..stop));
        }
        let rest = &s[start..];
        let (tok, len) = if let Some(body) = rest.strip_prefix("<!--") {
            let end = body.find("-->").map(|i| 4 + i + 3).unwrap_or(rest.len());
            (Tok::Comment(&rest[4..end.saturating_sub(3).max(4)]), end)
        } else if let Some(body) = rest.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").map(|i| 9 + i + 3).unwrap_or(rest.len());
            (Tok::CData(&rest[9..end.saturating_sub(3).max(9)]), end)
        } else if rest.starts_with("<!") {
            // DOCTYPE (có thể có phần khai báo trong ngoặc vuông)
            let mut depth = 0i32;
            let mut end = rest.len();
            for (i, c) in rest.bytes().enumerate().skip(2) {
                match c {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    b'>' if depth <= 0 => {
                        end = i + 1;
                        break;
                    }
                    _ => {}
                }
            }
            (Tok::Doctype(&rest[..end]), end)
        } else if rest.starts_with("<?") {
            let end = rest.find("?>").map(|i| i + 2).unwrap_or(rest.len());
            (Tok::Pi(&rest[..end]), end)
        } else if rest.starts_with("</") {
            let end = rest.find('>').map(|i| i + 1).unwrap_or(rest.len());
            let inner = rest[2..end.saturating_sub(1).max(2)].trim();
            let name = inner.split(|c: char| c.is_ascii_whitespace()).next().unwrap_or("");
            (Tok::End { name }, end)
        } else if rest.len() > 1 && is_name_start(rest.as_bytes()[1]) {
            // Thẻ mở: tên rồi thuộc tính tới «>» ngoài dấu nháy
            let rb = rest.as_bytes();
            let mut i = 1;
            while i < rb.len() && !rb[i].is_ascii_whitespace() && rb[i] != b'>' && rb[i] != b'/' {
                i += 1;
            }
            let name = &rest[1..i];
            let astart = i;
            let mut quote = 0u8;
            let mut end = rb.len();
            while i < rb.len() {
                let c = rb[i];
                if quote != 0 {
                    if c == quote {
                        quote = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    quote = c;
                } else if c == b'>' {
                    end = i + 1;
                    break;
                }
                i += 1;
            }
            let inner_end = if end <= rb.len() && end > astart && rb.get(end - 1) == Some(&b'>') { end - 1 } else { end };
            let mut attrs = &rest[astart..inner_end];
            let empty = attrs.trim_end().ends_with('/');
            if empty {
                attrs = attrs.trim_end().trim_end_matches('/');
            }
            if !empty {
                let low = name.to_ascii_lowercase();
                if low == "script" {
                    self.raw_until = Some("</script");
                } else if low == "style" {
                    self.raw_until = Some("</style");
                }
            }
            (Tok::Start { name, attrs, empty }, end)
        } else {
            (Tok::Stray, 1)
        };
        self.pos = start + len;
        Some((tok, start..start + len))
    }
}

fn memchr(c: u8, b: &[u8], from: usize) -> Option<usize> {
    b[from..].iter().position(|&x| x == c).map(|i| from + i)
}

/// Phần tên không tiền tố: «dc:title» → «title», «epub:type» → «type».
pub fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Các thuộc tính thô: (tên, giá trị thô chưa giải thực thể, có nháy không).
pub fn attrs(raw: &str) -> Vec<(&str, &str, bool)> {
    let b = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let ns = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'/' {
            i += 1;
        }
        if ns == i {
            break;
        }
        let name = &raw[ns..i];
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let vs = i + 1;
                let ve = memchr(q, b, vs).unwrap_or(b.len());
                out.push((name, &raw[vs..ve], true));
                i = (ve + 1).min(b.len());
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                out.push((name, &raw[vs..i], false));
            }
        } else {
            out.push((name, "", false));
        }
    }
    out
}

/// Giá trị đã giải thực thể của thuộc tính `name` (so tên không tiền tố, không phân biệt hoa thường).
pub fn attr<'a>(raw: &'a str, name: &str) -> Option<Cow<'a, str>> {
    attrs(raw).into_iter().find(|(n, _, _)| local(n).eq_ignore_ascii_case(name)).map(|(_, v, _)| decode(v))
}

/// Thuộc tính đúng tên (kể cả tiền tố), vd. «epub:type».
pub fn attr_exact<'a>(raw: &'a str, name: &str) -> Option<Cow<'a, str>> {
    attrs(raw).into_iter().find(|(n, _, _)| n.eq_ignore_ascii_case(name)).map(|(_, v, _)| decode(v))
}

/// Giải thực thể: `&lt;` `&#123;` `&#x1F600;` và thực thể có tên của HTML. Thực thể lạ giữ nguyên.
pub fn decode(s: &str) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        match parse_ref(tail) {
            Some((c, len)) => {
                out.push(c);
                rest = &tail[len..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// `&…;` ở đầu `s` → (ký tự, độ dài). None = không phải tham chiếu hợp lệ.
pub fn parse_ref(s: &str) -> Option<(char, usize)> {
    let end = s[1..].find(|c: char| c == ';' || c == '&' || c == '<' || c.is_whitespace()).map(|i| i + 1)?;
    if s.as_bytes().get(end) != Some(&b';') || end > 40 {
        return None;
    }
    let name = &s[1..end];
    let c = if let Some(n) = name.strip_prefix('#') {
        let v = if let Some(h) = n.strip_prefix('x').or_else(|| n.strip_prefix('X')) { u32::from_str_radix(h, 16).ok()? } else { n.parse().ok()? };
        char::from_u32(v)?
    } else {
        char::from_u32(entity(name)?)?
    };
    Some((c, end + 1))
}

/// Mã của thực thể có tên (HTML 4 + `apos`).
pub fn entity(name: &str) -> Option<u32> {
    ENTITIES.binary_search_by(|(n, _)| n.cmp(&name)).ok().map(|i| ENTITIES[i].1)
}

/// Thực thể XML định sẵn.
pub fn xml_entity(name: &str) -> bool {
    matches!(name, "lt" | "gt" | "amp" | "quot" | "apos")
}

/// Bảng thực thể HTML 4.01 (+ apos), xếp theo tên (tìm nhị phân).
static ENTITIES: &[(&str, u32)] = &[
    ("AElig", 198), ("Aacute", 193), ("Acirc", 194), ("Agrave", 192), ("Alpha", 913), ("Aring", 197), ("Atilde", 195),
    ("Auml", 196), ("Beta", 914), ("Ccedil", 199), ("Chi", 935), ("Dagger", 8225), ("Delta", 916), ("ETH", 208),
    ("Eacute", 201), ("Ecirc", 202), ("Egrave", 200), ("Epsilon", 917), ("Eta", 919), ("Euml", 203), ("Gamma", 915),
    ("Iacute", 205), ("Icirc", 206), ("Igrave", 204), ("Iota", 921), ("Iuml", 207), ("Kappa", 922), ("Lambda", 923),
    ("Mu", 924), ("Ntilde", 209), ("Nu", 925), ("OElig", 338), ("Oacute", 211), ("Ocirc", 212), ("Ograve", 210),
    ("Omega", 937), ("Omicron", 927), ("Oslash", 216), ("Otilde", 213), ("Ouml", 214), ("Phi", 934), ("Pi", 928),
    ("Prime", 8243), ("Psi", 936), ("Rho", 929), ("Scaron", 352), ("Sigma", 931), ("THORN", 222), ("Tau", 932),
    ("Theta", 920), ("Uacute", 218), ("Ucirc", 219), ("Ugrave", 217), ("Upsilon", 933), ("Uuml", 220), ("Xi", 926),
    ("Yacute", 221), ("Yuml", 376), ("Zeta", 918), ("aacute", 225), ("acirc", 226), ("acute", 180), ("aelig", 230),
    ("agrave", 224), ("alefsym", 8501), ("alpha", 945), ("amp", 38), ("and", 8743), ("ang", 8736), ("apos", 39),
    ("aring", 229), ("asymp", 8776), ("atilde", 227), ("auml", 228), ("bdquo", 8222), ("beta", 946), ("brvbar", 166),
    ("bull", 8226), ("cap", 8745), ("ccedil", 231), ("cedil", 184), ("cent", 162), ("chi", 967), ("circ", 710),
    ("clubs", 9827), ("cong", 8773), ("copy", 169), ("crarr", 8629), ("cup", 8746), ("curren", 164), ("dArr", 8659),
    ("dagger", 8224), ("darr", 8595), ("deg", 176), ("delta", 948), ("diams", 9830), ("divide", 247), ("eacute", 233),
    ("ecirc", 234), ("egrave", 232), ("empty", 8709), ("emsp", 8195), ("ensp", 8194), ("epsilon", 949), ("equiv", 8801),
    ("eta", 951), ("eth", 240), ("euml", 235), ("euro", 8364), ("exist", 8707), ("fnof", 402), ("forall", 8704),
    ("frac12", 189), ("frac14", 188), ("frac34", 190), ("frasl", 8260), ("gamma", 947), ("ge", 8805), ("gt", 62),
    ("hArr", 8660), ("harr", 8596), ("hearts", 9829), ("hellip", 8230), ("iacute", 237), ("icirc", 238), ("iexcl", 161),
    ("igrave", 236), ("image", 8465), ("infin", 8734), ("int", 8747), ("iota", 953), ("iquest", 191), ("isin", 8712),
    ("iuml", 239), ("kappa", 954), ("lArr", 8656), ("lambda", 955), ("lang", 9001), ("laquo", 171), ("larr", 8592),
    ("lceil", 8968), ("ldquo", 8220), ("le", 8804), ("lfloor", 8970), ("lowast", 8727), ("loz", 9674), ("lrm", 8206),
    ("lsaquo", 8249), ("lsquo", 8216), ("lt", 60), ("macr", 175), ("mdash", 8212), ("micro", 181), ("middot", 183),
    ("minus", 8722), ("mu", 956), ("nabla", 8711), ("nbsp", 160), ("ndash", 8211), ("ne", 8800), ("ni", 8715),
    ("not", 172), ("notin", 8713), ("nsub", 8836), ("ntilde", 241), ("nu", 957), ("oacute", 243), ("ocirc", 244),
    ("oelig", 339), ("ograve", 242), ("oline", 8254), ("omega", 969), ("omicron", 959), ("oplus", 8853), ("or", 8744),
    ("ordf", 170), ("ordm", 186), ("oslash", 248), ("otilde", 245), ("otimes", 8855), ("ouml", 246), ("para", 182),
    ("part", 8706), ("permil", 8240), ("perp", 8869), ("phi", 966), ("pi", 960), ("piv", 982), ("plusmn", 177),
    ("pound", 163), ("prime", 8242), ("prod", 8719), ("prop", 8733), ("psi", 968), ("quot", 34), ("rArr", 8658),
    ("radic", 8730), ("rang", 9002), ("raquo", 187), ("rarr", 8594), ("rceil", 8969), ("rdquo", 8221), ("real", 8476),
    ("reg", 174), ("rfloor", 8971), ("rho", 961), ("rlm", 8207), ("rsaquo", 8250), ("rsquo", 8217), ("sbquo", 8218),
    ("scaron", 353), ("sdot", 8901), ("sect", 167), ("shy", 173), ("sigma", 963), ("sigmaf", 962), ("sim", 8764),
    ("spades", 9824), ("sub", 8834), ("sube", 8838), ("sum", 8721), ("sup", 8835), ("sup1", 185), ("sup2", 178),
    ("sup3", 179), ("supe", 8839), ("szlig", 223), ("tau", 964), ("there4", 8756), ("theta", 952), ("thetasym", 977),
    ("thinsp", 8201), ("thorn", 254), ("tilde", 732), ("times", 215), ("trade", 8482), ("uArr", 8657), ("uacute", 250),
    ("uarr", 8593), ("ucirc", 251), ("ugrave", 249), ("uml", 168), ("upsih", 978), ("upsilon", 965), ("uuml", 252),
    ("weierp", 8472), ("xi", 958), ("yacute", 253), ("yen", 165), ("yuml", 255), ("zeta", 950), ("zwj", 8205),
    ("zwnj", 8204),
];

/// Đổi thực thể có tên của HTML thành tham chiếu số (`&nbsp;` → `&#160;`), `&` lạc thành `&amp;` — để chương đọc được bằng bộ
/// đọc XML của WebKit (không có DTD XHTML thì `&nbsp;` là lỗi). Thực thể XML định sẵn và tham chiếu số giữ nguyên.
pub fn fix_refs(s: &str) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 16);
    let mut rest = s;
    let mut changed = false;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail[1..].find(|c: char| c == ';' || c == '&' || c == '<' || c.is_whitespace()).map(|j| j + 1);
        let name = end.filter(|&e| tail.as_bytes().get(e) == Some(&b';') && e <= 40).map(|e| &tail[1..e]);
        match name {
            Some(n) if xml_entity(n) || (n.starts_with('#') && parse_ref(tail).is_some()) => {
                out.push_str(&tail[..n.len() + 2]);
                rest = &tail[n.len() + 2..];
            }
            Some(n) if entity(n).is_some() => {
                out.push_str(&format!("&#{};", entity(n).unwrap_or(63)));
                rest = &tail[n.len() + 2..];
                changed = true;
            }
            _ => {
                out.push_str("&amp;");
                rest = &tail[1..];
                changed = true;
            }
        }
    }
    out.push_str(rest);
    if changed { Cow::Owned(out) } else { Cow::Borrowed(s) }
}

/// Chữ thuần của một đoạn XML / HTML (bỏ thẻ, giải thực thể, gộp khoảng trắng) — tên mục lục, tên chương.
pub fn plain_text(s: &str) -> String {
    let mut out = String::new();
    for (t, _) in tokens(s) {
        match t {
            Tok::Text(x) => out.push_str(&decode(x)),
            Tok::CData(x) => out.push_str(x),
            Tok::Start { name, .. } | Tok::End { name } if matches!(name.to_ascii_lowercase().as_str(), "br" | "p" | "div") => out.push(' '),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Thẻ không có phần thân trong HTML (không bao giờ có thẻ đóng).
pub fn void_element(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "br" | "img" | "hr" | "meta" | "link" | "input" | "area" | "base" | "col" | "embed" | "param" | "source" | "track" | "wbr"
    )
}

/// Kiểm cú pháp XML (đủ dùng cho chương sách): thẻ mở / đóng khớp đúng tên, thuộc tính có nháy và không trùng, không có `<` lạc,
/// mọi `&` là tham chiếu hợp lệ (thực thể XML định sẵn / số). Trả lý do đầu tiên khi sai.
pub fn check_xml(s: &str) -> Result<(), String> {
    let mut stack: Vec<&str> = Vec::new();
    let mut root_seen = false;
    for (t, r) in tokens(s) {
        match t {
            Tok::Start { name, attrs: a, empty } => {
                if stack.is_empty() && root_seen {
                    return Err(format!("nhiều phần tử gốc ở byte {}", r.start));
                }
                let list = attrs(a);
                for (i, (n, v, quoted)) in list.iter().enumerate() {
                    if !quoted {
                        return Err(format!("thuộc tính «{n}» không có nháy ở byte {}", r.start));
                    }
                    if list[..i].iter().any(|(m, _, _)| m == n) {
                        return Err(format!("thuộc tính «{n}» lặp ở byte {}", r.start));
                    }
                    if v.contains('<') || bad_amp(v) {
                        return Err(format!("giá trị thuộc tính «{n}» sai ở byte {}", r.start));
                    }
                }
                if !empty {
                    stack.push(name);
                } else if stack.is_empty() {
                    root_seen = true;
                }
            }
            Tok::End { name } => match stack.pop() {
                Some(open) if open == name => {
                    if stack.is_empty() {
                        root_seen = true;
                    }
                }
                Some(open) => return Err(format!("thẻ đóng </{name}> không khớp <{open}> ở byte {}", r.start)),
                None => return Err(format!("thẻ đóng </{name}> thừa ở byte {}", r.start)),
            },
            Tok::Text(x) => {
                if bad_amp(x) {
                    return Err(format!("«&» lạc ở byte {}", r.start));
                }
                if stack.is_empty() && !x.trim().is_empty() {
                    return Err(format!("chữ ngoài phần tử gốc ở byte {}", r.start));
                }
            }
            Tok::Stray => return Err(format!("«<» lạc ở byte {}", r.start)),
            _ => {}
        }
    }
    if let Some(open) = stack.last() {
        return Err(format!("thiếu thẻ đóng </{open}>"));
    }
    if !root_seen {
        return Err("không có phần tử gốc".into());
    }
    Ok(())
}

/// Có `&` không mở một tham chiếu XML hợp lệ (thực thể XML định sẵn / số).
fn bad_amp(s: &str) -> bool {
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        let tail = &rest[i..];
        let ok = match tail[1..].find(';') {
            Some(e) if e < 40 => {
                let n = &tail[1..e + 1];
                xml_entity(n) || (n.starts_with('#') && parse_ref(tail).is_some())
            }
            _ => false,
        };
        if !ok {
            return true;
        }
        rest = &tail[1..];
    }
    false
}

/// Giải mã byte của một tệp chữ: UTF-8 (bỏ BOM), UTF-16 (có BOM); không phải UTF-8 hợp lệ thì coi là Windows-1252. Trả (chuỗi,
/// có phải đổi mã không).
pub fn decode_bytes(b: &[u8]) -> (String, bool) {
    if let Some(r) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return (String::from_utf8_lossy(r).into_owned(), false);
    }
    if b.len() >= 2 && (b[..2] == [0xFF, 0xFE] || b[..2] == [0xFE, 0xFF]) {
        let le = b[0] == 0xFF;
        let units: Vec<u16> =
            b[2..].chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        return (String::from_utf16_lossy(&units), true);
    }
    match std::str::from_utf8(b) {
        Ok(s) => (s.to_string(), false),
        Err(_) => (b.iter().map(|&c| cp1252(c)).collect(), true),
    }
}

/// Một byte Windows-1252 → ký tự.
pub fn cp1252(c: u8) -> char {
    const HI: [u16; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D, 0x017D, 0x8F,
        0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    if (0x80..0xA0).contains(&c) {
        char::from_u32(HI[(c - 0x80) as usize] as u32).unwrap_or('?')
    } else {
        c as char
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_table_sorted() {
        assert!(ENTITIES.windows(2).all(|w| w[0].0 < w[1].0), "bảng thực thể phải xếp theo tên");
        assert_eq!(entity("nbsp"), Some(160));
        assert_eq!(entity("Aacute"), Some(193));
        assert_eq!(entity("zzz"), None);
    }

    #[test]
    fn tokenize_keeps_positions() {
        let s = r#"<?xml version="1.0"?><html xmlns="x"><head><title>A &amp; B</title></head><body><p class='a'>Hi<br/></p><!-- c --></body></html>"#;
        let toks: Vec<_> = tokens(s).collect();
        assert!(matches!(toks[0].0, Tok::Pi(_)));
        assert_eq!(toks[1].0, Tok::Start { name: "html", attrs: r#" xmlns="x""#, empty: false });
        let br = toks.iter().find(|(t, _)| matches!(t, Tok::Start { name: "br", .. })).unwrap();
        assert_eq!(br.0, Tok::Start { name: "br", attrs: "", empty: true });
        assert_eq!(&s[br.1.clone()], "<br/>");
        let joined: String = toks.iter().map(|(_, r)| &s[r.clone()]).collect();
        assert_eq!(joined, s);
        assert_eq!(check_xml(s), Ok(()));
    }

    #[test]
    fn script_content_is_raw() {
        let s = "<html><head><script>if (a < b && c) {}</script></head><body/></html>";
        let toks: Vec<_> = tokens(s).map(|(t, _)| t).collect();
        assert!(toks.contains(&Tok::Text("if (a < b && c) {}")));
    }

    #[test]
    fn attrs_and_decode() {
        let a = attrs(r#" href="ch1.xhtml#n1" class=big data-x='a"b' checked"#);
        assert_eq!(a, vec![("href", "ch1.xhtml#n1", true), ("class", "big", false), ("data-x", "a\"b", true), ("checked", "", false)]);
        assert_eq!(attr(r#" epub:type="noteref" HREF="a&amp;b""#, "href").as_deref(), Some("a&b"));
        assert_eq!(attr_exact(r#" epub:type="noteref""#, "epub:type").as_deref(), Some("noteref"));
        assert_eq!(decode("Tr&#7847;n &amp; &nbsp;x &#x1EA0; &bogus;"), "Trần & \u{a0}x Ạ &bogus;");
    }

    #[test]
    fn fix_refs_for_xml() {
        assert_eq!(fix_refs("a&nbsp;b &amp; c &#160; d & e &copy;"), "a&#160;b &amp; c &#160; d &amp; e &#169;");
        assert!(matches!(fix_refs("không có gì"), Cow::Borrowed(_)));
    }

    #[test]
    fn xml_checks() {
        assert!(check_xml("<a><b></a></b>").is_err());
        assert!(check_xml("<a><br></a>").is_err());
        assert!(check_xml("<a x=1></a>").is_err());
        assert!(check_xml("<a x='1' x='2'></a>").is_err());
        assert!(check_xml("<a>&nbsp;</a>").is_err());
        assert!(check_xml("<a>1 < 2</a>").is_err());
        assert_eq!(check_xml("<a>&#160;&lt;<b/></a>"), Ok(()));
        // sách vnthuquan: <br></br>, <meta ...></meta>
        assert_eq!(check_xml("<html><head><meta http-equiv='x'></meta></head><body><br clear='none'></br></body></html>"), Ok(()));
    }

    #[test]
    fn plain_and_bytes() {
        assert_eq!(plain_text("<span>Chương&nbsp;1</span><br/>  Một Đêm "), "Chương 1 Một Đêm");
        assert_eq!(decode_bytes(b"\xEF\xBB\xBFabc").0, "abc");
        assert_eq!(decode_bytes(&[0x93, b'a', 0x94]).0, "\u{201c}a\u{201d}");
        let u16le: Vec<u8> = [0xFF, 0xFE, b'H', 0, 0xA1, 0x1E].to_vec();
        assert_eq!(decode_bytes(&u16le).0, "Hạ");
    }
}
