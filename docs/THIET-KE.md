# GramBook — Phương án thiết kế (bản nháp 17/11/2026)

App **đọc sách / ebook lưu trên Telegram** (không chính thức) cho **iPad / iPhone / iPod touch iOS 5 – 6 đã jailbreak**. Máy thử
chính: **iPad mini 1** (iOS 6.1.x). Cùng nền với GramMusic / GramMini: lõi **Rust** nói chuyện thẳng với Telegram bằng MTProto, vỏ
**Objective-C / UIKit** dựng bằng Theos. Một gói `.deb`, cài qua Cydia.

| Tham khảo | Lấy gì |
| --- | --- |
| `E:\DEV_CODE\GramMusic` (1.0.2) | **Toàn bộ nền**: lõi Rust (`gm-net` thẳng / MTProxy `dd` `ee` / SOCKS5, DNS mã hoá, grammers đã vá, phiên, `service.rs` / `gm-ffi` `gm_init` / `gm_call`), **quét từng chat** (`sync.rs`, M7 — lượt tài liệu dùng lại gần nguyên), **cổng 127.0.0.1 + tệp thưa + bảng khúc** (`stream.rs`), đệm (`cache.rs`), bỏ dấu / tìm (`text.rs`), playlist → bộ sưu tập (`playlists.rs`); app: GMRootController, GMTheme (kính iOS 6), GMCompat, GMImages, GMCore, GMLog, màn Đăng nhập / Proxy / Nguồn / Cài đặt / Dữ liệu và bộ nhớ, GMDebug; bộ dựng Theos + Rust armv7, `tools-local` (`deploy.sh`, `ios.sh` — đã có máy `ipad` = iPad mini 1), `symcheck`, bản phát hành, nguồn Cydia |
| iBooks 3.1.3 cho iOS 5 (`Tham_Khao/`, chỉ có `.ipa` đã giải mã) | **Cách làm của lõi đọc sách** — đọc tài nguyên và tên lớp trong tệp chạy, **không chép mã, font, ảnh của Apple** (như GramMusic với MeloVault). Chi tiết ở mục 1 |

---

## 0. Tóm tắt các quyết định (anh đã duyệt 17/11/2026 — DECISIONS B1–B6; bước 0: B7–B9)

1. **Fork GramMusic** (như GramMusic fork GramMini — M1). Phần chép sang giữ nguyên tên (`gm-net`, `gm-core`, `gm_init` /
   `gm_call`, `GMTheme`, `GMCompat`, `GMCore`…) để sửa lỗi bên nào chép sang bên kia dễ; lớp mới của phần đọc sách đặt tiền tố `GB`.
   Bundle id `vn.grambook.app`, dữ liệu `~/Library/GramBook`, đăng nhập riêng (một phiên chỉ dùng ở một app — GramMini D24).
2. **Bỏ khỏi bản fork**: GMPlayer, giải mã FLAC / OGG / Opus (symphonia, `gm-opus`), lời bài hát, nhạc trên máy, tweak
   GramLockControl, khung AVFoundation / MediaPlayer.
3. **Thư viện = quét cả tài khoản**, chỉ lượt tài liệu (`InputMessagesFilterDocument`), giữ tệp có đuôi sách. Nguồn ẩn / hiện được;
   thêm kênh sách công khai bằng @tên / link t.me.
4. **EPUB hiện bằng UIWebView + chia cột CSS** (`-webkit-column-width`) — đúng cách iBooks làm trên iOS 5, chạy cả iOS 5 và 6.
   UIWebView của iOS 5 / 6 không có chế độ chia trang sẵn (`paginationMode` có từ iOS 7).
5. **Lõi phục vụ nội dung sách qua cổng 127.0.0.1** (như giao thức riêng `x-ibooks-th:` của iBooks): đọc thẳng trong tệp zip,
   không giải nén ra đĩa; **chèn sẵn CSS + JS của màn đọc vào `<head>` từng chương** → không nháy chữ chưa định dạng.
6. **Định dạng**: GĐ1 **EPUB, MOBI, AZW3** (B3). Lõi chuyển MOBI / AZW3 (rồi TXT, FB2) thành «sách dạng EPUB» (danh sách chương
   + mục lục + HTML). **PDF** (GĐ2) do app vẽ bằng Quartz (CGPDF), không qua WebView.
7. **Giao diện**: phần thư viện dùng kính iOS 6 của GramMini / GramMusic (D23); màn đọc là «giấy»: toàn màn hình, nền Trắng /
   Giấy ngà / Đêm, **iPad trước** (ngang = 2 trang), **lật trang cong kiểu iBooks ngay từ GĐ1** (B5).
8. **Không làm** (ít nhất bản đầu): sách có DRM (Adobe ADEPT, Kindle), DjVu, đọc to (iOS 5 / 6 không có TTS công khai), cửa hàng sách,
   iCloud.

---

## 1. Lõi đọc sách của iBooks 3.1.3 — lấy gì

