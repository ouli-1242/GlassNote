"""生成 GlassNote 的应用图标。

用脚本而不是直接提交二进制，是为了让图标可复现、可改：改配色或字形只要动这里的常量。
输出到 src-tauri/icons/，覆盖 tauri.conf.json 里声明的全部尺寸与 .ico。

依赖：Pillow（仅开发期需要，不进运行时依赖）
用法：python scripts/gen-icons.py
"""

from __future__ import annotations

import os

from PIL import Image, ImageChops, ImageDraw, ImageFilter

OUT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src-tauri", "icons")
MASTER = 1024  # 主图尺寸，其余尺寸都由它降采样得到
SS = 2  # 超采样倍数，保证圆角与斜线有抗锯齿

GRAD_A = (124, 108, 255)  # 左上：靛蓝
GRAD_B = (34, 211, 238)  # 右下：青


def _lerp(a: int, b: int, t: float) -> int:
    return int(round(a + (b - a) * t))


def make_gradient(n: int) -> Image.Image:
    """先画极小的对角渐变再放大，避免逐像素 Python 循环。"""
    small = Image.new("RGB", (64, 64))
    px = small.load()
    for y in range(64):
        for x in range(64):
            t = (x / 63.0 + y / 63.0) / 2.0
            px[x, y] = (_lerp(GRAD_A[0], GRAD_B[0], t), _lerp(GRAD_A[1], GRAD_B[1], t), _lerp(GRAD_A[2], GRAD_B[2], t))
    return small.resize((n, n), Image.BICUBIC).convert("RGBA")


def rounded_mask(n: int, radius: float) -> Image.Image:
    mask = Image.new("L", (n, n), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, n - 1, n - 1], radius=radius, fill=255)
    return mask


def white_layer(n: int, alpha: Image.Image) -> Image.Image:
    layer = Image.new("RGBA", (n, n), (255, 255, 255, 0))
    layer.putalpha(alpha)
    return layer


def draw_glyph(n: int) -> Image.Image:
    """内容层：透明底上画白色字形。

    注意 PIL 的 ImageDraw 不做 alpha 混合 —— 在 RGBA 上直接填半透明色是"覆盖"而不是"叠加"。
    所以这里全部画在独立透明层上，最后用 alpha_composite 合上去。
    """
    layer = Image.new("RGBA", (n, n), (255, 255, 255, 0))
    d = ImageDraw.Draw(layer)
    u = n / 1024.0

    def stroke(points, width, alpha):
        w = max(1, int(width * u))
        pts = [(x * u, y * u) for x, y in points]
        d.line(pts, fill=(255, 255, 255, alpha), width=w, joint="curve")
        r = w / 2.0
        for px, py in (pts[0], pts[-1]):  # 圆头线帽
            d.ellipse([px - r, py - r, px + r, py + r], fill=(255, 255, 255, alpha))

    # 对勾：粗、偏上，保证 16px 下仍是可辨识的主形
    stroke([(238, 470), (382, 614), (664, 300)], 74, 255)

    # 两条清单线，宽度递减暗示"还有内容"
    stroke([(238, 748), (786, 748)], 58, 226)
    stroke([(238, 878), (612, 878)], 58, 150)

    return layer


def build_master() -> Image.Image:
    n = MASTER * SS
    base = make_gradient(n)

    # 顶部柔光：给平面渐变一点体积感。必须模糊，否则椭圆边界会显成一道生硬的弧线；
    # 强度也要克制，过亮会冲淡白色字形的对比度。
    glow = Image.new("L", (n, n), 0)
    ImageDraw.Draw(glow).ellipse([-n * 0.30, -n * 0.80, n * 1.00, n * 0.45], fill=62)
    glow = glow.filter(ImageFilter.GaussianBlur(n * 0.09))
    base = Image.alpha_composite(base, white_layer(n, glow))

    base = Image.alpha_composite(base, draw_glyph(n))

    # 外圆角：与内容 alpha 相乘，而不是替换 —— 替换会抹掉字形层的透明度
    base.putalpha(ImageChops.multiply(base.getchannel("A"), rounded_mask(n, radius=n * 0.225)))
    return base.resize((MASTER, MASTER), Image.LANCZOS)


def downscale(master: Image.Image, size: int) -> Image.Image:
    """逐级减半降采样，比一步缩到 16px 更锐利。"""
    img, cur = master, MASTER
    while cur // 2 >= size:
        cur //= 2
        img = img.resize((cur, cur), Image.LANCZOS)
    if cur != size:
        img = img.resize((size, size), Image.LANCZOS)
    return img


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    master = build_master()

    master.save(os.path.join(OUT_DIR, "icon.png"))
    for size in (32, 128, 256, 512):
        name = "128x128@2x.png" if size == 256 else f"{size}x{size}.png"
        downscale(master, size).save(os.path.join(OUT_DIR, name))

    # Windows 资源编译器与 NSIS 都要 icon.ico；多尺寸打包让任务栏/资源管理器各取所需
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    master.save(os.path.join(OUT_DIR, "icon.ico"), format="ICO", sizes=[(s, s) for s in ico_sizes])

    print("图标已生成 ->", os.path.abspath(OUT_DIR))
    for f in sorted(os.listdir(OUT_DIR)):
        print(f"  {f:20s} {os.path.getsize(os.path.join(OUT_DIR, f)):>8,} bytes")


if __name__ == "__main__":
    main()
