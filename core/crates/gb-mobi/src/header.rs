//! Bản ghi 0 của sách: đầu PalmDOC (16 byte) + đầu MOBI + EXTH (thông tin sách).
//!
//! Vị trí các trường tính từ đầu bản ghi 0 (đầu MOBI bắt đầu ở 0x10). Trường nằm ngoài độ dài đầu MOBI khai báo thì coi
//! như không có (chỉ số = [`NULL`]).

use crate::MobiError;

/// Giá trị «không có» của các trường chỉ số bản ghi.
pub(crate) const NULL: u32 = 0xFFFF_FFFF;

/// Bản ghi 0 đã đọc.
pub(crate) struct Header<'a> {
    /// 1 = không nén, 2 = PalmDOC, 17480 = HUFF/CDIC.
    pub compression: u16,
    pub text_length: u32,
    pub text_records: u16,
    pub record_size: u16,
    /// 0 = không mã hoá; 1, 2 = DRM.
    pub encryption: u16,
    /// Không có với PalmDOC thuần («TEXtREAd»).
    pub mobi: Option<Mobi<'a>>,
}

/// Đầu MOBI.
pub(crate) struct Mobi<'a> {
    /// 1252 hoặc 65001.
    pub encoding: u32,
    pub version: u32,
    pub full_name: &'a [u8],
    pub locale: u32,
    /// Bản ghi tài nguyên (ảnh…) đầu tiên.
    pub first_resource: u32,
    pub huff_offset: u32,
    pub huff_count: u32,
    /// KF8: bản ghi FDST.
    pub fdst: u32,
    /// Cờ dữ liệu đuôi bản ghi chữ (0xF2).
    pub extra_flags: u16,
    /// Chỉ mục NCX (mục lục).
    pub ncx: u32,
    /// KF8: chỉ mục mảnh (FRAG) và khung (SKEL).
    pub frag: u32,
    pub skel: u32,
    /// Các mục EXTH (kiểu, dữ liệu) theo thứ tự trong tệp.
    pub exth: Vec<(u32, &'a [u8])>,
}

impl<'a> Mobi<'a> {
    /// Mục EXTH đầu tiên kiểu `t`.
    pub fn exth(&self, t: u32) -> Option<&'a [u8]> {
        self.exth.iter().find(|(k, _)| *k == t).map(|(_, v)| *v)
    }

    /// Mọi mục EXTH kiểu `t` (vd tác giả — có thể lặp).
    pub fn exth_all(&self, t: u32) -> Vec<&'a [u8]> {
        self.exth
            .iter()
            .filter(|(k, _)| *k == t)
            .map(|(_, v)| *v)
            .collect()
    }

    /// Mục EXTH kiểu `t` đọc như số nguyên big-endian (≤ 4 byte).
    pub fn exth_u32(&self, t: u32) -> Option<u32> {
        let v = self.exth(t)?;
        if v.is_empty() || v.len() > 4 {
            return None;
        }
        Some(v.iter().fold(0u32, |a, &b| (a << 8) | b as u32))
    }
}