| Phần của iBooks (tên lớp / tệp trong `.ipa`) | Cách làm | GramBook |
| --- | --- | --- |
| `BKEpubBookParser`, `META-INF/container.xml`, `rootfile[@media-type='application/oebps-package+xml']`, `itemref`, `navPoint`, `BKEpubTOCParseOperation` | container → OPF → manifest / spine / NCX | Lõi Rust `book.rs` (mục 4.2) |
| `BKBookProtocol`, `x-ibooks-th`, `ibooks://bookid/…` | Giao thức URL riêng phục vụ tệp trong sách cho WebView | Cổng `127.0.0.1` của lõi, đường `/b/…` |
| `user_stylesheet_flowable.css.tmpl` | `html { -webkit-column-width: <rộng trang>; -webkit-column-gap: <khe>; width; height: <cao trang> }`; đếm trang theo `scrollWidth` | `reader.css` + `reader.js` (mục 5.1) |
| Cùng tệp: chủ đề | Đêm: `:root[theme] * { background: transparent; color: <chữ> }`, link màu riêng, SVG text đổi màu; Ngày: nền / chữ cho `:root` và `body` | Nền Trắng / Giấy ngà / Xám / Đêm |
| Cùng tệp: ảnh, chú thích | `img, svg, video { max-width / max-height: 100% }`, ảnh đứng một mình bám đáy dòng; `aside[epub\|type~="footnote"] { display: none }`; `@page { margin: 0 }`; sửa sách Calibre (`.calibre > div { position: static }`) | Giữ các luật này (viết lại) |
| `FontPresets~ipad.plist` | 12 nấc cỡ chữ, **mỗi font có bảng hệ số cỡ + giãn dòng riêng** (vd. Athelas 0,93 → 6,2 ×, giãn 1,42 → 1,16) | Cùng cách, với font của mình |
| `BKPaginationController`, `BKEpubPaginationLayoutOperation`, `BKChapterPageCount` | Đếm trang cả sách chạy nền, lưu theo bố cục | WebView ẩn đếm trang (mục 5.1) |
| `BKEpubCFILocation`, `BKAnchorPathLocation` | Vị trí đọc kiểu EPUB CFI — đổi cỡ chữ không mất chỗ | «CFI rút gọn» (mục 5.2) |
| `BKPDFRenderingManager`, `AEPdfRenderingCache`, `BKPdfTocParser` | Vẽ PDF bằng CGPDF + đệm ảnh trang, mục lục PDF | Màn PDF (mục 6) |
| `BKEpubSearchOperation`, `AEAnnotationTheme{Yellow,Green,Blue,Pink,Purple,Underline}`, `BKScrubberControl`, `BKBrightnessController`, `BKNavigationHistory`, `BKPageCurl`, `BKDictionaryManager` | Tìm trong sách, tô 5 màu + gạch chân, thanh kéo trang, độ sáng, «quay lại» sau khi bấm link, lật cong, tra từ | Mục 5.3 – 5.5 |

Không lấy: font Athelas / Iowan Old Style / Seravek / Charter bản Apple, ảnh kệ gỗ, bìa da, JS / CSS nguyên văn của Apple, phần cửa
hàng, iCloud, sách tương tác (`.ibooks`, widget, EQKit).

---

## 2. Máy đích

**iPad mini 1** (`iPad2,7`, A5 2 nhân 1 GHz, 512 MB, 1024 × 768 @1x, iOS 6.1.x) — máy thử chính, đã có GramMini và GramMusic. Vẫn
một `.deb` cho iOS 5.0 – 6.1.x như GramMusic; iPad 1 (256 MB) là máy yếu nhất — PDF lớn có thể phải giảm đệm.

Ngân sách trên iPad mini 1:

- Mở app tới Thư viện: ≤ 2 giây với 5.000 sách (bảng lấy từng trang 100 cuốn từ lõi, như GramMusic).
- Mở sách đã có trên máy tới đúng trang đang đọc: ≤ 1,5 giây. EPUB 2 MB chưa có: tải + mở ≤ 5 giây trên Wi-Fi.
- Lật trang trong chương ≤ 100 ms (chỉ dời khung, WebKit đã dàn sẵn); sang chương ≤ 700 ms.
- Đếm trang cả sách ở nền: sách 300 trang ≤ 30 giây, không làm giật lúc lật.
- RAM ≤ 90 MB khi đọc EPUB, ≤ 120 MB khi đọc PDF (đệm ảnh trang có trần).

---

## 3. Kiến trúc

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ App ObjC / UIKit (Theos, armv7, iOS 5.0)                                     │
│  GMRootController: Đang đọc │ Thư viện │ Nguồn │ Bộ sưu tập │ Cài đặt + thanh «Đọc tiếp» │
│  GBReaderViewController (EPUB / TXT / FB2 / MOBI)                            │
│    GBPageView: UIWebView + reader.js · GBPaginator: UIWebView ẩn đếm trang   │
│    thanh trên / dưới · Aa · Mục lục · Tìm · Dấu trang · Tô màu / Ghi chú     │
│  GBPdfViewController: CGPDF + CATiledLayer, đệm ảnh trang                    │
└──────────┬──────────────────────────────▲─────────────────▲─────────────────┘
   gm_call (JSON)              sự kiện JSON │   UIWebView: GET http://127.0.0.1:<cổng>/b/…
┌──────────▼──────────────────────────────┴─────────────────┴─────────────────┐
│ libgm_ffi.a (Rust, armv7, build-std, panic=abort)                            │
│  gm-core: service · library (sách, tìm, xếp) · sync (quét tài liệu)          │
│           stream (tải tệp: khúc 256 KB, tệp thưa + bảng khúc; cổng 127.0.0.1) │
│           book (EPUB / zip, OPF, NCX / nav, chèn CSS + JS, gỡ font bị làm rối) │
│           convert (TXT / FB2 / MOBI / AZW3 → chương HTML) · search (không dấu) │
│           notes (vị trí đọc, dấu trang, tô màu, ghi chú) · collections · cache │
│  gm-net: thẳng / MTProxy dd ee / SOCKS5, DNS mã hoá · grammers 0.12 (vá)     │
└──────────┬───────────────────────────────────────────────────────────────────┘
           │ MTProto 2.0 / TCP
           ▼
    Máy chủ Telegram: messages.search (lọc tài liệu), upload.getFile
