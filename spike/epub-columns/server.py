#!/usr/bin/env python3
"""Bước 0 (DECISIONS B7): thử chia trang EPUB bằng cột CSS trên WebKit của iOS 5 / 6.

Máy chủ nhỏ làm đúng việc lõi Rust của GramBook sẽ làm: lấy chương thẳng trong tệp .epub (zip), chèn viewport + reader.css +
reader.js vào đầu <head>, trả application/xhtml+xml nếu chương đúng cú pháp XML (không thì text/html). Safari / UIWebView trên
máy thử chạy lần lượt từng chương theo «kế hoạch», reader.js đo rồi gửi về /report.

    python3 server.py <thư mục .epub> [cổng]        (mặc định 8765)

Trên máy thử: uiopen "http://<máy này>:8765/start?plan=perf"
    /start?plan=perf|one&b=<một phần tên sách>&fs=20&lh=1.5&ms=40&mt=40&mb=40&cols=1&t=day|sepia|night&f=<font>|book&st=gb|pub
    /fonts                      bảng thử font tiếng Việt
    /cmd?js=<mã JS>             gửi lệnh cho trang đang mở (chế độ «one»: trang hỏi lệnh 0,7 giây / lần)
"""
import html
import json
import os
import re
import sys
import threading
import time
import urllib.parse
import xml.etree.ElementTree as ET
import zipfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BOOKS_DIR = sys.argv[1]
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8765
HERE = os.path.dirname(os.path.abspath(__file__))
RESULTS = os.path.join(HERE, 'results.jsonl')

# Sách cho kế hoạch «perf»: chương rất lớn (Word HTML), sách 4 MB có chú thích, sách thường, sách nhiều ảnh, sách 109 chương, sách
# rất nhỏ.
PERF_BOOKS = ['Bê Trọc', 'Hồi kí Nguyễn Hiến Lê', 'Ai Cập Huyền Bí', 'NƯỚC MỸ', 'BẢY NGÀY TRONG ĐỒNG THÁP MƯỜI',
              'Tổ quốc ăn năn', 'Biển Có Màu Gì', 'Nhật ký Anne Frank']

FONTS = ['Georgia', 'Palatino', 'Times New Roman', 'Baskerville', 'Hoefler Text', 'Cochin', 'Didot', 'Bodoni 72',
         'Helvetica Neue', 'Helvetica', 'Arial', 'Verdana', 'Trebuchet MS', 'Optima', 'Gill Sans', 'Avenir', 'Avenir Next',
         'Futura', 'American Typewriter', 'Courier New', 'Literata', 'Merriweather', 'Noto Serif']


def local(tag):
    return tag.rsplit('}', 1)[-1]


class Book:
    def __init__(self, bid, path):
        self.id = bid
        self.path = path
        self.file = os.path.basename(path)
        self.lock = threading.Lock()
        z = zipfile.ZipFile(path)
        self.names = set(z.namelist())
        cont = ET.fromstring(z.read('META-INF/container.xml'))
        opf_path = [e for e in cont.iter() if local(e.tag) == 'rootfile'][0].get('full-path')
        opf = ET.fromstring(z.read(opf_path))
        base = opf_path.rsplit('/', 1)[0] + '/' if '/' in opf_path else ''
        manifest = {}
        self.title = self.file
        for e in opf.iter():
            if local(e.tag) == 'item':
                manifest[e.get('id')] = base + urllib.parse.unquote(e.get('href'))
            elif local(e.tag) == 'title' and e.text:
                self.title = e.text.strip()
        self.spine = [manifest[e.get('idref')] for e in opf.iter()
                      if local(e.tag) == 'itemref' and e.get('idref') in manifest and e.get('linear') != 'no']
        self.zip = z

    def read(self, name):
        with self.lock:
            return self.zip.read(name)


BOOKS = []
for i, f in enumerate(sorted(x for x in os.listdir(BOOKS_DIR) if x.lower().endswith('.epub'))):
    try:
        BOOKS.append(Book(i, os.path.join(BOOKS_DIR, f)))
    except Exception as e:  # sách hỏng: bỏ qua, báo
        print('bỏ qua', f, e, flush=True)


def find_book(part):
    part = part.lower()
    for b in BOOKS:
        if part in b.file.lower():
            return b
    return None


RUNS = {}
CMDS = []  # lệnh JS chờ trang «one» lấy
RUN_SEQ = [0]
LOCK = threading.Lock()


