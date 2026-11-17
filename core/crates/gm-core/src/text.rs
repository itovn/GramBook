//! Chữ: bỏ dấu để tìm / xếp (tiếng Việt và chữ Latinh có dấu), tách tên sách / tác giả từ tên tệp, chữ cái đầu cho thanh chữ
//! cái, định dạng sách theo đuôi / mime. Phần bỏ dấu / tìm / xếp chép từ GramMusic.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Chữ có dấu → chữ không dấu (đã viết thường). Không dùng bảng Unicode đầy đủ (thêm vài trăm KB vào tệp chạy): phủ tiếng
/// Việt + Latin-1 + Latin mở rộng A thường gặp.
const GROUPS: [(&str, char); 14] = [
    ("àáảãạăằắẳẵặâầấẩẫậäåāą", 'a'),
    ("çćč", 'c'),
    ("đď", 'd'),
    ("èéẻẽẹêềếểễệëēėęě", 'e'),
    ("ğ", 'g'),
    ("ìíỉĩịīįï", 'i'),
    ("łľ", 'l'),
    ("ñńň", 'n'),
    ("òóỏõọôồốổỗộơờớởỡợöøōő", 'o'),
    ("ŕř", 'r'),
    ("śšş", 's'),
    ("ť", 't'),
    ("ùúủũụưừứửữựüūůűų", 'u'),
    ("ỳýỷỹỵÿźżž", 'y'),
];

fn table() -> &'static HashMap<char, char> {
    static T: OnceLock<HashMap<char, char>> = OnceLock::new();
    T.get_or_init(|| {
        let mut m = HashMap::new();
        for (src, dst) in GROUPS {
            for c in src.chars() {
                // ź ż ž nằm chung dòng «y» cho gọn bảng — sửa lại ở đây
                m.insert(c, if matches!(c, 'ź' | 'ż' | 'ž') { 'z' } else { dst });
            }
        }
        m
    })
}

/// Viết thường + bỏ dấu: «Đêm Trăng Tình Yêu» → «dem trang tinh yeu». Dấu nối âm tách rời (U+0300–U+036F, tiếng Việt
/// gõ kiểu tổ hợp) bị bỏ; ß → ss, æ → ae, œ → oe.
pub fn fold(s: &str) -> String {
    let t = table();
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue;
        }
        for l in c.to_lowercase() {
            match l {
                'ß' => out.push_str("ss"),
                'æ' => out.push_str("ae"),
                'œ' => out.push_str("oe"),
                _ => out.push(*t.get(&l).unwrap_or(&l)),
            }
        }
    }
    out
}

/// Các từ cần tìm (đã bỏ dấu), bỏ khoảng trắng thừa.
pub fn query_words(q: &str) -> Vec<String> {
    fold(q).split_whitespace().map(str::to_string).collect()
}

/// `hay` (đã bỏ dấu) chứa đủ mọi từ.
pub fn matches(hay: &str, words: &[String]) -> bool {
    words.iter().all(|w| hay.contains(w.as_str()))
}

/// Khoá xếp theo tên: bỏ dấu, bỏ ký tự đầu không phải chữ / số (dấu ngoặc, «-», «.»), bỏ mạo từ «the » đầu tên tiếng Anh.
pub fn sort_key(s: &str) -> String {
    let f = fold(s);
    let t = f.trim_start_matches(|c: char| !c.is_alphanumeric());
    let t = t.strip_prefix("the ").unwrap_or(t);
    t.to_string()
}

/// Chữ cái cho thanh chữ cái: A–Z theo khoá xếp, còn lại «#».
pub fn letter(key: &str) -> char {
    match key.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => c.to_ascii_uppercase(),
        _ => '#',
    }
}