```

JS ↔ ObjC trên iOS 5 / 6 (chưa có WKWebView): ObjC gọi JS bằng `stringByEvaluatingJavaScriptFromString:`; JS báo lên bằng điều hướng
tới `gb://sự-kiện?…`, chặn ở `webView:shouldStartLoadWithRequest:`.

---

## 4. Thư viện và mở sách (lõi)

### 4.1 Sách là gì

Tin có tài liệu, theo đuôi / MIME:

| Nhóm | Đuôi | Giai đoạn |
| --- | --- | --- |
| EPUB 2 / 3 | `epub` | GĐ1 |
| Kindle (không DRM) | `mobi`, `azw3`, `azw`, `prc` | GĐ1 |
| Truyện chữ | `txt` | GĐ2 |
| PDF | `pdf` | GĐ2 |
| FictionBook | `fb2`, `fb2.zip` | GĐ2 |
| Truyện tranh | `cbz` | GĐ3 |
| Xem nhanh, không chia trang (UIWebView mở thẳng) | `doc`, `docx`, `rtf` | GĐ3 |
| Chưa đọc được — nhãn đỏ, vẫn tải về được | `djvu`, `cbr`, sách có DRM | — |

- **Thông tin từ Telegram**: tên tệp, cỡ, ngày, ảnh nhỏ (nếu tin có), **chú thích của tin** (≤ 500 ký tự — kênh sách hay ghi tên,
  tác giả, giới thiệu). Tên / tác giả đoán từ tên tệp: «Tác giả - Tên sách.epub», bỏ `[…]`, `(z-lib.org)`, `_` → dấu cách.
- **Sau lần mở đầu**: lõi đọc OPF (`dc:title`, `dc:creator`, `dc:language`, bìa) → cập nhật chỉ mục và ảnh bìa thật.
- **Khoá sách** = `<peer>_<id tin>` như GramMusic; trùng tệp (cùng mã tài liệu, chuyển tiếp nhiều nơi) chỉ hiện một lần ở Thư viện.
- **Chat riêng** (người): chỉ lấy định dạng chắc chắn là sách (EPUB / FB2 / MOBI / AZW3 / AZW / PRC); PDF / TXT / DOC trong chat
  riêng hay là giấy tờ (hoá đơn, hợp đồng) → chỉ lấy từ kênh, nhóm, Tin nhắn đã lưu; bật thêm được từng nguồn (B4).
- **Không tin `dc:language`** trong OPF (303 / 303 sách mẫu ghi `en-US` dù là tiếng Việt — B9).

### 4.2 Quét, lưu trữ, tìm

- Quét: `sync.rs` của GramMusic (M7) — `messages.search` từng chat, chỉ lượt tài liệu, lọc đuôi; lấy sách mới; kênh thêm tay quét
  riêng; cập nhật trực tiếp (sách mới / tin bị xoá); FLOOD_WAIT. Kênh sách có thể có hàng chục nghìn tệp → chỉ mục theo nguồn, lấy
  theo trang.
- Lưu (JSON như GramMusic, chưa dùng SQLite):

| Tệp | Nội dung |
| --- | --- |
| `sources.json`, `library/<peer>.json`, `sync.json` | Như GramMusic, bản ghi là sách: tên, tác giả, tên tệp, đuôi, cỡ, ngày, mã tài liệu, ảnh nhỏ, chú thích |
| `books/<khoá>.<đuôi>` (+ `.part` / `.map` khi đang tải) | Tệp gốc |
| `meta/<khoá>.json` | Đã đọc từ sách: danh sách chương, mục lục, bìa, ngôn ngữ, chiều trang, có DRM |
| `covers/<khoá>.jpg` | Bìa |
| `reading/<khoá>.json` | Vị trí đọc, dấu trang, tô màu, ghi chú, thời gian đọc, đã đọc xong — **không bao giờ bị dọn theo đệm** |
| `layout/<khoá>-<chữ ký>.json` | Số trang từng chương theo bố cục (chữ ký = cỡ khung + hướng + font + cỡ chữ + giãn dòng + lề) |
| `collections.json` | Muốn đọc, Yêu thích, Đã đọc xong, bộ sưu tập tự tạo |

- Tìm **không dấu** trên tên + tác giả + tên tệp + chú thích (`text.rs`); xếp Mới / Tên / Tác giả / Đọc gần đây; lọc định dạng,
  Đã tải, Chưa đọc, Đang đọc, Đã xong.

### 4.3 Tải sách

- Dùng lại `stream.rs`: khúc 256 KB qua `upload.getFile`, tệp thưa + `.map`, tắt app giữa chừng thì tải tiếp. Mở sách = tải đủ tệp
  (EPUB thường 0,3 – 5 MB → vài giây), tiến độ hiện trên bìa.
- Sách đã mở nằm trong đệm (mục 8); «Tải về máy» = không bao giờ tự xoá.
- **PDF lớn (GĐ3)**: mở khi chưa tải xong — `CGDataProviderCreateDirect` đọc qua lõi, trang nào cần thì lõi tải khúc đó trước (như
  «tải theo nhịp nghe» M10).

