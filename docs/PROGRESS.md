# GramBook — Tiến độ

## 17/11/2026 — khởi động dự án, bước 0

- Đọc GramMusic (tài liệu, lõi, app, bộ dựng, công cụ thử) và phân tích iBooks 3.1.3 cho iOS 5 (`Tham_Khao/`, chỉ có `.ipa`): tài
  nguyên (`user_stylesheet_flowable.css.tmpl`, `FontPresets~ipad.plist`, `epub-*.js`) và tên lớp trong tệp chạy (`BKEpubBookParser`,
  `BKPaginationController`, `BKEpubCFILocation`, `BKBookProtocol`…) → [THIET-KE.md](THIET-KE.md).
- Anh duyệt phương án (DECISIONS B1–B6): định dạng ưu tiên EPUB / MOBI / AZW3; chat riêng chỉ lấy EPUB / FB2 / MOBI; lật cong kiểu
  iBooks ngay; giữ `GM` cho phần chép, `GB` cho phần mới; một `.deb` iOS 5 + 6; dùng chung `api_id` với GramMusic.
- Kho sách mẫu: 303 EPUB vnthuquan (`Tham_Khao/ebook`) — khảo sát cấu trúc (B9).
- **Bước 0** trên iPad mini 1 (Safari, cùng WebKit với UIWebView), máy chủ thử `spike/epub-columns/` chạy trên máy Ubuntu
  (`~/TestLab/GramBook/spike`, cổng 8765; điều khiển trang đang mở qua `/cmd`):
  - chia cột CSS: 8 sách / 220 chương, đếm trang khớp tuyệt đối; đếm cả sách 0,1 – 9,5 s; chương 1,3 MB mở mất 2,8 s → lõi cắt
    chương lớn (B7);
  - font tiếng Việt: dò từng chữ trong 23 font — Palatino thiếu «ớ»; chọn Georgia (mặc định) + Literata đóng kèm + 6 font hệ thống (B8);
  - sửa ảnh cao gần bằng trang bị tràn đáy; thấy cần CSS viết thẳng vào trang, đo sau `load`, font đóng kèm phải đăng ký với hệ
    thống (font web về muộn làm đổi số trang).
- Song song: khảo sát tính năng các app đọc sách (Kindle, Apple Books, KOReader, Moon+, vBook…) theo yêu cầu của anh.

## 17/11/2026 (tối) — khảo sát app đọc sách, kho mẫu SMB, GĐ0 + lõi GĐ1

- **Khảo sát tính năng** các app đọc sách (iBooks / Apple Books, Kindle, Play Sách, Kobo, KOReader, Moon+, Marvin, KyBook, ReadEra,
  vBook…) → THIET-KE mục 11; đưa vào GĐ1: thanh chân trang, độ đậm chữ, «quay lại», vuốt mép chỉnh sáng, gửi sách qua Telegram, tôn
  trọng «Hạn chế lưu nội dung».
- **Kho sách mẫu trên máy chủ SMB nội bộ** (anh chỉ): 10.392 EPUB, 3.248 PDF, 131 MOBI / PRC, 50
  AZW3… Chép về `Tham_Khao/kindle` (181 tệp Kindle: 131 MOBI6 — 14 có DRM, 2 nén HUFF/CDIC; 50 AZW3 / KF8) và `Tham_Khao/epub2` (147
  EPUB, 5 cuốn mỗi thể loại). Không có EPUB 3; 7 cuốn dùng `&nbsp;` trong XHTML.
- **GĐ0 — fork GramMusic** (B1): chép lõi / app / bộ dựng / công cụ, bỏ phần nhạc (trình phát, giải mã, lời bài hát, nhạc trên máy,
  tweak màn hình khoá); đổi tên GramBook, `vn.grambook.app`, `~/Library/GramBook`; repo git mới (chưa commit).
