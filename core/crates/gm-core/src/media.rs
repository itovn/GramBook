//! Tệp trên Telegram: vị trí tải (kèm DC — chép từ GramMini), nhận sách từ tin nhắn, ảnh nhỏ / ảnh bìa.

use grammers_client::media::{Document, Downloadable, Media, PhotoSize};
use grammers_client::message::Message;
use grammers_client::tl;

use crate::library::Book;
use crate::text;

/// Vị trí tệp + DC giữ tệp. `PhotoSize` của grammers không mang DC (tải sẽ hỏi DC chính → tệp ở DC khác bị lỗi), nên bọc lại
/// kèm DC.
#[derive(Clone)]
pub struct Loc {
    pub loc: tl::enums::InputFileLocation,
    pub dc: Option<i32>,
    pub size: Option<usize>,
}

impl Downloadable for Loc {
    fn to_raw_input_location(&self) -> Option<tl::enums::InputFileLocation> {
        Some(self.loc.clone())
    }

    fn dc_id(&self) -> Option<i32> {
        self.dc
    }

    fn size(&self) -> Option<usize> {
        self.size
    }
}

/// Tệp của document.
pub fn document_loc(d: &Document) -> Option<Loc> {
    Some(Loc { loc: d.to_raw_input_location()?, dc: d.dc_id(), size: d.size() })
}

/// Ảnh nhỏ đính kèm tệp (bìa sách nếu người gửi / Telegram tạo): cỡ lớn nhất tải được.
pub fn doc_thumb_loc(d: &Document) -> Option<Loc> {
    let thumbs: Vec<PhotoSize> = d.thumbs().into_iter().filter(|t| t.to_raw_input_location().is_some()).collect();
    let t = thumbs.iter().max_by_key(|t| t.size())?;
    Some(Loc { loc: t.to_raw_input_location()?, dc: d.dc_id(), size: Some(t.size()) })
}

/// Ảnh đại diện nhỏ (160 px) của một peer.
pub fn avatar_loc(peer: &grammers_session::types::PeerRef, photo_id: i64, dc: i32) -> Loc {
    Loc {
        loc: tl::enums::InputFileLocation::InputPeerPhotoFileLocation(tl::types::InputPeerPhotoFileLocation {
            big: false,
            peer: peer.into(),
            photo_id,
        }),
        dc: Some(dc),
        size: None,
    }
}

/// Nơi tin nằm: chat riêng với người (không phải Tin nhắn đã lưu, không phải bot) hay nơi khác (Tin nhắn đã lưu, bot, nhóm, kênh).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    Private,
    Other,
}

/// Sách từ một tin: tài liệu có đuôi / mime sách. Chat riêng với người chỉ lấy định dạng chắc chắn là sách (EPUB / FB2 / MOBI /
/// AZW3… — PDF, TXT, DOC trong chat riêng hay là giấy tờ: DECISIONS B4). Tên / tác giả đoán từ tên tệp («Tên sách - Tác giả»);
/// mở sách xong lõi lấy tên thật trong sách. `peer` = khoá peer của chat chứa tin.
pub fn book_from(m: &Message, peer: &str, origin: Origin) -> Option<Book> {
    let Some(Media::Document(d)) = m.media() else { return None };
    let name = d.name().unwrap_or("").to_string();
    let mime = d.mime_type().unwrap_or("").to_string();
    let ext = text::ext_of(&name, &mime);
    if !text::book_ext(&ext) || (origin == Origin::Private && !text::private_ok(&ext)) {
        return None;
    }
    let size = d.size().unwrap_or(0) as u64;
    // Tệp rỗng / quá nhỏ để là sách (vài trăm byte — tệp hỏng, tệp thử)
    if size < 512 {
        return None;
    }
    let (title, author) = text::title_from_file(&name);
    let title = if title.is_empty() { "Sách không tên".to_string() } else { title };
    Some(Book {
        peer: peer.to_string(),
        id: m.id(),
        title,
        author: author.unwrap_or_default(),
        size,
        mime,
        name,
        ext,
        date: m.date().as_second(),
        doc: d.id(),
        thumb: doc_thumb_loc(&d).is_some(),
        topic: topic_of(m),
        caption: text::truncate_chars(m.text().trim(), 500),
        bt: None,
        ba: None,
        nf: noforwards(m),
    })
}

/// Tin trong chat bật «Hạn chế lưu nội dung»: Telegram gắn `noforwards` cho từng tin — app phải chặn gửi tiếp / lưu ra ngoài.
pub fn noforwards(m: &Message) -> bool {
    matches!(&m.raw, tl::enums::Message::Message(x) if x.noforwards)
}

/// Chủ đề của tin trong nhóm diễn đàn (cờ `forum_topic`): có `reply_to_top_id` → đó là chủ đề (tin trả lời một tin trong chủ đề);
/// không có → `reply_to_msg_id` chính là chủ đề. Không thuộc chủ đề nào (General / nhóm thường) → 0. Như GramMini `reply_topic`.
pub fn topic_of(m: &Message) -> i32 {
    let tl::enums::Message::Message(x) = &m.raw else { return 0 };
    let Some(tl::enums::MessageReplyHeader::Header(h)) = &x.reply_to else { return 0 };
    if !h.forum_topic {
        return 0;
    }
    h.reply_to_top_id.or(h.reply_to_msg_id).unwrap_or(0)
}

/// base64 (ảnh mờ thu nhỏ trong JSON).
pub fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_known() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }
}