### 4.4 EPUB (`book.rs`)

- Zip: đọc mục lục zip ở cuối tệp, giải nén từng mục khi cần (crate `zip`, deflate Rust thuần). `container.xml` → OPF → manifest,
  spine (bỏ mục `linear="no"` khỏi luồng đọc), metadata, bìa (`meta name="cover"` / `properties="cover-image"` / ảnh đầu tiên), chiều
  trang (`page-progression-direction`). Sách bố cục cố định (`rendition:layout pre-paginated`) → GĐ3.
- Mục lục: EPUB 3 `nav.xhtml` (`epub:type="toc"`), EPUB 2 `toc.ncx`; không có thì dựng từ spine + `h1` / `h2`.
- **Font nhúng bị làm rối** (`META-INF/encryption.xml`): IDPF (XOR 1040 byte đầu với SHA-1 của mã sách), Adobe (1024 byte với UUID) →
  lõi gỡ khi phục vụ. Nội dung bị mã hoá thật (`rights.xml`, ADEPT) → báo «Sách có DRM, không mở được».
- Cổng: `GET /b/<mã phiên>/<khoá sách>/<đường dẫn trong sách>`. Tệp (X)HTML: chèn đầu `<head>` thẻ viewport + **`<style>` của
  màn đọc viết thẳng vào trang** (khung cột, cỡ chữ, font, nền — không dùng `<link>`: CSS ngoài về muộn làm WebKit dàn trang hai lần,
  B7) + `<script src="/gb/reader.js">`. Chương đúng cú pháp XML → `application/xhtml+xml` (sách vnthuquan ngắt đoạn bằng `<br></br>` —
  đọc như HTML thì xuống dòng gấp đôi); sai cú pháp → `text/html` để WebKit đọc dễ dãi. `Content-Type` đúng cho css / svg / ảnh /
  font; `Cache-Control` để WebView giữ ảnh.
- **Cắt chương lớn**: chương > ~150 KB cắt thành nhiều phần ở ranh giới đoạn (`<br/>` hoặc cuối khối), đóng các thẻ đang mở rồi mở
  lại ở phần sau; app coi mỗi phần là một chương (mục lục vẫn theo chương gốc, neo `id` tra được sang phần chứa nó). Bước 0: chương
  1,3 MB mở mất 2,8 s và đổi cỡ chữ 1,4 s; ~2,1 ms / KB → phần ≤ 150 KB mở ≤ 0,4 s (B7).
- **Chuẩn hoá chữ về NFC** khi phục vụ (dấu tổ hợp → chữ dựng sẵn — font iOS 6 xếp dấu tổ hợp không đều).
- App mở lại sau khi nằm nền lâu: cổng có thể bị iOS thu hồi → mở lại như `stream.revive` (M23) trước khi nạp chương.

### 4.5 Định dạng khác → «sách dạng EPUB»

Chuyển một lần, lưu vào đệm, rồi phục vụ y như EPUB (cùng `reader.js`, cùng vị trí đọc, tìm, ghi chú):

- **MOBI / AZW3** (GĐ1): MOBI6 (PalmDOC / HUFF-CDIC; `prc`, `azw`) và KF8 (AZW3, và phần KF8 trong tệp MOBI «kép» của KindleGen:
  skeleton + fragment → HTML), ảnh từ bản ghi, mục lục từ NCX của KF8 / `filepos` của MOBI6; có DRM → báo. Thử crate `mobi` trước;
  thiếu KF8 thì tự viết phần đó. Cần sách mẫu MOBI / AZW3 thật từ kênh anh dùng (tạm thời: tự chuyển sách mẫu EPUB bằng Calibre).
- **TXT** (GĐ2 — truyện chữ, phổ biến ở Việt Nam): dò mã (BOM, UTF-8 hợp lệ, không thì Windows-1258 / CP1252; TCVN3 / VNI tuỳ chọn ở
  GĐ3); tách chương theo dòng «Chương N / Hồi N / Quyển N / Phần N / Chapter N» (không thấy thì cắt ~30 KB một phần ở chỗ xuống dòng);
  mỗi dòng một đoạn.
- **FB2** (GĐ2): XML → mỗi `<section>` cấp 1 một chương, ảnh base64 → tệp, mục lục từ `<title>`.

### 4.6 Tìm trong sách (`search.rs`)

Bóc chữ từng chương (bỏ thẻ), so **không dấu**, trả `{chương, vị trí, đoạn trích}` theo đợt (sự kiện), dừng được. App mở chương,
`reader.js` tìm lại chỗ đó, tô và nhảy tới trang có nó.

---

## 5. Màn đọc EPUB (app)

### 5.1 Chia trang (cùng cách iBooks)

- **Một UIWebView cho chương đang đọc** + một UIWebView ẩn để đếm trang. `reader.js`:
  - đặt `html { -webkit-column-width: W; -webkit-column-gap: G; height: H }`; ảnh / bảng `-webkit-column-break-inside: avoid`;
    **iPad ngang: 2 cột / màn** (sách mở đôi);
  - số cột chương = `(scrollWidth − lề + khe) / bề rộng trang` (khớp tuyệt đối ở bước 0 — B7), đo **sau `load`**; số màn = số cột
    / số cột một màn, làm tròn lên;
  - đi trang = đặt `scrollView.contentOffset`; tắt cuộn tay của WebView, app tự xử lý chạm / vuốt.
