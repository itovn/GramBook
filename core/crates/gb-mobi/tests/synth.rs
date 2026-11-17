//! Bài thử trên sách tổng hợp (không cần kho sách thật): MOBI6, KF8, tệp kép MOBI6 + KF8, dữ liệu hỏng / độc.

mod common;

use std::time::{Duration, Instant};

use common::*;
use gb_mobi::{convert, Format, MobiBook, MobiError, TocEntry};

fn doc(b: &MobiBook, path: &str) -> String {
    let it = b
        .spine
        .iter()
        .chain(&b.resources)
        .find(|i| i.path == path)
        .unwrap_or_else(|| panic!("không có {path}"));
    String::from_utf8(it.data.clone()).expect("UTF-8")
}

fn toc(title: &str, href: &str, depth: u32) -> TocEntry {
    TocEntry {
        title: title.into(),
        href: href.into(),
        depth,
    }
}

/// Sách MOBI6: guide «toc», ngắt trang, filepos, ảnh, thẻ Mobipocket, width / height. `filepos` ghi 10 chữ số nên thay
/// số không làm lệch vị trí.
fn mobi6_book() -> (Vec<Vec<u8>>, [usize; 3]) {
    let mut html = String::from(
        "<html><head><guide><reference type=\"toc\" title=\"Mục lục\" filepos=TOCPOS____ /></guide></head><body>\
         <h1>Tên sách</h1><p width=\"2em\" height=\"0pt\">Đoạn một<br></br>dòng hai <a filepos=CH2POS____>tới chương 2</a></p>\
         <p><img recindex=\"00001\" alt=\"hình\"/></p>",
    );
    let ch2 = html.len();
    html.push_str(
        "<mbp:pagebreak/><h2>Chương 2</h2><p>Nội <mbp:nu>dung</mbp:nu> &#273;ây <a filepos=0000099999>xa</a>.</p>\
         <div><font size=\"2\">Còn dài <b>chữ đậm",
    );
    let tocpos = html.len();
    html.push_str(
        "<mbp:pagebreak/>vẫn đậm</b></font></div><p>Mục lục</p><p><a filepos=CH1POS____>Chương 1</a></p>\
         <p><a filepos=CH2POS____>Chương 2</a></p></body></html>",
    );
    let ch1 = html.find("<h1>").unwrap();
    let html = html
        .replace("TOCPOS____", &format!("{tocpos:010}"))
        .replace("CH1POS____", &format!("{ch1:010}"))
        .replace("CH2POS____", &format!("{ch2:010}"));
    let exth = vec![
        (100, "Tác Giả".as_bytes().to_vec()),
        (503, "Sách Thử".as_bytes().to_vec()),
        (201, 0u32.to_be_bytes().to_vec()),
    ];
    (mobi6(&html, &[jpeg(1)], exth), [ch1, ch2, tocpos])
}

#[test]
fn mobi6_tong_hop() {
    let (recs, [ch1, ch2, tocpos]) = mobi6_book();
    let b = convert(&pdb(&recs)).unwrap();
    assert_eq!(b.format, Format::Mobi6);
    assert_eq!(b.title, "Sách Thử");
    assert_eq!(b.authors, vec!["Tác Giả".to_string()]);
    assert_eq!(b.language.as_deref(), Some("vi"));
    assert_eq!(b.cover.as_deref(), Some("images/00001.jpg"));
    assert_eq!(b.spine.len(), 3);
    assert!(b.spine.iter().all(|i| i.mime == "text/html"));
    let p0 = doc(&b, "text/part0000.html");
    assert!(p0.starts_with(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"/><title>Sách Thử</title></head><body>"
    ));
    assert!(
        p0.contains(&format!(
            "<h1 id=\"gbh1\"><a id=\"fp{ch1:010}\"></a>Tên sách</h1>"
        )),
        "{p0}"
    );
    assert!(
        p0.contains("style=\"text-indent:2em;margin-top:0pt;\""),
        "{p0}"
    );
    assert!(
        p0.contains("Đoạn một<br>dòng hai") && !p0.contains("</br>"),
        "{p0}"
    );
    assert!(
        p0.contains(&format!(
            "<a href=\"part0001.html#fp{ch2:010}\">tới chương 2</a>"
        )),
        "{p0}"
    );
    assert!(
        p0.contains("<img alt=\"hình\" src=\"../images/00001.jpg\"/>"),
        "{p0}"
    );
    assert!(!p0.contains("guide") && !p0.contains("mbp:"));
    let p1 = doc(&b, "text/part0001.html");
    // Đích filepos ngay ở thẻ ngắt trang → neo ở đầu chương sau; mbp:nu bị bỏ; liên kết ngoài chữ → neo cuối sách.
    assert!(
        p1.contains(&format!(
            "<h2 id=\"gbh2\"><a id=\"fp{ch2:010}\"></a>Chương 2</h2>"
        )),
        "{p1}"
    );
    assert!(p1.contains("Nội dung &#273;ây"), "{p1}");
    assert!(
        p1.contains("<a href=\"part0002.html#fp0000099999\">xa</a>"),
        "{p1}"
    );
    let p2 = doc(&b, "text/part0002.html");
    // Thẻ định dạng còn mở lúc ngắt được mở lại ở phần sau.
    assert!(p2.contains("<body><div><font size=\"2\"><b>"), "{p2}");
    assert!(
        p2.contains(&format!("<a id=\"fp{tocpos:010}\"></a>vẫn đậm")),
        "{p2}"
    );
    assert!(
        p2.ends_with("<a id=\"fp0000099999\"></a></body></html>"),
        "{p2}"
    );
    assert_eq!(
        b.toc,
        vec![
            toc("Chương 1", &format!("text/part0000.html#fp{ch1:010}"), 0),
            toc("Chương 2", &format!("text/part0001.html#fp{ch2:010}"), 0),
        ]
    );
    assert_eq!(b.resources.len(), 1);
    assert_eq!(b.resources[0].path, "images/00001.jpg");
    assert_eq!(b.resources[0].mime, "image/jpeg");
}

