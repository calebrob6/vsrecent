"""Generate vsrecent.ico — a multi-resolution app icon.

Design:
  - Rounded dark square background (matches the app's dark theme).
  - Three horizontal "project cards" stacked, evoking a recents picker.
  - Top card highlighted in VSCode blue (#007ACC) with a white play arrow
    on the right, suggesting "launch this one".
  - Other cards muted gray with a subtle filename strip.
  - Subtle outer glow / inner highlight for depth.

Outputs vsrecent.ico with sizes: 16, 24, 32, 48, 64, 128, 256 (PNG-encoded
within the .ico for the 256 entry).
"""
from PIL import Image, ImageDraw, ImageFilter
import os, sys

OUT = os.path.join(os.path.dirname(__file__), "vsrecent.ico")

# Colors
BG_TOP    = (30, 30, 30, 255)      # #1E1E1E
BG_BOT    = (45, 45, 48, 255)      # #2D2D30
BG_BORDER = (60, 60, 64, 255)
HILITE    = (0, 122, 204, 255)     # #007ACC VSCode blue
HILITE_HI = (40, 150, 224, 255)    # gradient top
ROW_BG    = (62, 62, 66, 255)      # muted gray rows
ROW_BG2   = (74, 74, 80, 255)
ROW_DETAIL= (140, 140, 145, 255)
WHITE     = (255, 255, 255, 255)
SHADOW    = (0, 0, 0, 110)

def rounded_rect_mask(size, radius):
    img = Image.new("L", size, 0)
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, size[0]-1, size[1]-1), radius=radius, fill=255)
    return img

def vgradient(size, top_rgba, bot_rgba):
    w, h = size
    base = Image.new("RGBA", (1, h))
    for y in range(h):
        t = y / max(1, h - 1)
        r = int(top_rgba[0] + (bot_rgba[0] - top_rgba[0]) * t)
        g = int(top_rgba[1] + (bot_rgba[1] - top_rgba[1]) * t)
        b = int(top_rgba[2] + (bot_rgba[2] - top_rgba[2]) * t)
        a = int(top_rgba[3] + (bot_rgba[3] - top_rgba[3]) * t)
        base.putpixel((0, y), (r, g, b, a))
    return base.resize((w, h))