def new_run(q):
    def num(k, d):
        try:
            return float(q.get(k, [d])[0])
        except ValueError:
            return d
    w, h = int(num('w', 768)), int(num('h', 1004))
    layout = {
        'w': w, 'h': h, 'fs': num('fs', 20), 'lh': num('lh', 1.5), 'ms': int(num('ms', 40)), 'mt': int(num('mt', 40)),
        'mb': int(num('mb', 40)), 'cols': int(num('cols', 1)), 't': q.get('t', ['day'])[0], 'f': q.get('f', ['Georgia'])[0],
        'st': q.get('st', ['gb'])[0],
    }
    plan = q.get('plan', ['one'])[0]
    steps = []
    if plan == 'perf':
        for part in PERF_BOOKS:
            b = find_book(part)
            if b:
                steps += [(b.id, i) for i in range(len(b.spine))]
    else:
        b = find_book(q.get('b', ['Ai Cập'])[0]) or BOOKS[0]
        ch = int(num('ch', 2))
        steps = [(b.id, min(ch, len(b.spine) - 1))]
    with LOCK:
        RUN_SEQ[0] += 1
        rid = 'r%d' % RUN_SEQ[0]
        RUNS[rid] = {'layout': layout, 'plan': plan, 'steps': steps, 'page': int(num('p', 0)), 'served': {}}
    return rid


def step_url(rid, k):
    run = RUNS[rid]
    if k >= len(run['steps']):
        return '/done?run=' + rid
    bid, ch = run['steps'][k]
    return '/r/%s/%d/%s' % (rid, k, urllib.parse.quote(BOOKS[bid].spine[ch]))


def reader_css(L):
    """Cùng cách iBooks (user_stylesheet_flowable.css.tmpl): <html> là khung nhiều cột, cao đúng một trang, các cột tràn sang phải."""
    pw = L['w'] / L['cols']          # một «trang» (cột + lề hai bên)
    cw = pw - 2 * L['ms']            # bề rộng chữ của một cột
    ch = L['h'] - L['mt'] - L['mb']  # bề cao chữ
    theme = {'day': ('#ffffff', '#1a1a1a'), 'sepia': ('#f4ecd8', '#5b4636'), 'night': ('#121212', '#c8c8c8')}[L['t']]
    font = '' if L['f'] == 'book' else "font-family: '%s', 'Courier New' !important;" % L['f']
    faces = ''
    if L['f'] in ('Literata', 'Noto Serif'):
        faces = "@font-face { font-family: '%s'; src: url('/font/%s.ttf'); }\n" % (L['f'], L['f'].replace(' ', ''))
    css = faces + """@namespace epub "http://www.idpf.org/2007/ops";
html {
  margin: 0 !important;
  padding: %(mt)dpx %(ms)dpx %(mb)dpx %(ms)dpx !important;
  -webkit-box-sizing: border-box !important;
  width: %(w)dpx !important;
  height: %(h)dpx !important;
  -webkit-column-width: %(cw)dpx !important;
  -webkit-column-gap: %(gap)dpx !important;
  -webkit-text-size-adjust: none;
  -webkit-touch-callout: none;
  background-color: %(bg)s !important;
}
body {
  margin: 0 !important;
  padding: 0 !important;
  word-wrap: break-word;
  color: %(fg)s;
}
img, svg, video {
  max-width: 100%% !important;
  max-height: %(imgh)dpx !important;
  -webkit-column-break-inside: avoid;
}
img { width: auto !important; height: auto !important; }
/* ảnh đứng riêng: bỏ khoảng chân chữ dưới ảnh → dòng chứa ảnh không cao hơn trang (ảnh cao gần bằng trang bị tràn đáy — B7) */
div > img:only-child, p > img:only-child, a > img:only-child, div > a:only-child > img, p > a:only-child > img,
center > img { vertical-align: bottom !important; }
table { max-width: 100%% !important; }
pre { white-space: pre-wrap !important; }
a { text-decoration: none; }
aside[epub|type~="footnote"], aside[epub|type~="endnote"], aside[epub|type~="rearnote"] { display: none !important; }
h1, h2, h3, h4, h5, h6, img, table { -webkit-column-break-inside: avoid; }
""" % dict(L, cw=cw, gap=2 * L['ms'], imgh=int(ch * 0.96), bg=theme[0], fg=theme[1])
    if L['st'] == 'gb':
        # «Kiểu của GramBook»: cỡ chữ / giãn dòng / font của người đọc thắng mọi kiểu ghi sẵn trong thẻ (Word HTML của vnthuquan:
        # style="LINE-HEIGHT: 150%", <font face=…>, <span style="FONT-SIZE: 13pt">); h1–h6, sup, sub, small, big giữ tỉ lệ.
        css += """body { font-size: %(fs)gpx !important; line-height: %(lh)g !important; text-align: justify !important; %(font)s }
body p, body div, body span, body font, body li, body td, body th, body blockquote, body a, body em, body i, body b,
body strong, body u, body center, body dd, body dt, body cite, body q, body label {
  font-size: inherit !important; line-height: inherit !important; font-family: inherit !important;
}
""" % dict(L, font=font)
    if L['t'] == 'night':
        css += """html *, body * { color: %s !important; background-color: transparent !important; border-color: #444 !important; }
a:link, a:visited, a:link *, a:visited * { color: #8A8EE4 !important; }
""" % theme[1]
    return css


