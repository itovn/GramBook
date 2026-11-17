//! Bản ghi tài nguyên: nhận ảnh theo byte đầu (JPEG / PNG / GIF / BMP), giải bản ghi phông «FONT» của KF8.
//!
//! Bản ghi không phải ảnh (FLIS, FCIS, SRCS, RESC, DATP, FDST, CMET, PAGE, BOUNDARY, bản ghi kết thúc `E9 8E 0D 0A`…)
//! không khớp byte đầu nào nên tự bị bỏ.

use crate::inflate;

/// (đuôi tệp, mime) nếu `d` là ảnh.
pub(crate) fn image_type(d: &[u8]) -> Option<(&'static str, &'static str)> {
    if d.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("jpg", "image/jpeg"))
    } else if d.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("png", "image/png"))
    } else if d.starts_with(b"GIF87a") || d.starts_with(b"GIF89a") {
        Some(("gif", "image/gif"))
    } else if d.len() > 26 && d.starts_with(b"BM") && {
        let size = u32::from_le_bytes([d[2], d[3], d[4], d[5]]) as usize;
        (26..=d.len() + 16).contains(&size)
    } {
        Some(("bmp", "image/bmp"))
    } else {
        None
    }
}

/// Phông đã giải: (dữ liệu, đuôi, mime).
pub(crate) type Font = (Vec<u8>, &'static str, &'static str);

/// Chặn cỡ phông sau giải nén.
const MAX_FONT: usize = 32 * 1024 * 1024;

/// Bản ghi «FONT»: cỡ gốc (u32), cờ (bit 0 = zlib, bit 1 = XOR), vị trí dữ liệu, độ dài + vị trí khoá XOR. XOR áp lên
/// 1040 byte đầu của dữ liệu (như Calibre `read_font_record`).
pub(crate) fn font(d: &[u8]) -> Option<Font> {
    if !d.starts_with(b"FONT") {
        return None;
    }
    let be = |o: usize| -> Option<usize> {
        let b = d.get(o..o + 4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let (usize_, flags, dstart, xor_len, xor_start) = (be(4)?, be(8)?, be(12)?, be(16)?, be(20)?);
    let mut data = d.get(dstart..)?.to_vec();
    if flags & 2 != 0 {
        let key = d.get(xor_start..xor_start.checked_add(xor_len)?)?;
        if key.is_empty() {
            return None;
        }
        for (n, b) in data.iter_mut().take(1040).enumerate() {
            *b ^= key[n % key.len()];
        }
    }
    if flags & 1 != 0 {
        data = inflate::zlib(&data, MAX_FONT.min(usize_.max(1)))?;
        if data.len() != usize_ {
            return None;
        }
    }
    let (ext, mime) = match data.get(0..4)? {
        b"\x00\x01\x00\x00" | b"true" | b"ttcf" => ("ttf", "font/ttf"),
        b"OTTO" => ("otf", "font/otf"),
        b"wOFF" => ("woff", "font/woff"),
        _ => return None,
    };
    Some((data, ext, mime))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nhan_anh() {
        assert_eq!(
            image_type(b"\xFF\xD8\xFF\xE0xx"),
            Some(("jpg", "image/jpeg"))
        );
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\nxx").map(|t| t.0), Some("png"));
        assert_eq!(image_type(b"GIF89a").map(|t| t.0), Some("gif"));
        let mut bmp = b"BM".to_vec();
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.resize(40, 0);
        assert_eq!(image_type(&bmp).map(|t| t.0), Some("bmp"));
        for other in [
            &b"FLIS\0\0"[..],
            b"FCIS",
            b"\xe9\x8e\r\n",
            b"BOUNDARY",
            b"RESC",
            b"",
            b"BM",
        ] {
            assert_eq!(image_type(other), None);
        }
    }

    #[test]
    fn ban_ghi_phong() {
        // Phông «true» nén zlib (khối lưu nguyên) rồi XOR khoá 2 byte.
        let plain = b"true\x00\x01rest of font";
        let mut z = vec![0x78, 0x01, 0x01];
        z.extend_from_slice(&(plain.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(plain.len() as u16)).to_le_bytes());
        z.extend_from_slice(plain);
        z.extend_from_slice(&[0, 0, 0, 0]);
        let key = [0x5Au8, 0xA5];
        let mut rec = b"FONT".to_vec();
        for v in [plain.len() as u32, 3, 26, 2, 24] {
            rec.extend_from_slice(&v.to_be_bytes());
        }
        rec.extend_from_slice(&key);
        for (n, b) in z.iter().enumerate() {
            rec.push(b ^ key[n % 2]);
        }
        let (data, ext, mime) = font(&rec).unwrap();
        assert_eq!(
            (data.as_slice(), ext, mime),
            (&plain[..], "ttf", "font/ttf")
        );
        // Hỏng: cắt cụt từng byte → None, không panic.
        for cut in 0..rec.len() {
            let _ = font(&rec[..cut]);
        }
        assert!(font(b"FONTxxxx").is_none());
    }
}
