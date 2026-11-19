"""Biểu tượng GramBook theo mẫu thứ hai của anh (Tham_Khao/logo/logo2.png, 2752 × 1536): ô kính viền neon (lam → tím → hồng cam,
mạch điện mảnh), màn hình chàm bên trong có quyển sách mở phát sáng, máy bay giấy bay ra, hạt sáng. Chỉ dùng Pillow.

    python tools/icon/neon_icon.py Tham_Khao/logo/logo2.png tools/icon/icon-1024.png
    python tools/icon/make_icon.py app/Resources                                       # các cỡ iOS 5 / 6

1. Ô biểu tượng trong mẫu: vuông x 713 – 2039, y 105 – 1433 (đo theo mép sáng của viền), nằm giữa ảnh; lấy lùi ~2 px vào trong để
   mép không dính nền tối của ảnh mẫu (dấu ✦ góc dưới phải của mẫu nằm ngoài ô).
2. Góc ô trong mẫu bo ≈ 25 % — tròn hơn mặt nạ iOS 5 / 6 (16,5 %, anh đã chọn ở B12). Nắn 4 góc: trong hộp góc K × K, mỗi tia từ
   điểm Q (góc trong của hộp) kéo dãn phần ngoài (dải viền + ít nền tối) để mép bo 25 % của mẫu khớp mép bo 16,5 %; phần trong
   (sách, máy bay) và hai cạnh hộp giữ nguyên → liền mạch với phần còn lại. Nắn bằng `Image.transform(MESH)` theo lưới 6 px.
3. Thu về 1024, mặt nạ bo 16,5 % trong suốt ngoài góc.
"""
import math
import sys

from PIL import Image, ImageChops, ImageDraw

S = 1024
RADIUS = 169                       # ≈ 16,5 % — trùng mặt nạ biểu tượng của iOS 5 / 6 (như glass_icon.py)
SRC = (715.0, 107.0, 1323.0)       # ô trong mẫu: trái, trên, cạnh (lùi 2 px vào trong mép sáng)
R_SRC = 334.0                      # bán kính góc ô trong mẫu (≈ 330 ở mép ngoài; góc dưới hơi tròn hơn → lấy 334 cho mép góc
                                   # không lẹm vào nền tối — 328 để lại viền tối 1 – 2 px ở hai góc dưới)
K = 560.0                          # hộp góc được nắn (px mẫu) — rộng hơn bán kính + dày viền (~156) + lề
BAND = 230.0                       # nắn dải ngoài cùng dày chừng này (viền ~156 + nền tối) dọc mỗi tia; phía trong giữ nguyên
CELL = 6                           # ô lưới MESH (px)


def edge_dist(r, a):
    """Khoảng cách từ Q = (K, K) theo tia hướng (−cos a, −sin a) tới mép ô bo góc bán kính r (góc ở gốc toạ độ)."""
    c, s = math.cos(a), math.sin(a)
    if c > 1e-9 and s / c <= (K - r) / K:
        return K / c                                      # cạnh trái (thẳng)
    if s > 1e-9 and c / s <= (K - r) / K:
        return K / s                                      # cạnh trên (thẳng)
    dd = -(K - r) * (c + s)                               # cung tròn tâm (r, r): nghiệm xa của |Q − C + t·d| = r
    return -dd + math.sqrt(dd * dd - 2 * (K - r) ** 2 + r * r)


def warp_local(u, v, r_dst):
    """Điểm (u, v) trong hộp góc (gốc ở góc ngoài) của ảnh đích → toạ độ tương ứng trong mẫu."""
    if u >= K or v >= K:
        return u, v
    wx, wy = K - u, K - v
    rho = math.hypot(wx, wy)
    if rho < 1e-9:
        return u, v
    a = math.atan2(wy, wx)
    l_dst, l_src = edge_dist(r_dst, a), edge_dist(R_SRC, a)
    rho0 = l_src - BAND
    if rho <= rho0:
        return u, v
    rho2 = rho0 + (rho - rho0) * (l_src - rho0) / (l_dst - rho0)
    rho2 = min(rho2, l_src - 3)    # ngoài mép (phần trong suốt): lấy màu viền, không lấy nền tối của mẫu → thu nhỏ không ám mép
    k = rho2 / rho
    return K - wx * k, K - wy * k


def warp(x, y, n, r_dst):
    """Toạ độ (x, y) của ảnh đích n × n → toạ độ trong ảnh mẫu (nắn góc nào thì lật về hộp góc trên trái)."""
    fx, fy = x > n / 2, y > n / 2
    u = n - x if fx else x
    v = n - y if fy else y
    u, v = warp_local(u, v, r_dst)
    x2 = n - u if fx else u
    y2 = n - v if fy else v
    return SRC[0] + x2 * SRC[2] / n, SRC[1] + y2 * SRC[2] / n


def rounded(size, radius, aa=4):
    big = Image.new("L", (size * aa, size * aa), 0)
    ImageDraw.Draw(big).rounded_rectangle((0, 0, size * aa - 1, size * aa - 1), radius=radius * aa, fill=255)
    return big.resize((size, size), Image.LANCZOS)


def main():
    src = Image.open(sys.argv[1]).convert("RGB")
    out = sys.argv[2]
    n = int(SRC[2])                                       # nắn ở cỡ gốc của mẫu rồi mới thu về 1024
    r_dst = RADIUS * n / S
    pts = {}
    grid = list(range(0, n, CELL)) + [n]
    for y in grid:
        for x in grid:
            pts[x, y] = warp(x, y, n, r_dst)
    mesh = []
    for y0, y1 in zip(grid, grid[1:]):
        for x0, x1 in zip(grid, grid[1:]):
            ul, ll, lr, ur = pts[x0, y0], pts[x0, y1], pts[x1, y1], pts[x1, y0]
            mesh.append(((x0, y0, x1, y1), (*ul, *ll, *lr, *ur)))
    big = src.transform((n, n), Image.MESH, mesh, resample=Image.BICUBIC)
    icon = big.resize((S, S), Image.LANCZOS).convert("RGBA")
    icon.putalpha(ImageChops.multiply(icon.getchannel("A"), rounded(S, RADIUS)))
    icon.save(out)
    print("đã tạo", out)


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")  # bảng mã cp1252 của Windows không in được tiếng Việt
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)
    main()