- **Lật trang**: *Cong trang* (mặc định, GĐ1 — `UIPageViewController` kiểu cong như iBooks, các trang là ảnh chụp, WebView sống phủ
  lên khi đứng yên — B5), *Trượt*, *Không hiệu ứng* (nhanh nhất), *Cuộn dọc* (không chia cột).
- **Vùng chạm**: mép trái / phải 20 % = trang trước / sau, giữa = hiện / ẩn thanh, vuốt ngang = lật; tuỳ chọn «chạm trái cũng sang
  trang sau» (cầm một tay). Chạm trúng link / chú thích thì `reader.js` (`elementFromPoint`) xử lý, không lật.
- **Sang chương**: phủ ảnh chụp trang hiện tại, nạp chương kế, nhảy trang đầu (hoặc trang cuối khi lùi), bỏ ảnh phủ — không thấy
  nháy. GĐ2: nạp sẵn chương kế trong WebView thứ hai khi còn ≤ 2 trang (nếu RAM cho phép).
- **Đếm trang cả sách**: WebView ẩn cùng kích thước nạp lần lượt các chương khi máy rảnh → `layout/…json`. Chưa xong: «Chương 3 ·
  trang 5 / 18» + % theo độ dài chương; xong: «Trang 45 / 312».

### 5.2 Vị trí đọc — «CFI rút gọn»

- Vị trí = {chương, đường dẫn phần tử (`/4/2/8…`), vị trí ký tự} của chữ đầu tiên trên trang, kèm % chương để dự phòng. Đổi font /
  cỡ chữ / xoay máy → tìm lại chữ đó → đúng trang.
- Lưu mỗi lần lật (gộp, ghi đĩa ≤ 1 lần / 5 giây), khi rời sách, khi xuống nền. «Đọc tiếp» mở đúng trang.

### 5.3 Thanh công cụ (chạm giữa màn)

- **Trên**: ‹ Thư viện · Mục lục (Mục lục / Dấu trang / Ghi chú) · tên sách · **Aa** · Tìm · Dấu trang (ruy-băng).
- **Dưới**: thanh kéo trang (kéo hiện bóng «Chương … · trang N»); «Trang 45 / 312 · còn 6 trang hết chương».
- **Luôn hiện, mờ** ở chân trang: số trang; tuỳ chọn giờ + pin (thanh trạng thái ẩn khi đọc).

### 5.4 Aa — kiểu chữ

- **Độ sáng**: `UIScreen.brightness` (iOS 5+) + lớp phủ tối thêm cho đọc đêm (tối hơn mức thấp nhất của máy).
- **Cỡ chữ** A− / A+ 12 nấc; mỗi font có bảng cỡ + giãn dòng riêng (như FontPresets của iBooks).
- **Font** (B8 — đã dò từng chữ tiếng Việt trên iPad mini 1): **Georgia** (mặc định), **Literata** (đóng kèm — font của Google Play
  Sách, OFL, đăng ký với hệ thống chứ không nạp `@font-face`), Times New Roman, Baskerville, Cochin, Helvetica Neue, Avenir Next,
  Verdana + «Theo sách». Không dùng Palatino (thiếu «ớ»), Hoefler Text, Didot, Optima, Gill Sans, Futura, Avenir, Trebuchet MS.
- **Nền**: Trắng / Giấy ngà / Xám / Đêm (Đêm: mọi màu nền trong sách trong suốt, chữ sáng, ảnh giữ nguyên); tự sang Đêm theo giờ (tuỳ
  chọn).
- Giãn dòng, lề (hẹp / vừa / rộng), căn đều, «Giữ kiểu của sách».
- Khoá xoay; iPad ngang: 1 hay 2 trang.
- **Giữ màn hình sáng**: 5 / 10 / 30 phút sau lần lật cuối / luôn (`idleTimerDisabled`).

### 5.5 Mục lục, dấu trang, tô màu, ghi chú

- **Mục lục**: cây, chương đang đọc tô đậm, số trang khi đã đếm xong. Trên iPad mở dạng popover.
- **Dấu trang**: nút ruy-băng; danh sách kèm câu đầu trang + ngày.
- **Chọn chữ** (giữ tay): menu Sao chép · Tô màu · Ghi chú · Tra từ · Tìm (`UIMenuController` + `UIMenuItem`, iOS 5+). Tô 5 màu +
  gạch chân; lưu theo CFI rút gọn (đầu, cuối) + đoạn chữ; `reader.js` vẽ lại khi nạp chương. Ghi chú: popover trên iPad. Danh sách
  ghi chú theo chương; **Xuất ghi chú** vào «Tin nhắn đã lưu» Telegram (GĐ2).
- **Chú thích cuối trang** (`epub:type="noteref"`): chạm → bóng hiện nội dung, không nhảy trang.
- **Link trong sách**: nhảy + nút «‹ Quay lại trang 45».
- **Tra từ**: `UIReferenceLibraryViewController` (iOS 5+, từ điển Anh có sẵn); từ điển Anh – Việt / Việt – Việt ngoại tuyến (StarDict)
  ở GĐ3.

---

## 6. Màn đọc PDF (GĐ2)

- `CGPDFDocument` mở ở luồng nền; mỗi trang = UIScrollView phóng to + `CATiledLayer` (vẽ ô theo mức phóng); ảnh trang nhỏ cho thanh
  kéo / lưới trang; đệm ảnh ±2 trang, có trần RAM.