/// Bỏ cụm rác hay gặp trong tên tệp sách: «(z-lib.org)», «[Sách hay]», «(Ebook)»… — mọi cụm trong ngoặc vuông, và cụm trong
/// ngoặc tròn có chữ «lib», «ebook», «www», «.com», «.net», «.org», «.vn».
fn strip_junk(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(['[', '(']) {
        out.push_str(&rest[..i]);
        let close = if rest[i..].starts_with('[') { ']' } else { ')' };
        match rest[i..].find(close) {
            Some(j) => {
                let inner = &rest[i + 1..i + j];
                let low = inner.to_ascii_lowercase();
                let junk = close == ']'
                    || ["lib", "ebook", "www", ".com", ".net", ".org", ".vn", "epub", "mobi", "azw"].iter().any(|w| low.contains(w));
                if !junk {
                    out.push_str(&rest[i..i + j + 1]);
                }
                rest = &rest[i + j + 1..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Tên sách + tác giả theo tên tệp (bỏ đuôi, «_» thành dấu cách, bỏ cụm rác): dạng «Tên sách - Tác giả» (cách đặt tên của
/// vnthuquan và đa số kênh sách tiếng Việt) thì tách ở dấu « - » **cuối**. Mở sách xong lõi lấy tên / tác giả thật trong OPF.
/// Tên ghi trong sách (OPF / EXTH) hay tên theo tệp: sách làm vội hay ghi tên không dấu («13 bi quyet trong hoc tap») trong khi tên
/// tệp có dấu («13 Bí Quyết Trong Học Tập») — cùng chữ khi bỏ dấu thì lấy bản có dấu; khác chữ thì giữ tên trong sách.
pub fn prefer_accented(meta: &str, file: &str) -> String {
    let (m, f) = (meta.trim(), file.trim());
    let plain = |s: &str| fold(s) == s.to_lowercase();
    if !m.is_empty() && !f.is_empty() && plain(m) && !plain(f) && fold(m) == fold(f) {
        return f.to_string();
    }
    m.to_string()
}

pub fn title_from_file(name: &str) -> (String, Option<String>) {
    let low = name.to_ascii_lowercase();
    let stem = if low.ends_with(".fb2.zip") {
        &name[..name.len() - 8]
    } else {
        match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() && e.len() <= 5 => s,
            _ => name,
        }
    };
    let clean = strip_junk(&stem.replace('_', " "));
    for sep in [" - ", " – ", " — "] {
        if let Some((t, a)) = clean.rsplit_once(sep) {
            let (t, a) = (t.trim(), a.trim());
            if !t.is_empty() && !a.is_empty() {
                return (t.to_string(), Some(a.to_string()));
            }
        }
    }
    (clean.trim().to_string(), None)
}

/// Đuôi tệp viết thường (≤ 5 ký tự chữ / số), hoặc theo mime. `.fb2.zip` → «fbz».
pub fn ext_of(name: &str, mime: &str) -> String {
    if name.to_ascii_lowercase().ends_with(".fb2.zip") {
        return "fbz".into();
    }
    let from_name = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()));
    from_name.unwrap_or_else(|| {
        match mime {
            "application/epub+zip" => "epub",
            "application/x-mobipocket-ebook" => "mobi",
            "application/x-mobi8-ebook" => "azw3",
            "application/vnd.amazon.ebook" | "application/vnd.amazon.mobi8-ebook" => "azw",
            "application/pdf" => "pdf",
            "application/x-fictionbook+xml" | "application/x-fictionbook" => "fb2",
            "text/plain" => "txt",
            "application/msword" => "doc",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
            "application/rtf" | "text/rtf" => "rtf",
            "image/vnd.djvu" | "image/x-djvu" => "djvu",
            "application/vnd.comicbook+zip" | "application/x-cbz" => "cbz",
            "application/vnd.comicbook-rar" | "application/x-cbr" => "cbr",
            _ => "bin",
        }
        .to_string()
    })
}

/// Đuôi được coi là sách (THIET-KE 4.1).
pub const BOOK_EXTS: [&str; 16] =
    ["epub", "mobi", "azw3", "azw", "prc", "pdf", "fb2", "fbz", "txt", "djvu", "djv", "cbz", "cbr", "doc", "docx", "rtf"];

/// Là sách.
pub fn book_ext(ext: &str) -> bool {
    BOOK_EXTS.contains(&ext)
}

/// Lấy cả trong chat riêng với người: định dạng chắc chắn là sách (PDF / TXT / DOC trong chat riêng hay là giấy tờ — DECISIONS B4).
pub fn private_ok(ext: &str) -> bool {
    matches!(ext, "epub" | "mobi" | "azw3" | "azw" | "prc" | "fb2" | "fbz")
}

/// Màn đọc của app mở được (GĐ1: EPUB; MOBI / AZW3 khi bộ chuyển xong).
pub fn readable_ext(ext: &str) -> bool {
    ext == "epub" || kindle_ext(ext)
}

/// Sách Kindle: lõi chuyển bằng gb-mobi rồi phục vụ y như EPUB.
pub fn kindle_ext(ext: &str) -> bool {
    matches!(ext, "mobi" | "azw3" | "azw" | "prc")
}

/// Nhóm định dạng cho bộ lọc của Thư viện: epub | kindle | pdf | text | other.
pub fn format_group(ext: &str) -> &'static str {
    match ext {
        "epub" => "epub",
        "mobi" | "azw3" | "azw" | "prc" => "kindle",
        "pdf" => "pdf",
        "txt" | "fb2" | "fbz" | "doc" | "docx" | "rtf" => "text",
        _ => "other",
    }
}

/// Content-Type cho tệp trong sách (cổng nội bộ phục vụ UIWebView).
pub fn mime_for_path(path: &str) -> &'static str {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "xhtml" | "xht" => "application/xhtml+xml; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "ttf" => "application/x-font-ttf",
        "otf" => "application/x-font-opentype",
        "woff" => "application/font-woff",
        "ncx" | "xml" | "opf" => "application/xml; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "mp3" => "audio/mpeg",
        "mp4" | "m4v" => "video/mp4",
        "m4a" => "audio/x-m4a",
        _ => "application/octet-stream",
    }
}

