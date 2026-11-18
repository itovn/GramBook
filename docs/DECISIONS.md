# GramBook — Các quyết định kỹ thuật

Thiết kế đầy đủ: [THIET-KE.md](THIET-KE.md). Các quyết định của GramMini (D1–D27) và GramMusic (M1–M23) áp dụng cho phần chép
sang (mạng, phiên, quét, tải, bộ dựng, giao diện) — ghi ở đây những gì riêng của GramBook.

## B1. Fork GramMusic; giữ `gm-` / `GM` cho phần chép, `GB` cho phần mới (anh duyệt) — 17/11/2026

- Chép từ GramMusic: `core/vendor` + `core/patches` (grammers vá), `gm-net` (thẳng / MTProxy / SOCKS5 / DNS mã hoá), `session.rs`,
  khung `service.rs` / `gm-ffi` (`gm_init` / `gm_call`), `sync.rs` (quét từng chat — M7), `stream.rs` (khúc 256 KB, tệp thưa + bảng
  khúc, cổng 127.0.0.1), `cache.rs`, `text.rs`, `playlists.rs` (→ bộ sưu tập), bộ dựng, `tools-local`; app: GMCompat, GMShims,
  GMLog, GMCore, GMImages, GMTheme, GMArt, GMRootController, màn Đăng nhập / Proxy / Nguồn / Cài đặt, GMDebug.
- Tên tệp / lớp / crate chép sang giữ nguyên → sửa lỗi bên nào thì so khác biệt và chép sang bên kia dễ. Lớp mới của phần đọc sách:
  tiền tố `GB` (GBReaderViewController, GBPageView…).
- Bundle id `vn.grambook.app`, dữ liệu `~/Library/GramBook`, đăng nhập riêng (một phiên chỉ dùng ở một app — GramMini D24).

## B2. Một `.deb` cho iOS 5 và iOS 6 (anh duyệt) — 17/11/2026

Như GramMusic: `TARGET := iphone:clang:9.3:5.0`, armv7, `symcheck` trên iOS 6.1.x (iPad mini 1, iPhone 5) và iOS 5.1.1 (iPad 1).
Máy thử chính: iPad mini 1. API mới hơn iOS 5.0 dùng thẳng là lỗi lúc dựng (`-Werror=unguarded-availability`).

## B3. Định dạng ưu tiên: EPUB, MOBI, AZW3 (anh duyệt) — 17/11/2026

GĐ1: EPUB 2 / 3, MOBI (MOBI6, gồm `prc` / `azw` không DRM), AZW3 (KF8). Sau: TXT, PDF, FB2, CBZ. Lõi chuyển MOBI / AZW3 thành
«sách dạng EPUB» (chương HTML + mục lục) rồi đọc chung một màn với EPUB.

## B4. Chat riêng chỉ lấy định dạng chắc chắn là sách (anh duyệt) — 17/11/2026

Chat với người: EPUB / FB2 / MOBI (cả AZW3 / AZW / PRC — cùng họ Kindle). PDF / TXT / DOC chỉ lấy từ kênh, nhóm và Tin nhắn đã lưu
(trong chat riêng hay là hoá đơn, hợp đồng, giấy tờ). Bật thêm được cho từng nguồn.

## B5. Lật trang cong kiểu iBooks ngay từ GĐ1 (anh duyệt) — 17/11/2026

- `UIPageViewController` kiểu cong (`UIPageViewControllerTransitionStylePageCurl`, có từ iOS 5): lật theo ngón tay, mặt sau trang
  in mờ ngược như iBooks; iPad ngang: gáy giữa, hai trang.
- Cách làm (dự kiến, thử ở GĐ1): các trang của `UIPageViewController` là **ảnh chụp trang**; UIWebView «sống» của chương nằm phủ
  lên khi trang đứng yên (chọn chữ, chạm link, tô màu). Bắt đầu lật → ẩn WebView (ảnh bên dưới giống hệt), cong trên ảnh; lật xong
  → dời WebView tới trang mới rồi hiện lại. Ảnh trang kế / trước được chụp sẵn khi trang vừa đứng yên.
