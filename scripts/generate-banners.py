"""Generate Dofus-tab GitHub banner images (dark and light variants)."""
import subprocess
import os
import glob
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICON_SVG_DARK = os.path.join(ROOT, "packaging/icons/hicolor/scalable/apps/io.github.gambl3r08.DofusTab.svg")
ICON_SVG_LIGHT = os.path.join(ROOT, "packaging/icons/hicolor/scalable/apps/io.github.gambl3r08.DofusTab-light.svg")
OUT_DIR = os.path.join(ROOT, "packaging/banners")
os.makedirs(OUT_DIR, exist_ok=True)

FONT_BOLD = "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf"
FONT_REG = "/usr/share/fonts/noto/NotoSans-Regular.ttf"

THEME = {
    "dark": {
        "top": (14, 20, 30),
        "bot": (24, 34, 48),
        "gold": (246, 208, 106),
        "text_main": (244, 248, 252),
        "text_dim": (154, 170, 192),
        "grid": (255, 255, 255, 7),
        "sheen": (255, 255, 255, 14),
        "glow_opacity": 0.07,
        "icon": ICON_SVG_DARK,
    },
    "light": {
        "top": (244, 246, 249),
        "bot": (225, 230, 238),
        "gold": (212, 140, 0),
        "text_main": (24, 34, 50),
        "text_dim": (80, 95, 115),
        "grid": (30, 45, 70, 6),
        "sheen": (255, 255, 255, 60),
        "glow_opacity": 0.05,
        "icon": ICON_SVG_LIGHT,
    },
}


def font(size, bold=False):
    try:
        return ImageFont.truetype(FONT_BOLD if bold else FONT_REG, size)
    except Exception:
        return ImageFont.load_default()


def render_icon(size, theme, name):
    tmp = os.path.join(OUT_DIR, f"_tmp-icon-{theme}-{name}-{size}.png")
    subprocess.run(
        ["rsvg-convert", "-w", str(size), "-h", str(size), THEME[theme]["icon"], "-o", tmp],
        check=True,
    )
    return Image.open(tmp).convert("RGBA")


def gradient(width, height, c1, c2):
    img = Image.new("RGB", (width, height), c1)
    draw = ImageDraw.Draw(img)
    for y in range(height):
        ratio = y / (height - 1)
        r = int(c1[0] + (c2[0] - c1[0]) * ratio)
        g = int(c1[1] + (c2[1] - c1[1]) * ratio)
        b = int(c1[2] + (c2[2] - c1[2]) * ratio)
        draw.line([(0, y), (width, y)], fill=(r, g, b))
    return img


def soft_grid(img, theme):
    draw = ImageDraw.Draw(img)
    w, h = img.size
    color = THEME[theme]["grid"]
    for x in range(0, w, 80):
        for y in range(0, h, 80):
            draw.ellipse([x - 1.5, y - 1.5, x + 1.5, y + 1.5], fill=color)


def top_sheen(img, theme):
    w, h = img.size
    overlay = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    draw.polygon(
        [(0, 0), (w, 0), (w, int(h * 0.22)), (0, int(h * 0.30))],
        fill=THEME[theme]["sheen"],
    )
    return Image.alpha_composite(img.convert("RGBA"), overlay)