/// Cắt chuỗi còn tối đa `n` ký tự (không cắt giữa chữ).
pub fn truncate_chars(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_vietnamese() {
        assert_eq!(fold("Đêm Trăng Tình Yêu"), "dem trang tinh yeu");
        assert_eq!(fold("Người Lạ Ơi"), "nguoi la oi");
        assert_eq!(fold("Cafe\u{301}"), "cafe");
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("ÀÁẢÃẠĂẰẮẲẴẶÂẦẤẨẪẬ"), "a".repeat(17));
        assert_eq!(fold("ÒÓỎÕỌÔỒỐỔỖỘƠỜỚỞỠỢ"), "o".repeat(17));
        assert_eq!(fold("ÙÚỦŨỤƯỪỨỬỮỰ ỲÝỶỸỴ Đ"), format!("{} {} d", "u".repeat(11), "y".repeat(5)));
    }

    #[test]
    fn search_words() {
        let hay = fold("Ai Cập Huyền Bí Paul Brunton");
        assert!(matches(&hay, &query_words("ai cap")));
        assert!(matches(&hay, &query_words("HUYỀN BÍ brunton")));
        assert!(!matches(&hay, &query_words("ba phut su that")));
    }

    #[test]
    fn sort_keys_and_letters() {
        assert_eq!(sort_key("The Old Man and the Sea"), "old man and the sea");
        assert_eq!(letter(&sort_key("Ánh Nắng")), 'A');
        assert_eq!(letter(&sort_key("Đời tôi")), 'D');
        assert_eq!(letter(&sort_key("28 Ngày Việt Nam")), '#');
        assert_eq!(letter(""), '#');
    }

    #[test]
    fn titles_from_files() {
        assert_eq!(title_from_file("Ai Cập Huyền Bí - Paul Brunton.epub"), ("Ai Cập Huyền Bí".into(), Some("Paul Brunton".into())));
        // tách ở dấu « - » cuối: tên sách có thể chứa « - »
        assert_eq!(
            title_from_file("Bóng - Tự truyện của một người - Đoan Trang.epub"),
            ("Bóng - Tự truyện của một người".into(), Some("Đoan Trang".into()))
        );
        assert_eq!(title_from_file("Nha_Gia_Kim_(z-lib.org).mobi"), ("Nha Gia Kim".into(), None));
        assert_eq!(title_from_file("[Sách hay] Đắc Nhân Tâm - Dale Carnegie.azw3"), ("Đắc Nhân Tâm".into(), Some("Dale Carnegie".into())));
        assert_eq!(title_from_file("Chiến tranh và hòa bình (tập 1).fb2.zip"), ("Chiến tranh và hòa bình (tập 1)".into(), None));
        assert_eq!(title_from_file("noext"), ("noext".into(), None));
    }

    #[test]
    fn exts_and_groups() {
        assert_eq!(ext_of("A.EPUB", ""), "epub");
        assert_eq!(ext_of("book.fb2.zip", "application/zip"), "fbz");
        assert_eq!(ext_of("", "application/x-mobipocket-ebook"), "mobi");
        assert_eq!(ext_of("weird.name.toolongext", "application/epub+zip"), "epub");
        assert!(book_ext("azw3") && !book_ext("mp3"));
        assert!(private_ok("epub") && private_ok("mobi") && !private_ok("pdf") && !private_ok("txt"));
        assert_eq!(format_group("prc"), "kindle");
        assert_eq!(mime_for_path("OEBPS/Text/ch01.xhtml"), "application/xhtml+xml; charset=utf-8");
        assert_eq!(truncate_chars("Đời tôi", 3), "Đời");
    }

    #[test]
    fn accented_title_wins() {
        assert_eq!(prefer_accented("13 bi quyet trong hoc tap", "13 Bí Quyết Trong Học Tập"), "13 Bí Quyết Trong Học Tập");
        assert_eq!(prefer_accented("Bích Huyết Kiếm", "Bich Huyet Kiem"), "Bích Huyết Kiếm");
        assert_eq!(prefer_accented("Tru Tien 001-258 tahoma", "Tru Tien"), "Tru Tien 001-258 tahoma");
        assert_eq!(prefer_accented("", "Bê Trọc"), "");
    }
}