- **Lõi** (`core/crates/gm-core`, viết lại phần sách): `text` (định dạng sách, «Tên sách - Tác giả», bỏ cụm rác tên tệp), `library`
  (sách, tên / tác giả thật từ OPF cho mọi bản trùng), `media` (nhận sách, luật chat riêng B4), `sync` (một lượt tài liệu), `files`
  (tải tệp — từ stream.rs), `zip` (đọc zip tự viết + miniz_oxide), `xml` (tách thẻ chịu lỗi, bảng thực thể HTML, kiểm cú pháp XML),
  `epub` (OPF, NCX, nav, bìa, font làm rối IDPF / Adobe, DRM), `chapter` (NFC, đổi thực thể, XHTML / HTML, cắt chương > 150 KB, id →
  phần), `serve` (cổng 127.0.0.1 phục vụ sách + reader.js, mở lại cổng — M23), `reading` (vị trí đọc, dấu trang, «Đang đọc»), `service`
  (lệnh mới — docs/FFI.md), `assets/reader.js` (màn đọc trong UIWebView). Crate `gb-mobi` (MOBI / AZW3) đang viết song song.
- **Thử trên PC**: 39 phép thử `gm-core` đạt — gồm đọc 450 EPUB mẫu (không cuốn nào phải dựng lại mục lục), chương 1,3 MB → 9 phần /
  186 ms (bản thử). `gm-cli book`: mở qua cổng thật — Bê Trọc 28 phần / 3,4 MB trong 0,53 s, Hồi kí Nguyễn Hiến Lê 48 phần / 0,69 s.
- **App** (GĐ1): khung 5 mục (Đang đọc, Thư viện, Nguồn, Bộ sưu tập, Cài đặt) + thanh «Đọc tiếp»; danh sách sách (tìm không dấu,
  xếp, lọc định dạng / tình trạng đọc), bìa tạm vẽ tay, sách trên máy (`~/Documents`), bộ sưu tập; **màn đọc**: UIWebView chia cột
  phủ trên UIPageViewController lật cong bằng ảnh chụp trang (B5), trượt / không hiệu ứng, chạm mép / giữa, link + chú thích (kể cả
  chú thích Word của vnthuquan), mục lục + dấu trang, bảng Aa (độ sáng + lớp tối, cỡ chữ 12 nấc theo từng font, 8 font + theo sách,
  4 nền, giãn dòng, lề, chữ đậm, căn đều, ngang 2 trang, kiểu lật), «quay lại», lưu vị trí, giữ màn hình sáng 10 phút. Font Literata
  đóng kèm (UIAppFonts).

## 17/11/2026 (khuya) — chạy trên iPad mini 1, sách Kindle, gửi sách, giờ / pin / độ sáng

- **Dựng + cài iPad mini 1** (iOS 6.1.3): sửa lỗi dựng iOS 5 (`objectAtIndexedSubscript:`, kiểu trả về xoay máy). Đo trên máy (lõi
  bản release): mở sách 9 – 12 ms; chương 1,3 MB của «Bê Trọc» → 9 phần XHTML trong 373 ms; ngang 2 trang, căn giữa tên chương, căn
  đều, giữ màu HTML Word; chạm lật dùng ảnh chụp sẵn; lật cong (mặt sau mờ như iBooks); bảng Aa, Mục lục (tô mục đang đọc); thanh
  dưới «Chương 4 · trang 1/29 · 62% cả sách»; «Đang đọc» + thanh «Đọc tiếp».
- **Sửa crash khi đóng sách lúc bảng Mục lục / Aa đang mở** (iPad): UIPopoverController bị giải phóng khi còn hiện → UIKit ném lỗi
  trong `dealloc` của màn đọc. Đóng popover trước (`close`, `dealloc`); mở sách khác khi đang đọc thì đóng hẳn sách cũ (lưu vị trí,
  `book.close`). Đã thử lại trên máy.
