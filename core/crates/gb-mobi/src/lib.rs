//! Chuyển sách Kindle thành «sách dạng EPUB» cho màn đọc của GramBook (THIET-KE 4.5, DECISIONS B3).
//!
//! Vào: toàn bộ byte của tệp `.mobi` / `.prc` / `.azw` (MOBI6), `.azw3` (KF8) hoặc tệp kép MOBI6 + KF8 (KindleGen).
//! Ra: [`MobiBook`] — các tệp chương (thứ tự đọc), ảnh / CSS / phông, mục lục, thông tin sách. Lõi (gm-core) phục vụ các tệp này
//! cho UIWebView y như mục trong một EPUB.
//!
//! HỢP ĐỒNG API — gm-core dựa vào đúng các kiểu / hàm dưới đây.
//!
//! Cách làm (chỉ dùng std, không phụ thuộc ngoài — dựng cho armv7-apple-ios6):
//! - `pdb`: bảng bản ghi PalmDB; `header`: đầu PalmDOC / MOBI / EXTH của bản ghi 0;
//! - `decomp`: bỏ dữ liệu đuôi bản ghi chữ rồi giải nén (PalmDOC LZ77, HUFF/CDIC);
//! - `index`: chỉ mục INDX (TAGX, IDXT, CNCX) — NCX, SKEL, FRAG;
//! - `mobi6`: HTML Mobipocket → chương HTML (neo filepos, ngắt `mbp:pagebreak`, ảnh recindex);
//! - `kf8`: AZW3 / phần KF8 của tệp kép → XHTML (khung + mảnh, `kindle:pos` / `kindle:embed` / `kindle:flow`);
//! - `xmlcheck`: kiểm cú pháp XML (tệp KF8 hỏng cú pháp phục vụ dạng `text/html`).

mod decomp;
mod header;
mod html;
mod index;
mod inflate;
mod kf8;
mod meta;
mod mobi6;
mod pdb;
mod resource;
mod text;
mod xmlcheck;

/// Một tệp trong sách đã chuyển.
#[derive(Clone, Debug)]
pub struct Item {
    /// Đường dẫn tương đối, chỉ chữ ASCII, `/` ngăn thư mục: «text/part0000.html», «images/00001.jpg», «styles/flow0001.css».
    /// Liên kết giữa các tệp trong sách dùng đúng các đường dẫn tương đối này.
    pub path: String,
    /// Content-Type: «application/xhtml+xml» (tệp KF8 đúng cú pháp XML), «text/html» (MOBI6), «text/css», «image/jpeg»…
    pub mime: String,
    pub data: Vec<u8>,
}

/// Một mục của mục lục.
#[derive(Clone, Debug, PartialEq)]
pub struct TocEntry {
    pub title: String,
    /// «text/part0003.html» hoặc «text/part0003.html#id» (neo có thật trong tệp đó).
    pub href: String,
    /// 0 = mục cấp ngoài cùng.
    pub depth: u32,
}

/// Định dạng gốc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// MOBI6 / PRC / AZW (HTML kiểu Mobipocket).
    Mobi6,
    /// KF8 (AZW3, hoặc phần KF8 của tệp kép).
    Kf8,
}

/// Sách đã chuyển.
#[derive(Clone, Debug)]
pub struct MobiBook {
    pub format: Format,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    /// Các tệp chương theo thứ tự đọc (spine).
    pub spine: Vec<Item>,
    /// Ảnh, CSS, phông… (không nằm trong spine).
    pub resources: Vec<Item>,
    pub toc: Vec<TocEntry>,
    /// Đường dẫn ảnh bìa trong `resources`.
    pub cover: Option<String>,
}

/// Lỗi chuyển.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MobiError {
    /// Không phải tệp Kindle / hỏng không đọc được (kèm lý do ngắn).
    Invalid(String),
    /// Sách có DRM (mã hoá) — không mở được.
    Drm,
    /// Kiểu nén / cấu trúc chưa hỗ trợ (kèm tên).
    Unsupported(String),
}

impl std::fmt::Display for MobiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobiError::Invalid(s) => write!(f, "Tệp Kindle hỏng: {s}"),
            MobiError::Drm => write!(f, "Sách có DRM — không mở được."),
            MobiError::Unsupported(s) => write!(f, "Chưa đọc được kiểu sách Kindle này ({s})."),
        }
    }
}

/// Chuyển cả tệp. Không bao giờ panic với dữ liệu hỏng / độc (trả lỗi).
pub fn convert(data: &[u8]) -> Result<MobiBook, MobiError> {
    let pdb = pdb::Pdb::parse(data)?;
    match &pdb.kind {
        b"BOOKMOBI" => {}
        b"TEXtREAd" => return mobi6::convert_palmdoc(&pdb),
        _ => {
            return Err(MobiError::Invalid(
                "không phải tệp Kindle / Mobipocket".into(),
            ))
        }
    }
    let h0 = header::parse(pdb.record(0), true)?;
    if h0.encryption != 0 {
        return Err(MobiError::Drm);
    }
    let m0 = h0
        .mobi
        .as_ref()
        .ok_or_else(|| MobiError::Invalid("thiếu đầu MOBI".into()))?;
    // KF8 thuần (AZW3): phiên bản 8 và có chỉ mục khung / FDST (PRC cũ có thể ghi phiên bản bậy).
    if m0.version == 8 && (m0.skel != header::NULL || m0.fdst != header::NULL) {
        return kf8::convert(&pdb, 0, &h0, None);
    }
    // Tệp kép: ưu tiên phần KF8; phần KF8 hỏng thì quay về MOBI6.
    if let Some(k8) = kf8_boundary(&pdb, m0) {
        if let Ok(h8) = header::parse(pdb.record(k8), true) {
            if h8.encryption != 0 {
                return Err(MobiError::Drm);
            }
            match kf8::convert(&pdb, k8, &h8, Some(&h0)) {
                Ok(b) => return Ok(b),
                Err(MobiError::Unsupported(s)) if s.contains("64 MB") => {
                    return Err(MobiError::Unsupported(s))
                }
                Err(_) => {}
            }
        }
    }
    mobi6::convert(&pdb, &h0)
}

/// Bản ghi đầu KF8 của tệp kép: EXTH 121 (bản ghi ngay sau bản ghi «BOUNDARY»), dự phòng: tìm bản ghi «BOUNDARY».
fn kf8_boundary(pdb: &pdb::Pdb, m0: &header::Mobi) -> Option<usize> {
    let is_kf8 = |i: usize| {
        let r = pdb.record(i);
        r.get(16..20) == Some(b"MOBI".as_slice())
            && r.get(0x24..0x28) == Some(&8u32.to_be_bytes()[..])
    };
    if let Some(v) = m0.exth_u32(121).filter(|&v| v != header::NULL && v > 0) {
        let k = v as usize;
        if k < pdb.len() && (pdb.record(k - 1) == b"BOUNDARY" || is_kf8(k)) {
            return Some(k);
        }
    }
    (1..pdb.len().saturating_sub(1))
        .find(|&i| pdb.record(i) == b"BOUNDARY" && is_kf8(i + 1))
        .map(|i| i + 1)
}