- Rủi ro: chụp ảnh trang chưa vẽ xong (thấy ở bước 0: WebKit vẽ chậm vùng ở xa trong chương 1,3 MB) → chờ vẽ xong mới chụp; chưa
  có ảnh thì lật trượt thay cho lần đó. Tuỳ chọn khác vẫn giữ: Trượt, Không hiệu ứng, Cuộn dọc.

## B6. Dùng chung `api_id` Telegram với GramMusic (anh duyệt) — 17/11/2026

`keys/telegram.txt` chép từ GramMusic (không lên git). `keys/nextdns.txt` (DNS mã hoá) cũng chép sang.

## B7. Bước 0 — chia trang bằng cột CSS trên WebKit của iOS 6: đạt — 17/11/2026

Thử bằng Safari trên iPad mini 1 (iOS 6.1.3, AppleWebKit 536.26 — cùng WebKit với UIWebView) + máy chủ nhỏ làm đúng việc lõi
sẽ làm (`spike/epub-columns/`: lấy chương thẳng trong zip, chèn CSS + JS vào `<head>`); 8 sách mẫu, 220 chương, iPad nằm ngang
(1024 × 672, 2 trang / màn, Georgia 20 px, giãn dòng 1,5, lề 40 px).

- **Chia cột đúng**: số cột = `(scrollWidth − lề + khe) / bề rộng trang` khớp tuyệt đối với cách dò chữ cuối của chương (220 / 220).
  Hai cột / màn thẳng hàng, căn đều, dấu tiếng Việt đúng; nền Giấy ngà và Đêm (Literata) đúng.
- **Tốc độ** (Safari, có JIT — UIWebView không có JIT nhưng phần dàn trang là mã gốc):

| Sách | Chương | Màn | Dung lượng | Đếm cả sách | Chương lâu nhất |
| --- | --- | --- | --- | --- | --- |
| Bê Trọc | 8 | 703 | 3,4 MB | 7,6 s | 2,8 s (chương 1,3 MB) |
| Hồi kí Nguyễn Hiến Lê | 34 | 648 | 4,2 MB | 9,5 s | 1,0 s |
| Ai Cập Huyền Bí | 15 | 174 | 0,4 MB | 0,9 s | 0,1 s |
| Tổ quốc ăn năn | 109 | 678 | 1,5 MB | 3,9 s | 0,1 s |
| Nước Mỹ… ngủ đường (25 ảnh) | 16 | 231 | 0,5 MB | 2,0 s | 0,3 s |

  Chương > 50 KB: ~2,1 ms / KB (dàn trang ~0,75 ms / KB). Đổi cỡ chữ tại chỗ: chương 1,3 MB mất 1,4 s.
- **Ảnh**: 1 ảnh cao gần bằng trang bị tràn đáy 18 px (dòng chứa ảnh cao hơn trang vì khoảng chân chữ) → `vertical-align: bottom`
  cho ảnh đứng riêng + ảnh cao tối đa 96 % trang: ảnh sang đầu cột mới, đủ.
- **Phải làm trong lõi / app** (từ bước 0):
  - Lõi **cắt chương > ~150 KB** thành nhiều phần ở ranh giới đoạn (đóng / mở lại thẻ đang mở) → mở chương ≤ 0,4 s, đổi cỡ chữ
    ≤ 0,15 s. 4 / 303 sách mẫu có chương > 200 KB, 17 có chương > 100 KB.
  - CSS **chèn thẳng vào `<head>`** (`<style>`), không `<link>`: lần đầu tải CSS ngoài, WebKit dàn trang một lượt chưa có cột rồi
    dàn lại (chương 1,3 MB: 2 lượt × 1,3 s). Đếm trang **sau `load`**.
  - Font đóng kèm phải **đăng ký với hệ thống** (`UIAppFonts` / `CTFontManagerRegisterGraphicsFont`), không dùng `@font-face`: font web
    về muộn → số trang đổi sau khi đã đếm (Literata: 722 cột lúc chưa có font, 589 cột khi có).
  - Trả `application/xhtml+xml` khi chương đúng cú pháp XML: sách vnthuquan ngắt đoạn bằng `<br></br>` — đọc như `text/html` thì mỗi
    `</br>` thành thêm một lần xuống dòng.
  - Ghi đè kiểu ghi sẵn trong thẻ (Word HTML: `style="LINE-HEIGHT: 150%"`, `<font face>`, `FONT-SIZE: 13pt`, màu chữ) bằng
    `font-size / line-height / font-family: inherit !important` cho thẻ thường (h1–h6, sup, sub, small giữ tỉ lệ); nền Đêm ghi đè cả
    màu chữ / màu nền.
  - Chuẩn hoá chữ về NFC khi phục vụ (7 / 303 sách mẫu dùng dấu tổ hợp).
