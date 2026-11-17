//! Bộ đọc zip tối giản cho EPUB: đọc mục lục ở cuối tệp (End of Central Directory → Central Directory), giải nén từng mục khi
//! cần (stored / deflate thô bằng `miniz_oxide`). Đọc bằng `read_at` (không dời con trỏ tệp) → nhiều yêu cầu của UIWebView đọc
//! song song được. Không dùng crate `zip` (nhiều phụ thuộc, đổi API liên tục) — EPUB chỉ dùng hai kiểu nén này.

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::path::Path;

use crate::files::read_at;

/// Một mục trong zip.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    method: u16,
    pub csize: u64,
    pub size: u64,
    /// Vị trí phần đầu cục bộ (local header) của mục.
    offset: u64,
}

pub struct Zip {
    file: File,
    entries: Vec<Entry>,
    by_name: HashMap<String, usize>,
    /// Tên viết thường → chỉ số (tìm không phân biệt hoa thường khi sách ghi sai đường dẫn).
    by_lower: HashMap<String, usize>,
}

/// Mục giải nén ra quá chừng này thì từ chối (zip độc / hỏng).
pub const MAX_ENTRY: u64 = 96 << 20;

fn le16(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn le32(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

fn le64(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap_or([0; 8]))
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

fn read_exact_at(f: &File, off: u64, len: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let mut got = 0;
    while got < len {
        let n = read_at(f, &mut buf[got..], off + got as u64)?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "zip ngắn hơn khai báo"));
        }
        got += n;
    }
    Ok(buf)
}

impl Zip {
    pub fn open(path: &Path) -> io::Result<Zip> {
        let file = File::open(path)?;
        let len = std::fs::metadata(path)?.len();
        if len < 22 {
            return Err(bad("tệp quá nhỏ, không phải zip"));
        }
        // EOCD: chữ ký 0x06054b50 trong 64 KB + 22 byte cuối (có thể có lời chú thích phía sau)
        let tail_len = len.min(65_557) as usize;
        let tail = read_exact_at(&file, len - tail_len as u64, tail_len)?;
        let eocd = (0..=tail_len - 22).rev().find(|&i| le32(&tail, i) == 0x0605_4b50).ok_or_else(|| bad("không thấy mục lục zip"))?;
        let mut count = le16(&tail, eocd + 10) as u64;
        let mut cd_size = le32(&tail, eocd + 12) as u64;
        let mut cd_off = le32(&tail, eocd + 16) as u64;
        // ZIP64: bộ định vị nằm ngay trước EOCD
        if (count == 0xFFFF || cd_off == 0xFFFF_FFFF) && eocd >= 20 && le32(&tail, eocd - 20) == 0x0706_4b50 {
            let z64 = le64(&tail, eocd - 20 + 8);
            let rec = read_exact_at(&file, z64, 56)?;
            if le32(&rec, 0) == 0x0606_4b50 {
                count = le64(&rec, 32);
                cd_size = le64(&rec, 40);
                cd_off = le64(&rec, 48);
            }
        }
        if cd_off.saturating_add(cd_size) > len || cd_size > 64 << 20 {
            return Err(bad("mục lục zip sai"));
        }
        let cd = read_exact_at(&file, cd_off, cd_size as usize)?;
        let mut entries = Vec::with_capacity(count.min(100_000) as usize);
        let mut p = 0usize;
        while p + 46 <= cd.len() && le32(&cd, p) == 0x0201_4b50 {
            let flags = le16(&cd, p + 8);
            let method = le16(&cd, p + 10);
            let mut csize = le32(&cd, p + 20) as u64;
            let mut size = le32(&cd, p + 24) as u64;
            let nlen = le16(&cd, p + 28) as usize;
            let xlen = le16(&cd, p + 30) as usize;
            let clen = le16(&cd, p + 32) as usize;
            let mut offset = le32(&cd, p + 42) as u64;
            let name_end = p + 46 + nlen;
            if name_end + xlen > cd.len() {
                break;
            }
            let raw = &cd[p + 46..name_end];
            // Cờ bit 11 = UTF-8; không có thì thường vẫn là ASCII / UTF-8 trong EPUB
            let name = if flags & 0x800 != 0 { String::from_utf8_lossy(raw).into_owned() } else { String::from_utf8_lossy(raw).into_owned() };
            // Trường mở rộng ZIP64 (id 1): cỡ thật / vị trí thật khi trường 32 bit = 0xFFFFFFFF
            let mut x = name_end;
            let xend = name_end + xlen;
            while x + 4 <= xend {
                let id = le16(&cd, x);
                let sz = le16(&cd, x + 2) as usize;
                if id == 1 {
                    let mut q = x + 4;
                    if size == 0xFFFF_FFFF && q + 8 <= xend {
                        size = le64(&cd, q);
                        q += 8;
                    }
                    if csize == 0xFFFF_FFFF && q + 8 <= xend {
                        csize = le64(&cd, q);
                        q += 8;
                    }
                    if offset == 0xFFFF_FFFF && q + 8 <= xend {
                        offset = le64(&cd, q);
                    }
                }
                x += 4 + sz;
            }
            if !name.ends_with('/') {
                entries.push(Entry { name, method, csize, size, offset });
            }
            p = name_end + xlen + clen;
        }
        if entries.is_empty() {
            return Err(bad("zip rỗng"));
        }
        let mut by_name = HashMap::with_capacity(entries.len());
        let mut by_lower = HashMap::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            by_name.entry(e.name.clone()).or_insert(i);
            by_lower.entry(e.name.to_lowercase()).or_insert(i);
        }
        Ok(Zip { file, entries, by_name, by_lower })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Mục theo tên (đúng tên, không thì không phân biệt hoa thường; bỏ «./», «/» đầu).
    pub fn find(&self, name: &str) -> Option<&Entry> {
        let n = name.trim_start_matches("./").trim_start_matches('/');
        self.by_name.get(n).or_else(|| self.by_lower.get(&n.to_lowercase())).map(|&i| &self.entries[i])
    }