def inject(data, rid, k, L, path):
    run = RUNS[rid]
    cfg = {'run': rid, 'k': k, 'layout': L, 'plan': run['plan'], 'next': step_url(rid, k + 1), 'page': run['page'],
           'path': path, 'book': run['steps'][k][0] if k < len(run['steps']) else -1}
    js = json.dumps(cfg, ensure_ascii=True).replace('<', '\\u003c').replace('>', '\\u003e').replace('&', '\\u0026')
    snippet = ('<meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no"/>'
               '<link rel="stylesheet" type="text/css" href="/gb/reader.css?run=%s"/>'
               '<script type="text/javascript">window.__t0=Date.now();window.__GB=%s;</script>'
               '<script type="text/javascript" src="/gb/reader.js"></script>') % (rid, js)
    s = data.decode('utf-8', 'replace')
    m = re.search(r'<head(\s[^>]*)?>', s, re.I)
    if m:
        s = s[:m.end()] + snippet + s[m.end():]
    else:
        m = re.search(r'<html(\s[^>]*)?>', s, re.I)
        pos = m.end() if m else 0
        s = s[:pos] + '<head>' + snippet + '</head>' + s[pos:]
    return s.encode('utf-8')


CTYPES = {'css': 'text/css', 'jpg': 'image/jpeg', 'jpeg': 'image/jpeg', 'png': 'image/png', 'gif': 'image/gif',
          'svg': 'image/svg+xml', 'ttf': 'application/x-font-ttf', 'otf': 'application/x-font-opentype', 'js': 'text/javascript',
          'xpgt': 'application/xml', 'ncx': 'application/xml'}


def fonts_page(start=0, count=99):
    sample = ('Người du khách cuối cùng đã về; QUYỂN THỨ NHẤT — Ỡ Ặ Ữ Ẫ Ổ Ợ Ẩ Ỷ · ỡ ặ ữ ẫ ổ ợ ẩ ỷ ự ừ ờ · Đ đ Ư ư Ơ ơ')
    rows = []
    for f in FONTS[start:start + count]:
        rows.append('<div class="r"><div class="n">%s</div><div style="font-family:\'%s\', \'Courier New\'; font-size:21px">%s'
                    '</div><div style="font-family:\'%s\', \'Courier New\'; font-size:15px">%s</div></div>'
                    % (html.escape(f), f, html.escape(sample), f, html.escape(sample)))
    faces = ''.join("@font-face { font-family: '%s'; src: url('/font/%s.ttf'); }" % (f, f.replace(' ', ''))
                    for f in ('Literata', 'Merriweather', 'Noto Serif'))
    return ('<!DOCTYPE html><html><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width, '
            'user-scalable=no"/><style>%s body{margin:12px 16px;background:#fff;color:#111} .r{margin:0 0 10px;'
            'border-bottom:1px solid #ddd;padding-bottom:6px} .n{font:11px Helvetica;color:#c00}</style></head><body>%s'
            '<script>%s</script></body></html>' % (faces, ''.join(rows), REMOTE_JS)).encode('utf-8')