- **Cắt lề trắng tự động** (đo vùng có mực trên ảnh nhỏ) — quan trọng với PDF khổ A4 trên màn 7,9″; **chế độ đêm** (đảo màu khi vẽ);
  giữ mức phóng «vừa chiều ngang» khi lật trang.
- Mục lục PDF (Outlines), dấu trang, nhảy tới trang; vị trí = số trang + điểm phóng.
- Tìm chữ trong PDF: GĐ3, chưa chắc (PDF tiếng Việt font nhúng hay thiếu bảng mã chữ).

---

## 7. Giao diện ngoài màn đọc

Thanh dưới tự làm (GMRootController của GramMusic), 5 mục:

| Mục | Có gì |
| --- | --- |
| **Đang đọc** | Sách đang đọc dở (bìa lớn, %, «Đọc tiếp»), sách mới về từ các nguồn, đọc xong gần đây |
| **Thư viện** | Lưới bìa (iPad dọc 5 cột, ngang 7) hoặc danh sách; tìm không dấu; lọc / xếp (mục 4.2); chạm bìa → thẻ sách |
| **Nguồn** | Như GramMusic: các chat có sách, ẩn / hiện, thêm kênh @tên / t.me, quét / lấy mới, nhóm có chủ đề |
| **Bộ sưu tập** | Muốn đọc, Yêu thích, Đã đọc xong + bộ sưu tập tự tạo (playlist của GramMusic) |
| **Cài đặt** | Tài khoản, Proxy, DNS mã hoá, Dữ liệu và bộ nhớ, Giao diện app, mặc định của màn đọc, phiên bản, Đăng xuất |

- **Thanh «Đọc tiếp»** thay thanh phát nhỏ: bìa, tên, «Chương 7 · 43 %»; chạm mở đúng trang.
- **Thẻ sách** (popover trên iPad): bìa, tên, tác giả, nguồn + ngày gửi, định dạng · cỡ, chú thích của tin, tiến độ; Đọc / Tải về
  máy / Thêm vào bộ sưu tập / Đánh dấu đã đọc / Xoá bản tải / Mở tin gốc.
- **Bìa**: ảnh nhỏ của tin (nếu có) → bìa thật trong sách sau lần mở đầu → không có thì bìa tự vẽ (màu theo tên, chữ tên sách + tác
  giả).
- **Sách trên máy** (GĐ2, như M15 của GramMusic): thư mục `Documents` (chia sẻ tệp qua iTunes), «Mở bằng GramBook» từ Safari / Mail
  (`CFBundleDocumentTypes` epub / pdf).
- **Biểu tượng app**: cùng họ GramMini / GramMusic (ô viền crôm, máy bay giấy + cuốn sách) — chờ mẫu của anh như M6.

---

## 8. Bộ nhớ đệm

- Sách đã mở = đệm: giới hạn mặc định 2 GB, giữ 60 ngày, đầy thì xoá sách lâu không mở nhất; sách đang đọc dở không bị xoá. Máy còn
  < 300 MB thì dọn tới khi còn 500 MB (GramMini D15).
- «Tải về máy»: không tự xoá. Vị trí đọc, dấu trang, ghi chú: không bao giờ bị dọn.
- Cài đặt → Dữ liệu và bộ nhớ: theo loại (Sách tải về / Sách đã mở / Bìa / Ảnh nguồn / Khác) và theo nguồn.

---

## 9. Lộ trình

### Bước 0 — thử kỹ thuật trên iPad mini 1: xong (17/11/2026)

- [x] Chia cột CSS bằng WebKit iOS 6 với 8 sách mẫu / 220 chương (Safari + máy chủ nhỏ `spike/epub-columns/`): đúng, đủ nhanh —
  B7. Phát hiện: cắt chương lớn, CSS viết thẳng vào trang, đo sau `load`, ảnh cao gần bằng trang, NFC.
- [x] Font tiếng Việt: dò từng chữ trong 23 font — B8.
- [ ] Còn để GĐ1 (trong app): iPad dọc, chụp ảnh trang cho lật cong, chọn chữ, RAM UIWebView, iPad 1.

### GĐ0 — khung (fork GramMusic)

- [ ] Chép GramMusic → GramBook, đổi tên / bundle id / thư mục dữ liệu, bỏ phần nhạc (mục 0.2); `cargo test` trên PC.
- [ ] Dựng `.deb` trên máy dựng Ubuntu, cài iPad mini 1, đăng nhập, `symcheck` iOS 6.1.x + iOS 5.1.1.

### GĐ1 — đọc EPUB / MOBI / AZW3 (bản thử đầu tiên)

- [ ] Lõi: quét sách, chỉ mục, tìm / xếp; tải sách; `book.rs` (zip, OPF, NCX / nav, chèn CSS + JS, gỡ font làm rối, cắt chương lớn,
  NFC); MOBI / AZW3 → chương HTML.
- [ ] App: Thư viện (lưới / danh sách), thẻ sách, Nguồn, Đang đọc + thanh «Đọc tiếp», sách trên máy (`Documents` — để thử trước khi
  đăng nhập); màn đọc: chia trang, **lật cong kiểu iBooks**, vùng chạm, mục lục, Aa (cỡ, font, nền, giãn dòng, lề, độ sáng), vị trí
  đọc + Đọc tiếp, dấu trang, iPad ngang 2 trang, giữ màn hình sáng; từ khảo sát (mục 11): thanh chân trang (giờ, pin, chương,
  «còn N trang»), độ đậm chữ, «quay lại» cho mọi bước nhảy, vuốt mép trái chỉnh sáng, gửi sách cho bạn qua Telegram, tôn trọng
  «Hạn chế lưu nội dung».