- **MOBI / AZW3 / AZW / PRC** (DECISIONS B10): crate `gb-mobi` xong (117 MOBI6 + 50 KF8 của kho SMB mở được, 14 DRM báo rõ, 0 lỗi;
  thử đột biến / tệp hỏng không panic) → nối vào `book.open` (chuyển lúc mở → `Store::Mem`, bìa ghi ra `covers/`), app coi MOBI /
  AZW3 / AZW / PRC là đọc được. Qua lõi thật trên PC (`gm-cli book`): 167 / 167 cuốn mở + lấy mọi phần qua cổng; sách thường < 100 ms.
  **Trên iPad mini 1** (chuyển + mở): «Bích Huyết Kiếm» MOBI 1 MB 266 ms, «Đạo Phật Ngày Nay» AZW3 65 ms, PRC 57 KB 30 ms, «Lộc Đỉnh
  Ký» MOBI 3,6 MB / 255 chương 712 ms; dàn trang chương 100 – 350 ms. Bìa lấy trong sách hiện ở «Đang đọc»; mục lục, «Đọc tiếp» đúng.
  Tên trong sách không dấu mà tên tệp có dấu («13 bi quyet…» / «13 Bí Quyết…») → lấy bản có dấu.
- **Sửa rò màn đọc**: đóng màn đọc không qua nút «Thư viện» thì NSTimer giữ màn đọc 10 phút (popover cũ còn trên màn hình, vị trí
  không lưu). Nay mọi đường đóng đều chạy `shutdown` (lưu vị trí, `book.close`, đóng popover, cập nhật «Đọc tiếp»), hẹn giờ huỷ khi
  màn đọc khuất.
- **Màn đọc**: giờ (trái) + pin (phải, có hình cục pin, tia sét khi sạc) ở dòng đầu trang; **vuốt dọc ở mép trái chỉnh độ sáng**
  (dưới mức thấp nhất của máy thì thêm lớp tối — dùng chung với thanh độ sáng của Aa); ghi thời gian nạp từng chương vào app.log.
- **Gửi sách cho bạn qua Telegram** (`chats.recent`, `book.forward`): bảng «⋯» → «Gửi cho bạn qua Telegram…» → chọn chat (Tin nhắn
  đã lưu + chat gần đây, bỏ kênh không có quyền đăng) → hỏi lại → chuyển tiếp tin gốc (không tải lên lại). Chưa thử được trên máy
  (chưa đăng nhập).
- **«Hạn chế lưu nội dung»**: lõi đọc cờ `noforwards` của tin (`nf` trong Book / `book.open`) → ẩn nút gửi, lõi từ chối chuyển tiếp,
  màn đọc không cho chọn / chép chữ.
- `symcheck`: 304 ký hiệu, thiếu 0 trên iPhone và iPad 1 (iOS 5.1.1). Biểu tượng tạm cho GramBook (sách mở + máy bay giấy, cùng
  họ với GramMusic — `tools/icon/make_icon.py draw`) để khỏi lẫn với GramMusic trên màn hình chính.

## 18/11/2026 — giao diện tối (DECISIONS B11)

- Cả app tối theo Cài đặt → Giao diện (Sáng / Tối / Tự động 19:00 – 6:00): Đang đọc, Thư viện, Nguồn, Bộ sưu tập, Cài đặt, bảng Aa,
  Mục lục, chọn chat… (trước đây các màn mới của GramBook không nhận kiểu chung — vẫn trắng). Tiêu đề nhóm vẽ lại cho cả hai giao diện.
- Sách mở lúc giao diện tối dùng nền Đêm (nền trang nhớ riêng cho sáng / tối). Công tắc «Giao diện tối» trong bảng Aa: trang và bảng
  đổi ngay, không đóng sách; khung app đổi khi đóng sách. Chế độ Tự động tới giờ là đổi cả khi đang đọc liền.
- Thử trên iPad mini 1: 5 mục ở giao diện tối và sáng, mở sách MOBI / AZW3 ở giao diện tối (nền Đêm), Aa + Mục lục tối, gạt công tắc
  trong Aa (Trắng ↔ Đêm tại chỗ), đổi sang sáng khi đang đọc rồi đóng sách (khung dựng lại sáng).

- **Đỡ mỏi mắt thêm**: ánh sáng ấm (lớp hổ phách phủ màn đọc, thanh «Ấm» trong Aa) và nút trăng / mặt trời trên thanh màn đọc (iPad) bật / tắt giao diện tối một chạm — thử trên iPad mini 1 (Đêm + ấm 70 %, Trắng + ấm, đổi sáng / tối tại chỗ).
- **Biểu tượng app theo mẫu anh đưa** (DECISIONS B12): cắt ô vuông từ ảnh mẫu, bo góc trong suốt, đủ các cỡ iOS 5 / 6 (29 – 144 px);
  cài lên iPad mini 1 (respring để SpringBoard nạp ảnh mới).
  Sửa theo ý anh: bo góc ít hơn (bằng mặt nạ iOS 6) + nền kính mờ kiểu macOS Tahoe (`tools/icon/glass_icon.py`).

