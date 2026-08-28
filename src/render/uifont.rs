//! 화면 글꼴 — 한글이 나오는 쪽.
//!
//! 전투 HUD 는 좁은 자리에 숫자를 많이 찍어야 해서 5x7 (`font`) 을 그대로 쓴다.
//! 설정·결과 화면은 읽으라고 있는 화면이라 한글이 필요하다.
//!
//! 조합형 글꼴을 손으로 짜는 것도, 실행시간에 TTF 래스터라이저를 끌어오는 것도
//! 이 프로젝트에는 과하다("첫 실행이 곧 실전"이라 손댈 곳이 적을수록 좋다).
//! unifont 는 애초에 16x16 비트맵 글꼴이니 **쓰는 글자만 미리 구워** 표로 들고
//! 다닌다 — 실행시간 의존성이 없고, 표는 20KB 남짓이다.
//!
//! 표를 만드는 것은 `tools/genfont.py` 다.

use super::Frame;

pub struct Glyph {
    /// 유니코드 코드포인트 (표는 이 값의 오름차순)
    pub cp: u16,
    /// 글자 폭(화소). 라틴 8, 한글 16
    pub w: u8,
    /// 16줄. 비트 15 가 맨 왼쪽 화소
    pub rows: [u16; 16],
}

pub const LINE: i32 = 16;

/// 표에 없는 글자를 대신할 빈 네모.
static TOFU: Glyph = Glyph {
    cp: 0,
    w: 8,
    rows: [
        0x0000, 0x0000, 0x7e00, 0x4200, 0x4200, 0x4200, 0x4200, 0x4200, 0x4200, 0x4200, 0x4200,
        0x4200, 0x7e00, 0x0000, 0x0000, 0x0000,
    ],
};

/// 표에 이 글자가 있는가. 회귀 테스트가 쓴다.
pub fn has_glyph(c: char) -> bool {
    let cp = c as u32;
    cp <= 0xFFFF
        && super::uifont_table::GLYPHS
            .binary_search_by_key(&(cp as u16), |g| g.cp)
            .is_ok()
}

#[inline]
fn glyph(c: char) -> &'static Glyph {
    let cp = c as u32;
    if cp > 0xFFFF {
        return &TOFU;
    }
    let cp = cp as u16;
    match super::uifont_table::GLYPHS.binary_search_by_key(&cp, |g| g.cp) {
        Ok(i) => &super::uifont_table::GLYPHS[i],
        Err(_) => &TOFU,
    }
}

/// 글자를 찍고, 다음 글자가 시작할 x 를 돌려준다.
pub fn text(frame: &mut Frame, x: i32, y: i32, s: &str, scale: i32, color: u32) -> i32 {
    let mut cx = x;
    for c in s.chars() {
        if c == ' ' {
            cx += 8 * scale;
            continue;
        }
        let g = glyph(c);
        for (row, bits) in g.rows.iter().enumerate() {
            if *bits == 0 {
                continue;
            }
            for col in 0..g.w as i32 {
                if bits & (1 << (15 - col)) == 0 {
                    continue;
                }
                let px = cx + col * scale;
                let py = y + row as i32 * scale;
                if scale == 1 {
                    frame.put(px, py, color);
                } else {
                    frame.blot(px, py, scale, color);
                }
            }
        }
        cx += g.w as i32 * scale;
    }
    cx
}

/// 찍었을 때 차지할 폭.
pub fn width(s: &str, scale: i32) -> i32 {
    s.chars()
        .map(|c| if c == ' ' { 8 } else { glyph(c).w as i32 } * scale)
        .sum()
}

/// 오른쪽 끝을 x 에 맞춰 찍는다. 표의 숫자 칸에 쓴다.
pub fn text_right(frame: &mut Frame, x: i32, y: i32, s: &str, scale: i32, color: u32) {
    text(frame, x - width(s, scale), y, s, scale, color);
}

/// 한복판을 x 에 맞춰 찍는다.
pub fn text_center(frame: &mut Frame, x: i32, y: i32, s: &str, scale: i32, color: u32) {
    text(frame, x - width(s, scale) / 2, y, s, scale, color);
}