fn be16(d: &[u8], off: usize) -> Option<u16> {
    let b = d.get(off..off.checked_add(2)?)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

fn be32(d: &[u8], off: usize) -> Option<u32> {
    let b = d.get(off..off.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Đọc bản ghi 0. `mobi_expected` = tệp «BOOKMOBI» (phải có đầu MOBI).
pub(crate) fn parse(rec0: &[u8], mobi_expected: bool) -> Result<Header<'_>, MobiError> {
    if rec0.len() < 16 {
        return Err(MobiError::Invalid("bản ghi đầu quá ngắn".into()));
    }
    let compression = be16(rec0, 0).unwrap_or(0);
    let text_length = be32(rec0, 4).unwrap_or(0);
    let text_records = be16(rec0, 8).unwrap_or(0);
    let record_size = be16(rec0, 10).unwrap_or(0);
    let encryption = be16(rec0, 12).unwrap_or(0);
    let mobi = if rec0.get(16..20) == Some(b"MOBI".as_slice()) {
        Some(parse_mobi(rec0))
    } else if mobi_expected {
        return Err(MobiError::Invalid("thiếu đầu MOBI".into()));
    } else {
        None
    };
    Ok(Header {
        compression,
        text_length,
        text_records,
        record_size,
        encryption,
        mobi,
    })
}

fn parse_mobi(rec0: &[u8]) -> Mobi<'_> {
    let header_len = be32(rec0, 0x14).unwrap_or(0);
    // Trường chỉ hợp lệ khi nằm trong đầu MOBI khai báo.
    let lim = rec0.len().min(16usize.saturating_add(header_len as usize));
    let h = &rec0[..lim];
    let u32_or = |off: usize, def: u32| be32(h, off).unwrap_or(def);
    let full_name = {
        let off = u32_or(0x54, 0) as usize;
        let len = u32_or(0x58, 0) as usize;
        match off.checked_add(len) {
            Some(end) if off >= 16 && end <= rec0.len() => &rec0[off..end],
            _ => &[][..],
        }
    };
    // Cờ đuôi bản ghi ở 0xF2 — chỉ khi đầu MOBI đủ dài (≥ 0xE4) và không vô lý (Calibre: ≤ 500).
    let extra_flags = if (0xE4..=500).contains(&header_len) {
        be16(h, 0xF2).unwrap_or(0)
    } else {
        0
    };
    let exth_flags = u32_or(0x80, 0);
    let exth = if exth_flags & 0x40 != 0 {
        parse_exth(rec0, 16usize.saturating_add(header_len as usize))
    } else {
        Vec::new()
    };
    Mobi {
        encoding: u32_or(0x1C, 1252),
        version: u32_or(0x24, 0),
        full_name,
        locale: u32_or(0x5C, 0),
        first_resource: u32_or(0x6C, NULL),
        huff_offset: u32_or(0x70, NULL),
        huff_count: u32_or(0x74, 0),
        fdst: u32_or(0xC0, NULL),
        extra_flags,
        ncx: u32_or(0xF4, NULL),
        frag: u32_or(0xF8, NULL),
        skel: u32_or(0xFC, NULL),
        exth,
    }
}

/// EXTH: «EXTH», độ dài, số mục, rồi các mục (kiểu u32, độ dài u32 tính cả 8 byte đầu, dữ liệu).
fn parse_exth(rec0: &[u8], off: usize) -> Vec<(u32, &[u8])> {
    let mut out = Vec::new();
    if rec0.get(off..off.saturating_add(4)) != Some(b"EXTH".as_slice()) {
        return out;
    }
    let count = be32(rec0, off + 8).unwrap_or(0).min(4096);
    let mut p = off + 12;
    for _ in 0..count {
        let (Some(t), Some(len)) = (be32(rec0, p), be32(rec0, p + 4)) else {
            break;
        };
        let len = len as usize;
        if len < 8 {
            break;
        }
        let Some(data) = p.checked_add(len).and_then(|end| rec0.get(p + 8..end)) else {
            break;
        };
        out.push((t, data));
        p += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bản ghi 0 tổng hợp: PalmDOC + MOBI (độ dài `hlen`) + EXTH.
    fn rec0(hlen: u32, exth: &[(u32, &[u8])]) -> Vec<u8> {
        let mut r = vec![0u8; 16 + hlen as usize];
        r[0..2].copy_from_slice(&2u16.to_be_bytes());
        r[4..8].copy_from_slice(&5000u32.to_be_bytes());
        r[8..10].copy_from_slice(&2u16.to_be_bytes());
        r[10..12].copy_from_slice(&4096u16.to_be_bytes());
        r[16..20].copy_from_slice(b"MOBI");
        r[0x14..0x18].copy_from_slice(&hlen.to_be_bytes());
        r[0x1C..0x20].copy_from_slice(&65001u32.to_be_bytes());
        r[0x24..0x28].copy_from_slice(&6u32.to_be_bytes());
        r[0x80..0x84].copy_from_slice(&0x40u32.to_be_bytes());
        if hlen >= 0xE4 {
            r[0xF2..0xF4].copy_from_slice(&3u16.to_be_bytes());
        }
        let mut e = Vec::new();
        for (t, v) in exth {
            e.extend_from_slice(&t.to_be_bytes());
            e.extend_from_slice(&(v.len() as u32 + 8).to_be_bytes());
            e.extend_from_slice(v);
        }
        r.extend_from_slice(b"EXTH");
        r.extend_from_slice(&(e.len() as u32 + 12).to_be_bytes());
        r.extend_from_slice(&(exth.len() as u32).to_be_bytes());
        r.extend_from_slice(&e);
        let name_off = r.len() as u32;
        r.extend_from_slice(b"Ten sach");
        r[0x54..0x58].copy_from_slice(&name_off.to_be_bytes());
        r[0x58..0x5C].copy_from_slice(&8u32.to_be_bytes());
        r
    }

    #[test]
    fn doc_dau_mobi_va_exth() {
        let r = rec0(
            0xE8,
            &[
                (100, b"A"),
                (100, b"B"),
                (503, "Tên".as_bytes()),
                (201, &[0, 0, 0, 1]),
            ],
        );
        let h = parse(&r, true).unwrap();
        assert_eq!(h.compression, 2);
        let m = h.mobi.unwrap();
        assert_eq!(m.encoding, 65001);
        assert_eq!(m.extra_flags, 3);
        assert_eq!(m.full_name, b"Ten sach");
        assert_eq!(m.exth_all(100), vec![b"A".as_slice(), b"B".as_slice()]);
        assert_eq!(m.exth(503), Some("Tên".as_bytes()));
        assert_eq!(m.exth_u32(201), Some(1));
        // Trường NCX ở 0xF4 nằm trong đầu dài 0xE8 → đọc được (ở đây = 0).
        assert_eq!(m.ncx, 0);
    }

    #[test]
    fn dau_ngan_khong_co_co_duoi() {
        let r = rec0(0xD0, &[]);
        let m = parse(&r, true).unwrap().mobi.unwrap();
        assert_eq!(m.extra_flags, 0);
        assert_eq!(m.ncx, NULL);
        // FDST (0xC0) vẫn nằm trong đầu dài 0xD0.
        assert_eq!(m.fdst, 0);
        // EXTH hỏng (độ dài mục < 8) → dừng, không panic.
        let mut bad = rec0(0xE8, &[(100, b"A")]);
        let p = 16 + 0xE8 + 12 + 4;
        bad[p..p + 4].copy_from_slice(&3u32.to_be_bytes());
        assert!(parse(&bad, true).unwrap().mobi.unwrap().exth.is_empty());
        assert!(parse(&r[..10], true).is_err());
        // Bản ghi bị cắt giữa đầu MOBI: trường thiếu lấy giá trị mặc định.
        let cut = parse(&r[..40], true).unwrap().mobi.unwrap();
        assert_eq!((cut.first_resource, cut.extra_flags), (NULL, 0));
        let mut no_mobi = r.clone();
        no_mobi[16..20].copy_from_slice(b"XXXX");
        assert!(parse(&no_mobi, true).is_err());
        assert!(parse(&no_mobi, false).unwrap().mobi.is_none());
    }
}