- [ ] **Đạt khi** (iPad mini 1): mở 303 sách mẫu + sách MOBI / AZW3 không lỗi; ngân sách mục 2; đổi cỡ chữ / xoay máy giữ đúng chỗ
  đang đọc; lật cong mượt.

### GĐ2 — đủ dùng hằng ngày

- [ ] PDF (mục 6). Đếm trang cả sách + thanh kéo trang. Tìm trong sách. Tô màu / ghi chú / xuất ghi chú.
- [ ] TXT, FB2. Bìa thật từ sách. Bộ sưu tập. «Mở bằng GramBook». Dữ liệu và bộ nhớ.

### GĐ3 — thêm

- [ ] Lật cong trang; tra từ ngoại tuyến Anh – Việt; thống kê thời gian đọc; đồng bộ vị trí đọc giữa các máy qua «Tin nhắn đã lưu» (phải
  bật); CBZ; xem nhanh doc / docx / rtf; mở PDF lớn khi chưa tải xong; EPUB bố cục cố định; TXT mã TCVN3 / VNI.

### Phát hành

- [ ] Như GramMusic: `build-release.sh`, `symcheck`, nguồn Cydia + trang giới thiệu, GitHub — khi anh muốn.

---

## 10. Rủi ro

| Rủi ro | Cách xử lý |
| --- | --- |
| WebKit iOS 6 chia cột sai với sách lạ (ảnh lớn, bảng, `position: absolute`, `float`) | Bước 0 thử sách thật; luật CSS như iBooks (ảnh ≤ 100 % trang, sửa sách Calibre); dự phòng cuộn dọc |
| Chương rất dài (1 – 2 MB một tệp) làm WebView chậm / tốn RAM trên A5 | Đo ở bước 0; lõi tự cắt chương quá lớn thành nhiều phần ở ranh giới đoạn |
| Đếm trang cả sách chậm trên A5 | Chạy lúc rảnh, lưu theo bố cục, chưa xong thì hiện % — không chặn việc đọc |
| XHTML hỏng, EPUB lạ (EPUB 3 chỉ có nav, không spine hợp lệ…) | Lõi dễ dãi khi đọc OPF; `text/html` cho XHTML hỏng; thử với kho sách thật của anh |
| Font hệ thống thiếu dấu tiếng Việt | Bước 0 kiểm; đóng Literata |
| Kênh sách rất lớn → quét lâu, FLOOD_WAIT | Như GramMusic: con trỏ lưu được, chờ đúng giây Telegram bảo, quét tiếp |
| PDF scan lớn ngốn RAM iPad 1 (256 MB) | Trần đệm ảnh theo RAM máy; vẽ ô theo mức phóng |
| Cổng 127.0.0.1 bị thu hồi khi app nằm nền lâu | Mở lại trước khi nạp chương (M23) |

---

## 11. Học từ các app đọc sách khác (khảo sát 17/11/2026)

Đã xem: iBooks 1.5 – 3 (thời iOS 5 / 6) và Apple Books, Kindle (iOS + máy e-ink), Google Play Sách, Kobo, KOReader, Moon+ Reader,
Marvin, KyBook 3, Lithium, ReadEra, FBReader, PocketBook, Readwise Reader, Libby; Waka, Fonos, vBook, TTV Translate, TYT, MTC; Legado;
các bot / công cụ sách trên Telegram. Chưa thấy app đọc nào lấy thư viện thẳng từ tài khoản Telegram — đây là chỗ khác biệt của
GramBook.