## 18/11/2026 (sáng) — đăng nhập iPad mini 1, demo gửi / đọc sách qua một chat riêng

- Đăng nhập tài khoản thử trên iPad mini 1 (anh đưa mã; mã đầu quá 3,5 giờ bị từ chối → xin mã mới). Tài khoản có 7 chat, 126 tệp
  nhưng không có sách → thư viện trống (đúng).
- **Gửi sách trên máy qua Telegram** (mới): `book.forward` với sách `file:` → lõi tải tệp lên (`upload_stream`) rồi gửi như tài
  liệu, giữ tên tệp; bảng «⋯» hiện «Gửi cho bạn qua Telegram…» cả cho sách trên máy. Demo theo yêu cầu của anh: gửi «Bê Trọc» (EPUB
  633 KB, 1,1 s) và «Bích Huyết Kiếm» (MOBI 1 MB, 1,6 s) vào chat riêng với tài khoản chính của anh.
- Quét lại: 2 cuốn đó hiện trong Thư viện (nguồn là chat riêng đó — luật chat riêng B4). Mở MOBI từ Telegram: tải 1 MB trong 1,07 s +
  chuyển / mở 147 ms; tên sách thật (có dấu) cập nhật sau lần mở. Chuyển tiếp sách Telegram (tin gốc) vào Tin nhắn đã lưu: 115 ms.

## 18/11/2026 — rà soát trước khi phát hành 1.0.0 (DECISIONS B13)

- Anh nhờ tự thử lại hết rồi commit, phát hành GitHub + nguồn Cydia như GramMusic.
- PC: phép thử lõi đạt; 617 / 617 sách mẫu không DRM mở được qua lõi thật. iPad mini 1: màn đọc, thư viện, bộ sưu tập, giao diện
  sáng / tối, gửi sách, sách từ Telegram — đạt. Sửa: đổi cỡ chữ trôi chỗ đọc; mở lại đúng cuốn sau khi iOS tắt app lúc nằm nền
  (`0xdead10cc` — WebKit giữ khoá tệp đệm shader); biểu tượng «Bộ sưu tập».
- Bản 1.0.0: README, trang giới thiệu + nguồn Cydia (nhánh `gh-pages`, ảnh chụp với sách mẫu), mô tả gói không dấu (M21 của
  GramMusic).

## 18/11/2026 — phát hành 1.0.0

- Gói `vn.grambook.app_1.0.0_iphoneos-arm.deb` (`tools/build-release.sh`): 3.207.510 byte, sha256 `1cf961cd…`, không còn mã thử;
  `symcheck` 305 ký hiệu, thiếu 0 trên iOS 6.1.3 (iPad mini 1) và iOS 5.1.1 (iPad 1).
