//! Thử trên kho sách thật `E:\DEV_CODE\GramBook\Tham_Khao\kindle` (sách có bản quyền — chỉ đọc tại chỗ, không chép vào
//! crate). Không có thư mục → bỏ qua. In tóm tắt: số sách theo định dạng, lỗi kèm lý do, thời gian tổng / lớn nhất.
//!
//! Chạy: `cargo test -p gb-mobi --release --test corpus -- --nocapture`.

#[path = "../src/xmlcheck.rs"]
mod xmlcheck;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gb_mobi::{Format, MobiBook, MobiError};

const CORPUS: &str = r"E:\DEV_CODE\GramBook\Tham_Khao\kindle";

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    v.sort();
    for p in v {
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Trường mã hoá của bản ghi 0 (đọc thẳng từ PDB, độc lập với crate).
fn encryption(d: &[u8]) -> u16 {
    let Some(off) = d
        .get(78..82)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
    else {
        return 0;
    };
    d.get(off + 12..off + 14)
        .map_or(0, |b| u16::from_be_bytes([b[0], b[1]]))
}

/// Số ký tự chữ nhìn thấy (bỏ thẻ, khoảng trắng).
fn visible_chars(html: &[u8]) -> usize {
    let s = String::from_utf8_lossy(html);
    let mut in_tag = false;
    let mut n = 0;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag && !c.is_whitespace() => n += 1,
            _ => {}
        }
    }
    n
}

fn ids_of(doc: &[u8]) -> HashSet<String> {
    let s = String::from_utf8_lossy(doc);
    let mut out = HashSet::new();
    for q in ['"', '\''] {
        let pat = format!(" id={q}");
        let mut rest: &str = &s;
        while let Some(i) = rest.find(&pat) {
            rest = &rest[i + pat.len()..];
            if let Some(e) = rest.find(q) {
                out.insert(rest[..e].to_string());
            }
        }
    }
    out
}

/// Các liên kết `partNNNN.html#id` / `../images/…` trong một tệp chương.
fn links_of(doc: &[u8]) -> Vec<String> {
    let s = String::from_utf8_lossy(doc);
    let mut out = Vec::new();
    for attr in ["href=\"", "src=\""] {
        let mut rest: &str = &s;
        while let Some(i) = rest.find(attr) {
            rest = &rest[i + attr.len()..];
            if let Some(e) = rest.find('"') {
                let v = &rest[..e];
                if v.starts_with("part") || v.starts_with("../") {
                    out.push(v.to_string());
                }
            }
        }
    }
    out
}

#[derive(Default)]
struct Stats {
    by_format: HashMap<&'static str, usize>,
    drm: usize,
    failures: Vec<(String, String)>,
    kf8_parts: usize,
    kf8_xml_ok: usize,
    kf8_xml_bad: Vec<(String, String)>,
    toc_entries: usize,
    links: usize,
    total: Duration,
    max: (Duration, String),
}