#[test]
fn mobi6_cp1252_va_tieu_de() {
    // cp1252: byte Latin-1 → UTF-8, thực thể giữ nguyên; không guide → mục lục theo tiêu đề.
    let html = b"<html><head><title>x</title></head><body><h1>Ch\xfa &#273;\xe2u</h1><p>\x93Tr\xedch\x94</p><h2>M\xf4t</h2>\
                 <p>a</p></body></html>";
    let mut r0 = Rec0 {
        encoding: 1252,
        text_len: html.len() as u32,
        text_records: 1,
        ..Default::default()
    };
    r0.full_name = b"T\xean s\xe1ch".to_vec();
    let b = convert(&pdb(&[r0.build(), html.to_vec(), EOF.to_vec()])).unwrap();
    assert_eq!(b.title, "Tên sách");
    let p0 = doc(&b, "text/part0000.html");
    assert!(
        p0.contains("<h1 id=\"gbh1\">Chú &#273;âu</h1><p>\u{201C}Trích\u{201D}</p>"),
        "{p0}"
    );
    assert_eq!(
        b.toc,
        vec![
            toc("Chú đâu", "text/part0000.html#gbh1", 0),
            toc("Môt", "text/part0000.html#gbh2", 1)
        ]
    );
}

#[test]
fn mobi6_phan_qua_lon_bi_cat() {
    // ~1,2 MB không ngắt trang → cắt ở đầu khối, mỗi phần ≤ ~300 KB + một khối.
    let mut html = String::from("<html><body>");
    for i in 0..12_000 {
        html.push_str(&format!(
            "<p>Đoạn số {i} — một câu chữ tiếng Việt khá dài cho đủ cỡ.</p>\n"
        ));
    }
    html.push_str("</body></html>");
    let b = convert(&pdb(&mobi6(&html, &[], vec![]))).unwrap();
    assert!(b.spine.len() >= 4, "{}", b.spine.len());
    for it in &b.spine {
        assert!(it.data.len() < 330 * 1024, "{} {}", it.path, it.data.len());
        let s = String::from_utf8(it.data.clone()).unwrap();
        assert!(
            s.contains("<body><p>Đoạn số") && s.ends_with("</p>\n</body></html>"),
            "{}",
            &s[..200]
        );
    }
    assert_eq!(b.toc.len(), b.spine.len());
    assert!(b.toc[1].title.starts_with("Đoạn số"), "{:?}", b.toc[1]);
}

