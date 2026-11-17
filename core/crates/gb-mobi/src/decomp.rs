//! Lấy chữ của sách: các bản ghi chữ (bỏ dữ liệu đuôi theo cờ 0xF2) rồi giải nén — không nén, PalmDOC (LZ77) hoặc
//! HUFF/CDIC.
//!
//! Mọi vị trí / độ dài đọc từ tệp đều được kiểm; tổng chữ giải nén chặn ở [`MAX_TEXT`].

use crate::header::{Header, NULL};
use crate::pdb::Pdb;
use crate::MobiError;

/// Chặn chữ giải nén (sách lớn nhất trong kho thử ≈ 59 MB).
pub(crate) const MAX_TEXT: usize = 64 * 1024 * 1024;

/// Độ dài một mục đuôi, đọc ngược từ cuối `d`: mỗi byte góp 7 bit (byte cuối = 7 bit thấp), dừng ở byte có bit 0x80.
fn trailing_entry(d: &[u8]) -> usize {
    let mut result = 0usize;
    let mut bitpos = 0u32;
    let mut size = d.len();
    while size > 0 {
        let v = d[size - 1];
        result |= ((v & 0x7F) as usize) << bitpos;
        bitpos += 7;
        size -= 1;
        if v & 0x80 != 0 || bitpos >= 28 {
            break;
        }
    }
    result
}

/// Tổng độ dài dữ liệu đuôi của một bản ghi chữ theo cờ (getSizeOfTrailingDataEntries): mỗi bit 1..15 là một mục có độ
/// dài tự ghi ở cuối (mục của bit cao nằm trong cùng), bit 0 = các byte ký tự đa byte chồng sang bản ghi sau (2 bit thấp
/// của byte cuối + 1), nằm sát chữ nhất.
pub(crate) fn trailing_size(rec: &[u8], flags: u16) -> usize {
    let mut num = 0usize;
    let mut f = flags >> 1;
    while f != 0 {
        if f & 1 != 0 {
            if num >= rec.len() {
                return rec.len();
            }
            num = num.saturating_add(trailing_entry(&rec[..rec.len() - num]));
        }
        f >>= 1;
    }
    if flags & 1 != 0 && num < rec.len() {
        num += (rec[rec.len() - num - 1] & 3) as usize + 1;
    }
    num.min(rec.len())
}

