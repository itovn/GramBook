"""Biểu tượng GramBook kiểu kính mờ macOS Tahoe (Liquid Glass) từ ảnh mẫu của anh — chỉ dùng Pillow.

    python3 glass_icon.py <ảnh mẫu> <ra.png> [debug]

1. Vùng ô biểu tượng trong ảnh mẫu (950 × 1021, góc bo ~200 px) → mô hình nền B: mỗi hàng nội suy tuyến tính giữa màu trung bình
   dải sát mép trái và dải sát mép phải (nền là dải màu dọc + quầng sáng giữa).
2. Độ khác |ảnh − B| → alpha tiền cảnh (sách, máy bay, vệt bay, quầng sáng).
3. Tấm kính mới P: dải màu dọc theo đúng bảng màu mẫu, các vùng sáng khuếch tán (kính mờ), hạt mờ, bóng sáng chéo, viền kính sáng
   (mạnh ở góc trên trái và dưới phải — ánh sáng khúc xạ ở mép), mép trong tối nhẹ phía dưới (độ dày kính).
4. Ghép: ra = ảnh + (1 − alpha)·(P − B) (thay nền, giữ tiền cảnh và quầng sáng), bóng đổ mềm dưới tiền cảnh, vệt bóng trên cùng.
5. Góc bo ≈ mặt nạ của iOS 5 / 6 (16,5 %) — iOS luôn tự bo, không vuông hơn được.
"""
import sys
from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageOps

S = 1024
RADIUS = 169                       # ≈ 16,5 % — trùng mặt nạ biểu tượng của iOS 5 / 6
REG = (120, 834, 1070, 1855)       # ô biểu tượng trong ảnh mẫu (950 × 1021)
BAND_L = (40, 90)                  # dải nền sát mép trái / phải (toạ độ trong vùng) — ngoài sách / máy bay
BAND_R = (860, 910)
BOX = (60, 100, 890, 920)          # khung chứa tiền cảnh (trong vùng, nằm trọn trong ô bo góc của mẫu)
SCALE = 1.08                       # phóng tiền cảnh lên tấm 1024
FEATHER = 56                       # mép mờ của khung ghép (px sau khi phóng)


def stretch(img, size):
    """Kéo ảnh (dải 1 px) ra cỡ `size` — Pillow 12 lỗi khi resize NEAREST từ ảnh rộng / cao 1 px."""
    w, h = img.size
    return img.transform(size, Image.EXTENT, (0, 0, w, h), Image.NEAREST)


def ramp_mask(w, h, x0, x1, horizontal=True):
    """Mặt nạ L: 0 trước x0, tăng tuyến tính tới 255 ở x1 (theo chiều ngang hoặc dọc)."""
    n = w if horizontal else h
    vals = [max(0, min(255, int(round(255 * (i - x0) / max(1, x1 - x0))))) for i in range(n)]
    line = Image.new("L", (n, 1))
    line.putdata(vals)
    if horizontal:
        return stretch(line, (w, h))
    return stretch(line.transpose(Image.ROTATE_270).transpose(Image.FLIP_LEFT_RIGHT), (w, h))


def band_profile(img, x0, x1):
    """Màu trung bình từng hàng của dải cột [x0, x1)."""
    return img.crop((x0, 0, x1, img.size[1])).resize((1, img.size[1]), Image.BOX)


def rounded(size, radius, aa=4):
    big = Image.new("L", (size * aa, size * aa), 0)
    ImageDraw.Draw(big).rounded_rectangle((0, 0, size * aa - 1, size * aa - 1), radius=radius * aa, fill=255)
    return big.resize((size, size), Image.LANCZOS)


def vgradient(size, stops):
    """Dải màu dọc nhiều mốc: stops = [(vị trí 0…1, (r, g, b))…]."""
    w, h = size
    col = Image.new("RGB", (1, h))
    px = []
    for y in range(h):
        t = y / (h - 1)
        for (p0, c0), (p1, c1) in zip(stops, stops[1:]):
            if p0 <= t <= p1:
                k = (t - p0) / max(1e-6, p1 - p0)
                k = k * k * (3 - 2 * k)  # mượt
                px.append(tuple(int(round(c0[i] + (c1[i] - c0[i]) * k)) for i in range(3)))
                break
    col.putdata(px)
    return stretch(col, (w, h))