def fontcheck_page():
    """Dò từng chữ cái tiếng Việt trong từng font: đo bề rộng chữ với hai font dự phòng khác hẳn nhau — bằng nhau thì font có chữ đó."""
    faces = ''.join("@font-face { font-family: '%s'; src: url('/font/%s.ttf'); }" % (f, f.replace(' ', ''))
                    for f in ('Literata', 'Noto Serif'))
    js = r'''
var R=[[0xC0,0xC3],[0xC8,0xCA],[0xCC,0xCD],[0xD2,0xD5],[0xD9,0xDA],[0xDD,0xDD],[0xE0,0xE3],[0xE8,0xEA],[0xEC,0xED],
[0xF2,0xF5],[0xF9,0xFA],[0xFD,0xFD],[0x102,0x103],[0x110,0x111],[0x128,0x129],[0x168,0x169],[0x1A0,0x1A1],[0x1AF,0x1B0],
[0x1EA0,0x1EF9]];
var CH=[];for(var i=0;i<R.length;i++)for(var c=R[i][0];c<=R[i][1];c++)CH.push(String.fromCharCode(c));
var FONTS=%s;
function w(font,fb,s){var e=document.createElement("span");e.style.cssText="font-family:'"+font+"','"+fb+"';font-size:40px;white-space:nowrap;position:absolute;left:0;top:0";
e.appendChild(document.createTextNode(s));document.body.appendChild(e);var x=e.offsetWidth;document.body.removeChild(e);return x;}
function run(){var out=[];for(var f=0;f<FONTS.length;f++){var miss=[];for(var k=0;k<CH.length;k++){var s=CH[k]+CH[k]+CH[k];
if(w(FONTS[f],"Courier New",s)!==w(FONTS[f],"Zapfino",s))miss.push(CH[k]);}
out.push(FONTS[f]+": "+(miss.length===CH.length?"KHÔNG CÓ FONT":(miss.length?"thiếu "+miss.length+"/"+CH.length+" "+miss.join(""):"đủ "+CH.length)));}
var x=new XMLHttpRequest();x.open("POST","/report",true);x.setRequestHeader("Content-Type","application/json");
x.send(JSON.stringify({run:"fontcheck",k:0,note:"\n"+out.join("\n")}));document.body.innerHTML="<pre>"+out.join("\n")+"</pre>";}
window.addEventListener("load",function(){setTimeout(run,2500)},false);
''' % json.dumps(FONTS)
    return ('<!DOCTYPE html><html><head><meta charset="utf-8"/><style>%s</style></head><body>đang dò…<span style="font-family:Literata">a'
            '</span><span style="font-family:\'Noto Serif\'">a</span><script>%s</script><script>%s</script></body></html>'
            % (faces, js, REMOTE_JS)).encode('utf-8')


# Điều khiển từ xa trang đang mở trên máy thử (uiopen mở thêm thẻ mới mỗi lần → dùng /cmd để đi tiếp trong cùng thẻ)
REMOTE_JS = ('(function p(){var x=new XMLHttpRequest();x.open("GET","/cmd?_="+Date.now(),true);x.onreadystatechange=function(){'
             'if(x.readyState!==4)return;var c=[];try{c=JSON.parse(x.responseText)}catch(e){}for(var i=0;i<c.length;i++){'
             'try{eval(c[i])}catch(e){}}setTimeout(p,700)};x.send(null)})();')


