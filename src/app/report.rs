//! 결과 리포트 — 전투가 끝난 뒤 무슨 일이 있었는지.
//!
//! 이 시뮬레이터의 결론은 "누가 이겼나"가 아니라 **"무엇이 얼마나 사람을
//! 줄였나"** 다. 병종별 손실률과 사인별 전사자를 나란히 놓는 이유다.

use crate::config::BattleConfig;
use crate::render::{rgb, uifont, Frame};
use crate::sim::unit_types::{stats, N_TYPES};
use crate::sim::{Outcome, World, CAUSE_NAMES, DT, N_CAUSES};

pub fn draw(frame: &mut Frame, world: &World, cfg: &BattleConfig, outcome: Outcome) {
    let (w, h) = (frame.w as i32, frame.h as i32);
    let bg = rgb(16, 18, 21);
    for p in frame.px.iter_mut() {
        *p = bg;
    }
    let ink = rgb(232, 232, 220);
    let dim = rgb(126, 128, 124);
    let hot = rgb(232, 176, 92);
    let red = rgb(214, 76, 62);
    let blue = rgb(92, 150, 236);
    let cx = w / 2;
    let s = &world.stats;

    // --- 판정 ---
    let (verdict, vcolor) = match outcome {
        Outcome::Victory(0) => ("공격측 승리", red),
        Outcome::Victory(_) => ("방어측 승리", blue),
        Outcome::Timeout => ("결판 없음", dim),
        Outcome::Ongoing => ("전투 중", dim),
    };
    uifont::text_center(frame, cx, 26, verdict, 2, vcolor);
    uifont::text_center(frame, cx, 62, &cfg.title(), 1, dim);
    let secs = world.tick as f32 * DT;
    uifont::text_center(
        frame,
        cx,
        82,
        &format!("전장 시간 {}분 {}초", secs as u32 / 60, secs as u32 % 60),
        1,
        dim,
    );

    // --- 양측 요약 ---
    let col = [cx - 470, cx + 40];
    for (t, &x) in col.iter().enumerate() {
        let c = if t == 0 { red } else { blue };
        let start = world.start_strength[t].max(1);
        uifont::text(
            frame,
            x,
            116,
            if t == 0 { "공격측" } else { "방어측" },
            1,
            c,
        );
        // "생존"만 보여주면 판정이 거꾸로 보인다. 이 시뮬레이터의 승패를 가르는
        // 것은 시체 수가 아니라 **아직 싸울 뜻이 있는 인원**이기 때문이다
        let lines = [
            ("투입", s.start_by_type[t].iter().sum::<u32>()),
            ("잔존", s.alive[t]),
            ("  그중 패주", s.routed[t]),
            ("전사", s.dead[t]),
            ("이탈", s.fled[t]),
        ];
        let mut y = 140;
        for (label, n) in lines {
            uifont::text(frame, x, y, label, 1, dim);
            uifont::text_right(frame, x + 150, y, &format!("{n}"), 1, ink);
            uifont::text_right(
                frame,
                x + 220,
                y,
                &format!("{}%", (n as f32 / start as f32 * 100.0).round() as i32),
                1,
                dim,
            );
            y += 20;
        }
    }

    // --- 병종별 ---
    let mut y = 258;
    uifont::text(frame, col[0], y, "병종", 1, hot);
    uifont::text_right(frame, col[0] + 300, y, "투입", 1, hot);
    uifont::text_right(frame, col[0] + 380, y, "전사", 1, hot);
    uifont::text_right(frame, col[0] + 450, y, "손실", 1, hot);
    uifont::text(frame, col[1], y, "병종", 1, hot);
    uifont::text_right(frame, col[1] + 300, y, "투입", 1, hot);
    uifont::text_right(frame, col[1] + 380, y, "전사", 1, hot);
    uifont::text_right(frame, col[1] + 450, y, "손실", 1, hot);
    y += 22;
    let top = y;
    for (t, &tx) in col.iter().enumerate() {
        let mut y = top;
        for ty in 0..N_TYPES {
            let n = s.start_by_type[t][ty];
            if n == 0 {
                continue;
            }
            let d = s.dead_by_type[t][ty];
            let x = tx;
            uifont::text(frame, x, y, stats(ty as u8).name, 1, ink);
            uifont::text_right(frame, x + 300, y, &format!("{n}"), 1, ink);
            uifont::text_right(frame, x + 380, y, &format!("{d}"), 1, ink);
            let ratio = d as f32 / n as f32 * 100.0;
            uifont::text_right(
                frame,
                x + 450,
                y,
                &format!("{}%", ratio.round() as i32),
                1,
                if ratio > 70.0 { red } else { dim },
            );
            y += 20;
        }
    }

    // --- 사인별 ---
    let used = (0..2)
        .map(|t| {
            (0..N_TYPES)
                .filter(|ty| s.start_by_type[t][*ty] > 0)
                .count()
        })
        .max()
        .unwrap_or(0) as i32;
    let mut y = top + 20 * used + 26;
    uifont::text(frame, col[0], y, "무엇에 죽었나", 1, hot);
    y += 22;
    for (t, &x) in col.iter().enumerate() {
        let total: u32 = s.dead_by_cause[t].iter().sum::<u32>().max(1);
        let mut yy = y;
        for (k, name) in CAUSE_NAMES.iter().enumerate() {
            let n = s.dead_by_cause[t][k];
            uifont::text(frame, x, yy, name, 1, dim);
            uifont::text_right(frame, x + 300, yy, &format!("{n}"), 1, ink);
            uifont::text_right(
                frame,
                x + 380,
                yy,
                &format!("{}%", (n as f32 / total as f32 * 100.0).round() as i32),
                1,
                dim,
            );
            yy += 20;
        }
    }

    // --- 병력 시계열 ---
    let gy = y + 20 * N_CAUSES as i32 + 24;
    let gh = (h - gy - 56).clamp(40, 200);
    let gx = col[0];
    let gw = (col[1] + 450 - col[0]).min(w - gx - 40);
    graph(frame, gx, gy, gw, gh, world, red, blue, dim);

    uifont::text_center(
        frame,
        cx,
        h - 30,
        "ENTER  설정으로 돌아가기      R  같은 설정으로 다시      ESC  종료",
        1,
        hot,
    );
}

/// 양측 병력이 시간에 따라 어떻게 깎였는지.
#[allow(clippy::too_many_arguments)]
fn graph(
    frame: &mut Frame,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    world: &World,
    red: u32,
    blue: u32,
    dim: u32,
) {
    let hist = &world.stats.history;
    if hist.len() < 2 || w < 10 || h < 10 {
        return;
    }
    for col in 0..w {
        frame.put(x + col, y + h, dim);
    }
    let peak = hist
        .iter()
        .map(|a| a[0].max(a[1]))
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    for col in 0..w {
        let i = (col as usize * (hist.len() - 1)) / (w.max(2) - 1) as usize;
        for (t, c) in [(0usize, red), (1usize, blue)] {
            let v = hist[i][t] as f32 / peak;
            let top = y + h - (v * h as f32) as i32;
            for py in top..=y + h {
                // 두 팀이 겹치면 위에 그리는 쪽이 이긴다. 채우지 않고 선만 긋는다
                if py <= top + 1 {
                    frame.put(x + col, py, c);
                }
            }
        }
    }
    uifont::text(
        frame,
        x,
        y - 18,
        &format!("병력 추이 (세로 최대 {})", peak as u32),
        1,
        dim,
    );
}
