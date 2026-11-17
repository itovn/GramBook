//! Thông tin sách từ EXTH (100 tác giả — có thể lặp, 101 nhà xuất bản, 103 giới thiệu, 503 tên cập nhật, 524 ngôn ngữ,
//! 201 / 202 ảnh bìa / ảnh nhỏ, 129 ảnh bìa KF8) và đầu MOBI (tên đầy đủ, mã vùng).

use crate::header::{Header, NULL};
use crate::pdb::Pdb;
use crate::text::{clean_label, decode_entities, decode_str, strip_tags, Encoding};

pub(crate) struct Meta {
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    /// Vị trí ảnh bìa / ảnh nhỏ tính từ bản ghi tài nguyên đầu (0 = ảnh 1).
    pub cover: Option<u32>,
    pub thumb: Option<u32>,
    /// KF8: số thứ tự ảnh bìa từ «kindle:embed:XXXX» (EXTH 129), 1 = ảnh đầu.
    pub cover_embed: Option<u32>,
}

/// Mã ngôn ngữ từ trường mã vùng (byte thấp = ngôn ngữ kiểu Windows).
fn locale_lang(locale: u32) -> Option<&'static str> {
    Some(match locale & 0xFF {
        0x01 => "ar",
        0x02 => "bg",
        0x03 => "ca",
        0x04 => "zh",
        0x05 => "cs",
        0x06 => "da",
        0x07 => "de",
        0x08 => "el",
        0x09 => "en",
        0x0A => "es",
        0x0B => "fi",
        0x0C => "fr",
        0x0D => "he",
        0x0E => "hu",
        0x10 => "it",
        0x11 => "ja",
        0x12 => "ko",
        0x13 => "nl",
        0x14 => "no",
        0x15 => "pl",
        0x16 => "pt",
        0x18 => "ro",
        0x19 => "ru",
        0x1A => "hr",
        0x1B => "sk",
        0x1D => "sv",
        0x1E => "th",
        0x1F => "tr",
        0x21 => "id",
        0x22 => "uk",
        0x2A => "vi",
        0x3E => "ms",
        _ => return None,
    })
}

/// Chuỗi thông tin: giải mã, giải thực thể, gộp khoảng trắng.
fn meta_str(b: &[u8], enc: Encoding) -> String {
    clean_label(&decode_str(b, enc), 1000)
}

/// Đọc thông tin sách. `outer` = đầu MOBI6 của tệp kép (bổ sung trường thiếu ở phần KF8).
pub(crate) fn read(pdb: &Pdb, h: &Header, outer: Option<&Header>) -> Meta {
    let heads: Vec<&Header> = std::iter::once(h).chain(outer).collect();
    let enc_of = |hh: &Header| {
        hh.mobi
            .as_ref()
            .map_or(Encoding::Cp1252, |m| Encoding::from_codepage(m.encoding))
    };
    // Mục EXTH kiểu t đầu tiên (ưu tiên phần KF8), kèm mã chữ của phần đó.
    let first = |t: u32| {
        heads.iter().find_map(|hh| {
            let m = hh.mobi.as_ref()?;
            m.exth(t)
                .map(|v| meta_str(v, enc_of(hh)))
                .filter(|s| !s.is_empty())
        })
    };
    let num = |t: u32| {
        heads
            .iter()
            .find_map(|hh| hh.mobi.as_ref()?.exth_u32(t))
            .filter(|&v| v != NULL)
    };
    let title = first(503)
        .or_else(|| {
            heads.iter().find_map(|hh| {
                let m = hh.mobi.as_ref()?;
                Some(meta_str(m.full_name, enc_of(hh))).filter(|s| !s.is_empty())
            })
        })
        .unwrap_or_else(|| {
            let n: String = decode_str(pdb.name, Encoding::Cp1252).replace('_', " ");
            clean_label(&n, 200)
        });
    let mut authors: Vec<String> = Vec::new();
    for hh in &heads {
        let Some(m) = hh.mobi.as_ref() else { continue };
        for a in m.exth_all(100) {
            let a = meta_str(a, enc_of(hh));
            if !a.is_empty() && !authors.contains(&a) {
                authors.push(a);
            }
        }
        if !authors.is_empty() {
            break;
        }
    }
    let language = first(524).or_else(|| {
        heads
            .iter()
            .find_map(|hh| locale_lang(hh.mobi.as_ref()?.locale))
            .map(String::from)
    });
    let description = heads.iter().find_map(|hh| {
        let m = hh.mobi.as_ref()?;
        let raw = decode_str(m.exth(103)?, enc_of(hh));
        // Giới thiệu hay có thẻ HTML (đôi khi đã thoát thành &lt;p&gt;).
        let plain = clean_label(&strip_tags(&decode_entities(&raw)), 4000);
        Some(plain).filter(|s| !s.is_empty())
    });
    let cover_embed = heads.iter().find_map(|hh| {
        let v = hh.mobi.as_ref()?.exth(129)?;
        let s = std::str::from_utf8(v).ok()?;
        let code = s.strip_prefix("kindle:embed:")?;
        let code: String = code
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        crate::kf8::base32(code.as_bytes())
    });
    Meta {
        title,
        authors,
        language,
        publisher: first(101),
        description,
        cover: num(201),
        thumb: num(202),
        cover_embed,
    }
}