#[test]
fn kf8_tong_hop() {
    let b = convert(&pdb(&kf8(None))).unwrap();
    assert_eq!(b.format, Format::Kf8);
    assert_eq!(b.title, "Sách KF8");
    assert_eq!(b.authors, vec!["Tác Giả".to_string()]);
    assert_eq!(b.language.as_deref(), Some("vi"));
    assert_eq!(b.cover.as_deref(), Some("images/00001.jpg"));
    assert_eq!(
        b.spine
            .iter()
            .map(|i| (i.path.as_str(), i.mime.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("text/part0000.html", "application/xhtml+xml"),
            ("text/part0001.html", "application/xhtml+xml")
        ]
    );
    let p0 = doc(&b, "text/part0000.html");
    assert!(
        p0.contains("<body aid=\"0\"><div id=\"gbpos2\" aid=\"1\"><h1>Chương 1</h1>"),
        "{p0}"
    );
    assert!(
        p0.contains("href=\"part0001.html#gbpos1\">sang chương 2</a>"),
        "{p0}"
    );
    assert!(p0.contains("href=\"part0001.html#c2\">mục 2.1</a>"), "{p0}");
    assert!(
        p0.contains("<img src=\"../images/00001.jpg\" alt=\"\"/>"),
        "{p0}"
    );
    assert!(p0.contains("href=\"../styles/flow0001.css\""), "{p0}");
    let p1 = doc(&b, "text/part0001.html");
    assert!(
        p1.contains("<body aid=\"2\"><div id=\"gbpos1\" aid=\"3\"><p>Mở đầu.</p><h2 id=\"c2\">"),
        "{p1}"
    );
    assert!(doc(&b, "styles/flow0001.css").contains("src: url(../fonts/00002.ttf)"));
    let font = b
        .resources
        .iter()
        .find(|i| i.path == "fonts/00002.ttf")
        .expect("phông");
    assert_eq!(
        (font.mime.as_str(), font.data.as_slice()),
        ("font/ttf", &b"true\x00\x01 synthetic font data"[..])
    );
    assert_eq!(
        b.toc,
        vec![
            toc("Chương 1", "text/part0000.html#gbpos2", 0),
            toc("Chương 2", "text/part0001.html#gbpos1", 0),
            toc("Mục 2.1", "text/part0001.html#c2", 1),
        ]
    );
}

/// Tệp kép: phần MOBI6 (EXTH 121 → bản ghi đầu KF8) + «BOUNDARY» + phần KF8. `shared` = phần KF8 không có ảnh riêng,
/// dùng ảnh / phông của phần MOBI6 (trường tài nguyên đầu «âm» so với đầu KF8).
fn joint(shared: bool, kf8_patch: impl Fn(&mut Vec<u8>)) -> Vec<u8> {
    let images = if shared {
        vec![jpeg(7), font_record(b"true\x00\x01 shared font")]
    } else {
        vec![jpeg(9)]
    };
    let html = "<html><body><p>Bản MOBI6 của sách.</p><p>Thêm vài chữ cho đủ.</p></body></html>";
    let n6 = mobi6(html, &images, vec![]).len();
    let k8i = n6 + 1;
    let m6 = mobi6(
        html,
        &images,
        vec![
            (503, "Bản MOBI6".as_bytes().to_vec()),
            (121, (k8i as u32).to_be_bytes().to_vec()),
        ],
    );
    let first_img = 2u32; // bản ghi 0 + 1 bản ghi chữ
    let rel = first_img.wrapping_sub(k8i as u32);
    let mut k8 = kf8(if shared { Some(rel) } else { None });
    kf8_patch(&mut k8[0]);
    let mut recs = m6;
    recs.push(b"BOUNDARY".to_vec());
    recs.extend(k8);
    pdb(&recs)
}

#[test]
fn tep_kep_uu_tien_kf8() {
    let b = convert(&joint(false, |_| {})).unwrap();
    assert_eq!((b.format, b.title.as_str()), (Format::Kf8, "Sách KF8"));
    assert_eq!(b.toc.len(), 3);
    assert_eq!(b.cover.as_deref(), Some("images/00001.jpg"));
    // Ảnh của phần KF8 (đánh dấu 1), không phải ảnh phần MOBI6 (9).
    let img = b
        .resources
        .iter()
        .find(|i| i.path == "images/00001.jpg")
        .unwrap();
    assert_eq!(img.data[11], 1);
    assert!(b.resources.iter().any(|i| i.path == "fonts/00002.ttf"));

    // Ảnh / phông dùng chung với phần MOBI6.
    let b = convert(&joint(true, |_| {})).unwrap();
    assert_eq!(b.format, Format::Kf8);
    let img = b
        .resources
        .iter()
        .find(|i| i.path == "images/00001.jpg")
        .unwrap();
    assert_eq!(img.data[11], 7);
    let font = b
        .resources
        .iter()
        .find(|i| i.path == "fonts/00002.ttf")
        .unwrap();
    assert_eq!(font.data, b"true\x00\x01 shared font");
}

#[test]
fn tep_kep_kf8_hong_thi_dung_mobi6() {
    // Kiểu nén lạ ở phần KF8 → dùng phần MOBI6.
    let b = convert(&joint(false, |r| {
        r[0..2].copy_from_slice(&99u16.to_be_bytes())
    }))
    .unwrap();
    assert_eq!((b.format, b.title.as_str()), (Format::Mobi6, "Bản MOBI6"));
    assert!(doc(&b, "text/part0000.html").contains("Bản MOBI6 của sách."));
    // Phần KF8 có DRM → cả sách DRM.
    assert_eq!(
        convert(&joint(false, |r| r[12..14].copy_from_slice(&2u16.to_be_bytes()))).unwrap_err(),
        MobiError::Drm
    );
}

#[test]
fn loi_ro_rang() {
    assert!(matches!(convert(b""), Err(MobiError::Invalid(_))));
    assert!(matches!(convert(&[0u8; 200]), Err(MobiError::Invalid(_))));
    let (mut recs, _) = mobi6_book();
    let mut r0 = recs[0].clone();
    r0[12..14].copy_from_slice(&2u16.to_be_bytes());
    recs[0] = r0;
    assert_eq!(convert(&pdb(&recs)).unwrap_err(), MobiError::Drm);
    let (mut recs, _) = mobi6_book();
    recs[0][0..2].copy_from_slice(&7u16.to_be_bytes());
    assert_eq!(
        convert(&pdb(&recs)).unwrap_err(),
        MobiError::Unsupported("kiểu nén 7".into())
    );
    // PalmDOC thuần (TEXtREAd).
    let text = b"Dong mot\nDong hai & <ba>\n\nDoan sau";
    let mut r0 = vec![0u8; 16];
    r0[0..2].copy_from_slice(&1u16.to_be_bytes());
    r0[4..8].copy_from_slice(&(text.len() as u32).to_be_bytes());
    r0[8..10].copy_from_slice(&1u16.to_be_bytes());
    r0[10..12].copy_from_slice(&4096u16.to_be_bytes());
    let mut d = pdb(&[r0, text.to_vec()]);
    d[60..68].copy_from_slice(b"TEXtREAd");
    let b = convert(&d).unwrap();
    assert_eq!(b.title, "Test Book");
    assert!(doc(&b, "text/part0000.html")
        .contains("<p>Dong mot</p>\n<p>Dong hai &amp; &lt;ba&gt;</p>\n<p>Doan sau</p>"));
    d[60..68].copy_from_slice(b"XXXXXXXX");
    assert!(matches!(convert(&d), Err(MobiError::Invalid(_))));
}

#[test]
fn bo_cuc_doc_khong_lam_cham() {
    // MOBI6: 200 000 thẻ <title> / <noscript> không đóng — không được tìm lại thẻ đóng cả sách mỗi lần.
    let mut html = String::from("<html><body><p>Mở đầu</p>");
    for i in 0..200_000 {
        html.push_str(if i % 2 == 0 {
            "<title>t"
        } else {
            "<noscript>n"
        });
    }
    html.push_str("</body></html>");
    let t = Instant::now();
    let b = convert(&pdb(&mobi6(&html, &[], vec![]))).unwrap();
    assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
    assert!(doc(&b, "text/part0000.html").contains("Mở đầu"));

    // KF8: 2000 mảnh chèn luân phiên đầu / cuối một khung 2 MB — chỗ hở không được dời qua lại vô hạn.
    let skel = format!("<html><body>{}</body></html>", "x".repeat(2_000_000));
    let piece = "<p>frag</p>";
    let n = 2000usize;
    let mut text = skel.clone();
    let mut entries = Vec::new();
    for i in 0..n {
        let at = if i % 2 == 0 {
            12
        } else {
            skel.len() - 14 + i * piece.len()
        };
        entries.push((
            format!("{at:010}").into_bytes(),
            vec![
                (2, vec![0]),
                (3, vec![0]),
                (4, vec![i as u32]),
                (6, vec![0, piece.len() as u32]),
            ],
        ));
        text.push_str(piece);
    }
    let trs = text_records(text.as_bytes());
    let skel_at = 1 + trs.len() as u32;
    let skel_idx = indx(
        &[(1, 1, 0x01), (6, 2, 0x02)],
        &[(
            b"SKEL0000000000".to_vec(),
            vec![(1, vec![n as u32]), (6, vec![0, skel.len() as u32])],
        )],
        &[],
    );
    let frag_at = skel_at + skel_idx.len() as u32;
    let frag_idx = indx(
        &[(2, 1, 0x01), (3, 1, 0x02), (4, 1, 0x04), (6, 2, 0x08)],
        &entries,
        &[b"P-//*[@aid='0']"],
    );
    let fdst_at = frag_at + frag_idx.len() as u32;
    let r0 = Rec0 {
        version: 8,
        text_len: text.len() as u32,
        text_records: trs.len() as u16,
        fdst: fdst_at,
        skel: skel_at,
        frag: frag_at,
        exth: vec![(100, b"A".to_vec()), (503, b"Nhieu manh".to_vec())],
        ..Default::default()
    };
    let mut recs = vec![r0.build()];
    recs.extend(trs);
    recs.extend(skel_idx);
    recs.extend(frag_idx);
    recs.push(fdst(&[(0, text.len() as u32)]));
    recs.push(EOF.to_vec());
    let t = Instant::now();
    let b = convert(&pdb(&recs)).unwrap();
    assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
    let d = &b.spine[0].data;
    assert_eq!(d.len(), text.len());
    assert_eq!(
        d.windows(piece.len())
            .filter(|w| *w == piece.as_bytes())
            .count(),
        n
    );
}

/// Bộ sinh số giả ngẫu nhiên (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn must_not_hang(d: &[u8], worst: &mut Duration) {
    let t = Instant::now();
    let _ = convert(d);
    let el = t.elapsed();
    if el > *worst {
        *worst = el;
    }
    assert!(el < Duration::from_secs(5), "chuyển quá lâu: {el:?}");
}