def corner_glow(img, theme):
    w, h = img.size
    glow = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    draw = ImageDraw.Draw(glow)
    gold = THEME[theme]["gold"]
    opacity = THEME[theme]["glow_opacity"]
    for i in range(25, 0, -1):
        alpha = int(opacity * 255 / (i ** 0.45))
        r = i * (max(w, h) // 7)
        draw.ellipse([w - r, -r // 2, w + r // 3, r], fill=(*gold, alpha))
    return Image.alpha_composite(img.convert("RGBA"), glow)


def centered_text(draw, text, y, fill, size, bold=False, w=None):
    w = w or draw.im.size[0]
    fnt = font(size, bold)
    bbox = draw.textbbox((0, 0), text, font=fnt)
    tw = bbox[2] - bbox[0]
    draw.text(((w - tw) // 2, y), text, fill=fill, font=fnt)


def base_canvas(width, height, theme):
    img = gradient(width, height, THEME[theme]["top"], THEME[theme]["bot"]).convert("RGBA")
    soft_grid(img, theme)
    img = top_sheen(img, theme)
    img = corner_glow(img, theme)
    return img


def make_banner(width, height, layout="wide", theme="dark"):
    img = base_canvas(width, height, theme)
    draw = ImageDraw.Draw(img)
    t = THEME[theme]
    gold = t["gold"]
    text_main = t["text_main"]
    text_dim = t["text_dim"]

    if layout == "wide":
        icon_size = int(height * 0.50)
        icon = render_icon(icon_size, theme, "wide")
        ix = int(width * 0.08)
        iy = (height - icon_size) // 2
        img.paste(icon, (ix, iy), icon)

        tx = ix + icon_size + int(width * 0.07)
        ty = height // 2 - int(height * 0.16)
        draw.text((tx, ty), "Dofus-tab", fill=text_main, font=font(int(height * 0.15), bold=True))
        ty += int(height * 0.18)
        draw.text((tx, ty), "Cross-platform window switcher for Dofus", fill=text_dim, font=font(int(height * 0.05)))
        ty += int(height * 0.08)
        draw.text((tx, ty), "Alt+1 … Alt+8  ·  Linux · Windows · macOS", fill=gold, font=font(int(height * 0.045)))

    elif layout == "social":
        icon_size = int(height * 0.42)
        icon = render_icon(icon_size, theme, "social")
        ix = (width - icon_size) // 2
        iy = int(height * 0.10)
        img.paste(icon, (ix, iy), icon)

        ty = iy + icon_size + int(height * 0.08)
        centered_text(draw, "Dofus-tab", ty, text_main, int(height * 0.14), bold=True)
        ty += int(height * 0.16)
        centered_text(draw, "Cross-platform window switcher for Dofus", ty, text_dim, int(height * 0.055))
        ty += int(height * 0.09)
        centered_text(draw, "Linux · Windows · macOS", ty, gold, int(height * 0.045))

    elif layout == "header":
        icon_size = int(height * 0.80)
        icon = render_icon(icon_size, theme, "header")
        ix = int(width * 0.04)
        iy = (height - icon_size) // 2
        img.paste(icon, (ix, iy), icon)

        tx = ix + icon_size + int(width * 0.04)
        ty = height // 2 - int(height * 0.22)
        draw.text((tx, ty), "Dofus-tab", fill=text_main, font=font(int(height * 0.32), bold=True))
        ty += int(height * 0.40)
        draw.text((tx, ty), "Cross-platform Dofus window switcher", fill=text_dim, font=font(int(height * 0.13)))

        tw = int(width * 0.16)
        th = int(height * 0.28)
        tx2 = int(width * 0.82)
        ty2 = height // 2 - int(height * 0.22)
        for i in range(4):
            ox = tx2 + (i % 2) * (tw + int(width * 0.015))
            oy = ty2 + (i // 2) * (th + int(height * 0.12))
            is_active = i == 0
            if theme == "dark":
                fill = (58, 85, 120) if is_active else (28, 42, 58)
                stroke = gold if is_active else (58, 79, 104)
            else:
                fill = (232, 238, 247) if is_active else (207, 216, 228)
                stroke = gold if is_active else (154, 170, 192)
            draw.rounded_rectangle([ox, oy, ox + tw, oy + th], radius=10, fill=fill, outline=stroke, width=3)
            if is_active:
                draw.rounded_rectangle([ox, oy + 8, ox + int(width * 0.006), oy + th - 8], radius=3, fill=gold)

    return img


def generate_icons(theme):
    """Render PNG icon set for a given theme into hicolor layout."""
    icon_dir = os.path.join(ROOT, "packaging/icons/hicolor")
    if theme == "dark":
        svg = ICON_SVG_DARK
        suffix = ""
    else:
        svg = ICON_SVG_LIGHT
        suffix = "-light"

    simple_svg = svg.replace(".svg", "-simple.svg")
    tiny_svg = svg.replace(".svg", "-16.svg")

    for s in [16, 32, 48]:
        src = tiny_svg if s == 16 else simple_svg
        out = os.path.join(icon_dir, f"{s}x{s}/apps/io.github.gambl3r08.DofusTab{suffix}.png")
        subprocess.run(["rsvg-convert", "-w", str(s), "-h", str(s), src, "-o", out], check=True)

    for s in [64, 128, 256, 512]:
        out = os.path.join(icon_dir, f"{s}x{s}/apps/io.github.gambl3r08.DofusTab{suffix}.png")
        subprocess.run(["rsvg-convert", "-w", str(s), "-h", str(s), svg, "-o", out], check=True)

    # ICO for Windows
    ico = os.path.join(ROOT, f"packaging/icons/dofus-tab{suffix}.ico")
    subprocess.run([
        "magick",
        os.path.join(icon_dir, f"16x16/apps/io.github.gambl3r08.DofusTab{suffix}.png"),
        os.path.join(icon_dir, f"32x32/apps/io.github.gambl3r08.DofusTab{suffix}.png"),
        os.path.join(icon_dir, f"48x48/apps/io.github.gambl3r08.DofusTab{suffix}.png"),
        os.path.join(icon_dir, f"64x64/apps/io.github.gambl3r08.DofusTab{suffix}.png"),
        os.path.join(icon_dir, f"128x128/apps/io.github.gambl3r08.DofusTab{suffix}.png"),
        os.path.join(icon_dir, f"256x256/apps/io.github.gambl3r08.DofusTab{suffix}.png"),
        ico,
    ], check=True)

    # Flat copies
    for s in [16, 32, 64, 128, 256, 512]:
        src = os.path.join(icon_dir, f"{s}x{s}/apps/io.github.gambl3r08.DofusTab{suffix}.png")
        dst = os.path.join(ROOT, f"packaging/icons/dofus-tab{suffix}-{s}.png")
        subprocess.run(["cp", src, dst], check=True)


if __name__ == "__main__":
    # Clean old outputs and temp files
    for f in glob.glob(os.path.join(OUT_DIR, "*.png")) + glob.glob(os.path.join(OUT_DIR, "_tmp-*.png")):
        os.remove(f)

    for theme in ["dark", "light"]:
        suffix = "" if theme == "dark" else "-light"
        make_banner(1280, 640, "wide", theme).save(os.path.join(OUT_DIR, f"github-banner{suffix}.png"), "PNG")
        make_banner(2560, 1280, "wide", theme).save(os.path.join(OUT_DIR, f"github-banner{suffix}@2x.png"), "PNG")
        make_banner(1200, 630, "social", theme).save(os.path.join(OUT_DIR, f"github-social{suffix}.png"), "PNG")
        make_banner(1920, 400, "header", theme).save(os.path.join(OUT_DIR, f"github-header{suffix}.png"), "PNG")
        generate_icons(theme)

    # Remove any leftover temp renders
    for f in glob.glob(os.path.join(OUT_DIR, "_tmp-*.png")):
        os.remove(f)

    print("Banners and icons generated in", OUT_DIR)
    for f in sorted(os.listdir(OUT_DIR)):
        print(" -", f)