- Chưa thử ở bước 0 (để GĐ1, trong app): iPad dọc 1 trang / màn, chụp ảnh trang cho lật cong, chọn chữ / tô màu, vị trí CFI rút gọn,
  RAM của UIWebView, iPad 1 (iOS 5.1.1).

## B8. Font tiếng Việt trên iOS 6 — 17/11/2026

Dò tự động từng chữ cái tiếng Việt (134 chữ: Ă Â Đ Ê Ô Ơ Ư + 5 dấu thanh, hoa / thường) trong từng font trên iPad mini 1
(`/fontcheck`: bề rộng chữ với hai font dự phòng khác hẳn nhau — bằng nhau thì font có chữ đó):

- **Đủ 134 / 134**: Georgia, Times New Roman, Baskerville, Cochin, Helvetica Neue, Helvetica, Arial, Verdana, Avenir Next, Courier
  New; Literata, Noto Serif (đóng kèm).
- **Thiếu**: Palatino (thiếu đúng «ớ» U+1EDB — «Với», «thế giới» bị vẽ bằng font khác), Avenir (24: ơ ư và các dấu trên ơ / ư),
  American Typewriter (44), Trebuchet MS (92), Didot / Bodoni 72 / Futura (94), Hoefler Text (98), Optima / Gill Sans (102).
- Màn đọc cho chọn: **Georgia** (mặc định), Literata, Times New Roman, Baskerville, Cochin, Helvetica Neue, Avenir Next, Verdana
  + «Theo sách». Đóng kèm **Literata** (4 kiểu thường / nghiêng / đậm / đậm nghiêng ≈ 1,27 MB, OFL). iOS 5 (iPad 1) cần dò lại
  (Avenir Next có từ iOS 6).

## B9. Kho sách mẫu (303 EPUB vnthuquan) — 17/11/2026

- Cả 303: EPUB 2.0, có NCX, không nav, không DRM, không font nhúng, XHTML đúng cú pháp XML, 174 ảnh JPEG (49 sách có ảnh). Tạo bằng
  bộ chuyển của vnthuquan.net: chương `.html` khai `application/xhtml+xml`, cả chương là một `<div>` lớn, đoạn ngắt bằng `<br></br>`,
  class CSS (`style28`, `viethead`…) không được định nghĩa ở đâu; chương lấy từ Word có HTML Word (`mso-…`, `<o:p>`).
- `dc:language` = `en-US` ở cả 303 cuốn dù là sách tiếng Việt → **không tin ngôn ngữ trong OPF** (đoán từ chữ trong sách khi cần —
  ngắt từ, tra từ).
- Chú thích kiểu Word: link tuyệt đối `http://thuvien.vnthuquan.net/…#_ftn1` + neo `name="_ftn1"` ở cuối cùng chương → `reader.js`
  coi link có phần `#…` trỏ tới neo có trong chương là chú thích trong sách (bóng chú thích), không mở trình duyệt.
- Trang «Mục lục» (`toc.html`) và «Chào mừng» / «Tạm biệt» nằm trong spine → giữ (là một phần sách), mục lục của app lấy từ NCX.

## B10. Sách Kindle: crate `gb-mobi` chuyển cả sách lúc mở, lõi phục vụ như EPUB — 17/11/2026

