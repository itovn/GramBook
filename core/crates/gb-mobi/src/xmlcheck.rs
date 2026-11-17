//! Kiểm tra nhanh một tài liệu có đúng cú pháp XML không — đủ để biết WebKit mở được dạng `application/xhtml+xml` (sai một
//! lỗi là trang vàng báo lỗi, mất chữ). Kiểm: thẻ đóng / mở khớp, một phần tử gốc, tên hợp lệ, giá trị thuộc tính có nháy,
//! không trùng thuộc tính, `&` là thực thể hợp lệ, không có `<` trong giá trị, `--` trong chú thích, UTF-8 hợp lệ, ký tự
//! điều khiển. Không kiểm DTD / không gian tên.
//!
//! Tệp tự đứng (không dùng gì khác của crate) để bài thử tích hợp nhúng được bằng `#[path]`.

#![allow(dead_code)]

/// `Ok(())` nếu `doc` đúng cú pháp XML; lỗi kèm vị trí byte.
pub fn check(doc: &[u8]) -> Result<(), String> {
    if let Err(e) = std::str::from_utf8(doc) {
        return Err(format!("UTF-8 hỏng ở {}", e.valid_up_to()));
    }
    let n = doc.len();
    let mut i = if doc.starts_with(b"\xEF\xBB\xBF") {
        3
    } else {
        0
    };
    let mut stack: Vec<&[u8]> = Vec::new();
    let mut root_done = false;
    let mut seen_root = false;
    // DOCTYPE XHTML: WebKit cho dùng thực thể HTML có tên (&nbsp;…).
    let mut html_entities = false;
    let err = |what: &str, at: usize| Err(format!("{what} ở {at}"));
    while i < n {
        if doc[i] != b'<' {
            let end = doc[i..]
                .iter()
                .position(|&b| b == b'<')
                .map_or(n, |k| i + k);
            let text = &doc[i..end];
            if stack.is_empty()
                && text
                    .iter()
                    .any(|&b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                return err("chữ ngoài phần tử gốc", i);
            }
            check_chars(text, i, html_entities)?;
            i = end;
            continue;
        }
        let rest = &doc[i..];
        if rest.starts_with(b"<!--") {
            let Some(p) = find(doc, i + 4, b"-->") else {
                return err("chú thích không đóng", i);
            };
            if find(&doc[..p], i + 4, b"--").is_some() {
                return err("'--' trong chú thích", i);
            }
            i = p + 3;
        } else if rest.starts_with(b"<![CDATA[") {
            if stack.is_empty() {
                return err("CDATA ngoài phần tử gốc", i);
            }
            let Some(p) = find(doc, i + 9, b"]]>") else {
                return err("CDATA không đóng", i);
            };
            i = p + 3;
        } else if rest.starts_with(b"<!DOCTYPE") || rest.starts_with(b"<!doctype") {
            if seen_root {
                return err("DOCTYPE sau phần tử gốc", i);
            }
            let Some(end) = doctype_end(doc, i) else {
                return err("DOCTYPE không đóng", i);
            };
            let d = &doc[i..end];
            html_entities =
                find(d, 0, b"//DTD XHTML").is_some() || find(d, 0, b"//DTD XHTML Mobile").is_some();
            i = end;
        } else if rest.starts_with(b"<?") {
            let Some(p) = find(doc, i + 2, b"?>") else {
                return err("<? không đóng", i);
            };
            i = p + 2;
        } else if rest.starts_with(b"</") {
            let (name, j) = read_name(doc, i + 2);
            if name.is_empty() {
                return err("tên thẻ đóng hỏng", i);
            }
            let j = skip_ws(doc, j);
            if doc.get(j) != Some(&b'>') {
                return err("thẻ đóng hỏng", i);
            }
            match stack.pop() {
                Some(open) if open == name => {}
                _ => return err("thẻ đóng không khớp", i),
            }
            if stack.is_empty() {
                root_done = true;
            }
            i = j + 1;
        } else {
            let (name, mut j) = read_name(doc, i + 1);
            if name.is_empty() {
                return err("'<' lạc", i);
            }
            if root_done {
                return err("nhiều phần tử gốc", i);
            }
            seen_root = true;
            let mut names: Vec<&[u8]> = Vec::new();
            let self_closing;
            loop {
                let k = skip_ws(doc, j);
                match doc.get(k) {
                    None => return err("thẻ không đóng", i),
                    Some(b'>') => {
                        self_closing = false;
                        j = k + 1;
                        break;
                    }
                    Some(b'/') => {
                        if doc.get(k + 1) != Some(&b'>') {
                            return err("'/' lạc trong thẻ", k);
                        }
                        self_closing = true;
                        j = k + 2;
                        break;
                    }
                    _ => {}
                }
                if k == j {
                    return err("thiếu khoảng trắng trước thuộc tính", k);
                }
                let (an, a_end) = read_name(doc, k);
                if an.is_empty() {
                    return err("tên thuộc tính hỏng", k);
                }
                if names.contains(&an) {
                    return err("trùng thuộc tính", k);
                }
                names.push(an);
                let e = skip_ws(doc, a_end);
                if doc.get(e) != Some(&b'=') {
                    return err("thuộc tính thiếu giá trị", k);
                }
                let v = skip_ws(doc, e + 1);
                let q = match doc.get(v) {
                    Some(&q) if q == b'"' || q == b'\'' => q,
                    _ => return err("giá trị thuộc tính thiếu nháy", v),
                };
                let Some(off) = doc[v + 1..].iter().position(|&b| b == q) else {
                    return err("nháy không đóng", v);
                };
                let val = &doc[v + 1..v + 1 + off];
                if val.contains(&b'<') {
                    return err("'<' trong giá trị thuộc tính", v);
                }
                check_chars(val, v + 1, html_entities)?;
                j = v + 2 + off;
            }
            if !self_closing {
                stack.push(name);
            } else if stack.is_empty() {
                root_done = true;
            }
            i = j;
        }
    }
    if !stack.is_empty() {
        return Err(format!(
            "thiếu thẻ đóng </{}>",
            String::from_utf8_lossy(stack[stack.len() - 1])
        ));
    }
    if !seen_root {
        return Err("không có phần tử gốc".into());
    }
    Ok(())
}

