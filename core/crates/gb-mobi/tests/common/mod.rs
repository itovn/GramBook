//! Dựng sách Kindle tổng hợp cho bài thử: PDB, bản ghi 0 (PalmDOC + MOBI + EXTH), chỉ mục INDX, FDST, phông FONT.

#![allow(dead_code)]

/// Mã hoá VWI (xuôi, byte cuối có bit 0x80).
pub fn vwi(mut v: u32) -> Vec<u8> {
    let mut b = vec![(v & 0x7F) as u8 | 0x80];
    v >>= 7;
    while v > 0 {
        b.insert(0, (v & 0x7F) as u8);
        v >>= 7;
    }
    b
}

/// Tệp PDB «BOOKMOBI» từ các bản ghi.
pub fn pdb(recs: &[Vec<u8>]) -> Vec<u8> {
    let mut d = vec![0u8; 78];
    d[..9].copy_from_slice(b"Test_Book");
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

/// Các trường của bản ghi 0.
pub struct Rec0 {
    pub version: u32,
    pub encoding: u32,
    pub compression: u16,
    pub encryption: u16,
    pub text_len: u32,
    pub text_records: u16,
    pub first_resource: u32,
    pub fdst: u32,
    pub skel: u32,
    pub frag: u32,
    pub ncx: u32,
    pub extra_flags: u16,
    pub exth: Vec<(u32, Vec<u8>)>,
    pub full_name: Vec<u8>,
}

pub const NULL: u32 = 0xFFFF_FFFF;

impl Default for Rec0 {
    fn default() -> Rec0 {
        Rec0 {
            version: 6,
            encoding: 65001,
            compression: 1,
            encryption: 0,
            text_len: 0,
            text_records: 0,
            first_resource: NULL,
            fdst: NULL,
            skel: NULL,
            frag: NULL,
            ncx: NULL,
            extra_flags: 0,
            exth: Vec::new(),
            full_name: b"Full Name".to_vec(),
        }
    }
}

impl Rec0 {
    pub fn build(&self) -> Vec<u8> {
        let hlen: u32 = 0x108;
        let mut r = vec![0u8; 16 + hlen as usize];
        let put32 =
            |r: &mut Vec<u8>, o: usize, v: u32| r[o..o + 4].copy_from_slice(&v.to_be_bytes());
        r[0..2].copy_from_slice(&self.compression.to_be_bytes());
        put32(&mut r, 4, self.text_len);
        r[8..10].copy_from_slice(&self.text_records.to_be_bytes());
        r[10..12].copy_from_slice(&4096u16.to_be_bytes());
        r[12..14].copy_from_slice(&self.encryption.to_be_bytes());
        r[16..20].copy_from_slice(b"MOBI");
        put32(&mut r, 0x14, hlen);
        put32(&mut r, 0x18, 2);
        put32(&mut r, 0x1C, self.encoding);
        put32(&mut r, 0x24, self.version);
        for o in (0x28..0x50).step_by(4) {
            put32(&mut r, o, NULL);
        }
        put32(&mut r, 0x5C, 0x2A);
        put32(&mut r, 0x6C, self.first_resource);
        put32(&mut r, 0x70, NULL);
        put32(&mut r, 0x80, 0x40);
        put32(&mut r, 0xC0, self.fdst);
        put32(&mut r, 0xC4, 1);
        r[0xF2..0xF4].copy_from_slice(&self.extra_flags.to_be_bytes());
        put32(&mut r, 0xF4, self.ncx);
        put32(&mut r, 0xF8, self.frag);
        put32(&mut r, 0xFC, self.skel);
        put32(&mut r, 0x100, NULL);
        put32(&mut r, 0x104, NULL);
        let mut e = Vec::new();
        for (t, v) in &self.exth {
            e.extend_from_slice(&t.to_be_bytes());
            e.extend_from_slice(&(v.len() as u32 + 8).to_be_bytes());
            e.extend_from_slice(v);
        }
        r.extend_from_slice(b"EXTH");
        r.extend_from_slice(&(e.len() as u32 + 12).to_be_bytes());
        r.extend_from_slice(&(self.exth.len() as u32).to_be_bytes());
        r.extend_from_slice(&e);
        let name_off = r.len() as u32;
        r.extend_from_slice(&self.full_name);
        r.extend_from_slice(&[0, 0, 0, 0]);
        put32(&mut r, 0x54, name_off);
        put32(&mut r, 0x58, self.full_name.len() as u32);
        r
    }
}

/// Cắt chữ thành bản ghi 4096 byte (không nén).
pub fn text_records(text: &[u8]) -> Vec<Vec<u8>> {
    text.chunks(4096).map(|c| c.to_vec()).collect()
}

/// Một mục chỉ mục dựng thử: tên + [(thẻ, giá trị)].
pub type IndexEntry = (Vec<u8>, Vec<(u8, Vec<u32>)>);

/// Chỉ mục INDX: bản ghi chính + một bản ghi mục (+ một CNCX nếu có chuỗi). `tagx`: (thẻ, giá trị / mục, mặt nạ — một
/// bit). `entries`: (tên, [(thẻ, giá trị)]).
pub fn indx(tagx: &[(u8, u8, u8)], entries: &[IndexEntry], cncx: &[&[u8]]) -> Vec<Vec<u8>> {
    let mut main = b"INDX".to_vec();
    let mut hdr = vec![0u32; 45];
    hdr[0] = 192;
    hdr[5] = 1;
    hdr[6] = 65001;
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
                    vals.extend(vwi(x));
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
    let mut recs = vec![main, rec];
    if !cncx.is_empty() {
        let mut c = Vec::new();
        for s in cncx {
            c.extend(vwi(s.len() as u32));
            c.extend_from_slice(s);
        }
        recs.push(c);
    }
    recs
}

/// Vị trí chuỗi thứ `i` trong bản ghi CNCX dựng từ `cncx`.
pub fn cncx_off(cncx: &[&[u8]], i: usize) -> u32 {
    cncx[..i]
        .iter()
        .map(|s| vwi(s.len() as u32).len() + s.len())
        .sum::<usize>() as u32
}

/// Số cơ số 32 của Kindle, đệm 0 tới `width` ký tự.
pub fn b32(mut v: u32, width: usize) -> String {
    let digits = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";
    let mut s = Vec::new();
    while v > 0 {
        s.push(digits[(v % 32) as usize]);
        v /= 32;
    }
    while s.len() < width {
        s.push(b'0');
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}

/// Bản ghi FDST.
pub fn fdst(flows: &[(u32, u32)]) -> Vec<u8> {
    let mut r = b"FDST".to_vec();
    r.extend_from_slice(&12u32.to_be_bytes());
    r.extend_from_slice(&(flows.len() as u32).to_be_bytes());
    for &(a, b) in flows {
        r.extend_from_slice(&a.to_be_bytes());
        r.extend_from_slice(&b.to_be_bytes());
    }
    r
}

/// Ảnh JPEG giả (đủ byte đầu để nhận dạng).
pub fn jpeg(tag: u8) -> Vec<u8> {
    let mut v = vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, tag,
    ];
    v.resize(200, tag);
    v
}

/// Bản ghi FONT: phông «true…» nén zlib (khối lưu nguyên) + XOR khoá 2 byte.
pub fn font_record(plain: &[u8]) -> Vec<u8> {
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
    rec
}

pub const EOF: [u8; 4] = [0xE9, 0x8E, 0x0D, 0x0A];

/// Sách MOBI6 tổng hợp: bản ghi 0, chữ (không nén), ảnh, bản ghi kết thúc.
pub fn mobi6(html: &str, images: &[Vec<u8>], exth: Vec<(u32, Vec<u8>)>) -> Vec<Vec<u8>> {
    let text = html.as_bytes();
    let trs = text_records(text);
    let r0 = Rec0 {
        text_len: text.len() as u32,
        text_records: trs.len() as u16,
        first_resource: if images.is_empty() {
            NULL
        } else {
            1 + trs.len() as u32
        },
        exth,
        ..Default::default()
    };
    let mut recs = vec![r0.build()];
    recs.extend(trs);
    recs.extend(images.iter().cloned());
    recs.push(EOF.to_vec());
    recs
}

/// Sách KF8 tổng hợp: 2 tệp XHTML (mỗi tệp một khung + một mảnh), 1 luồng CSS (dùng phông), 1 ảnh, 1 phông, NCX 3 mục.
/// `res_first` = trường «tài nguyên đầu» ghi vào đầu KF8 (None: bản ghi ảnh của chính phần này).
pub fn kf8(res_first: Option<u32>) -> Vec<Vec<u8>> {
    let skel = |aid: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title>\
             <link rel=\"stylesheet\" type=\"text/css\" href=\"kindle:flow:0001?mime=text/css\"/></head><body aid=\"{aid}\"></body></html>"
        )
    };
    let s0 = skel("0");
    let s1 = skel("2");
    let f1 = "<div aid=\"3\"><p>Mở đầu.</p><h2 id=\"c2\">Mục 2.1</h2><p>Hết.</p></div>";
    // Lệch của <h2 id="c2"> trong mảnh 1.
    let h2 = f1.find("<h2").unwrap() as u32;
    let f0 = format!(
        "<div aid=\"1\"><h1>Chương 1</h1><p>Xin chào <a href=\"kindle:pos:fid:0001:off:0000000000\">sang chương 2</a> \
         hoặc <a href=\"kindle:pos:fid:0001:off:{}\">mục 2.1</a>.</p>\
         <img src=\"kindle:embed:0001?mime=image/jpeg\" alt=\"\"/></div>",
        b32(h2, 10)
    );
    let css =
        "p { text-indent: 1em }\n@font-face { font-family: X; src: url(kindle:embed:0002) }\n";
    let flow0 = format!("{s0}{f0}{s1}{f1}");
    let text = format!("{flow0}{css}");
    let ins0 = s0.find("</body>").unwrap();
    let skel1_pos = s0.len() + f0.len();
    let ins1 = skel1_pos + s1.find("</body>").unwrap();
    let trs = text_records(text.as_bytes());
    let n_text = trs.len() as u32;
    // Bố cục bản ghi (tương đối với bản ghi 0 của phần KF8).
    let skel_at = 1 + n_text;
    let skel_idx = indx(
        &[(1, 1, 0x01), (6, 2, 0x02)],
        &[
            (
                b"SKEL0000000000".to_vec(),
                vec![(1, vec![1]), (6, vec![0, s0.len() as u32])],
            ),
            (
                b"SKEL0000000001".to_vec(),
                vec![(1, vec![1]), (6, vec![skel1_pos as u32, s1.len() as u32])],
            ),
        ],
        &[],
    );
    let frag_at = skel_at + skel_idx.len() as u32;
    let sel: [&[u8]; 2] = [b"P-//*[@aid='0']", b"P-//*[@aid='2']"];
    let frag_idx = indx(
        &[(2, 1, 0x01), (3, 1, 0x02), (4, 1, 0x04), (6, 2, 0x08)],
        &[
            (
                format!("{ins0:010}").into_bytes(),
                vec![
                    (2, vec![cncx_off(&sel, 0)]),
                    (3, vec![0]),
                    (4, vec![0]),
                    (6, vec![0, f0.len() as u32]),
                ],
            ),
            (
                format!("{ins1:010}").into_bytes(),
                vec![
                    (2, vec![cncx_off(&sel, 1)]),
                    (3, vec![1]),
                    (4, vec![1]),
                    (6, vec![0, f1.len() as u32]),
                ],
            ),
        ],
        &sel,
    );
    let ncx_at = frag_at + frag_idx.len() as u32;
    let labels: [&[u8]; 3] = [
        "Chương 1".as_bytes(),
        "Chương 2".as_bytes(),
        "Mục 2.1".as_bytes(),
    ];
    let ncx_idx = indx(
        &[(1, 1, 0x01), (3, 1, 0x02), (4, 1, 0x04), (6, 2, 0x08)],
        &[
            (
                b"000".to_vec(),
                vec![
                    (1, vec![0]),
                    (3, vec![cncx_off(&labels, 0)]),
                    (4, vec![0]),
                    (6, vec![0, 0]),
                ],
            ),
            (
                b"001".to_vec(),
                vec![
                    (1, vec![0]),
                    (3, vec![cncx_off(&labels, 1)]),
                    (4, vec![0]),
                    (6, vec![1, 0]),
                ],
            ),
            (
                b"002".to_vec(),
                vec![
                    (1, vec![0]),
                    (3, vec![cncx_off(&labels, 2)]),
                    (4, vec![1]),
                    (6, vec![1, h2]),
                ],
            ),
        ],
        &labels,
    );
    let fdst_at = ncx_at + ncx_idx.len() as u32;
    let res_at = fdst_at + 1;
    let r0 = Rec0 {
        version: 8,
        text_len: text.len() as u32,
        text_records: n_text as u16,
        first_resource: res_first.unwrap_or(res_at),
        fdst: fdst_at,
        skel: skel_at,
        frag: frag_at,
        ncx: ncx_at,
        exth: vec![
            (100, "Tác Giả".as_bytes().to_vec()),
            (503, "Sách KF8".as_bytes().to_vec()),
            (524, b"vi".to_vec()),
            (201, 0u32.to_be_bytes().to_vec()),
        ],
        ..Default::default()
    };
    let mut recs = vec![r0.build()];
    recs.extend(trs);
    recs.extend(skel_idx);
    recs.extend(frag_idx);
    recs.extend(ncx_idx);
    recs.push(fdst(&[
        (0, flow0.len() as u32),
        (flow0.len() as u32, text.len() as u32),
    ]));
    if res_first.is_none() {
        recs.push(jpeg(1));
        recs.push(font_record(b"true\x00\x01 synthetic font data"));
    }
    recs.push(EOF.to_vec());
    recs
}
