"""Biểu tượng GramBook theo mẫu anh đưa (Tham_Khao/logo/logo1.png — ảnh chụp màn hình iPhone 1320 × 2868): quyển sách mở phát
sáng, máy bay giấy bay ra từ trang sách, nền xanh lam → chàm. Ô biểu tượng trong ảnh mẫu cao hơn rộng (950 × 1021) → cắt ô vuông
944 px (lùi 3 px vào trong mép trái / phải, bớt lề trên / dưới — sách và máy bay vẫn cách mép ~110 px), bo góc lại như mẫu.

    python tools/icon/neon_icon.py Tham_Khao/logo/logo2.png tools/icon/icon-1024.png    # bản đang dùng: mẫu 2, ô kính viền neon (B15)
    python tools/icon/glass_icon.py Tham_Khao/logo/logo1.png tools/icon/icon-1024.png   # (mẫu 1: nền kính mờ kiểu macOS Tahoe — B12)
    python tools/icon/make_icon.py from-ref Tham_Khao/logo/logo1.png 123 873 944   # (bản đầu: cắt nguyên ô của mẫu) → icon-1024.png
    python tools/icon/make_icon.py app/Resources                                    # các cỡ iOS 5 / 6 từ icon-1024.png
    python tools/icon/make_icon.py draw                                             # (mẫu tạm vẽ bằng mã — trước khi có mẫu của anh)

Góc ngoài để trong suốt (SpringBoard iOS 5 / 6 giữ kênh alpha; bán kính góc ≈ 20 % — lớn hơn mặt nạ của iOS ≈ 17,5 % nên iOS không
cắt vào viền). `UIPrerenderedIcon` = có → iOS không phủ thêm vệt bóng (đã vẽ sẵn). Cần Pillow."""
import os
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageOps

HERE = os.path.dirname(os.path.abspath(__file__))
MASTER = os.path.join(HERE, "icon-1024.png")
S = 1024
RADIUS = 205

SIZES = {"Icon.png": 57, "Icon@2x.png": 114, "Icon-72.png": 72, "Icon-72@2x.png": 144, "Icon-Small.png": 29,
         "Icon-Small@2x.png": 58, "Icon-Small-50.png": 50, "Icon-Small-50@2x.png": 100}


def rounded_mask(size, radius, aa=4):
    """Mặt nạ ô bo góc, khử răng cưa (vẽ to `aa` lần rồi thu nhỏ)."""
    big = Image.new("L", (size * aa, size * aa), 0)
    ImageDraw.Draw(big).rounded_rectangle((0, 0, size * aa - 1, size * aa - 1), radius=radius * aa, fill=255)
    return big.resize((size, size), Image.LANCZOS)


