//! Giải nén zlib / deflate (RFC 1950 / 1951) tối giản, kiểu «puff» của zlib — chỉ dùng cho bản ghi phông (FONT) của KF8
//! nên ưu tiên gọn và an toàn hơn tốc độ. Mọi lỗi dữ liệu → `None`; kết quả chặn ở `max_out` byte.

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    buf: u32,
    cnt: u32,
}

impl Bits<'_> {
    fn need(&mut self, n: u32) -> Option<u32> {
        while self.cnt < n {
            let b = *self.d.get(self.pos)?;
            self.pos += 1;
            self.buf |= (b as u32) << self.cnt;
            self.cnt += 8;
        }
        let v = self.buf & ((1u32 << n) - 1);
        self.buf >>= n;
        self.cnt -= n;
        Some(v)
    }
}

/// Bảng Huffman chính tắc: số mã theo độ dài + ký hiệu xếp theo mã.
struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    /// Dựng từ độ dài mã. `None` nếu thừa mã (over-subscribed); thiếu mã thì vẫn nhận (giải mã sai sẽ lỗi sau).
    fn new(lengths: &[u8]) -> Option<Huffman> {
        let mut count = [0u16; 16];
        for &l in lengths {
            count[l as usize & 15] += 1;
        }
        let mut left: i32 = 1;
        for &c in &count[1..] {
            left = (left << 1) - c as i32;
            if left < 0 {
                return None;
            }
        }
        let mut offs = [0u16; 16];
        for len in 1..15 {
            offs[len + 1] = offs[len] + count[len];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbol[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Some(Huffman { count, symbol })
    }

    fn decode(&self, b: &mut Bits) -> Option<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.need(1)? as i32;
            let count = self.count[len] as i32;
            if code - count < first {
                return self.symbol.get((index + (code - first)) as usize).copied();
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        None
    }
}

const LBASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEXT: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DBASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DEXT: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn codes(
    b: &mut Bits,
    out: &mut Vec<u8>,
    lit: &Huffman,
    dist: &Huffman,
    max_out: usize,
) -> Option<()> {
    loop {
        let sym = lit.decode(b)? as usize;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            return Some(());
        } else {
            let s = sym - 257;
            let len = *LBASE.get(s)? as usize + b.need(*LEXT.get(s)? as u32)? as usize;
            let ds = dist.decode(b)? as usize;
            let d = *DBASE.get(ds)? as usize + b.need(*DEXT.get(ds)? as u32)? as usize;
            if d > out.len() {
                return None;
            }
            let from = out.len() - d;
            for k in 0..len {
                let c = out[from + k];
                out.push(c);
            }
        }
        if out.len() > max_out {
            return None;
        }
    }
}

/// Giải dòng deflate thô.
pub(crate) fn inflate(data: &[u8], max_out: usize) -> Option<Vec<u8>> {
    let mut b = Bits {
        d: data,
        pos: 0,
        buf: 0,
        cnt: 0,
    };
    let mut out = Vec::new();
    loop {
        let last = b.need(1)?;
        match b.need(2)? {
            0 => {
                // Khối lưu nguyên: bỏ bit lẻ, LEN / NLEN.
                b.buf = 0;
                b.cnt = 0;
                let p = b.pos;
                let h = data.get(p..p + 4)?;
                let len = u16::from_le_bytes([h[0], h[1]]);
                if len != !u16::from_le_bytes([h[2], h[3]]) {
                    return None;
                }
                let blk = data.get(p + 4..p + 4 + len as usize)?;
                out.extend_from_slice(blk);
                b.pos = p + 4 + len as usize;
            }
            1 => {
                let mut l = [8u8; 288];
                l[144..256].fill(9);
                l[256..280].fill(7);
                let lit = Huffman::new(&l)?;
                let dist = Huffman::new(&[5u8; 30])?;
                codes(&mut b, &mut out, &lit, &dist, max_out)?;
            }
            2 => {
                let nlen = b.need(5)? as usize + 257;
                let ndist = b.need(5)? as usize + 1;
                let ncode = b.need(4)? as usize + 4;
                if nlen > 286 || ndist > 30 {
                    return None;
                }
                const ORDER: [usize; 19] = [
                    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
                ];
                let mut cl = [0u8; 19];
                for &o in &ORDER[..ncode] {
                    cl[o] = b.need(3)? as u8;
                }
                let clh = Huffman::new(&cl)?;
                let mut lens = vec![0u8; nlen + ndist];
                let mut i = 0;
                while i < nlen + ndist {
                    let sym = clh.decode(&mut b)?;
                    let (val, rep) = match sym {
                        0..=15 => (sym as u8, 1),
                        16 => (*lens.get(i.checked_sub(1)?)?, 3 + b.need(2)? as usize),
                        17 => (0, 3 + b.need(3)? as usize),
                        18 => (0, 11 + b.need(7)? as usize),
                        _ => return None,
                    };
                    if i + rep > nlen + ndist {
                        return None;
                    }
                    lens[i..i + rep].fill(val);
                    i += rep;
                }
                if lens[256] == 0 {
                    return None;
                }
                let lit = Huffman::new(&lens[..nlen])?;
                let dist = Huffman::new(&lens[nlen..])?;
                codes(&mut b, &mut out, &lit, &dist, max_out)?;
            }
            _ => return None,
        }
        if out.len() > max_out {
            return None;
        }
        if last == 1 {
            return Some(out);
        }
    }
}