Đặc điểm kênh sách Việt trên Telegram: nhiều tệp EPUB đăng trần (không chú thích), tên tệp kiểu `SachMoi.net-ten-sach.epub` /
`sachmoi.net_ten_sach.epub`, chú thích hay chỉ là hashtag (#sachvanhoc), nhiều tin chuyển tiếp từ kênh khác → **lấy thông tin từ tệp
trước, chú thích / ảnh của tin chỉ bổ sung**; trùng lặp giữa các kênh là chuyện thường.

| Tính năng (app có) | Làm thế nào trên iOS 5 / 6 | GĐ |
| --- | --- | --- |
| **Thanh chân trang khi toàn màn hình**: giờ, pin, tên chương, «còn N trang trong chương» (iBooks, Kobo, KOReader, Kindle) | Số trang chương có từ chia cột | 1 |
| **Độ đậm chữ** (Apple Books, Kobo, KOReader) — chữ mảnh nhạt trên màn 1x của iPad mini 1 | `-webkit-text-stroke` / chọn độ đậm font | 1 |
| **«Quay lại» cho mọi bước nhảy** (mục lục, tìm, thanh kéo, link) + trang ghim (KOReader, Kindle Page Flip) | Ngăn xếp vị trí CFI rút gọn | 1 |
| **Vuốt mép trái chỉnh độ sáng** (Moon+, Kobo) | UIPanGestureRecognizer | 1 |
| **Làm sạch tên tệp kiểu kênh sách** (`SachMoi.net-`, `_`, `-` giữa chữ) | Lõi, lúc quét | 1 |
| **Gửi sách cho bạn qua Telegram** (chuyển tiếp — không tải lên lại) | `messages.forwardMessages` | 1 |
| **Tôn trọng «Hạn chế lưu nội dung»** của chat: ẩn Gửi / Chép chữ / Mở bằng app khác / Xuất cho sách từ chat đó (Telegram buộc ứng dụng làm vậy) | Cờ `noforwards` của tin / chat | 1 |
| **Ghép bài đăng thành thông tin sách**: ảnh bìa + chú thích + tệp cùng album (`grouped_id`) hoặc tin liền kề cùng người gửi; hashtag → thể loại (chưa app nào có) | Lõi hỏi các tin quanh tệp khi cần bìa / thông tin | 2 |
| **Gộp định dạng / bản trùng** của cùng một đầu sách từ nhiều kênh (ReadEra): EPUB > AZW3 > MOBI > PDF | Tên + tác giả (bỏ dấu, từ OPF), cỡ tệp | 2 |
| **Bộ sách nhiều tập**: «Tập / Quyển / Phần N», `calibre:series`; nút «Đọc tập tiếp», tải trước tập sau (Kobo, ReadEra, KyBook, KOReader) | Lõi | 2 |
| **Kệ «Mới từ kênh»** + báo sách mới (Kindle, kpub) | Lấy sách mới khi mở app; báo bằng UILocalNotification | 2 |
| **Bảng mã cũ** TCVN3 / VNI / VIQR → Unicode NFC (FBReader, Unikey) | Lõi, làm cùng đợt TXT | 2 |
| **Làm sạch TXT** (nối dòng ngắt cứng, thụt đầu dòng, lọc câu quảng cáo — Moon+, vBook, Legado), **sửa tên nhân vật cho truyện convert** (vBook, TTV) | Lõi, lúc chuyển TXT → chương | 2 |
| **Tự cuộn / tự lật trang** (Moon+, KyBook, Marvin, KOReader) | NSTimer | 2 |
| **Ánh sáng ấm dần buổi tối** (Play Sách Night Light, KOReader, Kobo); ảnh dịu đi khi nền Đêm | Lớp phủ màu hổ phách; CSS filter chỉ có từ iOS 6 | 2 |
| **Phím âm lượng lật trang** (Moon+) | Bắt sự kiện đổi âm lượng trong app / tweak | 2 |
| **Mục lục dựng lại từ đề mục** khi NCX lỗi, % đã đọc từng chương, **số trang ổn định** theo số ký tự (KOReader, KyBook) | Lõi, không cần dàn trang | 2 |
| **Xuất ghi chú** .md / .html / .txt / «My Clippings» kèm chương, số trang (KOReader, KyBook, Play Sách) | Gửi tệp vào Tin nhắn đã lưu | 2 |
| **Trích dẫn thành ảnh** (bìa + tên sách) gửi vào chat (Kindle) | UIGraphics / CoreText | 2 |
| **Sao lưu thư viện + ghi chú** vào Tin nhắn đã lưu; đồng bộ cả tô màu; «nhảy tới chỗ xa nhất đã đọc?» (KOReader, Kindle Whispersync) | Tệp qua Telegram | 2 – 3 |
| **Tra từ tiếng Việt nhiều âm tiết** (khớp 1 – 4 âm tiết quanh chỗ chạm) + từ điển Việt – Việt, Hán – Việt (Thiều Chửu), Anh – Việt (StarDict) | Lõi đọc StarDict; từ điển hệ thống iOS 6 không có tiếng Việt; kiểm giấy phép từng bộ | 2 – 3 |
| **Danh sách nhân vật** kiểu X-Ray rút gọn (cụm Viết Hoa ≥ 2 âm tiết, chạm xem các lần xuất hiện) | Lõi | 3 |
| **Mục tiêu đọc mỗi ngày + chuỗi ngày đọc**, thống kê (Apple Books, Kobo, Waka) | Từ thời gian đọc đã ghi | 3 |
| **Sổ từ vựng / ôn trích dẫn** (Kindle Vocabulary Builder, Readwise Daily Review) | Lõi | 3 |
| **Nghe đọc tiếng Việt** (vBook, TYT, Fonos) | Khó: iOS 5 / 6 không có giọng Việt — thử eSpeak NG (giọng máy) hoặc TTS trực tuyến | 3 (thử) |

**Không làm**: Popular Highlights, tóm tắt / hỏi đáp bằng AI (cần máy chủ); Word Wise (thư viện chủ yếu tiếng Việt, nặng cho A5);
ảnh thu nhỏ từng trang kiểu Page Flip (tràn RAM iPad 1 — thay bằng «quay lại» + mục lục có tiến độ); PDF dàn lại chữ / OCR / vẽ tay;
kho bot tìm sách dựng sẵn kiểu Z-Library (rủi ro bản quyền — để người dùng tự thêm kênh); phá DRM, nối kho Waka / Fonos / Kindle;
OPDS / Calibre, đọc nhanh RSVP, mạng xã hội sách.

---

## 12. Anh đã quyết (17/11/2026)

1. Định dạng ưu tiên: **EPUB, MOBI, AZW3** (B3).
2. Chat riêng: chỉ **EPUB / FB2 / MOBI** (cả AZW3 / AZW / PRC) (B4).
3. **Lật trang cong kiểu iBooks ngay** (B5).
4. Giữ `gm-` / `GM` cho phần chép từ GramMusic, `GB` cho phần mới (B1).
5. Một `.deb` cho iOS 5 + 6, có iPad 1 (B2).
6. Dùng chung `api_id` Telegram với GramMusic (B6).
7. Sách thử: 303 EPUB ở `Tham_Khao/ebook` (B9). Còn thiếu: sách MOBI / AZW3 thật; mẫu biểu tượng app.