fn find(s: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if from > s.len() || pat.is_empty() {
        return None;
    }
    s[from..]
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|p| from + p)
}

fn skip_ws(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    i
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b':' || b >= 0x80
}

fn is_name_char(b: u8) -> bool {
    is_name_start(b) || b.is_ascii_digit() || b == b'-' || b == b'.'
}

fn read_name(s: &[u8], i: usize) -> (&[u8], usize) {
    if !s.get(i).is_some_and(|&b| is_name_start(b)) {
        return (&[], i);
    }
    let mut j = i + 1;
    while j < s.len() && is_name_char(s[j]) {
        j += 1;
    }
    (&s[i..j], j)
}

/// Cuối DOCTYPE (sau `>`), bỏ qua tập con nội `[...]`.
fn doctype_end(s: &[u8], i: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut q = 0u8;
    for (k, &b) in s.iter().enumerate().skip(i) {
        if q != 0 {
            if b == q {
                q = 0;
            }
            continue;
        }
        match b {
            b'"' | b'\'' => q = b,
            b'[' => depth += 1,
            b']' => depth -= 1,
            b'>' if depth <= 0 => return Some(k + 1),
            _ => {}
        }
    }
    None
}

/// Kiểm chữ / giá trị: `&` phải là thực thể hợp lệ, không ký tự điều khiển, không `]]>`.
fn check_chars(t: &[u8], at: usize, html_entities: bool) -> Result<(), String> {
    let mut k = 0;
    while k < t.len() {
        let b = t[k];
        if b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r') {
            return Err(format!("ký tự điều khiển ở {}", at + k));
        }
        if b == b']' && t[k..].starts_with(b"]]>") {
            return Err(format!("']]>' trong chữ ở {}", at + k));
        }
        if b == b'&' {
            let Some(semi) = t[k + 1..].iter().take(40).position(|&c| c == b';') else {
                return Err(format!("'&' lạc ở {}", at + k));
            };
            let name = &t[k + 1..k + 1 + semi];
            let ok = match name {
                b"amp" | b"lt" | b"gt" | b"quot" | b"apos" => true,
                [b'#', b'x' | b'X', hex @ ..] => {
                    !hex.is_empty()
                        && hex.iter().all(|c| c.is_ascii_hexdigit())
                        && hex.len() <= 6
                        && valid_char(
                            u32::from_str_radix(std::str::from_utf8(hex).unwrap_or("0"), 16)
                                .unwrap_or(0),
                        )
                }
                [b'#', dec @ ..] => {
                    !dec.is_empty()
                        && dec.iter().all(|c| c.is_ascii_digit())
                        && dec.len() <= 7
                        && valid_char(std::str::from_utf8(dec).unwrap_or("0").parse().unwrap_or(0))
                }
                _ => {
                    html_entities
                        && !name.is_empty()
                        && name.iter().all(|c| c.is_ascii_alphanumeric())
                }
            };
            if !ok {
                return Err(format!(
                    "thực thể hỏng &{}; ở {}",
                    String::from_utf8_lossy(name),
                    at + k
                ));
            }
            k += semi + 2;
            continue;
        }
        k += 1;
    }
    Ok(())
}

fn valid_char(c: u32) -> bool {
    matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

#[cfg(test)]
mod tests {
    use super::check;

    #[test]
    fn dung_cu_phap() {
        let ok = [
            "<?xml version=\"1.0\"?>\n<!DOCTYPE html>\n<html><body a='1' b=\"x &amp; &#273; &#x111;\"><p>A<br/>B</p><!-- c --></body></html>\n",
            "<a><![CDATA[ <x> & ]]></a>",
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"x\"><html>&nbsp;</html>",
            "\u{feff}<r/>",
        ];
        for d in ok {
            assert_eq!(check(d.as_bytes()), Ok(()), "{d}");
        }
    }

    #[test]
    fn sai_cu_phap() {
        let bad = [
            "<html><p>A</html>",
            "<html><p a=1/></html>",
            "<html><p a></p></html>",
            "<html>&nbsp;</html>",
            "<html>a & b</html>",
            "<a></a><b></b>",
            "text<a/>",
            "<a b='1' b='2'/>",
            "<a b='<'/>",
            "<a><!-- x -- y --></a>",
            "<a>&#0;</a>",
            "<a>\u{1}</a>",
            "<a",
            "",
            "<a></b>",
            "<a>]]></a>",
            "<a b='1'c='2'/>",
        ];
        for d in bad {
            assert!(check(d.as_bytes()).is_err(), "{d}");
        }
        assert!(check(b"<a>\xff</a>").is_err());
    }
}