/// Giải dòng zlib (2 byte đầu + deflate; bỏ qua tổng kiểm Adler-32 ở cuối).
pub(crate) fn zlib(data: &[u8], max_out: usize) -> Option<Vec<u8>> {
    let (&cmf, &flg) = (data.first()?, data.get(1)?);
    if cmf & 0x0F != 8 || !((cmf as u16) << 8 | flg as u16).is_multiple_of(31) || flg & 0x20 != 0 {
        return None;
    }
    inflate(&data[2..], max_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn giai_nen_zlib() {
        // zlib.compress(b"hello hello hello hello") — khối Huffman cố định.
        let fixed = [
            0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x27, 0x01, 0x68, 0x03,
            0x08, 0xb1,
        ];
        assert_eq!(
            zlib(&fixed, 1000).as_deref(),
            Some(&b"hello hello hello hello"[..])
        );
        // zlib.compress(b"abc", 0) — khối lưu nguyên.
        let stored = [
            0x78, 0x01, 0x01, 0x03, 0x00, 0xfc, 0xff, 0x61, 0x62, 0x63, 0x02, 0x4d, 0x01, 0x27,
        ];
        assert_eq!(zlib(&stored, 1000).as_deref(), Some(&b"abc"[..]));
        // Vượt giới hạn / cắt cụt / đầu sai → None, không panic.
        assert_eq!(zlib(&fixed, 5), None);
        for cut in 0..fixed.len() - 4 {
            assert_eq!(zlib(&fixed[..cut], 1000), None);
        }
        assert_eq!(zlib(&[0x78, 0x9d, 0, 0], 10), None);
        for i in 2..fixed.len() {
            for v in [0u8, 0xFF, 0x55] {
                let mut d = fixed;
                d[i] = v;
                let _ = zlib(&d, 1000);
            }
        }
    }

    /// zlib.compress(chữ, 9) với chữ = 400 từ chọn bằng LCG (khối Huffman động), dựng bằng Python lúc viết.
    const DYNAMIC: [u8; 398] = [
        0x78, 0xda, 0x5d, 0x54, 0x49, 0x12, 0x83, 0x30, 0x0c, 0xfb, 0x0a, 0x5f, 0xa3, 0x5c, 0xca,
        0xa1, 0xe1, 0xd0, 0xf4, 0x90, 0xdf, 0x17, 0x2f, 0x92, 0x6c, 0x66, 0x4a, 0x49, 0xf0, 0x12,
        0x5b, 0x92, 0x73, 0xbc, 0xcf, 0xb1, 0x1d, 0xf6, 0x37, 0xf7, 0x8f, 0x16, 0xdf, 0xfd, 0xb7,
        0xbd, 0xf7, 0x73, 0x7b, 0xed, 0xdb, 0xe7, 0x9a, 0x5c, 0xfe, 0xae, 0xb3, 0xfb, 0xf8, 0xe6,
        0x75, 0x0d, 0xb3, 0x86, 0xd3, 0xf2, 0xb7, 0xe2, 0x17, 0xdf, 0xb7, 0xcb, 0xb8, 0xc3, 0x6c,
        0x6b, 0x6f, 0x8b, 0xb2, 0xdc, 0x70, 0xf1, 0x6f, 0x7b, 0x24, 0xf3, 0x84, 0x2b, 0xce, 0xcb,
        0x6f, 0x96, 0xf1, 0x5e, 0xce, 0xf0, 0xca, 0x9d, 0x19, 0x6a, 0x3e, 0xfb, 0xec, 0x41, 0xe6,
        0x66, 0xc9, 0xbd, 0x3e, 0x5b, 0xd8, 0x63, 0x56, 0xf3, 0x8c, 0xb4, 0x97, 0xba, 0xa0, 0xa7,
        0x59, 0x89, 0x03, 0x2b, 0x68, 0x49, 0x66, 0xb6, 0x50, 0xea, 0x5b, 0x28, 0x8c, 0x88, 0xb8,
        0xc1, 0xbc, 0x98, 0x59, 0x60, 0x09, 0x05, 0x8b, 0x64, 0x3d, 0x80, 0xc2, 0x37, 0x03, 0x35,
        0x24, 0xf0, 0x71, 0xc6, 0x8a, 0xb6, 0xe0, 0x90, 0x99, 0x94, 0x81, 0x7f, 0x16, 0xe5, 0x91,
        0x41, 0xcc, 0xfd, 0xc3, 0xd1, 0x07, 0xe9, 0x06, 0x6c, 0xec, 0xdb, 0xcf, 0x88, 0x00, 0xf6,
        0x15, 0x45, 0x35, 0x88, 0x73, 0x6b, 0x16, 0x86, 0x33, 0x07, 0xb5, 0x52, 0x09, 0x84, 0x21,
        0x93, 0x81, 0x27, 0x72, 0x85, 0x83, 0x2a, 0x24, 0x33, 0x0b, 0x2a, 0x74, 0xdb, 0x93, 0x5b,
        0xc2, 0x10, 0x32, 0x58, 0x62, 0x34, 0x31, 0x01, 0xfd, 0xc0, 0x0d, 0x67, 0xaa, 0xd0, 0x54,
        0x16, 0x92, 0xa2, 0x25, 0x81, 0xb0, 0x7a, 0x57, 0x4d, 0x50, 0xee, 0x0d, 0xa0, 0x93, 0x1e,
        0x1e, 0x4b, 0x16, 0xda, 0xc0, 0x64, 0xd5, 0x16, 0x40, 0x3d, 0xe4, 0x70, 0x35, 0x79, 0x91,
        0xab, 0x1c, 0x03, 0x6e, 0x50, 0x00, 0xa0, 0x28, 0x53, 0x35, 0x0b, 0x60, 0x15, 0x1d, 0x34,
        0xf5, 0x28, 0x0b, 0x73, 0x87, 0xec, 0x6d, 0x2c, 0x90, 0xa7, 0xa2, 0x98, 0xb5, 0x92, 0xee,
        0x2e, 0x73, 0x8c, 0xa2, 0x58, 0x02, 0xa8, 0xdd, 0x0f, 0x73, 0x0e, 0xaa, 0x31, 0xef, 0x55,
        0x26, 0x9c, 0x88, 0x44, 0x83, 0x07, 0x80, 0x68, 0x61, 0x55, 0x43, 0xd0, 0xe4, 0x28, 0xb4,
        0x43, 0xaf, 0xd2, 0xf7, 0x35, 0x0a, 0x0a, 0x4d, 0xfa, 0xe4, 0x93, 0x10, 0xd4, 0x8b, 0x6a,
        0x48, 0x74, 0x6d, 0x72, 0x84, 0x66, 0x61, 0xf9, 0x21, 0x9f, 0x32, 0xa3, 0x2c, 0x86, 0x3d,
        0xa8, 0x5c, 0xca, 0x21, 0x75, 0x23, 0x95, 0x85, 0x19, 0x7c, 0x01, 0x86, 0x51, 0xef, 0xea,
        0x99, 0xc9, 0x51, 0x77, 0xb9, 0x29, 0x25, 0x92, 0xa5, 0x02, 0x9f, 0x7a, 0x94, 0xcc, 0x74,
        0x4f, 0x68, 0x70, 0xdb, 0x82, 0xf2, 0xd5, 0xcd, 0x97, 0xdd, 0x8a, 0x24, 0xcc, 0x38, 0xa5,
        0x9d, 0x05, 0xff, 0x01, 0x30, 0xb5, 0x35, 0xfc,
    ];

    #[test]
    fn giai_nen_khoi_dong() {
        let words = [
            "mot", "hai", "ba", "bon", "nam", "sau", "bay", "tam", "chin", "muoi",
        ];
        let mut x: u64 = 12345;
        let mut v = Vec::new();
        for _ in 0..400 {
            x = (x * 1103515245 + 12345) & 0x7fff_ffff;
            v.push(words[((x >> 16) % 10) as usize]);
        }
        let want = v.join(" ").into_bytes();
        assert_eq!(zlib(&DYNAMIC, 1 << 20).as_deref(), Some(want.as_slice()));
        assert_eq!(zlib(&DYNAMIC, 100), None);
        for cut in 0..DYNAMIC.len() - 4 {
            assert!(zlib(&DYNAMIC[..cut], 1 << 20).is_none_or(|o| o != want));
        }
        for i in (2..DYNAMIC.len()).step_by(3) {
            let mut d = DYNAMIC;
            d[i] ^= 0x5A;
            let _ = zlib(&d, 1 << 20);
        }
    }
}