/// Giải nén PalmDOC (LZ77 kiểu Palm) một bản ghi, nối vào `out`. Tham chiếu lùi chỉ trong bản ghi này; tham chiếu hỏng bị
/// bỏ qua. Lỗi khi `out` vượt `limit`.
///
/// Đường nhanh: giải vào bộ đệm trên ngăn xếp (bản ghi thường ra đúng 4096 byte); bản ghi nở quá 8 KB (dữ liệu lạ / độc)
/// thì giải lại theo đường chậm có kiểm giới hạn từng bước.
pub(crate) fn palmdoc(src: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<(), ()> {
    const CAP: usize = 8192;
    let mut buf = [0u8; CAP];
    let mut o = 0usize;
    let mut i = 0usize;
    let n = src.len();
    while i < n {
        // Mỗi bước ghi ≤ 10 byte.
        if o + 16 > CAP {
            return palmdoc_slow(src, out, limit);
        }
        let c = src[i];
        i += 1;
        match c {
            0x01..=0x08 => {
                let k = (c as usize).min(n - i);
                buf[o..o + k].copy_from_slice(&src[i..i + k]);
                o += k;
                i += k;
            }
            0x00 | 0x09..=0x7F => {
                buf[o] = c;
                o += 1;
            }
            0xC0..=0xFF => {
                buf[o] = b' ';
                buf[o + 1] = c ^ 0x80;
                o += 2;
            }
            0x80..=0xBF => {
                let Some(&c2) = src.get(i) else { break };
                i += 1;
                let pair = ((c as usize) << 8) | c2 as usize;
                let dist = (pair >> 3) & 0x07FF;
                let len = (pair & 7) + 3;
                if dist == 0 || dist > o {
                    continue;
                }
                let from = o - dist;
                if dist >= len {
                    buf.copy_within(from..from + len, o);
                } else {
                    for k in 0..len {
                        buf[o + k] = buf[from + k];
                    }
                }
                o += len;
            }
        }
    }
    if out.len() + o > limit {
        return Err(());
    }
    out.extend_from_slice(&buf[..o]);
    Ok(())
}

/// Đường chậm của [`palmdoc`]: ghi thẳng vào `out`, kiểm giới hạn từng bước.
fn palmdoc_slow(src: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<(), ()> {
    let base = out.len();
    let mut i = 0;
    while i < src.len() {
        let c = src[i];
        i += 1;
        match c {
            // 1..8: chép nguyên n byte kế.
            0x01..=0x08 => {
                let end = (i + c as usize).min(src.len());
                out.extend_from_slice(&src[i..end]);
                i = end;
            }
            0x00 | 0x09..=0x7F => out.push(c),
            // Dấu cách + ký tự.
            0xC0..=0xFF => {
                out.push(b' ');
                out.push(c ^ 0x80);
            }
            // Cặp byte: 11 bit khoảng lùi, 3 bit độ dài (3..10).
            0x80..=0xBF => {
                let Some(&c2) = src.get(i) else { break };
                i += 1;
                let pair = ((c as usize) << 8) | c2 as usize;
                let dist = (pair >> 3) & 0x07FF;
                let len = (pair & 7) + 3;
                if dist == 0 || dist > out.len() - base {
                    continue;
                }
                let from = out.len() - dist;
                if dist >= len {
                    out.extend_from_within(from..from + len);
                } else {
                    for k in 0..len {
                        let b = out[from + k];
                        out.push(b);
                    }
                }
            }
        }
        if out.len() > limit {
            return Err(());
        }
    }
    Ok(())
}

/// Một mục từ điển CDIC.
#[derive(Clone, Copy)]
enum Phrase<'a> {
    /// Chữ thô (cờ 0x8000) — dùng nguyên.
    Raw(&'a [u8]),
    /// Còn nén (giải lần đầu dùng tới rồi nhớ lại).
    Packed(&'a [u8]),
    /// Đã giải: khoảng trong `Huff::arena`.
    Done(usize, usize),
    /// Đang giải — gặp lại nghĩa là tự tham chiếu vòng (dữ liệu hỏng).
    Busy,
}

/// Bộ giải nén HUFF/CDIC (Mobipocket «DH», kiểu nén 17480).
pub(crate) struct Huff<'a> {
    /// Bảng 1: theo 8 bit đầu của mã → (độ dài mã, mã kết thúc, mã lớn nhất).
    dict1: Vec<(u32, bool, u64)>,
    mincode: [u64; 33],
    maxcode: [u64; 33],
    phrases: Vec<Phrase<'a>>,
    arena: Vec<u8>,
}

/// Chặn một mục từ điển sau khi giải, và tổng bộ nhớ các mục đã giải (dữ liệu độc có thể lồng mục để nở cấp số nhân).
const PHRASE_LIMIT: usize = 64 * 1024;
const ARENA_LIMIT: usize = 32 * 1024 * 1024;

fn be32(d: &[u8], o: usize) -> Option<u32> {
    let b = d.get(o..o.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// 8 byte big-endian từ `pos` (thiếu thì đệm 0).
fn be64_padded(d: &[u8], pos: usize) -> u64 {
    let mut v = 0u64;
    for k in 0..8 {
        v = (v << 8) | d.get(pos.saturating_add(k)).copied().unwrap_or(0) as u64;
    }
    v
}

impl<'a> Huff<'a> {
    /// `huff` = bản ghi HUFF, `cdics` = các bản ghi CDIC theo thứ tự.
    pub fn new(huff: &'a [u8], cdics: &[&'a [u8]]) -> Result<Huff<'a>, String> {
        if huff.get(0..8) != Some(b"HUFF\x00\x00\x00\x18".as_slice()) {
            return Err("đầu HUFF hỏng".into());
        }
        let off1 = be32(huff, 8).ok_or("HUFF ngắn")? as usize;
        let off2 = be32(huff, 12).ok_or("HUFF ngắn")? as usize;
        let mut dict1 = Vec::with_capacity(256);
        for i in 0..256 {
            let v = be32(huff, off1.saturating_add(i * 4)).ok_or("HUFF: bảng 1 bị cắt")?;
            let codelen = v & 0x1F;
            let term = v & 0x80 != 0;
            if codelen == 0 || (codelen <= 8 && !term) {
                return Err("HUFF: bảng 1 hỏng".into());
            }
            let maxcode = (((v >> 8) as u64 + 1) << (32 - codelen)).wrapping_sub(1);
            dict1.push((codelen, term, maxcode));
        }
        let mut mincode = [0u64; 33];
        let mut maxcode = [0u64; 33];
        maxcode[0] = (1u64 << 32) - 1;
        for codelen in 1..=32usize {
            let at = off2.saturating_add((codelen - 1) * 8);
            let lo = be32(huff, at).ok_or("HUFF: bảng 2 bị cắt")?;
            let hi = be32(huff, at.saturating_add(4)).ok_or("HUFF: bảng 2 bị cắt")?;
            mincode[codelen] = (lo as u64) << (32 - codelen);
            maxcode[codelen] = (((hi as u64) + 1) << (32 - codelen)).wrapping_sub(1);
        }
        let mut phrases = Vec::new();
        for cdic in cdics {
            if cdic.get(0..8) != Some(b"CDIC\x00\x00\x00\x10".as_slice()) {
                return Err("đầu CDIC hỏng".into());
            }
            let total = be32(cdic, 8).unwrap_or(0) as usize;
            let bits = be32(cdic, 12).unwrap_or(0);
            if bits > 16 {
                return Err("CDIC: số bit hỏng".into());
            }
            let n = (1usize << bits).min(total.saturating_sub(phrases.len()));
            for j in 0..n {
                let Some(o) = cdic.get(16 + j * 2..18 + j * 2) else {
                    break;
                };
                let off = u16::from_be_bytes([o[0], o[1]]) as usize;
                let phrase = match cdic.get(16 + off..18 + off) {
                    Some(l) => {
                        let blen = u16::from_be_bytes([l[0], l[1]]) as usize;
                        let start = 18 + off;
                        let data = &cdic[start..(start + (blen & 0x7FFF)).min(cdic.len())];
                        if blen & 0x8000 != 0 {
                            Phrase::Raw(data)
                        } else {
                            Phrase::Packed(data)
                        }
                    }
                    None => Phrase::Raw(&[]),
                };
                phrases.push(phrase);
            }
        }
        Ok(Huff {
            dict1,
            mincode,
            maxcode,
            phrases,
            arena: Vec::new(),
        })
    }

    /// Giải một bản ghi, nối vào `out` (lỗi khi vượt `limit` hoặc dữ liệu hỏng).
    pub fn decode(&mut self, data: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<(), ()> {
        self.unpack(data, out, limit, 0)
    }

    fn unpack(
        &mut self,
        data: &[u8],
        out: &mut Vec<u8>,
        limit: usize,
        depth: u32,
    ) -> Result<(), ()> {
        if depth > 32 {
            return Err(());
        }
        let mut bitsleft = data.len() as i64 * 8;
        let mut pos = 0usize;
        let mut x = be64_padded(data, pos);
        // Số bit còn dùng được ở nửa thấp của cửa sổ 64 bit (luôn 1..=32 khi lấy mã).
        let mut n: i32 = 32;
        loop {
            if n <= 0 {
                pos += 4;
                x = be64_padded(data, pos);
                n += 32;
            }
            let code = (x >> n) & 0xFFFF_FFFF;
            let (mut codelen, term, mut maxcode) = self.dict1[(code >> 24) as usize];
            if !term {
                while codelen < 33 && code < self.mincode[codelen as usize] {
                    codelen += 1;
                }
                if codelen > 32 {
                    return Err(());
                }
                maxcode = self.maxcode[codelen as usize];
            }
            n -= codelen as i32;
            bitsleft -= codelen as i64;
            if bitsleft < 0 {
                break;
            }
            let r = maxcode.checked_sub(code).ok_or(())? >> (32 - codelen);
            self.emit(usize::try_from(r).map_err(|_| ())?, out, limit, depth)?;
        }
        Ok(())
    }

    fn emit(&mut self, r: usize, out: &mut Vec<u8>, limit: usize, depth: u32) -> Result<(), ()> {
        let packed = match self.phrases.get(r).copied() {
            Some(Phrase::Raw(d)) => {
                out.extend_from_slice(d);
                None
            }
            Some(Phrase::Done(s, e)) => {
                out.extend_from_slice(&self.arena[s..e]);
                None
            }
            Some(Phrase::Packed(d)) => Some(d),
            Some(Phrase::Busy) | None => return Err(()),
        };
        if let Some(d) = packed {
            self.phrases[r] = Phrase::Busy;
            let mut tmp = Vec::new();
            self.unpack(d, &mut tmp, PHRASE_LIMIT, depth + 1)?;
            if self.arena.len() + tmp.len() > ARENA_LIMIT {
                return Err(());
            }
            let s = self.arena.len();
            self.arena.extend_from_slice(&tmp);
            self.phrases[r] = Phrase::Done(s, self.arena.len());
            out.extend_from_slice(&tmp);
        }
        if out.len() > limit {
            Err(())
        } else {
            Ok(())
        }
    }
}

fn load_huff<'a>(pdb: &Pdb<'a>, base: usize, h: &Header) -> Result<Huff<'a>, MobiError> {
    let m = h
        .mobi
        .as_ref()
        .ok_or_else(|| MobiError::Invalid("HUFF/CDIC thiếu đầu MOBI".into()))?;
    if m.huff_offset == NULL || m.huff_count == 0 {
        return Err(MobiError::Invalid("thiếu bản ghi HUFF".into()));
    }
    let first = base.saturating_add(m.huff_offset as usize);
    let count = (m.huff_count as usize).min(256);
    let cdics: Vec<&[u8]> = (1..count)
        .map(|i| pdb.record(first.saturating_add(i)))
        .collect();
    Huff::new(pdb.record(first), &cdics).map_err(MobiError::Invalid)
}

/// Bộ giải theo kiểu nén.
enum Codec<'a> {
    Plain,
    PalmDoc,
    Huff(Box<Huff<'a>>),
}

impl Codec<'_> {
    fn decode(&mut self, data: &[u8], out: &mut Vec<u8>) -> Result<(), MobiError> {
        let r = match self {
            Codec::Plain => {
                out.extend_from_slice(data);
                if out.len() > MAX_TEXT {
                    Err(())
                } else {
                    Ok(())
                }
            }
            Codec::PalmDoc => palmdoc(data, out, MAX_TEXT),
            Codec::Huff(h) => h.decode(data, out, MAX_TEXT),
        };
        r.map_err(|_| {
            if out.len() > MAX_TEXT {
                MobiError::Unsupported("chữ giải nén quá 64 MB".into())
            } else {
                MobiError::Invalid("dữ liệu nén HUFF/CDIC hỏng".into())
            }
        })
    }
}

