# GramBook

App **đọc sách / ebook lưu trên Telegram** (không chính thức) cho **iPhone / iPad / iPod touch chạy iOS 5 và iOS 6 đã jailbreak**.
Mở app là có thư viện: GramBook tự tìm mọi tệp sách trong Tin nhắn đã lưu, nhóm, kênh của tài khoản Telegram — **EPUB, MOBI, AZW3
(AZW, PRC)** — tìm không dấu, xếp theo tên / tác giả, đọc với **lật trang cong**, chia cột như sách in, giao diện tối và ánh sáng ấm
cho đỡ mỏi mắt ban đêm. Gửi sách cho bạn bè ngay trong app.

Cùng nền với [GramMusic](https://github.com/itovn/GramMusic) và [GramMini](https://github.com/itovn/GramMini): lõi **Rust** nói
chuyện thẳng với Telegram bằng MTProto ([grammers](https://codeberg.org/Lonami/grammers)), vỏ **Objective-C / UIKit** dựng bằng
Theos, giao diện iOS 6 + kính. Không cần server riêng — phiên đăng nhập chỉ nằm trên máy. Một gói `.deb` chạy cả iOS 5 và 6, cài qua
Cydia.

> GramBook dùng Telegram API nhưng **không phải** sản phẩm của Telegram, cũng không liên quan tới Apple.

**Trang giới thiệu:** <https://itovn.github.io/GramBook/> · **Nhóm Telegram:** [t.me/tivikit](https://t.me/tivikit) — hỏi đáp, báo lỗi, bản mới.

## Máy chạy được

iOS 5.0 – 6.1.x, đã jailbreak: iPhone 4 / 4S / 5, iPad 1 / 2 / 3 / 4 / mini 1, iPod touch 4 / 5. Đã thử trên iPad mini 1 (iOS 6.1.3);
`symcheck` (mọi hàm hệ thống app gọi đều có) trên iOS 6.1.4 và iOS 5.1.1.

## Có gì

- **Thư viện từ cả tài khoản**: quét Tin nhắn đã lưu, nhóm, kênh (chat riêng chỉ lấy EPUB / MOBI / AZW3 / FB2 — khỏi lẫn tài liệu
  công việc); **Nguồn**: ẩn / hiện từng chat, thêm kênh sách công khai bằng @tên hoặc link t.me; **Sách trên máy** (thư mục Documents).
- Tìm không dấu; xếp Mới nhất / Tên A–Z / Tác giả + thanh chữ cái; lọc theo định dạng, tình trạng đọc; tên và tác giả thật lấy
  trong sách sau lần mở đầu.
- **Màn đọc**: chia cột như sách in (iPad ngang: 2 trang), **lật cong theo ngón tay**, trượt hoặc không hiệu ứng; chạm mép để lật;
  mục lục, dấu trang, «quay lại» sau mỗi lần nhảy; chú thích hiện tại chỗ; giữ đúng chỗ đang đọc khi đổi cỡ chữ / xoay máy; dòng đầu
  trang có giờ, pin, tên chương; «còn N trang hết chương».
- **Aa**: 12 cỡ chữ, 8 font có đủ dấu tiếng Việt (kèm Literata), nền Trắng / Giấy ngà / Xám / Đêm, giãn dòng, lề, chữ đậm hơn, căn
  đều, giữ kiểu của sách.
- **Đỡ mỏi mắt**: giao diện tối cả app (bật tay hoặc tự động 19:00 – 6:00), nền trang tối riêng cho ban đêm, **ánh sáng ấm** (lọc
  bớt ánh sáng xanh — iOS 5 / 6 không có Night Shift), vuốt dọc mép trái chỉnh độ sáng (tối hơn mức thấp nhất của máy).
- **MOBI / AZW3 / AZW / PRC**: lõi tự chuyển sách Kindle (MOBI6, KF8, HUFF/CDIC) thành chương HTML — không cần Calibre.
- **Đang đọc** + thanh «Đọc tiếp», bộ sưu tập, Muốn đọc, Yêu thích, **Tải về máy** để đọc không cần mạng; iOS tắt app lúc nằm nền
  thì mở lại đúng cuốn, đúng trang.
- **Gửi cho bạn qua Telegram**: chuyển tiếp tin chứa sách (không tải lên lại) hoặc gửi sách trên máy; tôn trọng «Hạn chế lưu nội
  dung» của chat (không gửi tiếp, không chép chữ).
- **MTProxy** (`dd`, `ee` fake-TLS) và SOCKS5; **DNS mã hoá** (DoH / DoT: NextDNS → Cloudflare → Google, lỗi hết thì DNS của máy).

## Cài đặt

- **Cydia:** thêm nguồn `https://itovn.github.io/GramBook/cydia/` rồi cài **GramBook**.
- **Tay:** tải `vn.grambook.app_<bản>_iphoneos-arm.deb` ở [Releases](https://github.com/itovn/GramBook/releases), chép vào máy,
  `dpkg -i`, rồi `uicache` (hoặc khởi động lại SpringBoard).

## Dựng từ mã nguồn

Máy Ubuntu có [Theos](https://theos.dev) (SDK iPhoneOS 9.3) và Rust nightly (`rust-src`):

1. Tạo `api_id` / `api_hash` của riêng bạn ở [my.telegram.org](https://my.telegram.org), ghi vào `keys/telegram.txt` (`api_id=…`,
   `api_hash=…` — không có trên git). Tuỳ chọn: mã hồ sơ NextDNS vào `keys/nextdns.txt` (không có thì DNS mã hoá đi thẳng
   Cloudflare → Google).
2. `bash tools/build-app.sh` — bản thử (có công cụ thử `app/Debug/`); `bash tools/build-release.sh` — bản phát hành.

Lõi dựng cho armv7 Cortex-A8 bằng target tự định nghĩa (`tools/rust/armv7-apple-ios6.json`, `-Z build-std`); thư viện mã hoá `ring`
được vá cho ARM 32-bit của Apple (`tools/rust/vendor-ring.sh`). Chạy thử lõi trên PC:

```sh
cd core
cargo test -p gm-core -p gb-mobi -p gm-net
cargo run -p gm-cli -- book ../pc-data <tệp .epub / .mobi / .azw3>   # mở sách qua lõi thật, lấy mọi chương qua cổng nội bộ
cargo run -p gb-mobi --example dump -- <tệp .mobi / .azw3> <thư mục>  # xem sách Kindle đã chuyển
```

## Tài liệu

| Tệp | Nội dung |
| --- | --- |
| [docs/THIET-KE.md](docs/THIET-KE.md) | Thiết kế: kiến trúc, thư viện, màn đọc, giao diện, lộ trình |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Các quyết định kỹ thuật và kết quả kiểm chứng |
| [docs/FFI.md](docs/FFI.md) | Hợp đồng lệnh / sự kiện giữa app và lõi, cổng phục vụ sách, reader.js |
| [docs/PROGRESS.md](docs/PROGRESS.md) | Nhật ký tiến độ |

## Giấy phép

[GPL-3.0](LICENSE) (dùng lại mã GPL-3.0 của GramMusic / GramMini). `core/vendor/grammers-*` giữ giấy phép MIT / Apache-2.0 của
grammers; font Literata giữ giấy phép [SIL OFL 1.1](app/Resources/Fonts/OFL.txt); DNS mã hoá chép từ ChimToTV (GPL-3.0).
