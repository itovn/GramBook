//! Chỉ mục INDX của MOBI / KF8 (NCX = mục lục, SKEL = khung tệp, FRAG = mảnh, …).
//!
//! Bản ghi chính: đầu «INDX» + bảng TAGX (các thẻ: số hiệu, số giá trị / mục, mặt nạ, cờ hết byte điều khiển). Sau nó là
//! `count` bản ghi INDX chứa mục (vị trí từng mục trong bảng IDXT), rồi `ncncx` bản ghi CNCX (chuỗi: độ dài VWI + byte; vị
//! trí = số thứ tự bản ghi × 0x10000 + vị trí trong bản ghi). Mỗi mục: tên (1 byte độ dài + byte), các byte điều khiển,
//! rồi giá trị các thẻ (số nguyên độ rộng thay đổi — VWI, 7 bit / byte, byte cuối có bit 0x80).

use std::collections::HashMap;

use crate::pdb::Pdb;

/// Một mục chỉ mục: tên + các thẻ (số hiệu → giá trị).
#[derive(Clone, Debug, Default)]
pub(crate) struct Entry {
    pub ident: Vec<u8>,
    pub tags: Vec<(u8, Vec<u32>)>,
}

impl Entry {
    pub fn tag(&self, t: u8) -> Option<&[u32]> {
        self.tags
            .iter()
            .find(|(k, _)| *k == t)
            .map(|(_, v)| v.as_slice())
    }

    /// Giá trị thứ `i` của thẻ `t`.
    pub fn val(&self, t: u8, i: usize) -> Option<u32> {
        self.tag(t)?.get(i).copied()
    }
}

/// Chỉ mục đã đọc.
#[derive(Default)]
pub(crate) struct Index {
    pub entries: Vec<Entry>,
    pub cncx: HashMap<u32, Vec<u8>>,
}

/// Số nguyên độ rộng thay đổi (đọc xuôi) ở `pos`: (giá trị, số byte). Quá 5 byte / tràn → `None`.
pub(crate) fn vwi(d: &[u8], pos: usize) -> Option<(u32, usize)> {
    let mut v: u32 = 0;
    for k in 0..5 {
        let b = *d.get(pos.checked_add(k)?)?;
        v = v.checked_mul(128)? | (b & 0x7F) as u32;
        if b & 0x80 != 0 {
            return Some((v, k + 1));
        }
    }
    None
}