#[test]
fn du_lieu_hong_khong_panic() {
    let samples = [pdb(&mobi6_book().0), pdb(&kf8(None)), joint(true, |_| {})];
    // Số lần đổi byte / mẫu: mặc định 1500; chạy dài: GB_MOBI_FUZZ=200000 cargo test --release --test synth.
    let iters: usize = std::env::var("GB_MOBI_FUZZ")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1500);
    let mut rng = Rng(std::env::var("GB_MOBI_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0x9E37_79B9_7F4A_7C15));
    let mut worst = Duration::ZERO;
    for s in &samples {
        // Cắt cụt.
        for cut in (0..s.len()).step_by((s.len() / 400).max(1)) {
            must_not_hang(&s[..cut], &mut worst);
        }
        // Đổi byte ngẫu nhiên — nhiều ở đầu tệp / bản ghi 0 / bảng chỉ mục.
        for _ in 0..iters {
            let mut d = s.clone();
            for _ in 0..1 + rng.below(8) {
                let i = if rng.below(2) == 0 {
                    rng.below(d.len().min(2048))
                } else {
                    rng.below(d.len())
                };
                d[i] = match rng.below(4) {
                    0 => 0,
                    1 => 0xFF,
                    2 => d[i] ^ (1 << rng.below(8)),
                    _ => rng.next() as u8,
                };
            }
            must_not_hang(&d, &mut worst);
        }
    }
    // Rác.
    for len in [0usize, 1, 77, 78, 79, 100, 1000, 70_000] {
        let mut d: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        must_not_hang(&d, &mut worst);
        if len >= 78 {
            d[60..68].copy_from_slice(b"BOOKMOBI");
            must_not_hang(&d, &mut worst);
        }
    }
    // Trường đầu vô lý: số bản ghi chữ / tài nguyên / chỉ mục trỏ lung tung.
    let (recs, _) = mobi6_book();
    for (off, val) in [
        (8usize, 0xFFFFu32),
        (4, 0xFFFF_FFFF),
        (0x6C, 0),
        (0x6C, 3),
        (0xF4, 1),
        (0xF4, 2),
        (0xC0, 1),
        (0x24, 8),
    ] {
        let mut r = recs.clone();
        if off == 8 {
            r[0][8..10].copy_from_slice(&(val as u16).to_be_bytes());
        } else {
            r[0][off..off + 4].copy_from_slice(&val.to_be_bytes());
        }
        must_not_hang(&pdb(&r), &mut worst);
    }
    let k = kf8(None);
    for (off, val) in [
        (0xFCusize, 1u32),
        (0xF8, 1),
        (0xC0, 2),
        (0xF4, 1),
        (0xFC, 0),
        (0x6C, 1),
        (0x6C, 0xFFFF_FFF0),
    ] {
        let mut r = k.clone();
        r[0][off..off + 4].copy_from_slice(&val.to_be_bytes());
        must_not_hang(&pdb(&r), &mut worst);
    }
    println!("lâu nhất: {worst:?}");
}