/// Chữ của sách (bản ghi `base + 1` … `base + số bản ghi chữ`), đã bỏ đuôi và giải nén.
pub(crate) fn extract_text(pdb: &Pdb, base: usize, h: &Header) -> Result<Vec<u8>, MobiError> {
    let first = base.saturating_add(1);
    let end = first.saturating_add(h.text_records as usize).min(pdb.len());
    if h.text_records == 0 || first >= end {
        return Err(MobiError::Invalid("không có bản ghi chữ".into()));
    }
    let mut codec = match h.compression {
        1 => Codec::Plain,
        2 => Codec::PalmDoc,
        17480 => Codec::Huff(Box::new(load_huff(pdb, base, h)?)),
        c => return Err(MobiError::Unsupported(format!("kiểu nén {c}"))),
    };
    let mut flags = h.mobi.as_ref().map_or(0, |m| m.extra_flags);
    // Kiểm cờ đuôi bằng bản ghi chữ đầu: bản ghi (trừ cuối) giải ra đúng «record size» byte. Cờ đọc sai (tệp cũ ghi rác ở
    // 0xF2) mà bỏ cờ lại khớp thì bỏ cờ.
    if flags != 0 && h.record_size > 0 && end - first > 1 {
        let want = h.record_size as usize;
        let rec = pdb.record(first);
        let mut a = Vec::new();
        let ok_a = codec
            .decode(&rec[..rec.len() - trailing_size(rec, flags)], &mut a)
            .is_ok()
            && a.len() == want;
        if !ok_a {
            let mut b = Vec::new();
            if codec.decode(rec, &mut b).is_ok() && b.len() == want {
                flags = 0;
            }
        }
    }
    let hint = (h.text_length as usize)
        .min(MAX_TEXT)
        .min(pdb_size_hint(pdb).saturating_mul(8));
    let mut out = Vec::with_capacity(hint);
    for i in first..end {
        let rec = pdb.record(i);
        let data = &rec[..rec.len() - trailing_size(rec, flags)];
        codec.decode(data, &mut out)?;
    }
    if out.is_empty() {
        return Err(MobiError::Invalid("sách không có chữ".into()));
    }
    Ok(out)
}

