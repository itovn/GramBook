//! EPUB 2 / 3: `META-INF/container.xml` → OPF (thông tin, manifest, spine) → mục lục (EPUB 3 `nav`, EPUB 2 NCX; không có thì dựng
//! theo tên chương), bìa, font bị làm rối (IDPF / Adobe — gỡ khi phục vụ), sách có DRM. Cách làm như `BKEpubBookParser` của
//! iBooks (THIET-KE 1, 4.4). Kho mẫu: 303 + 147 sách (DECISIONS B9) — EPUB 2, NCX, `dc:language` hay ghi sai.

use std::collections::HashMap;

use crate::xml::{self, Tok};
use crate::zip::Zip;

/// Một mục spine (chương theo thứ tự đọc).
#[derive(Clone, Debug, PartialEq)]
pub struct SpineItem {
    /// Đường dẫn trong zip.
    pub path: String,
    pub mime: String,
    /// `linear="no"` (trang phụ: chú thích, bìa) — vẫn giữ trong spine, app có thể bỏ qua khi lật.
    pub linear: bool,
    pub size: u64,
}

/// Một mục lục.
#[derive(Clone, Debug, PartialEq)]
pub struct TocEntry {
    pub title: String,
    /// Đường dẫn trong zip (không có `#…`).
    pub path: String,
    pub frag: Option<String>,
    pub depth: u32,
}

/// Font bị làm rối.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Obf {
    /// IDPF: XOR 1040 byte đầu với SHA-1 của mã sách.
    Idpf([u8; 20]),
    /// Adobe: XOR 1024 byte đầu với 16 byte của UUID.
    Adobe([u8; 16]),
}

#[derive(Clone, Debug, Default)]
pub struct Epub {
    pub opf: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub identifier: String,
    pub series: Option<(String, f32)>,
    pub spine: Vec<SpineItem>,
    pub toc: Vec<TocEntry>,
    /// Ảnh bìa (đường dẫn trong zip).
    pub cover: Option<String>,
    /// Trang lật từ phải sang trái.
    pub rtl: bool,
    /// Bố cục cố định (`rendition:layout pre-paginated`).
    pub fixed: bool,
    /// Nội dung bị mã hoá thật (Adobe ADEPT…).
    pub drm: bool,
    pub obfuscated: HashMap<String, Obf>,
    /// Đường dẫn → mime (mọi mục manifest).
    pub manifest: HashMap<String, String>,
}

/// Thư mục của một đường dẫn trong zip («OEBPS/Text/ch1.xhtml» → «OEBPS/Text/»).
pub fn dir_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "",
    }
}

