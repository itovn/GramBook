//! Lõi GramBook (phần không dính giao diện): phiên đăng nhập (`session`, chép từ GramMini), thư viện sách lấy từ Telegram
//! (`library`, `sync`, `media`), tải tệp (`files`), mở EPUB (`zip`, `xml`, `epub`, `chapter`), cổng nội bộ phục vụ sách cho
//! UIWebView (`serve`), vị trí đọc / dấu trang (`reading`), bộ sưu tập (`playlists`), dịch vụ cho app (`service`).

pub mod cache;
pub mod chapter;
pub mod epub;
pub mod files;
pub mod library;
pub mod media;
pub mod playlists;
pub mod reading;
pub mod serve;
pub mod session;
pub mod text;
pub mod xml;
pub mod zip;

pub use session::FileSession;
pub mod service;
pub mod sync;

pub use service::{Config, Core, CoreError, Emit};

/// Nhật ký lõi cho các module không giữ `Core` (files, serve): service.rs gắn hàm ghi `core.log` lúc mở lõi.
static LOG: std::sync::OnceLock<std::sync::Arc<dyn Fn(&str) + Send + Sync>> = std::sync::OnceLock::new();

pub fn set_logger(f: std::sync::Arc<dyn Fn(&str) + Send + Sync>) {
    let _ = LOG.set(f);
}

pub fn dlog(msg: &str) {
    if let Some(f) = LOG.get() {
        f(msg);
    }
}