fn check_book(name: &str, b: &MobiBook, st: &mut Stats) -> Result<(), String> {
    if b.spine.is_empty() {
        return Err("không có chương".into());
    }
    let chars: usize = b.spine.iter().map(|i| visible_chars(&i.data)).sum();
    if chars < 500 {
        return Err(format!("quá ít chữ ({chars})"));
    }
    if b.format == Format::Kf8 && (b.title.trim().is_empty() || b.authors.is_empty()) {
        return Err(format!(
            "KF8 thiếu tên / tác giả: {:?} / {:?}",
            b.title, b.authors
        ));
    }
    let mut files: HashMap<&str, &[u8]> = HashMap::new();
    for it in b.spine.iter().chain(&b.resources) {
        if !it.path.is_ascii() || it.path.starts_with('/') {
            return Err(format!("đường dẫn lạ {}", it.path));
        }
        if files.insert(&it.path, &it.data).is_some() {
            return Err(format!("trùng đường dẫn {}", it.path));
        }
    }
    let mut ids: HashMap<&str, HashSet<String>> = HashMap::new();
    for it in &b.spine {
        ids.insert(&it.path, ids_of(&it.data));
    }
    let target_ok = |href: &str| -> bool {
        let (file, anchor) = href
            .split_once('#')
            .map_or((href, None), |(f, a)| (f, Some(a)));
        match (ids.get(file), anchor) {
            (Some(_), None) => true,
            (Some(set), Some(a)) => set.contains(a),
            (None, _) => files.contains_key(file),
        }
    };
    for e in &b.toc {
        st.toc_entries += 1;
        if e.title.trim().is_empty() {
            return Err(format!("mục lục có nhãn rỗng → {}", e.href));
        }
        if !e.href.starts_with("text/")
            || !ids.contains_key(e.href.split('#').next().unwrap_or(""))
            || !target_ok(&e.href)
        {
            return Err(format!(
                "mục lục trỏ tới chỗ không có: {} → {}",
                e.title, e.href
            ));
        }
    }
    // Liên kết trong chương: tệp / neo / ảnh phải có thật.
    for it in &b.spine {
        for l in links_of(&it.data) {
            st.links += 1;
            let full = if let Some(r) = l.strip_prefix("../") {
                r.to_string()
            } else {
                format!("text/{l}")
            };
            if !target_ok(&full) {
                return Err(format!("{}: liên kết hỏng {l}", it.path));
            }
        }
    }
    if let Some(c) = &b.cover {
        if !files.contains_key(c.as_str()) {
            return Err(format!("ảnh bìa không có: {c}"));
        }
    }
    if b.format == Format::Kf8 {
        for it in &b.spine {
            st.kf8_parts += 1;
            match xmlcheck::check(&it.data) {
                Ok(()) => st.kf8_xml_ok += 1,
                Err(e) => st.kf8_xml_bad.push((format!("{name} {}", it.path), e)),
            }
        }
    }
    Ok(())
}

/// Sách thật bị làm hỏng (đổi byte ngẫu nhiên ở đầu tệp, bản ghi 0, chỉ mục, HUFF/CDIC; cắt cụt): không panic, không treo.
#[test]
fn kho_sach_that_bi_hong() {
    let root = Path::new(CORPUS);
    if !root.is_dir() {
        eprintln!("bỏ qua: không có thư mục {CORPUS}");
        return;
    }
    let mut files = Vec::new();
    walk(root, &mut files);
    let mut picked: Vec<(u64, PathBuf)> = Vec::new();
    // Mỗi loại lấy vài sách nhỏ nhất: AZW3, MOBI6 PalmDOC, MOBI6 HUFF/CDIC.
    for (ext_kf8, comp) in [(true, 2u16), (false, 2), (false, 17480)] {
        let mut v: Vec<(u64, PathBuf)> = files
            .iter()
            .filter_map(|f| {
                let d = std::fs::read(f).ok()?;
                let off = u32::from_be_bytes(d.get(78..82)?.try_into().ok()?) as usize;
                let c = u16::from_be_bytes(d.get(off..off + 2)?.try_into().ok()?);
                let is_kf8 = f.extension().is_some_and(|e| e == "azw3");
                (encryption(&d) == 0 && c == comp && is_kf8 == ext_kf8)
                    .then(|| (d.len() as u64, f.clone()))
            })
            .collect();
        v.sort();
        picked.extend(v.into_iter().take(2));
    }
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut n = 0;
    let mut worst = Duration::ZERO;
    for (_, f) in &picked {
        let orig = std::fs::read(f).unwrap();
        let rec_off = |d: &[u8], i: usize| {
            d.get(78 + 8 * i..82 + 8 * i)
                .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
        };
        let nrec = u16::from_be_bytes([orig[76], orig[77]]) as usize;
        for round in 0..120 {
            let mut d = orig.clone();
            if round % 10 == 9 {
                d.truncate((next() as usize) % d.len());
            } else {
                for _ in 0..1 + next() % 6 {
                    // Vùng: đầu PDB / bản ghi 0 / một bản ghi bất kỳ (chỉ mục, HUFF…) / bất kỳ.
                    let i = match next() % 4 {
                        0 => (next() as usize) % (78 + 8 * nrec).min(d.len()),
                        1 => rec_off(&d, 0) + (next() as usize) % 1024,
                        2 => rec_off(&d, (next() as usize) % nrec) + (next() as usize) % 256,
                        _ => (next() as usize) % d.len(),
                    };
                    if i < d.len() {
                        d[i] = next() as u8;
                    }
                }
            }
            let t = Instant::now();
            let _ = gb_mobi::convert(&d);
            let el = t.elapsed();
            worst = worst.max(el);
            assert!(
                el < Duration::from_secs(20),
                "{}: quá lâu {el:?}",
                f.display()
            );
            n += 1;
        }
    }
    println!(
        "sách hỏng: {n} lần chuyển trên {} sách, lâu nhất {worst:?}",
        picked.len()
    );
}

