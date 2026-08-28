#!/usr/bin/env python3
"""화면에 쓰는 글꼴을 unifont 에서 구워 Rust 표로 낸다.

한글은 조합형 글꼴을 손으로 짜기에는 너무 크고, TTF 래스터라이저를 실행시간에
끌어오면 "첫 실행이 곧 실전"인 이 프로젝트에 위험만 는다. unifont 는 애초에
16x16 비트맵 글꼴이라 그대로 떠서 표로 구우면 된다 — 실행시간 의존성 0.

쓰는 글자만 굽는다. src/ 를 훑어 문자열에 실제로 나온 한글을 모으므로,
UI 문구를 고치고 이 스크립트를 다시 돌리면 표도 따라온다.

  python3 tools/genfont.py > src/render/uifont_table.rs
"""
import re
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/opentype/unifont/unifont.otf"
SIZE = 16
ROOT = Path(__file__).resolve().parent.parent

# 라틴 밖의 글자는 전부 굽는다 — 한글뿐 아니라 —, ·, ←, × 같은 것도 쓴다
EXTRA = re.compile(r"[^\x00-\x7f]")


STRING = re.compile(r'"(?:[^"\\]|\\.)*"', re.S)


def used_extra() -> set:
    """주석이 아니라 **문자열 리터럴**에 든 라틴 밖 글자만 모은다.

    주석까지 긁으면 글자 수가 몇 배로 뛰고, 설명 한 줄 고칠 때마다 글꼴 표가
    통째로 흔들린다."""
    seen = set()
    for p in sorted(ROOT.glob("src/**/*.rs")):
        for lit in STRING.findall(p.read_text(encoding="utf-8")):
            seen.update(EXTRA.findall(lit))
    return seen


def bake(font, ch):
    """글자 하나를 (폭, 16줄 비트) 로 뜬다. 비트 15 가 맨 왼쪽 화소.

    unifont 는 한글을 라틴 문자보다 두 줄 아래에 앉힌다. 섞어 쓰면 한글만
    내려앉아 보이므로 구울 때 끌어올려 밑선을 맞춘다."""
    lift = 2 if ord(ch) >= 0x3131 else 0
    img = Image.new("L", (32, 32), 0)
    ImageDraw.Draw(img).text((0, -lift), ch, font=font, fill=255)
    px = img.load()
    rows = []
    for y in range(SIZE):
        bits = 0
        for x in range(SIZE):
            if px[x, y] > 110:
                bits |= 1 << (15 - x)
        rows.append(bits)
    # 실제로 칠해진 오른쪽 끝을 재서 폭을 정한다. unifont 는 ASCII 8, 한글 16
    right = 0
    for r in rows:
        for x in range(SIZE):
            if r & (1 << (15 - x)):
                right = max(right, x + 1)
    w = 16 if right > 8 else 8
    return w, rows


def main():
    font = ImageFont.truetype(FONT, SIZE)
    chars = [chr(c) for c in range(0x20, 0x7F)] + sorted(used_extra())
    out = []
    for ch in chars:
        w, rows = bake(font, ch)
        if ch == " ":
            w = 8
        out.append((ord(ch), w, rows))
    out.sort(key=lambda e: e[0])

    w = sys.stdout.write
    w("//! 화면 글꼴 표 — `tools/genfont.py` 가 unifont 에서 구웠다.\n")
    w("//!\n")
    w("//! **손으로 고치지 말 것.** UI 문구에 새 글자를 쓰면 생성기를 다시 돌린다.\n")
    w("//! 비트 15 가 글자의 맨 왼쪽 화소다.\n\n")
    w("use super::uifont::Glyph;\n\n")
    w(f"pub static GLYPHS: [Glyph; {len(out)}] = [\n")
    for cp, gw, rows in out:
        body = ", ".join(f"0x{r:04x}" for r in rows)
        w(f"    Glyph {{ cp: 0x{cp:04x}, w: {gw}, rows: [{body}] }},\n")
    w("];\n")


if __name__ == "__main__":
    main()
