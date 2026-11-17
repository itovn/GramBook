//! Mã chữ: cp1252 → UTF-8, sửa UTF-8 hỏng, thực thể HTML, thoát ký tự, làm sạch nhãn.
//!
//! Sách MOBI cp1252 tiếng Việt (kho thử: 45 cuốn) ghi chữ Việt bằng byte cp1252 cho các chữ có trong Latin-1 (à, á, â, ê,
//! ô…) và thực thể số cho phần còn lại (`&#273;` = đ, `&#7841;` = ạ, có cuốn dùng dấu tổ hợp `a&#769;`). Chỉ cần đổi
//! byte cp1252 sang UTF-8, giữ nguyên thực thể — WebKit tự hiểu.

/// Mã chữ của sách (trường «text encoding» của đầu MOBI).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Encoding {
    /// Windows-1252 (Mobipocket cũ).
    Cp1252,
    /// UTF-8 (65001). Byte không thành UTF-8 hợp lệ được đọc như cp1252 (sách trộn mã).
    Utf8,
}

impl Encoding {
    /// 65001 → UTF-8, 1252 → cp1252; mã lạ: đọc như UTF-8 (byte hỏng → cp1252) — đúng cho cả hai kiểu thường gặp.
    pub fn from_codepage(cp: u32) -> Encoding {
        match cp {
            1252 => Encoding::Cp1252,
            _ => Encoding::Utf8,
        }
    }
}

/// Byte 0x80..0x9F của cp1252 (chỗ trống giữ mã C1 như WHATWG).
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
];