def blob(size, box, color, alpha, blur):
    layer = Image.new("RGBA", size, color + (0,))
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).ellipse(box, fill=alpha)
    layer.putalpha(m.filter(ImageFilter.GaussianBlur(blur)))
    return layer


def main():
    src = Image.open(sys.argv[1]).convert("RGB")
    out_path = sys.argv[2]
    debug = len(sys.argv) > 3
    R = src.crop(REG)
    W, H = R.size

    # 1. Mô hình nền B
    L = stretch(band_profile(R, *BAND_L), (W, H))
    Rr = stretch(band_profile(R, *BAND_R), (W, H))
    cl, cr = (BAND_L[0] + BAND_L[1]) / 2, (BAND_R[0] + BAND_R[1]) / 2
    B = Image.composite(Rr, L, ramp_mask(W, H, cl, cr))
    prof = band_profile(R, *BAND_L)
    for y in (110, 300, 510, 700, 910):
        print("nền hàng", y, "trái", prof.getpixel((0, y)), "phải", band_profile(R, *BAND_R).getpixel((0, y)))

    # 2. Alpha tiền cảnh
    d = ImageChops.difference(R, B).split()
    D = ImageChops.lighter(d[0], ImageChops.lighter(d[1], d[2]))
    A = D.point(lambda v: max(0, min(255, int((v - 10) * 255 / 45)))).filter(ImageFilter.GaussianBlur(1.2))

    # 3. Cắt khung tiền cảnh, phóng
    bw, bh = BOX[2] - BOX[0], BOX[3] - BOX[1]
    tw, th = int(round(bw * SCALE)), int(round(bh * SCALE))
    C = R.crop(BOX).resize((tw, th), Image.LANCZOS)
    Bb = B.crop(BOX).resize((tw, th), Image.BILINEAR)
    Ab = A.crop(BOX).resize((tw, th), Image.BILINEAR)
    ox, oy = (S - tw) // 2, (S - th) // 2

    # 4. Tấm kính P — bảng màu lấy từ nền mẫu (đỉnh / giữa / đáy), kéo dài ra cả tấm
    top = prof.getpixel((0, 110))
    mid = prof.getpixel((0, 510))
    bot = prof.getpixel((0, 910))
    lift = lambda c, k: tuple(min(255, int(v + (255 - v) * k)) for v in c)
    deep = lambda c, k: tuple(int(v * (1 - k)) for v in c)
    P = vgradient((S, S), [(0, lift(top, 0.12)), (0.08, top), (0.5, mid), (0.9, bot), (1, deep(bot, 0.12))]).convert("RGBA")
    # kính mờ: ánh sáng khuếch tán (lam nhạt trên trái, tím dưới phải, sáng mờ giữa)
    P = Image.alpha_composite(P, blob((S, S), (-260, -300, 620, 520), (170, 235, 255), 120, 150))
    P = Image.alpha_composite(P, blob((S, S), (520, 600, 1300, 1320), (150, 110, 255), 90, 170))
    P = Image.alpha_composite(P, blob((S, S), (200, 260, 820, 860), (210, 235, 255), 50, 190))
    # hạt kính mờ (rất nhẹ)
    grain = Image.effect_noise((S, S), 22).filter(ImageFilter.GaussianBlur(0.7))
    gl = Image.new("RGBA", (S, S), (255, 255, 255, 0))
    gl.putalpha(grain.point(lambda v: max(0, v - 128) // 6))
    P = Image.alpha_composite(P, gl)
    # bóng đổ mềm dưới tiền cảnh (lớp kính nổi trên nền)
    sh = Image.new("L", (S, S), 0)
    sh.paste(Ab, (ox, oy + 20))
    sh = sh.filter(ImageFilter.GaussianBlur(22)).point(lambda v: v * 0.5)
    shadow = Image.new("RGBA", (S, S), (14, 16, 60, 0))
    shadow.putalpha(sh)
    P = Image.alpha_composite(P, shadow)

    # 5. Ghép tiền cảnh: ra = C + (1 − A)(P − B)
    Pb = P.convert("RGB").crop((ox, oy, ox + tw, oy + th))
    inv = ImageOps.invert(Ab).convert("RGB")
    pos = ImageChops.multiply(ImageChops.subtract(Pb, Bb), inv)
    neg = ImageChops.multiply(ImageChops.subtract(Bb, Pb), inv)
    O = ImageChops.subtract(ImageChops.add(C, pos), neg)
    fx = ramp_mask(tw, th, 0, FEATHER)
    fx = ImageChops.multiply(fx, fx.transpose(Image.FLIP_LEFT_RIGHT))
    fy = ramp_mask(tw, th, 0, FEATHER, horizontal=False)
    fy = ImageChops.multiply(fy, fy.transpose(Image.FLIP_TOP_BOTTOM))
    F = ImageChops.multiply(fx, fy)
    final_box = Image.composite(O, Pb, F)
    icon = P.copy()
    icon.paste(final_box, (ox, oy))

    # 6. Kính: vệt bóng trên, mép dưới tối nhẹ, viền sáng (mạnh ở góc trên trái / dưới phải), viền trong mềm
    sheen = Image.new("RGBA", (S, S), (255, 255, 255, 0))
    sm = Image.new("L", (S, S), 0)
    ImageDraw.Draw(sm).ellipse((-380, -760, S + 380, 360), fill=255)
    fade = vgradient((S, S), [(0, (95, 95, 95)), (0.35, (0, 0, 0)), (1, (0, 0, 0))]).convert("L")
    sheen.putalpha(ImageChops.multiply(sm.filter(ImageFilter.GaussianBlur(30)), fade))
    icon = Image.alpha_composite(icon, sheen)
    inner = Image.new("RGBA", (S, S), (10, 8, 50, 0))
    im = Image.new("L", (S, S), 0)
    ImageDraw.Draw(im).rounded_rectangle((0, 0, S - 1, S - 1), radius=RADIUS, outline=255, width=40)
    im = ImageChops.multiply(im.filter(ImageFilter.GaussianBlur(26)), vgradient((S, S), [(0, (0, 0, 0)), (0.6, (0, 0, 0)), (1, (110, 110, 110))]).convert("L"))
    inner.putalpha(im)
    icon = Image.alpha_composite(icon, inner)
    # viền kính: vẽ to 2 lần cho nét mịn
    A2 = 2
    rim = Image.new("L", (S * A2, S * A2), 0)
    ImageDraw.Draw(rim).rounded_rectangle((3 * A2, 3 * A2, (S - 4) * A2, (S - 4) * A2), radius=(RADIUS - 3) * A2, outline=255, width=5 * A2)
    rim = rim.resize((S, S), Image.LANCZOS)
    # cường độ theo đường chéo: sáng ở góc trên trái và dưới phải, mờ ở hai góc kia
    # (tính thẳng theo t = (x + y) / 2S trên lưới 256 rồi phóng — xoay ảnh dải màu để lại đường nối ở góc)
    n = 256
    kv = []
    for y in range(n):
        for x in range(n):
            t = (x + y) / (2.0 * (n - 1))
            kv.append(int(70 + 185 * (abs(t - 0.5) * 2) ** 1.4))
    k = Image.new("L", (n, n))
    k.putdata(kv)
    k = k.resize((S, S), Image.BILINEAR)
    rim_a = ImageChops.multiply(rim, k)
    rim_l = Image.new("RGBA", (S, S), (235, 248, 255, 0))
    rim_l.putalpha(rim_a)
    icon = Image.alpha_composite(icon, rim_l)
    glow = Image.new("L", (S, S), 0)
    ImageDraw.Draw(glow).rounded_rectangle((8, 8, S - 9, S - 9), radius=RADIUS - 8, outline=255, width=18)
    glow = ImageChops.multiply(glow.filter(ImageFilter.GaussianBlur(14)), k).point(lambda v: v * 0.45)
    gl2 = Image.new("RGBA", (S, S), (200, 235, 255, 0))
    gl2.putalpha(glow)
    icon = Image.alpha_composite(icon, gl2)

    # 7. Bo góc
    icon.putalpha(ImageChops.multiply(icon.getchannel("A"), rounded(S, RADIUS)))
    icon.save(out_path)
    if debug:
        A.save(out_path.replace(".png", "-alpha.png"))
        B.save(out_path.replace(".png", "-bg.png"))
        P.save(out_path.replace(".png", "-plate.png"))
    print("đã ghi", out_path)


if __name__ == "__main__":
    main()