- `core/crates/gb-mobi` (chỉ dùng std, dựng được cho armv7-apple-ios6): PalmDB → đầu PalmDOC / MOBI / EXTH → giải nén (PalmDOC LZ77,
  HUFF/CDIC) → **MOBI6**: HTML Mobipocket cắt ở `mbp:pagebreak` (lưới an toàn ~300 KB), neo `filepos` → `id="fp…"`, ảnh `recindex` →
  `images/NNNNN.jpg`; **KF8 / AZW3**: khung + mảnh (SKEL / FRAG / FDST), `kindle:pos` / `kindle:embed` / `kindle:flow` → đường dẫn
  thường, font KF8 gỡ rối + giải nén zlib; tệp kép MOBI6 + KF8 ưu tiên phần KF8. Mục lục: chỉ mục NCX → trang `toc` của guide →
  trang «MỤC LỤC» → đề mục h1–h3 → mỗi phần một mục.
- Kết quả `MobiBook` (tệp chương theo thứ tự đọc + ảnh / CSS / font + mục lục + thông tin) → `Store::Mem` của `serve.rs`; chương đi
  cùng đường với EPUB (`chapter.rs`: NFC, thực thể, XHTML / HTML, cắt phần > 150 KB). Bìa ghi ra `covers/<khoá>.book.jpg` lúc mở.
- Kho mẫu SMB (181 tệp Kindle): 117 MOBI6 + 50 KF8 mở được, 14 có DRM (báo «Sách có DRM»), 0 lỗi — cả qua lõi thật trên PC (`gm-cli
  book`: mở + lấy mọi phần qua cổng). 720 bản hỏng / cắt cụt + 450.000 lần đột biến ngẫu nhiên: không panic. PC: sách thường < 100 ms;
  cuốn lớn nhất (`Co Chan Nhan C1 3600.prc`, 23,5 MB, 59 MB chữ, 3.628 chương) 0,57 s, đỉnh RAM ~120 MB — trên A5 chậm hơn vài lần;
  iPad 1 (256 MB) có thể không mở nổi những tệp cỡ này (GĐ2: lưu bản chuyển ra đĩa thay vì giữ trong RAM).
- Sách cp1252 tiếng Việt (45 cuốn): chữ có trong Latin-1 là byte cp1252, chữ khác là thực thể số → đổi cp1252 sang UTF-8, giữ thực
  thể. Một cuốn mã VNI thật («The gioi co Vua.prc») chưa chuyển (để GĐ2 cùng TCVN3 / VNI cho TXT).
- `dc:language` / EXTH 524 thường sai («en») → gb-mobi đoán `vi` khi chữ rõ là tiếng Việt.

## B11. Giao diện tối (anh yêu cầu «đỡ mỏi mắt») — 18/11/2026

- Dùng lại chế độ Sáng / Tối / Tự động (tối từ 19:00 tới 6:00) của GramMusic (`GMTheme`, Cài đặt → Giao diện). Lỗi port: kiểu kính /
  hình nền / màu tối chỉ áp cho bảng có nguồn dữ liệu tên `GM…` → mọi màn mới `GB…` (Đang đọc, Thư viện, Bộ sưu tập, sách trên máy,
  Aa, Mục lục, chọn chat…) vẫn trắng. Nay áp cho cả `GB…`; tiêu đề nhóm của bảng thường vẽ lại (`GMTheme plainHeader:` — dải xám
  mặc định của iOS 6 chói khi tối); chữ cái tra nhanh A–Z màu chủ đạo (iOS 6).
- **Nền trang nhớ riêng cho giao diện sáng và tối** (`gb.theme` mặc định Trắng, `gb.theme.dark` mặc định Đêm): bật tối là trang tối
  theo, tắt là về nền cũ (Giấy ngà…); chọn nền trong Aa lúc đang tối thì lưu cho giao diện tối.
- Bảng Aa có công tắc **«Giao diện tối»** (đặt chế độ Sáng / Tối cố định; Tự động đổi lại ở Cài đặt). Đổi giao diện khi đang đọc
  (công tắc, tới giờ của chế độ Tự động — màn đọc kiểm mỗi 15 giây, mở lại app): đổi nền trang tại chỗ, giữ chỗ đang đọc, bảng Aa
  tô lại ngay; khung app phía sau dựng lại khi đóng sách (dựng lại ngay thì phải đóng màn đọc).