#[test]
fn kho_sach_that() {
    let root = Path::new(CORPUS);
    if !root.is_dir() {
        eprintln!("bỏ qua: không có thư mục {CORPUS}");
        return;
    }
    let mut files = Vec::new();
    walk(root, &mut files);
    let mut st = Stats::default();
    for f in &files {
        let name = f.strip_prefix(root).unwrap_or(f).display().to_string();
        let data = std::fs::read(f).expect("đọc tệp");
        let t = Instant::now();
        let r = gb_mobi::convert(&data);
        let el = t.elapsed();
        st.total += el;
        if el > st.max.0 {
            st.max = (el, name.clone());
        }
        match (encryption(&data) != 0, r) {
            (true, Err(MobiError::Drm)) => st.drm += 1,
            (true, other) => st.failures.push((
                name,
                format!("có DRM nhưng kết quả {:?}", other.map(|b| b.title)),
            )),
            (false, Err(e)) => st.failures.push((name, format!("{e:?}"))),
            (false, Ok(b)) => {
                *st.by_format
                    .entry(if b.format == Format::Kf8 {
                        "KF8"
                    } else {
                        "MOBI6"
                    })
                    .or_default() += 1;
                if let Err(e) = check_book(&name, &b, &mut st) {
                    st.failures.push((name, e));
                }
            }
        }
    }
    println!("\n=== gb-mobi: kho sách thật ({} tệp) ===", files.len());
    println!(
        "MOBI6 ok: {}  KF8 ok: {}  DRM: {}",
        st.by_format.get("MOBI6").unwrap_or(&0),
        st.by_format.get("KF8").unwrap_or(&0),
        st.drm
    );
    println!(
        "mục lục: {} mục; liên kết trong chương đã kiểm: {}",
        st.toc_entries, st.links
    );
    let pct = if st.kf8_parts > 0 {
        100.0 * st.kf8_xml_ok as f64 / st.kf8_parts as f64
    } else {
        100.0
    };
    println!(
        "KF8: {}/{} tệp đúng cú pháp XML ({pct:.1}%)",
        st.kf8_xml_ok, st.kf8_parts
    );
    for (f, e) in st.kf8_xml_bad.iter().take(10) {
        println!("  XML hỏng: {f}: {e}");
    }
    println!(
        "thời gian: tổng {:?}, lớn nhất {:?} ({})",
        st.total, st.max.0, st.max.1
    );
    for (f, e) in &st.failures {
        println!("LỖI {f}: {e}");
    }
    assert!(st.failures.is_empty(), "{} tệp lỗi", st.failures.len());
    assert_eq!(st.drm, 14, "số sách DRM");
    assert!(pct >= 95.0, "KF8 đúng cú pháp XML: {pct:.1}%");
}