def render(size):
    """Render the icon at a given square size and return RGBA Image."""
    s = size
    # Work at 4x supersample for AA, then downsample.
    ss = 4 if s >= 32 else 2
    W = s * ss

    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # Background: rounded square with vertical gradient.
    radius = int(W * 0.18)
    bg = vgradient((W, W), BG_TOP, BG_BOT)
    mask = rounded_rect_mask((W, W), radius)
    img.paste(bg, (0, 0), mask)

    # Subtle 1-px inner border for definition.
    border = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    bd = ImageDraw.Draw(border)
    bd.rounded_rectangle(
        (0, 0, W - 1, W - 1),
        radius=radius,
        outline=BG_BORDER,
        width=max(1, ss),
    )
    img.alpha_composite(border)

    # Geometry for three stacked rows.
    pad_x = int(W * 0.16)
    pad_top = int(W * 0.18)
    row_h = int(W * 0.155)
    gap = int(W * 0.06)
    row_w = W - 2 * pad_x
    row_radius = max(2, int(row_h * 0.28))

    # Bottom and middle rows (muted)
    for i in [2, 1]:
        y0 = pad_top + i * (row_h + gap)
        y1 = y0 + row_h
        # Slight color variation
        fill = ROW_BG if i == 1 else ROW_BG2
        # Soft drop shadow
        sh = Image.new("RGBA", (W, W), (0, 0, 0, 0))
        sd = ImageDraw.Draw(sh)
        sd.rounded_rectangle(
            (pad_x, y0 + max(1, ss), pad_x + row_w, y1 + max(1, ss)),
            radius=row_radius,
            fill=SHADOW,
        )
        sh = sh.filter(ImageFilter.GaussianBlur(radius=ss * 1.5))
        img.alpha_composite(sh)
        d.rounded_rectangle(
            (pad_x, y0, pad_x + row_w, y1),
            radius=row_radius,
            fill=fill,
        )
        # Filename strip (left chunk darker, right portion empty)
        strip_w = int(row_w * 0.55)
        strip_h = max(2, int(row_h * 0.22))
        strip_y = y0 + (row_h - strip_h) // 2
        d.rounded_rectangle(
            (pad_x + int(row_h * 0.35), strip_y,
             pad_x + int(row_h * 0.35) + strip_w, strip_y + strip_h),
            radius=strip_h // 2,
            fill=ROW_DETAIL,
        )

    # Top (highlighted) row with VSCode-blue gradient + play arrow
    y0 = pad_top
    y1 = y0 + row_h
    # Drop shadow
    sh = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    sd = ImageDraw.Draw(sh)
    sd.rounded_rectangle(
        (pad_x, y0 + max(1, ss * 2), pad_x + row_w, y1 + max(1, ss * 2)),
        radius=row_radius,
        fill=(0, 0, 0, 150),
    )
    sh = sh.filter(ImageFilter.GaussianBlur(radius=ss * 2.0))
    img.alpha_composite(sh)

    # Blue gradient fill for the highlight row
    grad = vgradient((row_w, row_h), HILITE_HI, HILITE)
    rmask = rounded_rect_mask((row_w, row_h), row_radius)
    layer = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    layer.paste(grad, (pad_x, y0), rmask)
    img.alpha_composite(layer)

    # Filename strip on the highlighted row (white)
    strip_w = int(row_w * 0.50)
    strip_h = max(2, int(row_h * 0.22))
    strip_y = y0 + (row_h - strip_h) // 2
    d.rounded_rectangle(
        (pad_x + int(row_h * 0.35), strip_y,
         pad_x + int(row_h * 0.35) + strip_w, strip_y + strip_h),
        radius=strip_h // 2,
        fill=(255, 255, 255, 235),
    )

    # Play arrow (triangle) on the right side of the highlight row
    arrow_h = int(row_h * 0.55)
    arrow_w = int(arrow_h * 0.95)
    cx = pad_x + row_w - int(row_h * 0.45) - arrow_w // 2
    cy = y0 + row_h // 2
    d.polygon(
        [
            (cx - arrow_w // 2, cy - arrow_h // 2),
            (cx + arrow_w // 2, cy),
            (cx - arrow_w // 2, cy + arrow_h // 2),
        ],
        fill=WHITE,
    )

    # Subtle top inner highlight for "glassy" feel
    hl = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    hd = ImageDraw.Draw(hl)
    hd.rounded_rectangle(
        (max(1, ss), max(1, ss), W - 1 - max(1, ss), int(W * 0.45)),
        radius=radius,
        fill=(255, 255, 255, 18),
    )
    hl = hl.filter(ImageFilter.GaussianBlur(radius=ss * 2))
    # Clip the highlight to the rounded background
    hl_clipped = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    hl_clipped.paste(hl, (0, 0), mask)
    img.alpha_composite(hl_clipped)

    # Downsample with high-quality filter
    if ss > 1:
        img = img.resize((s, s), Image.LANCZOS)
    return img


def main():
    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [render(s) for s in sizes]

    # Save individual PNGs for visual inspection
    preview_dir = os.path.join(os.path.dirname(__file__), "_icon_preview")
    os.makedirs(preview_dir, exist_ok=True)
    for s, im in zip(sizes, images):
        im.save(os.path.join(preview_dir, f"icon_{s}.png"))

    # Build the ICO container manually so every size is preserved as a
    # PNG-encoded frame (Pillow's built-in .save("ICO") collapses sizes).
    import io, struct
    pngs = []
    for im in images:
        buf = io.BytesIO()
        im.save(buf, format="PNG", optimize=True)
        pngs.append(buf.getvalue())

    n = len(images)
    header = struct.pack("<HHH", 0, 1, n)  # reserved, type=1 (ICO), count
    dir_size = 6 + 16 * n
    entries = b""
    offset = dir_size
    for s, png in zip(sizes, pngs):
        # Width/height stored as a single byte; 0 means 256.
        w = 0 if s == 256 else s
        h = 0 if s == 256 else s
        entries += struct.pack(
            "<BBBBHHII",
            w,        # width
            h,        # height
            0,        # color count (0 for >=256 colors)
            0,        # reserved
            1,        # color planes
            32,       # bits per pixel
            len(png), # bytes in resource
            offset,   # offset to resource
        )
        offset += len(png)

    with open(OUT, "wb") as f:
        f.write(header)
        f.write(entries)
        for png in pngs:
            f.write(png)

    print(f"Wrote {OUT} ({os.path.getsize(OUT)} bytes)")
    print("Sizes:", sizes)


if __name__ == "__main__":
    main()
