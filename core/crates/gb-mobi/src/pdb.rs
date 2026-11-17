//! Tệp PalmDB (PDB): đầu tệp 78 byte + bảng bản ghi (record) 8 byte / mục. Sách Kindle là một PDB kiểu «BOOKMOBI»
//! (PalmDOC thuần: «TEXtREAd»).
//!
//! Bản ghi là lát cắt mượn từ dữ liệu gốc (không chép). Vị trí hỏng (vượt cuối tệp, lùi lại) cho bản ghi rỗng thay vì lỗi —
//! phần đọc phía trên tự kiểm tra nội dung từng bản ghi.

use crate::MobiError;

/// Một tệp PDB đã đọc bảng bản ghi.
pub(crate) struct Pdb<'a> {
    data: &'a [u8],
    /// `type` + `creator` (8 byte): «BOOKMOBI», «TEXtREAd»…
    pub kind: [u8; 8],
    /// Tên trong đầu PDB (≤ 32 byte, tới byte 0).
    pub name: &'a [u8],
    ranges: Vec<(usize, usize)>,
}

impl<'a> Pdb<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Pdb<'a>, MobiError> {
        if data.len() < 78 {
            return Err(MobiError::Invalid("tệp quá ngắn".into()));
        }
        let mut kind = [0u8; 8];
        kind.copy_from_slice(&data[60..68]);
        let n = u16::from_be_bytes([data[76], data[77]]) as usize;
        if n == 0 {
            return Err(MobiError::Invalid("không có bản ghi".into()));
        }
        let table_end = 78 + n * 8;
        if table_end > data.len() {
            return Err(MobiError::Invalid("bảng bản ghi bị cắt".into()));
        }
        let offs: Vec<usize> = (0..n)
            .map(|i| {
                let p = 78 + i * 8;
                u32::from_be_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]) as usize
            })
            .collect();
        let ranges = (0..n)
            .map(|i| {
                let start = offs[i].min(data.len());
                let end = if i + 1 < n {
                    offs[i + 1].min(data.len())
                } else {
                    data.len()
                };
                (start, end.max(start))
            })
            .collect();
        let name_end = data[..32].iter().position(|&b| b == 0).unwrap_or(32);
        Ok(Pdb {
            data,
            kind,
            name: &data[..name_end],
            ranges,
        })
    }

    /// Số bản ghi.
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Bản ghi thứ `i` (rỗng nếu không có).
    pub fn record(&self, i: usize) -> &'a [u8] {
        match self.ranges.get(i) {
            Some(&(s, e)) => &self.data[s..e],
            None => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bang_ban_ghi_hong_khong_panic() {
        // 3 bản ghi: vị trí thứ hai vượt cuối tệp, thứ ba lùi lại.
        let mut d = vec![0u8; 78 + 24 + 10];
        d[..4].copy_from_slice(b"test");
        d[60..68].copy_from_slice(b"BOOKMOBI");
        d[77] = 3;
        let total = d.len() as u32;
        d[78..82].copy_from_slice(&102u32.to_be_bytes());
        d[86..90].copy_from_slice(&(total + 100).to_be_bytes());
        d[94..98].copy_from_slice(&50u32.to_be_bytes());
        let p = Pdb::parse(&d).unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p.name, b"test");
        assert_eq!(p.record(0).len(), 10);
        assert!(p.record(1).is_empty());
        assert_eq!(p.record(2).len(), d.len() - 50);
        assert!(p.record(7).is_empty());
        assert!(Pdb::parse(&d[..80]).is_err());
    }
}
