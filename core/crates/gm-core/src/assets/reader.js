/* GramBook — reader.js: mã màn đọc chạy trong UIWebView (iOS 5 / 6 — ES5, không JIT).
 *
 * Lõi chèn vào đầu <head> mỗi chương: window.GBC (cấu hình màn đọc, app đặt bằng reader.config) + window.GBD (sách, chương, phần)
 * rồi tệp này. Cách chia trang như iBooks (DECISIONS B7): <html> cao đúng một trang, các cột tràn sang phải, mỗi «trang» rộng
 * PW = W / số cột một màn. CSS tạo ngay lúc đọc <head> (trước khi dàn trang — khỏi dàn trang hai lần), đo sau `load`.
 *
 * App gọi (stringByEvaluatingJavaScriptFromString): GB.info(), GB.goTo(i), GB.goToEnd(), GB.pos(), GB.goToPos(c), GB.goToId(id),
 * GB.goToFraction(f), GB.setConfig(json), GB.tap(x, y), GB.drain(). Tin gửi app: hàng đợi + điều hướng «gb://ping» (app chặn ở
 * webView:shouldStartLoadWithRequest:, rồi gọi GB.drain() lấy hết).
 */
(function () {
    'use strict';
    var C = window.GBC || {};
    var D = window.GBD || {};
    var GB = window.GB = { page: 0, ready: false, q: [] };
    var XHTML = 'http://www.w3.org/1999/xhtml';

    var THEMES = {
        day: { bg: '#ffffff', fg: '#1b1b1b', link: '#2a5db0' },
        sepia: { bg: '#f4ecd8', fg: '#5b4636', link: '#7a4f1d' },
        gray: { bg: '#5a5a5c', fg: '#e8e8e8', link: '#a9c8ff' },
        night: { bg: '#121212', fg: '#c8c8c8', link: '#8a8ee4' }
    };

    function num(v, d) { v = parseFloat(v); return isNaN(v) ? d : v; }

    function geom() {
        var g = {};
        g.W = num(C.w, window.innerWidth || 768);
        g.H = num(C.h, window.innerHeight || 1024);
        g.cols = C.cols === 2 ? 2 : 1;
        g.ms = num(C.ms, 40);
        g.mt = num(C.mt, 40);
        g.mb = num(C.mb, 40);
        g.PW = g.W / g.cols;           // một trang: cột chữ + lề hai bên
        g.CW = g.PW - 2 * g.ms;        // bề rộng chữ
        g.CH = g.H - g.mt - g.mb;      // bề cao chữ
        g.scroll = !!C.scroll;
        return g;
    }
    var G = geom();

    function css() {
        var t = THEMES[C.theme] || THEMES.day;
        var bg = C.bg || t.bg, fg = C.fg || t.fg, link = C.link || t.link;
        var night = C.theme === 'night' || C.theme === 'gray';
        var s = '@namespace epub "http://www.idpf.org/2007/ops";\n';
        if (G.scroll) {
            s += 'html{margin:0!important;padding:' + G.mt + 'px ' + G.ms + 'px ' + G.mb + 'px ' + G.ms + 'px!important;' +
                '-webkit-text-size-adjust:none;-webkit-touch-callout:none;background-color:' + bg + '!important;}\n';
        } else {
            s += 'html{margin:0!important;padding:' + G.mt + 'px ' + G.ms + 'px ' + G.mb + 'px ' + G.ms + 'px!important;' +
                '-webkit-box-sizing:border-box!important;width:' + G.W + 'px!important;height:' + G.H + 'px!important;' +
                '-webkit-column-width:' + G.CW + 'px!important;-webkit-column-gap:' + (2 * G.ms) + 'px!important;' +
                '-webkit-text-size-adjust:none;-webkit-touch-callout:none;background-color:' + bg + '!important;}\n';
        }
        s += 'body{margin:0!important;padding:0!important;word-wrap:break-word;color:' + fg + ';background:transparent!important;}\n' +
            'img,svg,video{max-width:100%!important;max-height:' + Math.floor(G.CH * 0.96) + 'px!important;-webkit-column-break-inside:avoid;}\n' +
            'img{width:auto!important;height:auto!important;}\n' +
            /* ảnh đứng riêng: bỏ khoảng chân chữ → dòng chứa ảnh không cao hơn trang (B7) */
            'div>img:only-child,p>img:only-child,a>img:only-child,center>img{vertical-align:bottom!important;}\n' +
            'h1,h2,h3,h4,h5,h6,img,table{-webkit-column-break-inside:avoid;}\n' +
            'table{max-width:100%!important;}pre{white-space:pre-wrap!important;}a{text-decoration:none;color:' + link + ';}\n' +
            'aside[epub|type~="footnote"],aside[epub|type~="endnote"],aside[epub|type~="rearnote"]{display:none!important;}\n' +
            '.gb-hl{background-color:rgba(255,220,0,.45)!important;}\n';
        if (C.st !== 'pub') {
            /* «Kiểu của GramBook»: cỡ chữ / giãn dòng / font của người đọc thắng kiểu ghi sẵn trong thẻ (Word HTML: LINE-HEIGHT 150%,
               <font face>, FONT-SIZE 13pt…); h1–h6, sup, sub, small, big giữ tỉ lệ */
            var ff = C.ff ? "font-family:'" + String(C.ff).replace(/'/g, '') + "',Georgia,serif!important;" : '';
            s += 'body{font-size:' + num(C.fs, 20) + 'px!important;line-height:' + num(C.lh, 1.5) + '!important;' + ff +
                (C.just === false ? 'text-align:left!important;' : 'text-align:justify!important;') + '}\n' +
                'body p,body div,body span,body font,body li,body td,body th,body blockquote,body a,body em,body i,body b,' +
                'body strong,body u,body center,body dd,body dt,body cite,body q,body label{font-size:inherit!important;' +
                'line-height:inherit!important;font-family:inherit!important;}\n' +
                /* căn lề: chỉ đặt cho body — dòng có align="center" (tên sách, «Chương 1» của vnthuquan) vẫn giữ căn giữa */
                'h1,h2,h3,h4,h5,h6{line-height:1.3!important;}\n';
        }
        if (C.weight > 0) {
            s += 'body{-webkit-text-stroke:' + (C.weight > 1 ? '0.45' : '0.25') + 'px ' + fg + ';}\n';
        }
        if (night) {
            s += 'html *,body *{color:' + fg + '!important;background-color:transparent!important;border-color:#555!important;}\n' +
                'a:link,a:visited,a:link *,a:visited *{color:' + link + '!important;}\n' +
                'img{opacity:.85;}\n';
        }
        if (C.nf) {
            /* sách từ chat «hạn chế lưu nội dung»: không cho chọn / chép chữ */
            s += 'html,body,body *{-webkit-user-select:none!important;}\n';
        }
        return s;
    }

    var styleEl = null;
    function applyStyle() {
        var head = document.getElementsByTagName('head')[0] || document.documentElement;
        if (!styleEl) {
            styleEl = document.createElementNS ? document.createElementNS(XHTML, 'style') : document.createElement('style');
            styleEl.setAttribute('type', 'text/css');
            styleEl.setAttribute('id', 'gb-style');
            head.appendChild(styleEl);
        }
        styleEl.textContent = css();
    }
    // Ngay lúc đọc <head>: kiểu đã có trước khi dàn trang
    applyStyle();

    // ───────── Đếm trang, đi trang ─────────

    function cols() {
        if (G.scroll) return 1;
        var sw = document.documentElement.scrollWidth;
        return Math.max(1, Math.round((sw - G.ms + 2 * G.ms) / G.PW));
    }
    GB.screens = function () {
        if (G.scroll) return Math.max(1, Math.ceil(document.documentElement.scrollHeight / G.H));
        return Math.max(1, Math.ceil(cols() / G.cols));
    };
    GB.goTo = function (i) {
        var n = GB.screens();
        i = Math.max(0, Math.min(n - 1, i | 0));
        GB.page = i;
        if (G.scroll) window.scrollTo(0, i * G.H); else window.scrollTo(i * G.W, 0);
        return i;
    };
    GB.goToEnd = function () { return GB.goTo(GB.screens() - 1); };
    GB.goToFraction = function (f) { return GB.goTo(Math.floor(num(f, 0) * GB.screens())); };

    /** Màn chứa điểm x (toạ độ tài liệu). */
    function screenAtX(x) {
        if (G.scroll) return 0;
        return Math.max(0, Math.floor(Math.floor((x - G.ms + 1) / G.PW) / G.cols));
    }
    function screenOfRect(r) {
        if (!r) return GB.page;
        if (G.scroll) return Math.max(0, Math.floor((r.top + window.pageYOffset) / G.H));
        return screenAtX(r.left + window.pageXOffset);
    }

    GB.info = function () {
        return JSON.stringify({ ready: GB.ready, screens: GB.screens(), page: GB.page, cols: cols(), s: D.s, p: D.p, n: D.n, b: D.b,
            w: G.W, h: G.H, sw: document.documentElement.scrollWidth });
    };

    // ───────── Vị trí đọc: «CFI rút gọn» = đường dẫn chỉ số childNodes từ <body> + vị trí ký tự ─────────

    function pathOf(node) {
        var parts = [], body = document.body;
        while (node && node !== body && node.parentNode) {
            var i = 0, c = node;
            while ((c = c.previousSibling)) i++;
            parts.unshift(i);
            node = node.parentNode;
        }
        return node === body ? '/' + parts.join('/') : null;
    }
    function nodeAt(path) {
        var n = document.body;
        if (!path || path === '/') return n;
        var p = path.split('/');
        for (var i = 1; i < p.length && n; i++) n = n.childNodes[parseInt(p[i], 10)];
        return n || null;
    }

    /** Vị trí ở góc trên trái màn đang xem: {c: "đường:dẫn:ký_tự", f: tỉ lệ trong phần, t: đoạn chữ}. */
    GB.pos = function () {
        var r = null, x, y;
        // Thử vài điểm dọc mép trái cột đầu của màn (dòng đầu có thể là khoảng trống / ảnh)
        for (var k = 0; k < 6 && !r; k++) {
            x = G.ms + 2;
            y = G.mt + 2 + k * Math.floor(G.CH / 6);
            r = document.caretRangeFromPoint ? document.caretRangeFromPoint(x, y) : null;
            if (r && r.startContainer && r.startContainer.nodeType !== 3 && k < 5) r = null;
        }
        var c = null, t = '';
        if (r) {
            var node = r.startContainer, off = r.startOffset;
            var p = pathOf(node);
            if (p) c = p + ':' + off;
            if (node.nodeType === 3) t = node.nodeValue.substr(off, 40);
        }
        var n = GB.screens();
        return JSON.stringify({ c: c, f: n > 1 ? GB.page / n : 0, t: t, page: GB.page, screens: n });
    };

    /** Màn chứa vị trí `c` (không lật); -1 = không tìm thấy. */
    GB.screenOfPos = function (c) {
        if (!c) return 0;
        var i = c.lastIndexOf(':');
        var node = nodeAt(i > 0 ? c.substring(0, i) : c);
        var off = i > 0 ? parseInt(c.substring(i + 1), 10) || 0 : 0;
        if (!node) return -1;
        var rect = null;
        if (node.nodeType === 3) {
            var len = node.nodeValue.length;
            var rg = document.createRange();
            rg.setStart(node, Math.min(off, len));
            rg.setEnd(node, Math.min(off + 1, len));
            var rs = rg.getClientRects();
            rect = rs.length ? rs[0] : rg.getBoundingClientRect();
        } else if (node.getBoundingClientRect) {
            rect = node.getBoundingClientRect();
        }
        // toạ độ client → tài liệu: getClientRects tính theo màn đang cuộn
        return Math.min(GB.screens() - 1, screenOfRect(rect));
    };
    GB.goToPos = function (c) {
        var s = GB.screenOfPos(c);
        return s < 0 ? -1 : GB.goTo(s);
    };

    GB.screenOfId = function (id) {
        var el = document.getElementById(id) || (document.getElementsByName ? document.getElementsByName(id)[0] : null);
        if (!el) return -1;
        return Math.min(GB.screens() - 1, screenOfRect(el.getBoundingClientRect()));
    };
    GB.goToId = function (id) {
        var s = GB.screenOfId(id);
        return s < 0 ? -1 : GB.goTo(s);
    };

    // ───────── Chạm: link, chú thích ─────────

    function closestLink(el) {
        while (el && el.nodeType === 1) {
            if ((el.localName || el.nodeName).toLowerCase() === 'a' && el.getAttribute('href')) return el;
            el = el.parentNode;
        }
        return null;
    }

    /** Neo đích trong chính chương này (`#id` / link tuyệt đối của Word tới «…#_ftn1» — sách vnthuquan, B9). */
    function localTarget(href) {
        var h = href.indexOf('#');
        if (h < 0) return null;
        var id = decodeURIComponent(href.substring(h + 1));
        var path = href.substring(0, h);
        var here = location.href.split('#')[0].split('?')[0];
        var same = path === '' || path === here || /^https?:\/\/[^\/]*vnthuquan/i.test(path) || here.indexOf(path) === here.length - path.length;
        if (!same) return null;
        return document.getElementById(id) || (document.getElementsByName ? document.getElementsByName(id)[0] : null);
    }

    function isNote(a, target) {
        var t = (a.getAttributeNS && a.getAttributeNS('http://www.idpf.org/2007/ops', 'type')) || a.getAttribute('epub:type') || '';
        if (/noteref/.test(t)) return true;
        var href = a.getAttribute('href') || '';
        if (/#_?(ftn|edn|fn|footnote|note)/i.test(href)) return true;
        var tt = target && ((target.getAttributeNS && target.getAttributeNS('http://www.idpf.org/2007/ops', 'type')) || '');
        return /footnote|endnote|rearnote/.test(tt || '');
    }

    /** Nội dung chú thích: khối chứa neo đích (đoạn / aside / li). */
    function noteHTML(target) {
        var el = target;
        // neo rỗng <a name="_ftn1"></a> → lấy khối cha
        while (el && el.nodeType === 1 && (el.textContent || '').replace(/\s+/g, '').length < 2) el = el.parentNode;
        while (el && el.nodeType === 1 && /^(a|span|sup|font|b|i|em|strong)$/i.test(el.localName || el.nodeName)) el = el.parentNode;
        if (!el || el === document.body) return null;
        return el.innerHTML;
    }

    /** Chạm tại (x, y) của WebView: {k: "link", href} | {k: "note", html} | {k: "sel"} (đang chọn chữ) | {k: "none"}. */
    GB.tap = function (x, y) {
        var sel = window.getSelection ? window.getSelection() : null;
        if (sel && sel.rangeCount && !sel.isCollapsed) {
            sel.removeAllRanges();
            return JSON.stringify({ k: 'sel' });
        }
        var el = document.elementFromPoint(x, y);
        var a = closestLink(el);
        if (!a) return JSON.stringify({ k: 'none' });
        var href = a.getAttribute('href');
        var abs = a.href || href;
        var target = localTarget(abs);
        if (target && isNote(a, target)) {
            var html = noteHTML(target);
            if (html) return JSON.stringify({ k: 'note', html: html, id: target.id || target.getAttribute('name') });
        }
        if (target) return JSON.stringify({ k: 'link', href: abs, local: target.id || target.getAttribute('name') || '' });
        return JSON.stringify({ k: 'link', href: abs });
    };

    // Không để WebView tự đi theo link (app lo — GB.tap)
    document.addEventListener('click', function (e) {
        var a = closestLink(e.target);
        if (a) e.preventDefault();
    }, true);

    // ───────── Đổi cấu hình tại chỗ (cỡ chữ, font, nền, lề…) ─────────

    // anchor: chỗ đang đọc lúc lật trang / nhảy gần nhất (app giữ) — đổi cỡ chữ / xoay máy nhiều lần vẫn về đúng chỗ đó, không trôi dần
    // về trước như khi lấy đầu trang hiện tại sau mỗi lần dàn lại
    GB.setConfig = function (json, anchor) {
        var pos = anchor || (GB.ready ? JSON.parse(GB.pos()).c : null);
        try { C = typeof json === 'string' ? JSON.parse(json) : json; } catch (e) { return 'LỖI cấu hình'; }
        window.GBC = C;
        G = geom();
        applyStyle();
        if (pos) GB.goToPos(pos); else GB.goTo(0);
        return GB.info();
    };

    // ───────── Tin gửi app ─────────

    var pinged = false;
    GB.send = function (type, data) {
        data = data || {};
        data.e = type;
        GB.q.push(data);
        if (!pinged) {
            pinged = true;
            setTimeout(function () { location.href = 'gb://ping'; }, 0);
        }
    };
    GB.drain = function () {
        var q = GB.q;
        GB.q = [];
        pinged = false;
        return JSON.stringify(q);
    };

    window.addEventListener('load', function () {
        GB.ready = true;
        GB.send('ready', JSON.parse(GB.info()));
    }, false);
})();