fn be32(d: &[u8], o: usize) -> Option<u32> {
    let b = d.get(o..o.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn be16(d: &[u8], o: usize) -> Option<u16> {
    let b = d.get(o..o.checked_add(2)?)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

/// Một dòng TAGX: (thẻ, số giá trị / mục, mặt nạ, cờ hết).
type TagX = (u8, u8, u8, u8);

/// Chặn số bản ghi chỉ mục / số mục (tệp độc).
const MAX_RECORDS: usize = 4096;
const MAX_ENTRIES: usize = 200_000;

/// Đọc chỉ mục có bản ghi chính `idx`.
pub(crate) fn read(pdb: &Pdb, idx: usize) -> Result<Index, String> {
    let main = pdb.record(idx);
    if main.get(0..4) != Some(b"INDX".as_slice()) {
        return Err("không phải bản ghi INDX".into());
    }
    let hlen = be32(main, 4).ok_or("INDX ngắn")? as usize;
    let count = (be32(main, 24).ok_or("INDX ngắn")? as usize).min(MAX_RECORDS);
    let ncncx = (be32(main, 52).unwrap_or(0) as usize).min(MAX_RECORDS);
    // TAGX ngay sau đầu INDX (độ dài đầu); dự phòng: trường vị trí TAGX ở 0xB4.
    let tagx_at = if main.get(hlen..hlen.saturating_add(4)) == Some(b"TAGX".as_slice()) {
        hlen
    } else {
        be32(main, 0xB4)
            .map(|v| v as usize)
            .filter(|&o| main.get(o..o.saturating_add(4)) == Some(b"TAGX".as_slice()))
            .ok_or("thiếu TAGX")?
    };
    let (cb_count, tagx) = read_tagx(main, tagx_at).ok_or("TAGX hỏng")?;
    let mut ix = Index::default();
    for r in 0..ncncx {
        let rec = pdb.record(idx.saturating_add(count).saturating_add(1 + r));
        read_cncx(rec, (r as u32) << 16, &mut ix.cncx);
    }
    for r in 1..=count {
        let rec = pdb.record(idx.saturating_add(r));
        read_entries(rec, cb_count, &tagx, &mut ix.entries);
        if ix.entries.len() > MAX_ENTRIES {
            return Err("chỉ mục quá lớn".into());
        }
    }
    Ok(ix)
}

fn read_tagx(d: &[u8], at: usize) -> Option<(usize, Vec<TagX>)> {
    let first_entry = be32(d, at + 4)? as usize;
    let cb_count = be32(d, at + 8)? as usize;
    if cb_count == 0 || cb_count > 8 {
        return None;
    }
    let mut tags = Vec::new();
    let mut i = 12;
    while i + 4 <= first_entry.min(12 + 4 * 64) {
        let t = d.get(at + i..at + i + 4)?;
        tags.push((t[0], t[1], t[2], t[3]));
        i += 4;
    }
    Some((cb_count, tags))
}

fn read_cncx(rec: &[u8], base: u32, out: &mut HashMap<u32, Vec<u8>>) {
    let mut pos = 0usize;
    while pos < rec.len() {
        let Some((len, used)) = vwi(rec, pos) else {
            break;
        };
        let start = pos + used;
        let Some(end) = start.checked_add(len as usize).filter(|&e| e <= rec.len()) else {
            break;
        };
        if len > 0 {
            out.insert(base | pos as u32, rec[start..end].to_vec());
        }
        pos = end;
    }
}

fn read_entries(rec: &[u8], cb_count: usize, tagx: &[TagX], out: &mut Vec<Entry>) {
    if rec.get(0..4) != Some(b"INDX".as_slice()) {
        return;
    }
    let (Some(idxt), Some(n)) = (be32(rec, 20), be32(rec, 24)) else {
        return;
    };
    let idxt = idxt as usize;
    if rec.get(idxt..idxt.saturating_add(4)) != Some(b"IDXT".as_slice()) {
        return;
    }
    let n = (n as usize).min(MAX_ENTRIES);
    let mut offs = Vec::with_capacity(n + 1);
    for j in 0..n {
        match be16(rec, idxt + 4 + 2 * j) {
            Some(o) => offs.push(o as usize),
            None => break,
        }
    }
    offs.push(idxt);
    for w in offs.windows(2) {
        let (start, end) = (w[0], w[1].min(rec.len()));
        if start >= end {
            continue;
        }
        let ilen = rec[start] as usize;
        let Some(ident) = rec.get(start + 1..start + 1 + ilen) else {
            continue;
        };
        let tags = tag_map(rec, start + 1 + ilen, end, cb_count, tagx);
        out.push(Entry {
            ident: ident.to_vec(),
            tags,
        });
    }
}

/// Giá trị các thẻ của một mục (thuật toán getTagMap của KindleUnpack).
fn tag_map(
    rec: &[u8],
    start: usize,
    end: usize,
    cb_count: usize,
    tagx: &[TagX],
) -> Vec<(u8, Vec<u32>)> {
    let mut out = Vec::new();
    let Some(cbytes) = rec.get(start..start.saturating_add(cb_count)) else {
        return out;
    };
    let mut data = start + cb_count;
    let mut cb_index = 0usize;
    // (thẻ, số lần lặp | None, số byte | None, giá trị / mục)
    let mut pending: Vec<(u8, Option<u32>, Option<u32>, u8)> = Vec::new();
    for &(tag, per, mask, eof) in tagx {
        if eof & 1 != 0 {
            cb_index += 1;
            continue;
        }
        let Some(&cb) = cbytes.get(cb_index) else {
            break;
        };
        if mask == 0 {
            continue;
        }
        let value = cb & mask;
        if value == 0 {
            continue;
        }
        if value == mask {
            if mask.count_ones() > 1 {
                // Mọi bit của mặt nạ đều bật: một VWI cho biết số BYTE chứa các giá trị.
                let Some((v, used)) = vwi(rec, data).filter(|_| data < end) else {
                    break;
                };
                data += used;
                pending.push((tag, None, Some(v), per));
            } else {
                pending.push((tag, Some(1), None, per));
            }
        } else {
            pending.push((
                tag,
                Some((value >> mask.trailing_zeros()) as u32),
                None,
                per,
            ));
        }
    }
    for (tag, count, bytes, per) in pending {
        let mut vals = Vec::new();
        match (count, bytes) {
            (Some(c), _) => {
                for _ in 0..(c as usize * per as usize).min(1024) {
                    let Some((v, used)) = vwi(&rec[..end], data) else {
                        break;
                    };
                    data += used;
                    vals.push(v);
                }
            }
            (None, Some(b)) => {
                let mut used_total = 0usize;
                while used_total < b as usize && vals.len() < 1024 {
                    let Some((v, used)) = vwi(&rec[..end], data) else {
                        break;
                    };
                    data += used;
                    used_total += used;
                    vals.push(v);
                }
            }
            _ => {}
        }
        out.push((tag, vals));
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Một mục dựng thử: tên + [(thẻ, giá trị)].
    type TestEntry<'a> = (&'a [u8], Vec<(u8, Vec<u32>)>);

    /// Mã hoá VWI (xuôi).
    pub(crate) fn enc_vwi(mut v: u32) -> Vec<u8> {
        let mut b = vec![(v & 0x7F) as u8 | 0x80];
        v >>= 7;
        while v > 0 {
            b.insert(0, (v & 0x7F) as u8);
            v >>= 7;
        }
        b
    }

    /// Dựng một chỉ mục tổng hợp (bản ghi chính + 1 bản ghi mục + 1 CNCX). `tagx`: (thẻ, giá trị / mục, mặt nạ).
    /// `entries`: (tên, [(thẻ, giá trị)]). Mỗi thẻ có mặt dùng mặt nạ 1 bit hoặc «đếm» theo mặt nạ.
    pub(crate) fn build(
        tagx: &[(u8, u8, u8)],
        entries: &[TestEntry],
        cncx: &[&[u8]],
    ) -> Vec<Vec<u8>> {
        let mut main = b"INDX".to_vec();
        let mut hdr = vec![0u32; 45];
        hdr[0] = 192; // độ dài đầu
        hdr[5] = 1; // số bản ghi mục
        hdr[12] = if cncx.is_empty() { 0 } else { 1 };
        for v in &hdr {
            main.extend_from_slice(&v.to_be_bytes());
        }
        main.resize(192, 0);
        main.extend_from_slice(b"TAGX");
        main.extend_from_slice(&(12 + 4 * (tagx.len() as u32 + 1)).to_be_bytes());
        main.extend_from_slice(&1u32.to_be_bytes());
        for &(t, per, mask) in tagx {
            main.extend_from_slice(&[t, per, mask, 0]);
        }
        main.extend_from_slice(&[0, 0, 0, 1]);
        // Bản ghi mục.
        let mut body = Vec::new();
        let mut offs = Vec::new();
        for (ident, tags) in entries {
            offs.push(192 + body.len());
            body.push(ident.len() as u8);
            body.extend_from_slice(ident);
            let mut cb = 0u8;
            let mut vals = Vec::new();
            for &(t, per, mask) in tagx {
                if let Some((_, v)) = tags.iter().find(|(k, _)| *k == t) {
                    let reps = v.len() as u8 / per.max(1);
                    cb |= (reps << mask.trailing_zeros()) & mask;
                    for &x in v {
                        vals.extend(enc_vwi(x));
                    }
                }
            }
            body.push(cb);
            body.extend(vals);
        }
        let idxt = 192 + body.len();
        let mut rec = b"INDX".to_vec();
        let mut h = vec![0u32; 45];
        h[0] = 192;
        h[4] = idxt as u32;
        h[5] = entries.len() as u32;
        for v in &h {
            rec.extend_from_slice(&v.to_be_bytes());
        }
        rec.resize(192, 0);
        rec.extend(body);
        rec.extend_from_slice(b"IDXT");
        for o in offs {
            rec.extend_from_slice(&(o as u16).to_be_bytes());
        }
        let mut c = Vec::new();
        for s in cncx {
            c.extend(enc_vwi(s.len() as u32));
            c.extend_from_slice(s);
        }
        let mut recs = vec![main, rec];
        if !cncx.is_empty() {
            recs.push(c);
        }
        recs
    }

    /// PDB tổng hợp từ các bản ghi.
    pub(crate) fn pdb_bytes(recs: &[Vec<u8>]) -> Vec<u8> {
        let mut d = vec![0u8; 78];
        d[60..68].copy_from_slice(b"BOOKMOBI");
        d[76..78].copy_from_slice(&(recs.len() as u16).to_be_bytes());
        let mut off = 78 + 8 * recs.len() + 2;
        for r in recs {
            d.extend_from_slice(&(off as u32).to_be_bytes());
            d.extend_from_slice(&[0, 0, 0, 0]);
            off += r.len();
        }
        d.extend_from_slice(&[0, 0]);
        for r in recs {
            d.extend_from_slice(r);
        }
        d
    }

    #[test]
    fn vwi_doc_xuoi() {
        assert_eq!(vwi(&[0x81], 0), Some((1, 1)));
        assert_eq!(vwi(&[0x01, 0x80], 0), Some((128, 2)));
        assert_eq!(vwi(&[0x7F, 0x7F, 0xFF], 0), Some((0x1FFFFF, 3)));
        assert_eq!(vwi(&[0x01, 0x01], 0), None);
        assert_eq!(vwi(&[0x7F; 6], 0), None);
        assert_eq!(vwi(&[0x81], 5), None);
        for v in [0u32, 1, 127, 128, 300, 70000, u32::MAX >> 4] {
            assert_eq!(vwi(&enc_vwi(v), 0), Some((v, enc_vwi(v).len())));
        }
    }

    #[test]
    fn doc_chi_muc_tong_hop() {
        // Giống chỉ mục SKEL / FRAG: thẻ 1 (1 giá trị), thẻ 6 (2 giá trị), thẻ 3 «đếm» qua mặt nạ 2 bit.
        let tagx = [(1u8, 1u8, 0x01u8), (6, 2, 0x02), (3, 1, 0x0C)];
        let entries: Vec<TestEntry> = vec![
            (b"SKEL0000000000", vec![(1, vec![2]), (6, vec![0, 1234])]),
            (
                b"SKEL0000000001",
                vec![(6, vec![1234, 70000]), (3, vec![5, 6])],
            ),
        ];
        let recs = build(&tagx, &entries, &[b"Ch\xc6\xb0\xc6\xa1ng 1", b"Hai"]);
        let data = pdb_bytes(&recs);
        let pdb = Pdb::parse(&data).unwrap();
        let ix = read(&pdb, 0).unwrap();
        assert_eq!(ix.entries.len(), 2);
        assert_eq!(ix.entries[0].ident, b"SKEL0000000000");
        assert_eq!(ix.entries[0].val(1, 0), Some(2));
        assert_eq!(ix.entries[0].tag(6), Some(&[0, 1234][..]));
        assert_eq!(ix.entries[1].tag(1), None);
        assert_eq!(ix.entries[1].tag(6), Some(&[1234, 70000][..]));
        assert_eq!(ix.entries[1].tag(3), Some(&[5, 6][..]));
        assert_eq!(
            ix.cncx.get(&0).map(|v| v.as_slice()),
            Some("Chương 1".as_bytes())
        );
        assert_eq!(ix.cncx.get(&11).map(|v| v.as_slice()), Some(&b"Hai"[..]));
        // Bản ghi mục (không có TAGX) / không phải INDX / không có → lỗi, không panic.
        assert!(read(&pdb, 1).is_err());
        assert!(read(&pdb, 2).is_err());
        assert!(read(&pdb, 9).is_err());
    }

    #[test]
    fn chi_muc_hong_khong_panic() {
        let tagx = [(1u8, 1u8, 0x01u8), (6, 2, 0x02)];
        let entries: Vec<TestEntry> = vec![(b"A", vec![(1, vec![7]), (6, vec![1, 2])])];
        let recs = build(&tagx, &entries, &[b"x"]);
        // Cắt / làm hỏng từng byte của bản ghi mục.
        for cut in 0..recs[1].len() {
            let mut r = recs.clone();
            r[1].truncate(cut);
            let data = pdb_bytes(&r);
            let _ = read(&Pdb::parse(&data).unwrap(), 0);
        }
        for i in 0..recs[1].len() {
            for v in [0u8, 0x7F, 0x80, 0xFF] {
                let mut r = recs.clone();
                r[1][i] = v;
                let data = pdb_bytes(&r);
                let _ = read(&Pdb::parse(&data).unwrap(), 0);
            }
        }
    }
}
