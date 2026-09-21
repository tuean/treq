#!/usr/bin/env python3
"""生成 treq 应用图标（紫罗兰圆角方块 + 白色发送箭头）。

产出：
  assets/icon.png          1024×1024 源图
  assets/AppIcon.icns      macOS 应用图标（scripts/bundle.sh 用它打 .app）

用法：python3 scripts/make_icon.py
"""

import shutil
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw

S = 1024
TOP = (0xA2, 0x93, 0xEA)   # 顶部紫罗兰
BOTTOM = (0x5F, 0x4C, 0xBE)  # 底部深紫
WHITE = (0xFF, 0xFF, 0xFF)


def gradient(size: int) -> Image.Image:
    img = Image.new("RGB", (size, size))
    d = ImageDraw.Draw(img)
    for y in range(size):
        # 对角渐变：左上亮 → 右下暗
        t = (y / (size - 1)) * 0.85
        c = tuple(round(TOP[i] + (BOTTOM[i] - TOP[i]) * t) for i in range(3))
        d.line([(0, y), (size, y)], fill=c)
    return img


def arrow(draw: ImageDraw.ImageDraw, size: int) -> None:
    """发送箭头：一个圆头横杠 + 一个 V 形箭头，`»` 的感觉。"""
    u = size / 1024  # 以 1024 为设计基准
    bar_h = round(72 * u)
    y = size // 2
    # 横杠
    draw.rounded_rectangle(
        [round(250 * u), y - bar_h // 2, round(600 * u), y + bar_h // 2],
        radius=bar_h // 2,
        fill=WHITE,
    )
    # 箭头（两条粗线凑成的 V）
    w = round(78 * u)
    tip = (round(742 * u), y)
    draw.line(
        [(round(560 * u), y - round(150 * u)), tip], fill=WHITE, width=w, joint="curve"
    )
    draw.line(
        [tip, (round(560 * u), y + round(150 * u))], fill=WHITE, width=w, joint="curve"
    )
    # 圆头（PIL 的 line 端点不圆）
    for cx, cy in [(round(560 * u), y - round(150 * u)), (round(560 * u), y + round(150 * u))]:
        draw.ellipse([cx - w // 2, cy - w // 2, cx + w // 2, cy + w // 2], fill=WHITE)
    draw.ellipse([tip[0] - w // 2, tip[1] - w // 2, tip[0] + w // 2, tip[1] + w // 2], fill=WHITE)


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    assets = root / "assets"
    assets.mkdir(exist_ok=True)

    # 背景渐变 + macOS 风格圆角（半径 ≈ 边长 × 0.2237）
    radius = round(S * 0.2237)
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, S - 1, S - 1], radius=radius, fill=255)

    icon = gradient(S)
    arrow(ImageDraw.Draw(icon), S)

    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(icon, (0, 0), mask)
    png = assets / "icon.png"
    out.save(png)
    print("wrote", png, out.size)

    # .icns：iconutil 需要 .iconset 目录里的固定命名（中间产物放 target/，不进仓库）
    iconset = root / "target" / "AppIcon.iconset"
    if iconset.exists():
        shutil.rmtree(iconset)
    iconset.mkdir(parents=True)
    for px in (16, 32, 64, 128, 256, 512):
        out.resize((px, px), Image.LANCZOS).save(iconset / f"icon_{px}x{px}.png")
        out.resize((px * 2, px * 2), Image.LANCZOS).save(iconset / f"icon_{px}x{px}@2x.png")
    icns = assets / "AppIcon.icns"
    subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(icns)], check=True)
    print("wrote", icns)


if __name__ == "__main__":
    main()