    pub fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// Đọc + giải nén một mục.
    pub fn read(&self, name: &str) -> io::Result<Vec<u8>> {
        let e = self.find(name).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("không có «{name}» trong sách")))?;
        self.read_entry(e)
    }

    pub fn read_entry(&self, e: &Entry) -> io::Result<Vec<u8>> {
        if e.size > MAX_ENTRY || e.csize > MAX_ENTRY {
            return Err(bad("mục quá lớn"));
        }
        let lh = read_exact_at(&self.file, e.offset, 30)?;
        if le32(&lh, 0) != 0x0403_4b50 {
            return Err(bad("phần đầu mục zip sai"));
        }
        let data_off = e.offset + 30 + le16(&lh, 26) as u64 + le16(&lh, 28) as u64;
        let raw = read_exact_at(&self.file, data_off, e.csize as usize)?;
        match e.method {
            0 => Ok(raw),
            8 => {
                let out = miniz_oxide::inflate::decompress_to_vec_with_limit(&raw, MAX_ENTRY as usize)
                    .map_err(|err| bad(&format!("giải nén lỗi: {err:?}")))?;
                Ok(out)
            }
            m => Err(bad(&format!("kiểu nén zip {m} chưa hỗ trợ"))),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Zip nhỏ tự ghép (stored + deflate) cho phép thử — EPUB tối giản dùng ở epub.rs / chapter.rs.
    pub(crate) fn make_zip(path: &Path, files: &[(&str, &[u8], bool)]) {
        let mut out = Vec::new();
        let mut cd = Vec::new();
        for (name, data, deflate) in files {
            let comp = if *deflate { miniz_oxide::deflate::compress_to_vec(data, 6) } else { data.to_vec() };
            let off = out.len() as u32;
            let method: u16 = if *deflate { 8 } else { 0 };
            let crc = 0u32; // bộ đọc không kiểm CRC
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0]);
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0, 0, 0, 0]);
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(comp.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&comp);
            cd.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            cd.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
            cd.extend_from_slice(&method.to_le_bytes());
            cd.extend_from_slice(&[0, 0, 0, 0]);
            cd.extend_from_slice(&crc.to_le_bytes());
            cd.extend_from_slice(&(comp.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            cd.extend_from_slice(&off.to_le_bytes());
            cd.extend_from_slice(name.as_bytes());
        }
        let cd_off = out.len() as u32;
        out.extend_from_slice(&cd);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(cd.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        std::fs::write(path, out).unwrap();
    }

    #[test]
    fn reads_stored_and_deflated() {
        let d = std::env::temp_dir().join(format!("gb-zip-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("a.zip");
        let big = "Người du khách cuối cùng đã về. ".repeat(500);
        make_zip(&p, &[("mimetype", b"application/epub+zip", false), ("OEBPS/Text/ch1.xhtml", big.as_bytes(), true)]);
        let z = Zip::open(&p).unwrap();
        assert_eq!(z.entries().len(), 2);
        assert_eq!(z.read("mimetype").unwrap(), b"application/epub+zip");
        assert_eq!(z.read("oebps/text/CH1.xhtml").unwrap(), big.as_bytes());
        assert!(z.read("nope").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn opens_real_sample_if_present() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../Tham_Khao/ebook");
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let mut n = 0;
        for e in rd.flatten().take(40) {
            let z = Zip::open(&e.path()).unwrap_or_else(|err| panic!("{:?}: {err}", e.path()));
            assert!(z.has("META-INF/container.xml"));
            for en in z.entries() {
                z.read_entry(en).unwrap();
            }
            n += 1;
        }
        assert!(n > 0);
    }
}
