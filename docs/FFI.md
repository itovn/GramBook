# GramBook — Hợp đồng giữa app (Objective-C) và lõi Rust

Lõi: `core/crates/gm-ffi` → `libgm_ffi.a` (dịch vụ ở `core/crates/gm-core/src/service.rs`). App: `app/Core/GMCore.m`. Cách gọi giống
hệt GramMusic / GramMini.

## Hàm C

```c
typedef void (*gm_cb)(const char *json, void *ctx);
int  gm_init(const char *config_json, gm_cb on_event, void *ctx);
void gm_call(const char *request_json, gm_cb on_result, void *ctx);
```

- `gm_init` — một lần lúc mở app. `config_json`: `{"data_dir", "device_model", "system_version", "app_version", "lang"}`. Trả 0 =
  được; 1 = cấu hình sai; 2 = bản dựng thiếu api_id (`keys/telegram.txt`); 3 = không mở được dữ liệu; 4 = đã mở.
- `gm_call` — chạy nền. `request_json`: `{"m": "<nhóm.lệnh>", "a": {…}}`. `on_result` được gọi **đúng một lần** với
  `{"ok": <kết quả>}` hoặc `{"err": {"code", "msg", …}}`.
- Dữ liệu ở `data_dir` (`~/Library/GramBook`): xem THIET-KE 4.2.

## Kiểu dữ liệu

- **Khoá peer**: chuỗi số kiểu Bot API (`"123"` người, `"-123"` nhóm thường, `"-100123"` kênh / siêu nhóm).
- **Khoá sách**: `"<peer>_<id tin>"`; sách trên máy: `"file:<đường dẫn>"` (lõi đọc thẳng tệp).
- `Book`: `{"key", "peer", "id", "title", "author" (tên trong sách nếu đã mở, không thì theo tên tệp), "ext", "fmt" (epub | kindle |
  pdf | text | other), "size", "date", "thumb" (tin có ảnh nhỏ), "ok" (màn đọc mở được), "src" (tên nguồn), "caption" (≤ 200 ký tự),
  "topic", "fav", "want", "dl": null | "cached" (đã mở — có trong đệm) | "queued" | "offline", "progress" (0…1, null = chưa đọc),
  "finished", "nf" (tin «hạn chế lưu nội dung» — cờ `noforwards` của Telegram: app không cho gửi tiếp / chép chữ / mở bằng app
  khác)}`; sách không còn trong thư viện (trong bộ sưu tập): `{"key", "title", "author", "ext", "missing": true, "ok": false}`.
- `Pos` (vị trí đọc): `{"s" (chương — chỉ số spine), "p" (phần của chương lớn), "c" («/3/1/5:120» — CFI rút gọn), "f" (tỉ lệ trong
  phần), "t" (vài chữ ở đó)}`.
- `Reading`: `{"key", "title", "author", "ext", "pos", "progress", "chapter", "opened", "started", "finished", "seconds", "marks":
  [{id, pos, date, chapter}]}`.
- `Source`: như GramMusic (`count` = số sách).

## Lệnh