def quad(p0, c, p1, n=48):
    """Các điểm trên đường cong bậc hai p0 → p1 (điểm điều khiển c)."""
    pts = []
    for i in range(n + 1):
        t = i / n
        x = (1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * c[0] + t * t * p1[0]
        y = (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * c[1] + t * t * p1[1]
        pts.append((x, y))
    return pts


def page(side, dy=0, grow=0):
    """Một trang sách mở (side −1 trái / +1 phải), toạ độ 1024: mép trên / dưới cong lên từ gáy ra ngoài như cánh chim."""
    cx = 512
    ox = cx + side * (342 + grow)          # mép ngoài
    top = quad((cx, 452 + dy), (cx + side * 175, 372 + dy), (ox, 412 + dy))
    bot = quad((ox, 722 + dy + grow), (cx + side * 175, 688 + dy + grow), (cx, 762 + dy + grow))
    return top + bot


def scaled(pts, k):
    return [(x * k, y * k) for x, y in pts]


def draw():
    A = 2                                   # vẽ to gấp đôi rồi thu nhỏ (khử răng cưa)
    W = S * A
    # 1. Nền kính xanh: chuyển màu dọc + sọc chéo mờ + quầng sáng giữa
    grad = Image.linear_gradient("L").resize((W, W))
    bg = ImageOps.colorize(grad, black=(70, 176, 242), white=(8, 62, 140)).convert("RGBA")
    stripes = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(stripes)
    for k in range(-W, 2 * W, 70 * A):
        d.line([(k, 0), (k - W, W)], fill=(255, 255, 255, 16), width=22 * A)
    bg = Image.alpha_composite(bg, stripes)
    glow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse([200 * A, 300 * A, 824 * A, 880 * A], fill=(190, 230, 255, 110))
    bg = Image.alpha_composite(bg, glow.filter(ImageFilter.GaussianBlur(90 * A)))

    # 2. Quyển sách: bìa cứng xanh thẫm → hai lớp gáy giấy → trang giấy kem + dòng chữ
    book = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(book)
    shadow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).polygon(scaled(page(-1, 34, 26), A), fill=(0, 20, 60, 120))
    ImageDraw.Draw(shadow).polygon(scaled(page(1, 34, 26), A), fill=(0, 20, 60, 120))
    book = Image.alpha_composite(book, shadow.filter(ImageFilter.GaussianBlur(16 * A)))
    d = ImageDraw.Draw(book)
    for side in (-1, 1):
        d.polygon(scaled(page(side, 22, 22), A), fill=(13, 52, 110, 255))      # bìa cứng
    for side in (-1, 1):
        d.polygon(scaled(page(side, 14, 8), A), fill=(176, 196, 220, 255))     # gáy giấy dưới
        d.polygon(scaled(page(side, 7, 4), A), fill=(214, 226, 240, 255))      # gáy giấy trên
        d.polygon(scaled(page(side), A), fill=(255, 253, 246, 255))            # trang
    # bóng gáy giữa hai trang
    spine = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    ImageDraw.Draw(spine).line(scaled([(512, 452), (512, 762)], A), fill=(120, 140, 170, 150), width=14 * A)
    book = Image.alpha_composite(book, spine.filter(ImageFilter.GaussianBlur(6 * A)))
    d = ImageDraw.Draw(book)
    # dòng chữ trên trang trái (đầy) và trang phải (nửa dưới — nửa trên dành cho đường bay)
    for side, rows in ((-1, range(7)), (1, range(3, 7))):
        for r in rows:
            dy = 70 + r * 36
            x0, x1 = 512 + side * 48, 512 + side * (300 if r % 3 != 2 else 230)
            pts = quad((x0, 452 + dy - 4), ((x0 + x1) / 2, 380 + dy + 14), (x1, 414 + dy))
            d.line(scaled(pts, A), fill=(170, 190, 212, 255), width=11 * A, joint="curve")

    # 3. Máy bay giấy bay ra từ trang phải + vệt chấm
    plane = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(plane)
    for i, (x, y) in enumerate(quad((610, 500), (600, 380), (700, 318), 9)[1:-1]):
        r = 9 + i * 0.6
        d.ellipse([(x - r) * A, (y - r) * A, (x + r) * A, (y + r) * A], fill=(255, 255, 255, 150 + i * 10))
    N, L, F, T, K = (862, 140), (582, 278), (700, 316), (716, 396), (744, 334)
    d.polygon(scaled([N, L, F], A), fill=(255, 255, 255, 255))               # cánh
    d.polygon(scaled([N, F, T], A), fill=(214, 233, 250, 255))               # thân
    d.polygon(scaled([F, T, K], A), fill=(150, 196, 236, 255))               # nếp gấp
    pshadow = plane.filter(ImageFilter.GaussianBlur(10 * A))
    pshadow = Image.merge("RGBA", (*[Image.new("L", (W, W), 0)] * 3, pshadow.getchannel("A").point(lambda v: v * 0.45)))
    pshadow = ImageChops.offset(pshadow, 6 * A, 12 * A)

    icon = Image.alpha_composite(bg, book)
    icon = Image.alpha_composite(icon, pshadow)
    icon = Image.alpha_composite(icon, plane)

    # 4. Vệt bóng kiểu iOS 6 (nửa trên) + viền bạc
    gloss = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    gm = Image.new("L", (W, W), 0)
    ImageDraw.Draw(gm).ellipse([-420 * A, -760 * A, 1444 * A, 470 * A], fill=255)
    fade = Image.linear_gradient("L").resize((W, W)).point(lambda v: int(max(0, 78 - v * 0.42)))
    gloss.putalpha(ImageChops.multiply(gm, fade))
    gloss = Image.merge("RGBA", (*[Image.new("L", (W, W), 255)] * 3, gloss.getchannel("A")))
    icon = Image.alpha_composite(icon, gloss)
    ring = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(ring)
    d.rounded_rectangle([3 * A, 3 * A, W - 3 * A, W - 3 * A], radius=RADIUS * A, outline=(70, 86, 104, 255), width=8 * A)
    d.rounded_rectangle([10 * A, 10 * A, W - 10 * A, W - 10 * A], radius=(RADIUS - 8) * A, outline=(232, 240, 248, 220), width=7 * A)
    icon = Image.alpha_composite(icon, ring)

    icon = icon.resize((S, S), Image.LANCZOS)
    icon.putalpha(ImageChops.multiply(icon.getchannel("A"), rounded_mask(S, RADIUS)))
    icon.save(MASTER)
    print("đã vẽ", MASTER)


def from_ref(path, x, y, side):
    src = Image.open(path).convert("RGBA")
    icon = src.crop((x, y, x + side, y + side)).resize((S, S), Image.LANCZOS)
    icon.putalpha(rounded_mask(S, RADIUS))
    icon.save(MASTER)
    print("đã cắt", MASTER)


def sizes(out):
    big = Image.open(MASTER).convert("RGBA")
    os.makedirs(out, exist_ok=True)
    for name, px in SIZES.items():
        img = big.resize((px, px), Image.LANCZOS)
        if px <= 72:
            # cỡ nhỏ: làm nét lại chút (dòng chữ, mép trang mảnh)
            a = img.getchannel("A")
            img = img.filter(ImageFilter.UnsharpMask(radius=1, percent=60, threshold=2))
            img.putalpha(a)
        img.save(os.path.join(out, name))
    print("đã tạo", len(SIZES), "cỡ trong", os.path.normpath(out))


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")  # bảng mã cp1252 của Windows không in được tiếng Việt
    if len(sys.argv) == 2 and sys.argv[1] == "draw":
        draw()
    elif len(sys.argv) == 6 and sys.argv[1] == "from-ref":
        from_ref(sys.argv[2], *map(int, sys.argv[3:6]))
    elif len(sys.argv) == 2:
        sizes(sys.argv[1])
    else:
        print(__doc__)
        sys.exit(2)