fn pdb_size_hint(pdb: &Pdb) -> usize {
    (0..pdb.len()).map(|i| pdb.record(i).len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unpack(src: &[u8]) -> Vec<u8> {
        let mut o = Vec::new();
        palmdoc(src, &mut o, 1 << 20).unwrap();
        o
    }

    #[test]
    fn palmdoc_cac_loai_ma() {
        // Ký tự thường, 0x00, chép nguyên (0x02 "\xE9\x80"), dấu cách + ký tự (0xE1 = " a").
        assert_eq!(unpack(b"ab\x00\x02\xE9\x80\xE1"), b"ab\x00\xE9\x80 a");
        // "abcd" + tham chiếu lùi 4, dài 4 → "abcdabcd": 0x80 | (4 << 3 | (4-3)) = 0x8000 | 0x21.
        assert_eq!(unpack(b"abcd\x80\x21"), b"abcdabcd");
        // Chồng lấn: "ab" + lùi 2 dài 10 → "ab" + "ababababab".
        let pair: u16 = 0x8000 | (2 << 3) | (10 - 3);
        let mut v = b"ab".to_vec();
        v.extend_from_slice(&pair.to_be_bytes());
        assert_eq!(unpack(&v), b"abababababab");
        // Lùi quá xa / khoảng 0 → bỏ qua; cặp thiếu byte cuối → dừng; chép nguyên thiếu byte → cắt.
        assert_eq!(unpack(b"a\x80\x21b"), b"ab");
        assert_eq!(unpack(b"a\x80\x00"), b"a");
        assert_eq!(unpack(b"a\x80"), b"a");
        assert_eq!(unpack(b"a\x05xy"), b"axy");
        // Vượt giới hạn → lỗi.
        let mut o = Vec::new();
        assert!(palmdoc(b"abcdefgh", &mut o, 4).is_err());
        // Bản ghi nở quá bộ đệm nhanh (8 KB) → đường chậm cho cùng kết quả.
        let mut big = b"abcdefgh".to_vec();
        let pair: u16 = 0x8000 | (8 << 3) | 7;
        for _ in 0..2000 {
            big.extend_from_slice(&pair.to_be_bytes());
        }
        let fast = unpack(&big);
        let mut slow = Vec::new();
        palmdoc_slow(&big, &mut slow, 1 << 20).unwrap();
        assert_eq!(fast.len(), 8 + 2000 * 10);
        assert_eq!(fast, slow);
        assert!(fast.chunks(8).all(|c| c == &b"abcdefgh"[..c.len()]));
    }

    #[test]
    fn duoi_ban_ghi() {
        // Không cờ.
        assert_eq!(trailing_size(b"hello", 0), 0);
        // Cờ 1 (đa byte): byte cuối 0x02 → 3 byte.
        assert_eq!(trailing_size(b"hello\x41\x42\x02", 1), 3);
        // Cờ 2: một mục, độ dài 3 ghi ở byte cuối (0x83 = bit dừng + 3).
        assert_eq!(trailing_size(b"helloXY\x83", 2), 3);
        // Cờ 3: mục cờ 2 (2 byte: "Z" + 0x82) ngoài cùng, rồi đa byte (byte 0x00 → 1).
        assert_eq!(trailing_size(b"hello\x00Z\x82", 3), 3);
        // Độ dài nhiều byte: 0x81 0x00 → (1 << 7) | 0 = 128.
        let mut r = vec![b'x'; 200];
        r.extend_from_slice(&[0x81, 0x00]);
        assert_eq!(trailing_size(&r, 2), 128);
        // Cờ 6: hai mục.
        assert_eq!(trailing_size(b"textA\x82B\x82", 6), 4);
        // Hỏng: độ dài vượt bản ghi → cả bản ghi.
        assert_eq!(trailing_size(b"ab\xff", 2), 3);
        assert_eq!(trailing_size(b"", 7), 0);
        assert_eq!(trailing_size(b"\x03", 1), 1);
    }

    /// Bộ HUFF/CDIC tổng hợp: mọi mã dài 8 bit, mã kết thúc; với 8 bit đầu = c, «mã lớn nhất» = ((2c + 1) << 24) - 1 nên
    /// chỉ số mục r = (maxcode - code) >> 24 = c. Mục 0 = "ab" (thô), mục 1 = nén (hai mã 0), mục 2 = nén tự tham chiếu.
    fn huff_tables() -> (Vec<u8>, Vec<u8>) {
        let mut huff = b"HUFF\x00\x00\x00\x18".to_vec();
        huff.extend_from_slice(&24u32.to_be_bytes());
        huff.extend_from_slice(&(24u32 + 1024).to_be_bytes());
        huff.resize(24, 0);
        for c in 0..256u32 {
            huff.extend_from_slice(&(((2 * c) << 8) | 0x80 | 8).to_be_bytes());
        }
        huff.resize(24 + 1024 + 256, 0);
        let mut cdic = b"CDIC\x00\x00\x00\x10".to_vec();
        cdic.extend_from_slice(&3u32.to_be_bytes());
        cdic.extend_from_slice(&8u32.to_be_bytes());
        let entries: [(&[u8], bool); 3] = [(b"ab", true), (&[0, 0], false), (&[2], false)];
        let mut body = Vec::new();
        let mut offs = Vec::new();
        for (data, raw) in entries {
            // Vị trí tính từ byte 16; bảng vị trí chiếm 6 byte.
            offs.push((6 + body.len()) as u16);
            let blen = data.len() as u16 | if raw { 0x8000 } else { 0 };
            body.extend_from_slice(&blen.to_be_bytes());
            body.extend_from_slice(data);
        }
        for o in offs {
            cdic.extend_from_slice(&o.to_be_bytes());
        }
        cdic.extend_from_slice(&body);
        (huff, cdic)
    }

    #[test]
    fn huff_cdic_tong_hop() {
        let (huff, cdic) = huff_tables();
        let mut h = Huff::new(&huff, &[&cdic]).unwrap();
        // Mỗi byte dữ liệu = một mã.
        let mut out = Vec::new();
        h.decode(&[0], &mut out, 1000).unwrap();
        assert_eq!(out, b"ab");
        // Mục 1 (nén: hai mã 0) → "abab"; lần sau dùng bản đã giải.
        let mut out = Vec::new();
        h.decode(&[1], &mut out, 1000).unwrap();
        h.decode(&[1, 0], &mut out, 1000).unwrap();
        assert_eq!(out, b"ababababab");
        // Mục 2 tự tham chiếu → lỗi, không lặp vô tận.
        assert!(h.decode(&[2], &mut Vec::new(), 1000).is_err());
        // Chỉ số ngoài từ điển → lỗi.
        assert!(h.decode(&[200], &mut Vec::new(), 1000).is_err());
        // Vượt giới hạn → lỗi.
        assert!(h.decode(&[1, 1, 1], &mut Vec::new(), 5).is_err());
        // Đầu hỏng.
        assert!(Huff::new(b"HUFF", &[]).is_err());
        assert!(Huff::new(&huff, &[b"CDIC"]).is_err());
    }
}