| Lệnh | Tham số | Kết quả |
| --- | --- | --- |
| `auth.*`, `proxy.*`, `net.state` / `net.check` / `net.dns`, `app.flush` | như GramMusic | |
| `library.query` | `q`?, `source`?, `topic`?, `sort`: date \| title \| author, `filter`: all \| offline \| cached \| readable \| unread \| reading \| done, `fmt`?: epub \| kindle \| pdf \| text \| other, `offset`, `limit` (≤ 500) | `{total, gen, offset, items: [Book], sections: [{t, i, n}]}` |
| `library.keys` | như trên (không offset / limit) | `[khoá]` (≤ 5000) |
| `library.books` | `keys` | `[Book]` theo đúng thứ tự |
| `library.stats` | — | `{total, sources, gen, offline, full_done, last_sync, partial, sync}` |
| `sources.list` / `sources.set` / `sources.add` / `sources.topics` / `sources.remove` | như GramMusic | |
| `sync.start` / `sync.stop` / `sync.state` | như GramMusic (một lượt: tài liệu có đuôi sách) | |
| `book.open` | `key` | `{id, key, base (URL gốc của cổng — chương: base + href + "?p=N"), title, author, ext, format (epub \| mobi6 \| kf8), lang, rtl, fixed, series, spine: [{path, href, linear, size}], toc: [{title, s, frag, depth}], reading: Reading, cover, nf}`; tải tệp nếu chưa có (báo `file.progress`). EPUB đọc thẳng trong zip; MOBI / AZW3 / AZW / PRC: crate `gb-mobi` chuyển cả sách thành tệp chương + ảnh / CSS trong bộ nhớ lúc mở (bìa ghi ra `covers/`). Lỗi `FORMAT` (chưa đọc được), `DRM`, `BAD_BOOK`, `NO_BOOK` |
| `book.forward` | `key`, `to` (khoá peer hoặc "self" = Tin nhắn đã lưu) | `{sent, uploaded?}` — sách Telegram: chuyển tiếp tin gốc chứa tệp (không tải lên lại); sách trên máy (`file:`): tải tệp lên, gửi như tài liệu; lỗi `NOFORWARDS` (sách «hạn chế lưu nội dung»), `FORBIDDEN` (không được gửi vào chat đó), `NO_BOOK`, `PEER_UNKNOWN` |
| `chats.recent` | `limit`? (40) | `[{peer, name, kind: self \| user \| bot \| group \| channel, username, photo, photo_dc}]` — chat để gửi sách: «Tin nhắn đã lưu» trước, rồi theo tin mới nhất; bỏ kênh không có quyền đăng, tài khoản đã xoá |
| `book.chapter` | `id`, `s` | `{parts, xhtml, sizes}` — chương đã chuẩn bị (cắt phần — DECISIONS B7) |
| `book.locate` | `id`, `href` (URL đầy đủ của cổng) hoặc `path` + `frag` | `{s, p, frag}` — link / mục lục → chương + phần chứa neo |
| `book.close` | `id` | `{}` |
| `book.cancel` | `key` | `{}` — thôi tải sách đang mở dở |
| `reader.config` | `config`: `{w, h, cols, ms, mt, mb, fs, lh, ff, theme, just, weight, st, nf}` (`nf`: sách «hạn chế lưu nội dung» → không cho chọn chữ) | `{}` — chèn vào mọi chương (`window.GBC`), lưu lại |
| `reader.get` | — | cấu hình đã lưu |
| `serve.revive` | — | `{port, revived}` — cổng bị iOS thu hồi lúc nằm nền thì mở lại (GramMusic M23) |
| `reading.get` | `key` | `Reading` \| null |
| `reading.save` | `key`, `pos`, `progress`?, `chapter`?, `secs`? | `{}` — gộp ghi ≤ 1 lần / 5 giây |
| `reading.recent` | `limit`?, `finished`? | `[Reading]` mở gần nhất trước |
| `reading.finish` | `key`, `on` | `{}` |
| `reading.forget` | `key` | `{}` |
| `mark.add` | `key`, `pos`, `chapter`? | `{id}` |
| `mark.remove` | `key`, `id` | `{removed}` |
| `dl.add` / `dl.remove` / `dl.state` | `keys` | như GramMusic |
| `file.progress` | `key` | `{cached}` 0…1 |
| `media.cover` | `key` | `{path, book}` — bìa trong sách (tệp đã có / sách trên máy) hoặc ảnh nhỏ của tin; lỗi `NO_COVER` |
| `media.avatar` | `peer`, `photo`, `dc` | `{path}` |
| `col.list` | — | `{lists: [{id, name, count, updated, cover}], fav, want, recent, offline}` |
| `col.create` / `col.rename` / `col.delete` / `col.items` / `col.add` / `col.remove` / `col.move` | như `pl.*` của GramMusic; `col.items` nhận `id` số, "fav", "want", "recent" | |
| `fav.set` / `want.set` | `key`, `on` | `{on}` |
| `cache.stats` / `cache.clear` / `cache.set` | như GramMusic, loại: offline, books, cover, avatar, other | |
| `settings.get` / `settings.set` | `secure_dns` | |

## Sự kiện

| `e` | Trường |
| --- | --- |
| `auth.state` / `auth.dead` / `net.state` / `sync.state` / `library.changed` / `cache.cleared` | như GramMusic |
| `file.progress` / `file.done` / `file.error` | `key` (+ `have`, `total` khúc / `code`, `msg`) — tải tệp sách đang mở |
| `dl.progress` / `dl.done` / `dl.error` | hàng tải về máy |

## Cổng sách (serve.rs)

- `GET /b/<mã phiên>/<mã sách>/<đường dẫn trong sách>[?p=N]`: chương (phần N) — chữ về NFC, thực thể có tên → số, chèn sau `<head>`:
  viewport + `<script>window.GBC = <cấu hình>; window.GBD = {b, s, p, n, x}</script>` + `<script src="/gb/reader.js?v=…">`;
  `application/xhtml+xml` khi đúng cú pháp XML có không gian tên XHTML, không thì `text/html`. Tệp khác (ảnh, CSS, font — gỡ làm rối):
  `max-age=86400`.
- `GET /gb/reader.js` — mã màn đọc (`core/crates/gm-core/src/assets/reader.js`).

## reader.js (trong UIWebView)

App gọi bằng `stringByEvaluatingJavaScriptFromString:`: `GB.info()` → `{ready, screens, page, cols, s, p, n}`; `GB.goTo(i)`,
`GB.goToEnd()`, `GB.goToFraction(f)`, `GB.goToPos(c)`, `GB.goToId(id)` (trả trang, -1 = không thấy); `GB.screenOfPos(c)`,
`GB.screenOfId(id)` (không lật); `GB.pos()` → `{c, f, t, page, screens}`; `GB.setConfig(json)` (đổi kiểu tại chỗ, giữ chỗ đang
đọc); `GB.tap(x, y)` → `{k: link | note | sel | none, href, local, html}`; `GB.drain()` → các tin chờ (`ready`). Tin gửi app: hàng
đợi + điều hướng `gb://ping` (app chặn ở `webView:shouldStartLoadWithRequest:`).