class H(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.0'

    def log_message(self, fmt, *args):
        pass

    def send(self, code, body, ctype='text/plain; charset=utf-8', extra=None):
        self.send_response(code)
        self.send_header('Content-Type', ctype)
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Cache-Control', 'no-store' if ctype.startswith(('application/json', 'text/plain')) else 'max-age=60')
        for k, v in (extra or {}).items():
            self.send_header(k, v)
        self.end_headers()
        if self.command != 'HEAD':
            self.wfile.write(body)

    def do_HEAD(self):
        self.do_GET()

    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        q = urllib.parse.parse_qs(u.query)
        p = u.path
        if p == '/ping':
            print('ping từ', self.client_address[0], self.headers.get('User-Agent'), flush=True)
            return self.send(200, b'pong')
        if p == '/start':
            # đo khung nhìn thật rồi mới lập kế hoạch
            qs = u.query
            body = ('<!DOCTYPE html><html><head><meta name="viewport" content="width=device-width, initial-scale=1.0, '
                    'maximum-scale=1.0, user-scalable=no"/></head><body><script>location.replace("/go?%s&w="+innerWidth+'
                    '"&h="+innerHeight);</script></body></html>' % qs.replace('"', '')).encode('utf-8')
            return self.send(200, body, 'text/html; charset=utf-8')
        if p == '/go':
            rid = new_run(q)
            run = RUNS[rid]
            print('== %s %s %d bước, khung %sx%s' % (rid, run['plan'], len(run['steps']), run['layout']['w'],
                                                     run['layout']['h']), flush=True)
            return self.send(302, b'', extra={'Location': step_url(rid, 0)})
        if p == '/done':
            print('== xong', q.get('run'), flush=True)
            return self.send(200, ('<html><body><h1>xong</h1><script>%s</script></body></html>' % REMOTE_JS).encode('utf-8'),
                             'text/html; charset=utf-8')
        if p == '/fonts':
            return self.send(200, fonts_page(int(q.get('from', ['0'])[0]), int(q.get('n', ['99'])[0])),
                             'text/html; charset=utf-8')
        if p == '/fontcheck':
            return self.send(200, fontcheck_page(), 'text/html; charset=utf-8')
        if p.startswith('/font/'):
            fp = os.path.join(HERE, 'fonts', os.path.basename(p))
            if os.path.exists(fp):
                return self.send(200, open(fp, 'rb').read(), 'application/x-font-ttf')
            return self.send(404, b'')
        if p == '/gb/reader.js':
            return self.send(200, open(os.path.join(HERE, 'reader.js'), 'rb').read(), 'text/javascript; charset=utf-8')
        if p == '/gb/reader.css':
            run = RUNS.get(q.get('run', [''])[0])
            if not run:
                return self.send(404, b'')
            return self.send(200, reader_css(run['layout']).encode('utf-8'), 'text/css; charset=utf-8')
        if p == '/cmd':
            if 'js' in q:
                with LOCK:
                    CMDS.append(q['js'][0])
                return self.send(200, b'queued')
            with LOCK:
                out = list(CMDS)
                CMDS.clear()
            return self.send(200, json.dumps(out).encode('utf-8'), 'application/json')
        m = re.match(r'^/r/([^/]+)/(\d+)/(.+)$', p)
        if m:
            rid, k, path = m.group(1), int(m.group(2)), urllib.parse.unquote(m.group(3))
            run = RUNS.get(rid)
            if not run or k >= len(run['steps']):
                return self.send(404, b'no run')
            book = BOOKS[run['steps'][k][0]]
            if path not in book.names:
                return self.send(404, b'not in book')
            data = book.read(path)
            ext = path.rsplit('.', 1)[-1].lower()
            if ext in ('html', 'htm', 'xhtml', 'xml'):
                try:
                    ET.fromstring(data)
                    ctype = 'application/xhtml+xml; charset=utf-8'
                except ET.ParseError:
                    ctype = 'text/html; charset=utf-8'
                run['served'][k] = (time.time(), len(data), ctype.split(';')[0])
                return self.send(200, inject(data, rid, k, run['layout'], path), ctype)
            return self.send(200, data, CTYPES.get(ext, 'application/octet-stream'))
        if p == '/':
            rows = ''.join('<li>%d · %s · %d chương</li>' % (b.id, html.escape(b.title), len(b.spine)) for b in BOOKS)
            return self.send(200, ('<html><head><meta charset="utf-8"/></head><body><ul>%s</ul></body></html>' % rows)
                             .encode('utf-8'), 'text/html; charset=utf-8')
        return self.send(404, b'?')

    def do_POST(self):
        if self.path != '/report':
            return self.send(404, b'')
        n = int(self.headers.get('Content-Length', '0'))
        rep = json.loads(self.rfile.read(n).decode('utf-8'))
        run = RUNS.get(rep.get('run'))
        if rep.get('run') == 'fontcheck':
            print('== dò font' + rep.get('note', ''), flush=True)
            return self.send(200, b'ok')
        if run:
            served = run['served'].get(rep.get('k'))
            if served:
                rep['server_ms'] = int((time.time() - served[0]) * 1000)
                rep['bytes'] = served[1]
                rep['ctype'] = served[2]
            bid, ch = run['steps'][rep['k']]
            rep['file'] = BOOKS[bid].file
            rep['ch'] = ch
        with LOCK:
            with open(RESULTS, 'a', encoding='utf-8') as f:
                f.write(json.dumps(rep, ensure_ascii=False) + '\n')
        print('%s #%s %-28.28s ch%-3s %8s B tổng %5s ms | dom %5s layout %5s load %5s | cột %4s (sw %s, cuối %s) màn %4s | '
              'ảnh %s hỏng %s %s' % (rep.get('run'), rep.get('k'), rep.get('file', ''), rep.get('ch'), rep.get('bytes'),
                                     rep.get('server_ms'), rep.get('dom'), rep.get('layout'), rep.get('load'),
                                     rep.get('cols'), rep.get('sw'), rep.get('endCol'), rep.get('screens'), rep.get('imgs'),
                                     rep.get('imgBad'), rep.get('note', '')), flush=True)
        return self.send(200, b'ok')


if __name__ == '__main__':
    print('GramBook bước 0 — %d sách, cổng %d' % (len(BOOKS), PORT), flush=True)
    ThreadingHTTPServer(('0.0.0.0', PORT), H).serve_forever()