- Màn chờ lúc mở app (Default*.png) vẫn sáng — iOS 5 / 6 chỉ cho một ảnh tĩnh.
- Thêm cho đỡ mỏi mắt (anh nhắc lại yêu cầu): **ánh sáng ấm** — lớp hổ phách (~2700 K, tối đa 30 %) phủ màn đọc, thanh «Ấm» dưới độ sáng trong Aa (iOS 5 / 6 không có Night Shift); **nút trăng / mặt trời** trên thanh màn đọc (iPad) bật / tắt giao diện tối một chạm. Kéo độ sáng / ánh sáng ấm chỉ tô lại lớp phủ (trước đây kéo độ sáng làm dàn trang lại liên tục).

## B12. Biểu tượng app theo mẫu anh đưa — 18/11/2026

- Mẫu: `Tham_Khao/logo/logo1.png` (ảnh chụp màn hình iPhone 1320 × 2868 — quyển sách mở phát sáng, máy bay giấy bay ra từ trang
  sách, nền xanh lam → chàm; chữ «01 The Flight of Words» bên dưới không thuộc biểu tượng).
- Ô biểu tượng trong mẫu cao hơn rộng (950 × 1021) → cắt ô vuông 944 px (lùi 3 px vào trong mép trái / phải, bớt ~36 px lề trên /
  dưới — sách và máy bay vẫn cách mép ~110 px), không kéo méo hình. Góc bo ≈ 20 % để trong suốt (lớn hơn mặt nạ ≈ 17,5 % của iOS 5
  / 6 → iOS không cắt vào hình); `UIPrerenderedIcon` giữ nguyên (không phủ vệt bóng — mẫu đã có ánh sáng riêng). Lệnh:
  `python tools/icon/make_icon.py from-ref Tham_Khao/logo/logo1.png 123 873 944` rồi `python tools/icon/make_icon.py app/Resources`
  (cần Pillow — chạy trên máy dựng). Thay biểu tượng tạm vẽ bằng mã.
- Đổi biểu tượng của app đã cài: SpringBoard iOS 5 / 6 giữ ảnh cũ tới khi respring (`uicache` chưa đủ).
- Sửa theo ý anh (18/11): **bo góc ít hơn** — 16,5 % (≈ mặt nạ iOS 5 / 6; iOS luôn tự bo nên không vuông hơn được), **nền kính mờ kiểu macOS Tahoe** — `tools/icon/glass_icon.py` (chỉ Pillow): tách sách + máy bay khỏi nền mẫu (mô hình nền theo hàng từ hai dải sát mép, alpha theo độ khác), vẽ tấm kính mới cùng bảng màu (sáng khuếch tán, hạt mờ, vệt bóng trên, viền kính sáng ở góc trên trái / dưới phải, mép dưới tối nhẹ), ghép `ảnh + (1 − alpha)(kính − nền)`, bóng đổ mềm dưới sách. Pillow 12 trên máy dựng lỗi khi resize NEAREST → dùng `transform(EXTENT)`.

## B13. Rà soát trước khi phát hành 1.0.0 — 18/11/2026

- **PC**: phép thử lõi đạt hết (gm-core 40, gb-mobi 27 + 4 kho mẫu + 9 tổng hợp, gm-net 27); mở mọi sách mẫu qua lõi thật (`gm-cli
  book`: mở + lấy mọi phần của mọi chương qua cổng): 617 / 617 EPUB + Kindle không DRM, 14 DRM báo đúng. Cuốn duy nhất trên 300 ms là
  PRC 23,5 MB / 3.628 chương (455 ms trên PC).
- **iPad mini 1**: mở sách, lật trước / sau, nhảy chương, mục lục, dấu trang, đóng → mở lại đúng chỗ; tìm không dấu, tải về máy,
  Yêu thích, Muốn đọc, bộ sưu tập; giao diện sáng / tối; gửi sách (tải lên + chuyển tiếp); mở sách từ Telegram.