- GitHub: repo công khai **itovn/GramBook** (nhánh `main`), bản phát hành **v1.0.0** kèm `.deb`; trang giới thiệu kiểu iOS 6 + nguồn
  Cydia ở nhánh `gh-pages` (https://itovn.github.io/GramBook/, nguồn `https://itovn.github.io/GramBook/cydia/` — `tools/update-cydia-
  repo.sh`); tệp `.deb` trên nguồn Cydia khớp từng byte với bản trên GitHub. Trước khi đưa lên: rà IP / số điện thoại / tên chat /
  mã peer khỏi tài liệu.
- Đã cài bản phát hành 1.0.0 trên iPad mini 1 (thay bản thử): mở được, vẫn đăng nhập, tự mở lại sách đang đọc dở.
- **iPad 1 (iOS 5.1.1, 247 MB RAM)** — thử thật sau khi phát hành (máy khoá màn hình lâu, rồi phiên GramMini đang dùng): EPUB «Bê Trọc»
  mở 1,1 s, chương 1,3 MB → 9 phần / 29 màn; MOBI 0,6 s; AZW3 0,5 s; lật trang, nhảy chương, mục lục, Aa, đổi cỡ chữ, giao diện tối tại
  chỗ, Đang đọc — đúng, chữ tiếng Việt đủ dấu; không có báo cáo lỗi nào. Đã cài bản phát hành 1.0.0 thay bản thử.

## 18/11/2026 (chiều) — mặt sau tờ giấy khi lật cong (DECISIONS B14)

- Anh báo mặt sau tờ đang lật «trắng bệt, chưa giống giấy sách» → lật hai mặt, mặt sau do app vẽ: giấy ngà theo từng nền (Trắng / Giấy ngà
  / Xám / Đêm), chữ của chính trang đó in ngược mờ 12 %. Thử trên iPad mini 1 (vuốt + chạm, tới / lùi) và iPad 1 (chạm).
  Anh vuốt thử bằng tay trên iPad mini 1 (AZW3 «Đạo Phật Ngày Nay»): đạt.
- Bản dò giữa chừng làm mất thao tác cầm trang lật theo ngón tay (anh báo) — do cố ý để UIKit ném lỗi trong `setViewControllers`;
  bản sau bỏ, log xác nhận vuốt lật lại được. Gỡ `dataSource` / `delegate` của UIPageViewController khi đóng màn đọc.

## 18/11/2026 (chiều) — phát hành 1.0.1

- Gói `vn.grambook.app_1.0.1_iphoneos-arm.deb` (`tools/build-release.sh`): 3.208.340 byte, sha256 `87b9cfc7…`, không còn mã thử;
  `symcheck` 305 ký hiệu, thiếu 0 trên iOS 6.1.3 (iPad mini 1) và iOS 5.1.1 (iPad 1).
- GitHub: bản phát hành **v1.0.1** (Latest) kèm `.deb`; trang giới thiệu + trang mô tả Cydia ghi «Bản 1.0.1» (thêm mục thay đổi);
  nguồn Cydia có 1.0.0 và 1.0.1 — tệp trên nguồn Cydia khớp từng byte với bản trên GitHub (kiểm qua mạng sau khi Pages dựng xong).
- Đã cài bản phát hành 1.0.1 trên iPad mini 1 (thay bản thử; tự mở lại sách đang đọc) và iPad 1: mở được, không báo cáo lỗi.

## 19/11/2026 — biểu tượng app theo mẫu thứ hai (DECISIONS B15)

- Anh đưa mẫu mới `Tham_Khao/logo/logo2.png` (ô kính viền neon, sách mở + máy bay giấy trên nền chàm). Cắt ô biểu tượng của mẫu,
  nắn 4 góc từ bo 25 % về 16,5 % (bằng mặt nạ iOS 5 / 6) — `tools/icon/neon_icon.py`; đủ 8 cỡ iOS 5 / 6 (29 – 144 px), xem trên nền
  tối và sáng: rõ cả ở 29 px.

## 19/11/2026 — cập nhật OTA như GramMini (DECISIONS B16)

- Lõi: `http.get` / `http.download` (reqwest + rustls); app: `GMUpdate` (GitHub Releases, hỏi trước, tải, kiểm SHA-256), Cài đặt →
  Phiên bản (chạm) kiểm ngay; công cụ `gbupdate` (setuid root, chữ ký số RSA-3072 — khoá riêng của GramBook ở `keys/`), postinst,
  `tools/sign-release.sh`; bản thành 1.0.2 (control + Info.plist). Anh chốt: cập nhật xong **luôn respring** như GramMini.
- PC: `cargo test -p gm-core` 40 / 40; `gm-cli svc … http.download` tải gói v1.0.1 thật từ GitHub (qua chuyển hướng) 1,2 giây,
  SHA-256 khớp `digest` của GitHub, có `http.progress`; link `http://` bị từ chối.
- iPad mini 1 (iOS 6.1.3, bản thử): `gbupdate` root:wheel 4755, thư mục 0755. Kiểm tay qua GitHub: «bản mới nhất 1.0.1 (đang dùng
  1.0.2)» ~0,5–3 giây. `gbupdate --verify`: chữ ký đúng → 0; sửa 1 bit → 72; mobile gọi `--verify` → 64; tiến trình không phải
  GramBook gọi cài → 69; tệp ngoài thư mục tải → 65. **OTA trọn luồng** (`otalocal`, gói thử 1.0.2-2 ký bằng khoá mới, đang mở sách):
  `gbupdate → 0` → đóng app → `dpkg -i` → 0 → respring → app tự mở lại sau 14 giây, mở lại đúng cuốn, đúng trang (1/14 của «PHẦN
  II»); không `uicache` (Info.plist chỉ đổi số bản); thư mục tải + bản chép trong `/var/root` đã dọn. Cài lại cùng bản → hộp «Không
  cài được (bản này không mới hơn…, mã 73)»; chữ ký sai → «(chữ ký số không đúng, mã 72)».
- Lúc thử, app Cài đặt / nút Home / respring lạ trên iPad mini là của phiên Glass 6 (tweak Glass6SB / Glass6UI, CBFloat ghi «sẵn sàng
  (SpringBoard)» mỗi lần respring) — hỏi qua tin nhắn giữa các phiên, phiên đó dừng nhường máy rồi mới thử tiếp.

## 19/11/2026 — phát hành 1.0.2 (anh duyệt)

- Gói `vn.grambook.app_1.0.2_iphoneos-arm.deb` (`tools/build-release.sh`): 3.610.310 byte, sha256 `47a3f420…`, không còn mã thử,
  có `gbupdate` + postinst; `symcheck` 368 ký hiệu (gồm `gbupdate`), thiếu 0 trên iOS 6.1.3 (iPad mini 1) và iOS 5.1.1 (iPad 1). Ký:
  `tools/sign-release.sh` → `.sig` 384 byte.
- GitHub: bản phát hành **v1.0.2** (Latest) kèm `.deb` + `.sig`; API `releases/latest` có `digest` SHA-256 cho cả hai; tải lại từ
  GitHub = Cydia (cùng SHA-256), `openssl dgst -verify` chữ ký đúng. Trang giới thiệu «Bản 1.0.2» + mục «Tự cập nhật», trang mô tả
  Cydia có mục 1.0.2, ảnh biểu tượng của trang / nguồn Cydia đổi theo biểu tượng mới; nguồn Cydia có 1.0.0 / 1.0.1 / 1.0.2.
- **OTA từ GitHub thật — đạt** (iPad mini 1, bản thử 1.0.1-1 có `gbupdate` → bản phát hành 1.0.2): kiểm tự động sau 10 giây → hộp
  «Có bản mới 1.0.2» (ghi chú phát hành) → «Cập nhật» → «Đang tải bản 1.0.2… 73 % (2,5 MB / 3,4 MB)» → tải 2,2 giây + chữ ký →
  `gbupdate → 0` → `dpkg -i` → 0 → respring → app 1.0.2 tự mở lại sau 14 giây, mở lại cuốn đang đọc; kiểm lại: «đang dùng 1.0.2».
- iPad 1 (iOS 5.1.1, máy đang khoá — không chạm «Cập nhật» được): cài bản phát hành 1.0.2 bằng dpkg, `gbupdate` root 4755,
  `gbupdate --verify` gói phát hành → chữ ký đúng (Security.framework của iOS 5 kiểm được), chữ ký sửa → 72; respring để hiện biểu
  tượng mới.

### Tiếp theo

- [ ] Sao lưu `keys/ota-sign.key` ra ngoài máy (mất khoá → máy chạy 1.0.2 phải cập nhật qua Cydia một lần).
- [ ] Thử luồng «Cập nhật» trên iPad 1 (iOS 5) ở bản sau: sau respring iOS 5 về màn khoá, mở khoá là thấy bản mới.

- [ ] Anh thử bằng tay: vuốt dọc ở mép trái (độ sáng). (Lật cong theo ngón tay với sách Kindle: đạt — 18/11.)
- [ ] Thêm kênh sách thật (Nguồn → Thêm) để thử thư viện lớn: quét, tìm, bìa, tải về máy.
- [ ] iPhone 5 (iOS 6.1.4): không kết nối được lúc phát hành — thử giao diện điện thoại sau.
