/* GramBook — reader.js, bản thử bước 0 (DECISIONS B7).
 * Chia trang bằng cột CSS (reader.css do máy chủ / lõi sinh theo khung): <html> cao đúng một trang, các cột tràn sang phải,
 * mỗi «trang» rộng PW = W / số cột một màn. Ở đây: đếm cột, kiểm ảnh, đo thời gian rồi báo về /report; kế hoạch «perf» tự sang
 * chương kế, «one» đứng ở trang cần xem và nhận lệnh JS từ /cmd (điều khiển từ xa khi thử).
 * Viết cho WebKit của iOS 5 / 6: ES3/5, không dùng performance.now, không dùng classList trên SVG.
 */
(function () {
    'use strict';
    var C = window.__GB || {};
    var L = C.layout || {};
    var t0 = window.__t0 || Date.now();
    var COLS = L.cols || 1;
    var PW = L.w / COLS;                 // một trang: cột chữ + lề hai bên
    var CW = PW - 2 * L.ms;              // bề rộng chữ
    var CH = L.h - L.mt - L.mb;          // bề cao chữ
    var marks = {};
    var GB = window.GB = { page: 0 };

    function now() { return Date.now(); }

    // Chữ cuối cùng của chương (bỏ khoảng trắng, script, style): cột chứa nó = cột cuối.
    function lastText(node) {
        for (var c = node.lastChild; c; c = c.previousSibling) {
            if (c.nodeType === 3) {
                if (/\S/.test(c.nodeValue)) return c;
            } else if (c.nodeType === 1) {
                var tag = c.nodeName.toLowerCase();
                if (tag === 'script' || tag === 'style' || tag === 'head') continue;
                var r = lastText(c);
                if (r) return r;
                if (tag === 'img' || tag === 'svg' || tag === 'video' || tag === 'table') return c;
            }
        }
        return null;
    }

    function rectOfEnd(n) {
        if (n.nodeType === 1) return n.getBoundingClientRect();
        var s = n.nodeValue.replace(/\s+$/, '');
        var r = document.createRange();
        r.setStart(n, Math.max(0, s.length - 1));
        r.setEnd(n, s.length);
        var rs = r.getClientRects();
        return rs.length ? rs[rs.length - 1] : r.getBoundingClientRect();
    }

    // Cột (0…) chứa điểm x (toạ độ tài liệu)
    function colAt(x) { return Math.floor((x - L.ms) / PW); }

    GB.columns = function () {
        var n = lastText(document.body || document.documentElement);
        if (!n) return 1;
        var r = rectOfEnd(n);
        return colAt(r.left + window.pageXOffset) + 1;
    };
    GB.screens = function () { return Math.max(1, Math.ceil(GB.columns() / COLS)); };
    GB.goTo = function (i) {
        GB.page = i;
        window.scrollTo(i * L.w, 0);
        return i;
    };
    GB.next = function () { return GB.goTo(GB.page + 1); };
    GB.prev = function () { return GB.goTo(Math.max(0, GB.page - 1)); };

    function checkImages() {
        var imgs = document.getElementsByTagName('img'), bad = [];
        for (var i = 0; i < imgs.length; i++) {
            var r = imgs[i].getBoundingClientRect();
            var x = r.left + window.pageXOffset, y = r.top + window.pageYOffset;
            var colL = L.ms + colAt(x + 1) * PW;
            var why = '';
            if (r.width > CW + 1) why += 'rộng ';
            if (x + r.width > colL + CW + 1) why += 'tràn-cột ';
            if (r.height > CH + 1) why += 'cao ';
            if (y + r.height > L.mt + CH + 1) why += 'đáy ';
            if (y < L.mt - 1) why += 'đỉnh ';
            if (why) bad.push(i + ':' + why + Math.round(r.width) + 'x' + Math.round(r.height) + '@' + Math.round(x) + ',' + Math.round(y));
        }
        return { n: imgs.length, bad: bad };
    }

    function post(obj, done) {
        var x = new XMLHttpRequest();
        x.open('POST', '/report', true);
        x.setRequestHeader('Content-Type', 'application/json');
        x.onreadystatechange = function () { if (x.readyState === 4 && done) done(); };
        x.send(JSON.stringify(obj));
    }

    function poll() {
        var x = new XMLHttpRequest();
        x.open('GET', '/cmd?_=' + now(), true);
        x.onreadystatechange = function () {
            if (x.readyState !== 4) return;
            var cmds = [];
            try { cmds = JSON.parse(x.responseText); } catch (e) { }
            for (var i = 0; i < cmds.length; i++) {
                var res;
                var t = now();
                try { res = String(eval(cmds[i])); } catch (e) { res = 'LỖI ' + e; }
                post({ run: C.run, k: C.k, note: 'cmd ' + cmds[i] + ' → ' + res + ' (' + (now() - t) + ' ms)' });
            }
            setTimeout(poll, 700);
        };
        x.send(null);
    }

    document.addEventListener('DOMContentLoaded', function () {
        marks.dom = now() - t0;
        var t = now();
        marks.sw = document.documentElement.scrollWidth;   // buộc tính kiểu + dàn trang
        marks.layout = now() - t;
        marks.bsw = document.body ? document.body.scrollWidth : -1;
    }, false);

    window.addEventListener('load', function () {
        marks.load = now() - t0;
        var t = now();
        var cols = GB.columns();
        marks.count = now() - t;
        var im = checkImages();
        var rep = {
            run: C.run, k: C.k, dom: marks.dom, layout: marks.layout, load: marks.load, count: marks.count,
            sw: marks.sw, bsw: marks.bsw, cols: cols, endCol: cols - 1, screens: Math.ceil(cols / COLS),
            imgs: im.n, imgBad: im.bad.length, note: im.bad.slice(0, 4).join(' | '),
            vw: window.innerWidth, vh: window.innerHeight, ua: navigator.userAgent
        };
        if (C.plan === 'perf') {
            post(rep, function () { setTimeout(function () { location.replace(C.next); }, 30); });
        } else {
            GB.goTo(C.page || 0);
            rep.note = 'đứng ở trang ' + GB.page + '/' + rep.screens + ' ' + rep.note;
            post(rep);
            poll();
        }
    }, false);
})();