/// Giải mã %XX trong href.
pub fn percent_decode(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = ((b[i + 1] as char).to_digit(16), (b[i + 2] as char).to_digit(16)) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Ghép href tương đối với thư mục gốc rồi chuẩn hoá «./», «../». Trả (đường dẫn, phần sau `#`). Link ra ngoài (http:, mailto:…)
/// → None.
pub fn resolve(base_dir: &str, href: &str) -> Option<(String, Option<String>)> {
    let href = href.trim();
    if href.is_empty() {
        return None;
    }
    let (p, frag) = match href.split_once('#') {
        Some((p, f)) => (p, Some(f.to_string()).filter(|f| !f.is_empty())),
        None => (href, None),
    };
    if p.contains(':') && !p.starts_with('/') {
        let scheme = p.split(':').next().unwrap_or("");
        if scheme.len() > 1 {
            return None;
        }
    }
    let p = percent_decode(p.split('?').next().unwrap_or(p));
    let joined = if p.is_empty() {
        base_dir.trim_end_matches('/').to_string()
    } else if let Some(abs) = p.strip_prefix('/') {
        abs.to_string()
    } else {
        format!("{base_dir}{p}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    Some((parts.join("/"), frag))
}

fn is_image(mime: &str, path: &str) -> bool {
    mime.starts_with("image/") || matches!(path.rsplit('.').next().map(str::to_ascii_lowercase).as_deref(), Some("jpg" | "jpeg" | "png" | "gif"))
}

/// Ảnh đầu tiên trong một trang (X)HTML.
fn first_image(zip: &Zip, page: &str) -> Option<String> {
    let bytes = zip.read(page).ok()?;
    let (s, _) = xml::decode_bytes(&bytes);
    let dir = dir_of(page);
    for (t, _) in xml::tokens(&s) {
        if let Tok::Start { name, attrs, .. } = t {
            let n = xml::local(name).to_ascii_lowercase();
            let src = match n.as_str() {
                "img" => xml::attr(attrs, "src"),
                "image" => xml::attr_exact(attrs, "xlink:href").or_else(|| xml::attr(attrs, "href")),
                _ => None,
            };
            if let Some(src) = src {
                if let Some((p, _)) = resolve(dir, &src) {
                    if zip.has(&p) {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

/// Đọc sách.
pub fn parse(zip: &Zip) -> Result<Epub, String> {
    let container = zip.read("META-INF/container.xml").map_err(|_| "Không phải EPUB (thiếu META-INF/container.xml).".to_string())?;
    let (container, _) = xml::decode_bytes(&container);
    let mut opf_path = None;
    for (t, _) in xml::tokens(&container) {
        if let Tok::Start { name, attrs, .. } = t {
            if xml::local(name) == "rootfile" {
                let p = xml::attr(attrs, "full-path").map(|p| p.into_owned());
                let mt = xml::attr(attrs, "media-type").map(|m| m.into_owned()).unwrap_or_default();
                if opf_path.is_none() || mt == "application/oebps-package+xml" {
                    opf_path = p;
                    if mt == "application/oebps-package+xml" {
                        break;
                    }
                }
            }
        }
    }
    let opf_path = opf_path
        .or_else(|| zip.entries().iter().find(|e| e.name.to_ascii_lowercase().ends_with(".opf")).map(|e| e.name.clone()))
        .ok_or("EPUB thiếu tệp OPF.")?;
    let opf_bytes = zip.read(&opf_path).map_err(|e| format!("Không đọc được {opf_path}: {e}"))?;
    let (opf, _) = xml::decode_bytes(&opf_bytes);
    let base = dir_of(&opf_path).to_string();

    let mut ep = Epub { opf: opf_path.clone(), ..Default::default() };
    // id → (đường dẫn, mime, properties)
    let mut items: HashMap<String, (String, String, String)> = HashMap::new();
    let mut spine_refs: Vec<(String, bool)> = Vec::new();
    let mut toc_id: Option<String> = None;
    let mut cover_id: Option<String> = None;
    let mut guide_cover: Option<String> = None;
    let mut unique_id_ref: Option<String> = None;
    let mut identifiers: Vec<(Option<String>, String)> = Vec::new();
    let mut creators: Vec<(String, Option<String>)> = Vec::new();
    let mut series: Option<String> = None;
    let mut series_index: f32 = 0.0;
    // Phần tử dc:* đang mở: (tên, id, role, vị trí bắt đầu chữ)
    let mut open_dc: Option<(String, Option<String>, Option<String>, usize)> = None;
    for (t, r) in xml::tokens(&opf) {
        match t {
            Tok::Start { name, attrs, empty } => {
                let n = xml::local(name);
                match n {
                    "package" => {
                        unique_id_ref = xml::attr(attrs, "unique-identifier").map(|v| v.into_owned());
                    }
                    "item" => {
                        let id = xml::attr(attrs, "id").map(|v| v.into_owned()).unwrap_or_default();
                        let href = xml::attr(attrs, "href").map(|v| v.into_owned()).unwrap_or_default();
                        let mt = xml::attr(attrs, "media-type").map(|v| v.into_owned()).unwrap_or_default();
                        let props = xml::attr(attrs, "properties").map(|v| v.into_owned()).unwrap_or_default();
                        if let Some((p, _)) = resolve(&base, &href) {
                            ep.manifest.insert(p.clone(), mt.clone());
                            items.insert(id, (p, mt, props));
                        }
                    }
                    "spine" => {
                        toc_id = xml::attr(attrs, "toc").map(|v| v.into_owned());
                        ep.rtl = xml::attr(attrs, "page-progression-direction").is_some_and(|v| v == "rtl");
                    }
                    "itemref" => {
                        if let Some(id) = xml::attr(attrs, "idref") {
                            let linear = xml::attr(attrs, "linear").is_none_or(|v| v != "no");
                            spine_refs.push((id.into_owned(), linear));
                        }
                    }
                    "meta" => {
                        let mname = xml::attr(attrs, "name").map(|v| v.into_owned());
                        let content = xml::attr(attrs, "content").map(|v| v.into_owned());
                        let prop = xml::attr(attrs, "property").map(|v| v.into_owned());
                        match mname.as_deref() {
                            Some("cover") => cover_id = content,
                            Some("calibre:series") => series = content,
                            Some("calibre:series_index") => series_index = content.and_then(|c| c.parse().ok()).unwrap_or(0.0),
                            _ => {}
                        }
                        if prop.as_deref() == Some("rendition:layout") && !empty {
                            open_dc = Some(("layout".into(), None, None, r.end));
                        }
                    }
                    "reference" => {
                        if xml::attr(attrs, "type").is_some_and(|v| v.eq_ignore_ascii_case("cover")) {
                            guide_cover = xml::attr(attrs, "href").and_then(|h| resolve(&base, &h)).map(|(p, _)| p);
                        }
                    }
                    "title" | "creator" | "language" | "publisher" | "description" | "identifier" if !empty => {
                        let id = xml::attr(attrs, "id").map(|v| v.into_owned());
                        let role = xml::attr(attrs, "role").map(|v| v.into_owned());
                        open_dc = Some((n.to_string(), id, role, r.end));
                    }
                    _ => {}
                }
            }
            Tok::End { name } => {
                let n = xml::local(name);
                if let Some((open, id, role, from)) = open_dc.take() {
                    if open == n || (open == "layout" && n == "meta") {
                        let val = xml::plain_text(&opf[from..r.start]);
                        match open.as_str() {
                            "title" if ep.title.is_empty() => ep.title = val,
                            "creator" if !val.is_empty() => creators.push((val, role)),
                            "language" if ep.language.is_none() && !val.is_empty() => ep.language = Some(val),
                            "publisher" if !val.is_empty() => ep.publisher = Some(val),
                            "description" if !val.is_empty() => ep.description = Some(val),
                            "identifier" => identifiers.push((id, val)),
                            "layout" => ep.fixed = val == "pre-paginated",
                            _ => {}
                        }
                    } else {
                        open_dc = Some((open, id, role, from));
                    }
                }
            }
            _ => {}
        }
    }
    // Tác giả: ưu tiên vai «aut»
    let auts: Vec<String> = creators.iter().filter(|(_, r)| r.as_deref() == Some("aut")).map(|(n, _)| n.clone()).collect();
    ep.authors = if auts.is_empty() { creators.into_iter().map(|(n, _)| n).collect() } else { auts };
    ep.identifier = identifiers
        .iter()
        .find(|(id, _)| id.is_some() && id.as_deref() == unique_id_ref.as_deref())
        .or_else(|| identifiers.first())
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    ep.series = series.filter(|s| !s.is_empty()).map(|s| (s, series_index));

    // Spine
    for (id, linear) in spine_refs {
        if let Some((p, mt, _)) = items.get(&id) {
            if let Some(e) = zip.find(p) {
                ep.spine.push(SpineItem { path: e.name.clone(), mime: mt.clone(), linear, size: e.size });
            }
        }
    }
    if ep.spine.is_empty() {
        // OPF hỏng: lấy mọi tệp (X)HTML trong manifest theo thứ tự tên
        let mut v: Vec<&(String, String, String)> = items.values().filter(|(_, mt, _)| mt.contains("html")).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        for (p, mt, _) in v {
            if let Some(e) = zip.find(p) {
                ep.spine.push(SpineItem { path: e.name.clone(), mime: mt.clone(), linear: true, size: e.size });
            }
        }
    }
    if ep.spine.is_empty() {
        return Err("EPUB không có chương nào.".into());
    }

    // Bìa
    let cover_item = cover_id.as_ref().and_then(|id| items.get(id)).cloned();
    ep.cover = match cover_item {
        Some((p, mt, _)) if is_image(&mt, &p) && zip.has(&p) => Some(p),
        Some((p, mt, _)) if mt.contains("html") => first_image(zip, &p),
        _ => None,
    }
    .or_else(|| items.values().find(|(p, _, props)| props.split_whitespace().any(|x| x == "cover-image") && zip.has(p)).map(|v| v.0.clone()))
    .or_else(|| guide_cover.as_deref().and_then(|g| if is_image("", g) { Some(g.to_string()) } else { first_image(zip, g) }))
    .or_else(|| {
        items
            .iter()
            .find(|(id, (p, mt, _))| is_image(mt, p) && (id.to_lowercase().contains("cover") || p.to_lowercase().contains("cover")) && zip.has(p))
            .map(|(_, v)| v.0.clone())
    });

    // Mục lục: EPUB 3 nav, rồi NCX
    let nav = items.values().find(|(_, _, props)| props.split_whitespace().any(|x| x == "nav")).map(|v| v.0.clone());
    if let Some(nav) = nav {
        ep.toc = parse_nav(zip, &nav);
    }
    if ep.toc.is_empty() {
        let ncx = toc_id
            .as_ref()
            .and_then(|id| items.get(id))
            .map(|v| v.0.clone())
            .or_else(|| items.values().find(|(_, mt, _)| mt == "application/x-dtbncx+xml").map(|v| v.0.clone()));
        if let Some(ncx) = ncx {
            ep.toc = parse_ncx(zip, &ncx);
        }
    }
    ep.toc.retain(|t| zip.has(&t.path));
    if ep.toc.is_empty() {
        ep.toc = fallback_toc(zip, &ep.spine);
    }

    // Font bị làm rối / DRM
    if let Ok(enc) = zip.read("META-INF/encryption.xml") {
        let (enc, _) = xml::decode_bytes(&enc);
        let idpf_key = sha1(ep.identifier.chars().filter(|c| !matches!(c, ' ' | '\t' | '\r' | '\n')).collect::<String>().as_bytes());
        let adobe_key = adobe_key(&ep.identifier);
        let mut algo: Option<String> = None;
        for (t, _) in xml::tokens(&enc) {
            if let Tok::Start { name, attrs, .. } = t {
                match xml::local(name) {
                    "EncryptedData" => algo = None,
                    "EncryptionMethod" => algo = xml::attr(attrs, "Algorithm").map(|v| v.into_owned()),
                    "CipherReference" => {
                        let Some(uri) = xml::attr(attrs, "URI") else { continue };
                        let path = resolve("", &uri).map(|(p, _)| p).unwrap_or_default();
                        match algo.as_deref() {
                            Some("http://www.idpf.org/2008/embedding") => {
                                ep.obfuscated.insert(path, Obf::Idpf(idpf_key));
                            }
                            Some("http://ns.adobe.com/pdf/enc#RC") => {
                                if let Some(k) = adobe_key {
                                    ep.obfuscated.insert(path, Obf::Adobe(k));
                                }
                            }
                            _ => {
                                // Mã hoá thật: sách có DRM nếu đó là chương / ảnh (không chỉ font)
                                let mt = ep.manifest.get(&path).cloned().unwrap_or_default();
                                if !mt.contains("font") && !path.to_ascii_lowercase().ends_with(".ttf") && !path.to_ascii_lowercase().ends_with(".otf") {
                                    ep.drm = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if ep.title.trim().is_empty() {
        ep.title = String::new();
    }
    Ok(ep)
}

fn parse_ncx(zip: &Zip, path: &str) -> Vec<TocEntry> {
    let Ok(b) = zip.read(path) else { return Vec::new() };
    let (s, _) = xml::decode_bytes(&b);
    let dir = dir_of(path);
    let mut out = Vec::new();
    let mut depth: i32 = -1;
    let mut in_map = false;
    let mut label: Option<String> = None;
    let mut text_from: Option<usize> = None;
    // navPoint đang mở chưa ghi (chờ content)
    let mut pending: Vec<Option<String>> = Vec::new();
    for (t, r) in xml::tokens(&s) {
        match t {
            Tok::Start { name, attrs, empty } => match xml::local(name) {
                "navMap" => in_map = true,
                "navPoint" if in_map && !empty => {
                    depth += 1;
                    label = None;
                    pending.push(None);
                }
                "text" if in_map && !empty => text_from = Some(r.end),
                "content" if in_map => {
                    if let Some(src) = xml::attr(attrs, "src") {
                        if let Some((p, frag)) = resolve(dir, &src) {
                            let title = label.take().unwrap_or_default();
                            if let Some(last) = pending.last_mut() {
                                *last = Some(p.clone());
                            }
                            out.push(TocEntry { title, path: p, frag, depth: depth.max(0) as u32 });
                        }
                    }
                }
                _ => {}
            },
            Tok::End { name } => match xml::local(name) {
                "navMap" => in_map = false,
                "navPoint" if in_map => {
                    depth -= 1;
                    pending.pop();
                }
                "text" => {
                    if let Some(from) = text_from.take() {
                        let v = xml::plain_text(&s[from..r.start]);
                        if label.is_none() {
                            label = Some(v);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    out
}

fn parse_nav(zip: &Zip, path: &str) -> Vec<TocEntry> {
    let Ok(b) = zip.read(path) else { return Vec::new() };
    let (s, _) = xml::decode_bytes(&b);
    let dir = dir_of(path);
    let mut out = Vec::new();
    let mut in_toc = false;
    let mut nav_depth = 0i32;
    let mut ol_depth = 0i32;
    let mut link: Option<(String, Option<String>, usize)> = None;
    for (t, r) in xml::tokens(&s) {
        match t {
            Tok::Start { name, attrs, empty } => match xml::local(name).to_ascii_lowercase().as_str() {
                "nav" if !empty => {
                    if in_toc {
                        nav_depth += 1;
                    } else if xml::attr_exact(attrs, "epub:type").is_some_and(|v| v.split_whitespace().any(|x| x == "toc")) {
                        in_toc = true;
                        nav_depth = 1;
                        ol_depth = 0;
                    }
                }
                "ol" if in_toc && !empty => ol_depth += 1,
                "a" if in_toc && !empty => {
                    if let Some((p, frag)) = xml::attr(attrs, "href").and_then(|h| resolve(dir, &h)) {
                        link = Some((p, frag, r.end));
                    }
                }
                _ => {}
            },
            Tok::End { name } => match xml::local(name).to_ascii_lowercase().as_str() {
                "nav" if in_toc => {
                    nav_depth -= 1;
                    if nav_depth == 0 {
                        in_toc = false;
                    }
                }
                "ol" if in_toc => ol_depth -= 1,
                "a" if in_toc => {
                    if let Some((p, frag, from)) = link.take() {
                        let title = xml::plain_text(&s[from..r.start]);
                        out.push(TocEntry { title, path: p, frag, depth: (ol_depth - 1).max(0) as u32 });
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    out
}

/// Không có mục lục: mỗi chương (linear) một mục — tên lấy ở `<title>` / đề mục đầu, không có thì «Phần N».
fn fallback_toc(zip: &Zip, spine: &[SpineItem]) -> Vec<TocEntry> {
    let mut out = Vec::new();
    for (i, it) in spine.iter().enumerate().filter(|(_, it)| it.linear) {
        let mut title = String::new();
        if let Ok(b) = zip.read(&it.path) {
            let head = &b[..b.len().min(16 * 1024)];
            let (s, _) = xml::decode_bytes(head);
            let mut from: Option<(String, usize)> = None;
            for (t, r) in xml::tokens(&s) {
                match t {
                    Tok::Start { name, empty: false, .. } => {
                        let n = xml::local(name).to_ascii_lowercase();
                        if matches!(n.as_str(), "h1" | "h2" | "h3" | "title") && from.is_none() {
                            from = Some((n, r.end));
                        }
                    }
                    Tok::End { name } => {
                        let hit = from.as_ref().is_some_and(|(n, _)| xml::local(name).eq_ignore_ascii_case(n));
                        if hit {
                            let (n, f) = from.take().unwrap_or_default();
                            let v = xml::plain_text(&s[f..r.start]);
                            if !v.is_empty() {
                                title = v;
                                if n != "title" {
                                    break;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if title.is_empty() {
            title = format!("Phần {}", i + 1);
        }
        out.push(TocEntry { title, path: it.path.clone(), frag: None, depth: 0 });
    }
    out
}

/// Gỡ làm rối font (IDPF: 1040 byte đầu, Adobe: 1024 byte đầu).
pub fn deobfuscate(data: &mut [u8], o: &Obf) {
    match o {
        Obf::Idpf(k) => {
            for (i, b) in data.iter_mut().take(1040).enumerate() {
                *b ^= k[i % 20];
            }
        }
        Obf::Adobe(k) => {
            for (i, b) in data.iter_mut().take(1024).enumerate() {
                *b ^= k[i % 16];
            }
        }
    }
}

fn adobe_key(id: &str) -> Option<[u8; 16]> {
    let hex: String = id.trim_start_matches("urn:uuid:").chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() < 32 {
        return None;
    }
    let mut k = [0u8; 16];
    for (i, b) in k.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(k)
}

/// SHA-1 (khoá gỡ làm rối font IDPF) — vài chục dòng, khỏi thêm crate.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip::tests::make_zip;

    #[test]
    fn sha1_vectors() {
        let hex = |b: [u8; 20]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(hex(sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hex(sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn resolve_paths() {
        assert_eq!(resolve("OEBPS/", "Text/ch1.xhtml#p3"), Some(("OEBPS/Text/ch1.xhtml".into(), Some("p3".into()))));
        assert_eq!(resolve("OEBPS/Text/", "../Images/a%20b.jpg"), Some(("OEBPS/Images/a b.jpg".into(), None)));
        assert_eq!(resolve("OEBPS/", "http://vnthuquan.net/x#_ftn1"), None);
        assert_eq!(resolve("", "./toc.ncx"), Some(("toc.ncx".into(), None)));
    }

    fn tiny_epub(dir: &std::path::Path) -> std::path::PathBuf {
        let p = dir.join("t.epub");
        let opf = r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf"><dc:title>Ai Cập Huyền Bí</dc:title>
<dc:creator opf:role="aut">Paul Brunton</dc:creator><dc:language>en-US</dc:language><dc:identifier id="BookId">urn:uuid:x</dc:identifier>
<meta name="cover" content="cov"/><meta name="calibre:series" content="Huyền bí"/><meta name="calibre:series_index" content="2"/></metadata>
<manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/><item id="c1" href="Text/c1.xhtml" media-type="application/xhtml+xml"/>
<item id="c2" href="Text/c%202.xhtml" media-type="application/xhtml+xml"/><item id="cov" href="Images/cover.jpg" media-type="image/jpeg"/></manifest>
<spine toc="ncx"><itemref idref="c1"/><itemref idref="c2" linear="no"/></spine></package>"#;
        let ncx = r#"<?xml version="1.0"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>
<navPoint id="a" playOrder="1"><navLabel><text>Chương 1</text></navLabel><content src="Text/c1.xhtml"/>
<navPoint id="b" playOrder="2"><navLabel><text>Mục 1.1</text></navLabel><content src="Text/c1.xhtml#m11"/></navPoint></navPoint>
<navPoint id="c" playOrder="3"><navLabel><text>Chương&#160;2</text></navLabel><content src="Text/c%202.xhtml"/></navPoint></navMap></ncx>"#;
        let c1 = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>C1</title></head><body><h1>Một</h1><p id="m11">x</p></body></html>"#;
        make_zip(
            &p,
            &[
                ("mimetype", b"application/epub+zip", false),
                ("META-INF/container.xml", br#"<container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#, true),
                ("OEBPS/content.opf", opf.as_bytes(), true),
                ("OEBPS/toc.ncx", ncx.as_bytes(), true),
                ("OEBPS/Text/c1.xhtml", c1.as_bytes(), true),
                ("OEBPS/Text/c 2.xhtml", c1.as_bytes(), true),
                ("OEBPS/Images/cover.jpg", b"\xff\xd8\xff", false),
            ],
        );
        p
    }

    #[test]
    fn parses_opf_ncx_cover_series() {
        let d = std::env::temp_dir().join(format!("gb-epub-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let z = Zip::open(&tiny_epub(&d)).unwrap();
        let e = parse(&z).unwrap();
        assert_eq!(e.title, "Ai Cập Huyền Bí");
        assert_eq!(e.authors, vec!["Paul Brunton".to_string()]);
        assert_eq!(e.spine.len(), 2);
        assert_eq!(e.spine[1].path, "OEBPS/Text/c 2.xhtml");
        assert!(!e.spine[1].linear);
        assert_eq!(e.cover.as_deref(), Some("OEBPS/Images/cover.jpg"));
        assert_eq!(e.series, Some(("Huyền bí".to_string(), 2.0)));
        assert_eq!(e.toc.len(), 3);
        assert_eq!(e.toc[1], TocEntry { title: "Mục 1.1".into(), path: "OEBPS/Text/c1.xhtml".into(), frag: Some("m11".into()), depth: 1 });
        assert_eq!(e.toc[2].title, "Chương 2");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn real_samples_if_present() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../Tham_Khao");
        let mut paths = Vec::new();
        for sub in ["ebook", "epub2"] {
            collect(&root.join(sub), &mut paths);
        }
        if paths.is_empty() {
            return;
        }
        let (mut ok, mut no_toc, mut covers) = (0, 0, 0);
        for p in &paths {
            let z = Zip::open(p).unwrap_or_else(|e| panic!("{p:?}: {e}"));
            let e = parse(&z).unwrap_or_else(|e| panic!("{p:?}: {e}"));
            assert!(!e.spine.is_empty(), "{p:?}");
            assert!(!e.drm, "{p:?}");
            if e.toc.iter().all(|t| t.title.starts_with("Phần ")) {
                no_toc += 1;
            }
            if e.cover.is_some() {
                covers += 1;
            }
            ok += 1;
        }
        eprintln!("EPUB mẫu: {ok} cuốn đọc được, {covers} có bìa, {no_toc} phải dựng mục lục");
    }

    pub(crate) fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect(&p, out);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("epub")) {
                out.push(p);
            }
        }
    }
}