- **Đổi cỡ chữ trôi chỗ đang đọc**: mỗi lần dàn lại lấy đầu trang hiện tại làm mốc → A+ rồi A− lùi một trang. Nay app giữ mốc của lần
  lật / nhảy gần nhất (`settled`), `GB.setConfig(cfg, mốc)` về đúng mốc đó → A+ ×4 rồi về cỡ cũ, A− rồi về: đúng trang cũ.
- **iOS tắt app lúc nằm nền khi đã mở sách** (báo cáo `0xdead10cc` «suspended with locked system files: …/com.apple.opengl/
  shaders.maps»): WebKit giữ khoá tệp đệm shader OpenGL. Thử loại trừ trên tiến trình sạch: không phải hiệu ứng lật (cả «Không»),
  không phải ảnh chụp trang, không phải UIPageViewController, WebView mờ / đục, `suppressesIncrementalRendering`, giao diện tối, loại
  sách; gỡ WebView khỏi cửa sổ hay xin thêm 3 giây chạy nền cũng không gỡ được khoá. Chỉ Thư viện (chưa mở sách lần nào) thì không bị.
  → Làm như iBooks: nhớ sách đang mở (`gb.openBook`), mở app lại thì **tự mở lại đúng cuốn, đúng chỗ**; lần mở lại trước chưa tới được
  trang đầu (app chết khi mở sách đó) thì thôi, khỏi kẹt vòng. Thử: ra nền → iOS tắt app → vào lại → đúng cuốn, đúng dòng đầu trang.
- Biểu tượng mục «Bộ sưu tập» (hai ruy-băng chồng): đường viền hai ruy-băng cắt nhau trông như vỡ → xoá phần ruy-băng sau bị che.


## B14. Mặt sau tờ giấy khi lật cong: giấy ngà do app vẽ (anh yêu cầu «giống giấy sách») — 18/11/2026

- Lật một mặt (`doubleSided = NO`) thì iOS tự vẽ mặt sau: chữ trang in ngược phủ lớp trắng — trắng bệt, giao diện tối càng chói,
  không có API đổi màu. → **Lật hai mặt** (`doubleSided = YES`), mặt sau là `GBPageVC` riêng (`back`): nền `GBReaderPrefs backFor:`
  (Trắng → ngà nhạt `#F1EADA`, Giấy ngà → `#E8DCC0`, Xám → `#504F4C`, Đêm → `#1F1E1B` — sẫm hơn mặt trước một chút, như nằm
  trong bóng tờ đang lật) + ảnh mặt trước lật ngang (`UIImageOrientationUpMirrored`), mờ 12 % — chữ hằn qua giấy. Không vẽ ảnh mới:
  chỉ thêm lớp.
- Luật số trang của UIPageViewController (gáy trái, hai mặt), đã thử iOS 6.1.3: đặt **không hiệu ứng → 1** trang (đưa 2 là ném
  `NSInvalidArgumentException`); **có hiệu ứng → 2**: trang sẽ hiện + mặt sau tờ cong qua (tới: tờ đang đọc; lùi: tờ trước). Vuốt
  thì dataSource bị hỏi hai lần: trang n → mặt sau tờ n → trang n + 1 (lùi: trang n → mặt sau tờ n − 1 → trang n − 1).
- Lỗi ném giữa `setViewControllers` (bản dò) làm UIPageViewController kẹt: vuốt không cong theo ngón tay nữa — đừng bắt lỗi kiểu đó.
- `dealloc` màn đọc gỡ `dataSource` / `delegate` của UIPageViewController (iOS 5 / 6 là `assign`): có một lần app chết (EXC_BAD_ACCESS
  từ UIKit sau hoạt cảnh) khi mở sách khác lúc đang đọc; sau khi gỡ, thử lại (cả lúc trang đang lật) không bị.
- Thử: iPad mini 1 (iOS 6.1.3) — vuốt tới / lùi, kéo dở rồi thả, chạm lật, nền Giấy ngà + Đêm; iPad 1 (iOS 5.1.1) — chạm lật
  tới / lùi, không báo cáo lỗi. Anh vuốt thử bằng tay trên iPad mini 1: đạt. Phát hành trong bản 1.0.1.
