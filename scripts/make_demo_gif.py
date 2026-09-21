#!/usr/bin/env python3
"""Regenerate docs/demo.gif from the live three-act demo output.

Requires a Rust toolchain (`cargo`) and Pillow (`pip install Pillow`).

The renderer is deterministic: the same demo output and the same script
always produce identical frames. The demo itself is deterministic, so the
whole GIF is reproducible from source.

Usage (from the repository root):

    python scripts/make_demo_gif.py
"""

from __future__ import annotations

import pathlib
import subprocess
import sys

from PIL import Image, ImageDraw, ImageFont

REPO = pathlib.Path(__file__).resolve().parent.parent
OUT_PATH = REPO / "docs" / "demo.gif"
PROMPT = "$ cargo run -- fixture"

# GitHub-dark-ish terminal palette (also the GIF palette).
BG = (13, 17, 23)
BAR = (22, 27, 34)
BORDER = (48, 54, 61)
TEXT = (201, 209, 217)
DIM = (139, 148, 158)
YELLOW = (227, 179, 65)
RED = (248, 81, 73)
GREEN = (63, 185, 80)
BLUE = (88, 166, 255)

PALETTE_COLORS = [BG, BAR, BORDER, TEXT, DIM, YELLOW, RED, GREEN, BLUE]

FONT_SIZE = 17
LINE_GAP = 9
FPS_MS = 80  # 12.5 fps


def find_font(bold: bool) -> ImageFont.FreeTypeFont:
    candidates = (
        ["consolab.ttf"] if bold else ["consola.ttf", "DejaVuSansMono.ttf"]
    ) + (
        []
        if bold
        else ["/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", "/System/Library/Fonts/Menlo.ttc"]
    )
    for name in candidates:
        try:
            return ImageFont.truetype(name, FONT_SIZE)
        except OSError:
            continue
    return ImageFont.load_default()


def demo_output() -> list[str]:
    result = subprocess.run(
        ["cargo", "run", "--quiet", "--", "fixture"],
        cwd=REPO,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=True,
    )
    return result.stdout.rstrip("\n").splitlines()


def line_style(line: str) -> tuple[ImageFont.FreeTypeFont, tuple[int, int, int]]:
    font, bold_font = FONTS
    if line.startswith("Act "):
        return bold_font, YELLOW
    if line == "Provenance":
        return bold_font, TEXT
    if line.startswith("  ev-"):
        return font, BLUE
    if line.startswith("  Decision") and "DENY" in line:
        return font, RED
    if line.startswith("  Decision") and "ALLOW" in line:
        return font, GREEN
    if line.startswith("  Relations") or line.startswith("  judgment"):
        return font, DIM
    return font, TEXT


def line_delay_ms(line: str) -> int:
    if line == "":
        return 250
    if line.startswith("Act "):
        return 900
    if line.startswith("  Decision"):
        return 1100
    if line.startswith("  ev-"):
        return 750
    if line.startswith("  Authorization") or line.startswith("  Supervision"):
        return 850
    return 650


def build_frames(lines: list[str]) -> tuple[list[Image.Image], list[int]]:
    font, bold_font = FONTS
    advance = int(max(font.getlength("M"), 1)) + 1
    line_height = FONT_SIZE + LINE_GAP
    pad = 22
    bar_height = 42
    text_lines = 1 + 1 + len(lines)  # prompt + blank + output
    width = pad * 2 + max(advance * max(len(l) for l in lines + [PROMPT]), 640)
    height = bar_height + pad + text_lines * line_height + pad

    frames: list[Image.Image] = []
    durations: list[int] = []

    def new_frame() -> tuple[Image.Image, ImageDraw.ImageDraw]:
        img = Image.new("RGB", (width, height), BG)
        draw = ImageDraw.Draw(img)
        draw.rectangle([0, 0, width, bar_height], fill=BAR)
        draw.line([(0, bar_height), (width, bar_height)], fill=BORDER)
        for i, color in enumerate([RED, YELLOW, GREEN]):
            cx = 20 + i * 24
            draw.ellipse([cx, bar_height // 2 - 6, cx + 12, bar_height // 2 + 6], fill=color)
        draw.text((86, (bar_height - FONT_SIZE) // 2 - 2), "r2r-jev", font=bold_font, fill=DIM)
        return img, draw

    def emit(duration: int, visible: list[str], cursor: tuple[int, int] | None = None) -> None:
        img, draw = new_frame()
        y = bar_height + pad
        x = pad
        draw.text((x, y), "$", font=bold_font, fill=GREEN)
        draw.text((x + advance, y), PROMPT[2:], font=font, fill=TEXT)
        y += line_height * 2  # prompt line + blank line
        for line in visible:
            line_font, color = line_style(line)
            draw.text((x, y), line, font=line_font, fill=color)
            y += line_height
        if cursor is not None:
            cx, cy = cursor
            draw.rectangle([cx, cy, cx + advance - 3, cy + FONT_SIZE], fill=TEXT)
        frames.append(img.quantize(palette=PALETTE_IMG, dither=Image.Dither.NONE))
        durations.append(duration)

    # Type the prompt character by character.
    for n in range(1, len(PROMPT) + 1):
        emit(45, [], cursor=(pad + advance * n, bar_height + pad))
    emit(500, [], cursor=(pad + advance * len(PROMPT), bar_height + pad))

    # Reveal output lines one at a time, like real program output.
    visible: list[str] = []
    cursor_y = bar_height + pad + line_height  # after prompt+blank
    for line in lines:
        emit(line_delay_ms(line), list(visible))
        visible.append(line)
        cursor_y += line_height

    # Final hold with a blinking cursor.
    cursor_x = pad + advance * len(visible[-1]) if visible[-1] else pad
    for _ in range(6):
        emit(450, list(visible), cursor=(cursor_x, cursor_y - line_height))
        emit(450, list(visible))

    return frames, durations


def main() -> int:
    global FONTS, PALETTE_IMG
    FONTS = (find_font(bold=False), find_font(bold=True))
    palette = Image.new("P", (len(PALETTE_COLORS), 1))
    flat: list[int] = []
    for color in PALETTE_COLORS:
        flat.extend(color)
    flat.extend([0, 0, 0] * (256 - len(PALETTE_COLORS)))
    palette.putpalette(flat)
    PALETTE_IMG = palette

    lines = demo_output()
    frames, durations = build_frames(lines)
    frames[0].save(
        OUT_PATH,
        save_all=True,
        append_images=frames[1:],
        duration=durations,
        loop=0,
        optimize=True,
    )
    total_s = sum(durations) / 1000
    size_kb = OUT_PATH.stat().st_size // 1024
    print(f"wrote {OUT_PATH.relative_to(REPO)}: {len(frames)} frames, {total_s:.1f}s, {size_kb} KB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