pub(crate) fn cp1252_char(b: u8) -> char {
    match b {
        0x80..=0x9F => CP1252_HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// Ký tự điều khiển XML cấm (giữ tab, xuống dòng).
fn bad_control(b: u8) -> bool {
    b < 0x20 && b != b'\t' && b != b'\n' && b != b'\r'
}

fn push_char(out: &mut Vec<u8>, c: char) {
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
}

/// Thêm `src` (mã `enc`) vào `out` dạng UTF-8 sạch: bỏ ký tự điều khiển và U+FFFE / U+FFFF (XML cấm), byte UTF-8 hỏng
/// đọc như cp1252.
pub(crate) fn push_decoded(out: &mut Vec<u8>, src: &[u8], enc: Encoding) {
    match enc {
        Encoding::Cp1252 => {
            for &b in src {
                if b >= 0x80 {
                    push_char(out, cp1252_char(b));
                } else if !bad_control(b) {
                    out.push(b);
                }
            }
        }
        Encoding::Utf8 => {
            let mut rest = src;
            loop {
                match std::str::from_utf8(rest) {
                    Ok(_) => {
                        push_clean(out, rest);
                        return;
                    }
                    Err(e) => {
                        let good = e.valid_up_to();
                        push_clean(out, &rest[..good]);
                        let bad = e.error_len().unwrap_or(rest.len() - good).max(1);
                        for &b in &rest[good..(good + bad).min(rest.len())] {
                            if b >= 0x80 {
                                push_char(out, cp1252_char(b));
                            } else if !bad_control(b) {
                                out.push(b);
                            }
                        }
                        rest = &rest[(good + bad).min(rest.len())..];
                    }
                }
            }
        }
    }
}

/// Chép phần UTF-8 hợp lệ, bỏ ký tự điều khiển và U+FFFE / U+FFFF.
fn push_clean(out: &mut Vec<u8>, s: &[u8]) {
    let mut start = 0;
    let mut i = 0;
    while i < s.len() {
        let b = s[i];
        if bad_control(b) {
            out.extend_from_slice(&s[start..i]);
            i += 1;
            start = i;
        } else if b == 0xEF
            && s.get(i + 1) == Some(&0xBF)
            && matches!(s.get(i + 2), Some(&0xBE) | Some(&0xBF))
        {
            out.extend_from_slice(&s[start..i]);
            i += 3;
            start = i;
        } else {
            i += 1;
        }
    }
    out.extend_from_slice(&s[start..]);
}

/// Chuỗi từ byte (mã `enc`).
pub(crate) fn decode_str(b: &[u8], enc: Encoding) -> String {
    let mut v = Vec::with_capacity(b.len());
    push_decoded(&mut v, b, enc);
    String::from_utf8(v).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Bản UTF-8 sạch của `src` (đã là UTF-8 hoặc cp1252 theo `enc`).
pub(crate) fn clean_bytes(src: &[u8], enc: Encoding) -> Vec<u8> {
    let mut v = Vec::with_capacity(src.len() + src.len() / 16);
    push_decoded(&mut v, src, enc);
    v
}

/// Tên thực thể Latin-1 (U+00A0..U+00FF theo thứ tự).
const LATIN1_ENTITIES: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf",
    "laquo", "not", "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro",
    "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest",
    "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute",
    "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute",
    "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute",
    "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil",
    "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde",
    "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute", "ucirc",
    "uuml", "yacute", "thorn", "yuml",
];

/// Thực thể có tên khác hay gặp.
const OTHER_ENTITIES: [(&str, char); 30] = [
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("ndash", '\u{2013}'),
    ("mdash", '\u{2014}'),
    ("lsquo", '\u{2018}'),
    ("rsquo", '\u{2019}'),
    ("sbquo", '\u{201A}'),
    ("ldquo", '\u{201C}'),
    ("rdquo", '\u{201D}'),
    ("bdquo", '\u{201E}'),
    ("hellip", '\u{2026}'),
    ("bull", '\u{2022}'),
    ("euro", '\u{20AC}'),
    ("trade", '\u{2122}'),
    ("dagger", '\u{2020}'),
    ("Dagger", '\u{2021}'),
    ("permil", '\u{2030}'),
    ("lsaquo", '\u{2039}'),
    ("rsaquo", '\u{203A}'),
    ("OElig", '\u{152}'),
    ("oelig", '\u{153}'),
    ("Scaron", '\u{160}'),
    ("scaron", '\u{161}'),
    ("Yuml", '\u{178}'),
    ("ensp", '\u{2002}'),
    ("emsp", '\u{2003}'),
    ("thinsp", '\u{2009}'),
];

fn named_entity(name: &str) -> Option<char> {
    if let Some(i) = LATIN1_ENTITIES.iter().position(|&n| n == name) {
        return char::from_u32(0xA0 + i as u32);
    }
    OTHER_ENTITIES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|&(_, c)| c)
}

/// Giải thực thể HTML (`&#273;`, `&#x111;`, `&amp;`, `&agrave;`…). Thực thể lạ giữ nguyên.
pub(crate) fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        let end = tail
            .char_indices()
            .take(32)
            .find(|&(_, c)| c == ';' || c == '&' || c.is_whitespace() || c == '<');
        let decoded = match end {
            Some((j, ';')) => {
                let name = &tail[..j];
                let c = if let Some(num) = name.strip_prefix('#') {
                    let v = match num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                        Some(h) => u32::from_str_radix(h, 16).ok(),
                        None => num.parse::<u32>().ok(),
                    };
                    v.and_then(char::from_u32).filter(|&c| c != '\0')
                } else {
                    named_entity(name)
                };
                c.map(|c| (c, j + 1))
            }
            _ => None,
        };
        match decoded {
            Some((c, used)) => {
                out.push(c);
                rest = &tail[used..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Thoát chữ để đặt trong HTML / XML (cả trong giá trị thuộc tính).
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Bỏ thẻ HTML (giữ chữ).
pub(crate) fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Nhãn gọn: giải thực thể, gộp khoảng trắng, cắt ≤ `max` ký tự.
pub(crate) fn clean_label(s: &str, max: usize) -> String {
    let d = decode_entities(s);
    let mut out = String::with_capacity(d.len().min(max * 4));
    let mut n = 0;
    for w in d
        .split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|w| !w.is_empty())
    {
        if n > 0 {
            if n + 1 > max {
                break;
            }
            out.push(' ');
            n += 1;
        }
        for c in w.chars() {
            if n >= max {
                out.push('…');
                return out;
            }
            out.push(c);
            n += 1;
        }
    }
    out
}

/// Chữ (UTF-8 hoặc cp1252 + thực thể) có vẻ là tiếng Việt: đếm đ / ư / ơ / ă và chữ có dấu khối U+1EA0..U+1EF9, dạng
/// UTF-8 lẫn thực thể số, trong 256 KB đầu. Dùng khi sách không ghi ngôn ngữ (EXTH 524) — Mobipocket Creator mặc định
/// ghi mã vùng tiếng Anh cho cả sách Việt.
pub(crate) fn looks_vietnamese(t: &[u8]) -> bool {
    let t = &t[..t.len().min(256 * 1024)];
    let mut n = 0;
    let mut i = 0;
    while i + 1 < t.len() && n < 20 {
        let (b, c) = (t[i], t[i + 1]);
        match b {
            0xC4 if matches!(c, 0x82 | 0x83 | 0x90 | 0x91) => n += 1,
            0xC6 if matches!(c, 0xA0 | 0xA1 | 0xAF | 0xB0) => n += 1,
            0xE1 if matches!(c, 0xBA | 0xBB) => n += 1,
            b'&' if c == b'#' => {
                let digits: Vec<u8> = t[i + 2..]
                    .iter()
                    .take(5)
                    .take_while(|d| d.is_ascii_digit())
                    .copied()
                    .collect();
                let v: u32 = std::str::from_utf8(&digits)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                if matches!(
                    v,
                    258 | 259 | 272 | 273 | 416 | 417 | 431 | 432 | 7840..=7929
                ) {
                    n += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    n >= 20
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nhan_tieng_viet() {
        assert!(looks_vietnamese(
            "Đường đi khó không khó vì ngăn sông cách núi, mà khó vì lòng người ngại núi e sông. "
                .repeat(3)
                .as_bytes()
        ));
        assert!(looks_vietnamese(b"&#272;&#432;&#7901;ng &#273;i kh\xf3 &#7903; &#273;&#226;u &#273;&#7871;n &#273;&#7845;y".repeat(4).as_slice()));
        assert!(!looks_vietnamese(
            b"The quick brown fox jumps over the lazy dog. Cafe, naive, resume."
        ));
    }

    #[test]
    fn cp1252_sang_utf8() {
        let mut o = Vec::new();
        push_decoded(&mut o, b"Ti\xean \x93x\x94 \x80\x00\x01", Encoding::Cp1252);
        assert_eq!(String::from_utf8(o).unwrap(), "Tiên \u{201C}x\u{201D} €");
    }

    #[test]
    fn utf8_hong_doc_nhu_cp1252() {
        let mut o = Vec::new();
        push_decoded(&mut o, "Việt ".as_bytes(), Encoding::Utf8);
        push_decoded(
            &mut o,
            b"caf\xe9 \xef\xbf\xbe\x07ok \xe1\xbb",
            Encoding::Utf8,
        );
        assert_eq!(String::from_utf8(o).unwrap(), "Việt café ok á»");
    }

    #[test]
    fn thuc_the() {
        assert_eq!(
            decode_entities("&#272;&#7889;i &#x111; &agrave; &amp;&lt; &bogus; & x &#0;"),
            "Đối đ à &< &bogus; & x &#0;"
        );
        assert_eq!(escape("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
        assert_eq!(strip_tags("<p>Một <b>hai</b></p>ba"), " Một  hai  ba");
        assert_eq!(
            clean_label("  Ch&#432;&#417;ng\n 1 &nbsp; ", 100),
            "Chương 1"
        );
        assert_eq!(clean_label("abc def", 5), "abc d…");
    }
}
