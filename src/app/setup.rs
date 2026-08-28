//! 설정 화면 — 유저가 하는 일 전부.

use crate::config::{BattleConfig, Battlefield, Doctrine, ARMY_SIZES, BATTLEFIELDS, DOCTRINES};
use crate::render::{draw_ground, rgb, uifont, Camera, Decals, Frame};
use crate::sim::{World, WORLD_SIZE};

use super::Nav;

const ROWS: usize = 4;

pub struct Setup {
    pub cfg: BattleConfig,
    /// 지금 고르고 있는 줄
    pub row: usize,
    /// 전장 미리보기. 지형은 만드는 데 시간이 걸리므로 설정이 바뀔 때만 다시 굽는다
    preview: Option<Frame>,
    preview_for: Option<(Battlefield, u64, u32)>,
}

impl Default for Setup {
    fn default() -> Self {
        Self::new()
    }
}

impl Setup {
    pub fn new() -> Self {
        Self {
            cfg: BattleConfig::default(),
            row: 0,
            preview: None,
            preview_for: None,
        }
    }

    /// 입력 하나를 먹는다. 전투를 시작해야 하면 true.
    pub fn input(&mut self, nav: Nav) -> bool {
        let step: i64 = match nav {
            Nav::Left | Nav::LeftFast => -1,
            Nav::Right | Nav::RightFast => 1,
            Nav::Up => {
                self.row = (self.row + ROWS - 1) % ROWS;
                return false;
            }
            Nav::Down => {
                self.row = (self.row + 1) % ROWS;
                return false;
            }
            Nav::Enter => return true,
        };
        let fast = matches!(nav, Nav::LeftFast | Nav::RightFast);
        match self.row {
            0 => {
                let i = BATTLEFIELDS
                    .iter()
                    .position(|f| *f == self.cfg.field)
                    .unwrap_or(0);
                self.cfg.field = BATTLEFIELDS[wrap(i, step, BATTLEFIELDS.len())];
            }
            1 => {
                let i = ARMY_SIZES
                    .iter()
                    .position(|n| *n == self.cfg.total)
                    .unwrap_or(2);
                self.cfg.total = ARMY_SIZES[wrap(i, step, ARMY_SIZES.len())];
            }
            2 => {
                let i = DOCTRINES
                    .iter()
                    .position(|d| *d == self.cfg.doctrine)
                    .unwrap_or(0);
                self.cfg.doctrine = DOCTRINES[wrap(i, step, DOCTRINES.len())];
            }
            _ => {
                let d = if fast { 10 } else { 1 };
                let s = self.cfg.seed as i64 + step * d;
                self.cfg.seed = s.clamp(1, 9999) as u64;
            }
        }
        false
    }

    /// 지금 설정으로 전장을 한 장 구워 둔다. 설정이 그대로면 다시 굽지 않는다.
    fn ensure_preview(&mut self, w: usize, h: usize) {
        let key = (self.cfg.field, self.cfg.seed, self.cfg.total);
        if self.preview_for == Some(key)
            && self.preview.as_ref().map(|f| (f.w, f.h)) == Some((w, h))
        {
            return;
        }
        let sc = self.cfg.scenario();
        let mut world = World::new(sc.seed, 0);
        world.set_terrain(sc.map, sc.seed);
        if let Some((half, moat)) = sc.castle {
            world.place_castle(crate::map::castle::Castle::square(
                [WORLD_SIZE * 0.5, WORLD_SIZE * 0.5],
                half,
                moat,
            ));
        }
        // 개전 배치가 다 들어오도록 잘라 본다
        let span = span_of(&sc);
        let mut frame = Frame::new(w, h);
        let cam = Camera {
            center: [WORLD_SIZE * 0.5, WORLD_SIZE * 0.5],
            mpp: (span / w as f32).max(span * 0.62 / h as f32),
        };
        let decals = Decals::new(WORLD_SIZE, 4.0);
        draw_ground(&mut frame, &world, &decals, &cam);
        // 개전 배치를 두 색 막대로 겹쳐 놓는다. 어디에 얼마나 서는지가 보이도록
        for f in &sc.formations {
            let (sx, sy) = cam.to_screen(f.center, w, h);
            let half_w = (f.width * 0.5 / cam.mpp).max(1.0);
            let c = if f.team == 0 {
                rgb(198, 62, 52)
            } else {
                rgb(58, 106, 214)
            };
            for dx in -(half_w as i32)..=(half_w as i32) {
                for dy in -1..=1 {
                    frame.put(sx as i32 + dx, sy as i32 + dy, c);
                }
            }
        }
        self.preview = Some(frame);
        self.preview_for = Some(key);
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let (w, h) = (frame.w as i32, frame.h as i32);
        let bg = rgb(16, 18, 21);
        for p in frame.px.iter_mut() {
            *p = bg;
        }

        let ink = rgb(232, 232, 220);
        let dim = rgb(126, 128, 124);
        let hot = rgb(232, 176, 92);
        let red = rgb(214, 76, 62);

        let big = if w >= 1100 && h >= 700 { 2 } else { 1 };
        let body = 1;
        let cx = w / 2;

        uifont::text_center(frame, cx, 26, "ORC-WAR", big * 2, red);
        uifont::text_center(
            frame,
            cx,
            26 + 34 * big,
            "고대 대규모 전투 시뮬레이터",
            big,
            dim,
        );

        // --- 미리보기 ---
        // 세로 예산을 먼저 떼어 놓고 남는 만큼만 미리보기에 준다.
        // 반대로 하면 창이 작을 때 항목이 화면 밖으로 밀려난다
        let head_h = 26 + 34 * big + 22 * big + 26;
        let rows_h = ROWS as i32 * 26 + 46;
        let foot_h = 84;
        let pv_max_h = (h - head_h - rows_h - foot_h).max(90);
        let pv_w = ((w - 200).min(pv_max_h * 2)).clamp(300, 1000) as usize;
        let pv_h = (pv_w / 2).min(pv_max_h as usize);
        let pv_y = head_h;
        self.ensure_preview(pv_w, pv_h);
        if let Some(pv) = &self.preview {
            let ox = (w - pv_w as i32) / 2;
            for row in 0..pv_h {
                let dy = pv_y + row as i32;
                if dy < 0 || dy >= h {
                    continue;
                }
                for col in 0..pv_w {
                    frame.put(ox + col as i32, dy, pv.px[row * pv_w + col]);
                }
            }
            // 테두리
            for col in 0..pv_w as i32 {
                frame.put(ox + col, pv_y - 1, dim);
                frame.put(ox + col, pv_y + pv_h as i32, dim);
            }
        }

        // --- 항목 ---
        let mut y = pv_y + pv_h as i32 + 28;
        let label_x = cx - 420;
        let value_x = cx - 250;
        let blurb_x = cx - 60;
        let per_team = self.cfg.total / 2;
        let rows: [(&str, String, &str); ROWS] = [
            (
                "전장",
                self.cfg.field.name().to_string(),
                self.cfg.field.blurb(),
            ),
            (
                "병력",
                format!("{}", self.cfg.total),
                "양팀 합계. 목표 규모는 300000 이다",
            ),
            (
                "편성",
                self.cfg.doctrine.name().to_string(),
                self.cfg.doctrine.blurb(),
            ),
            (
                "씨앗",
                format!("{}", self.cfg.seed),
                "같은 씨앗이면 지형도 전투도 똑같이 되풀이된다",
            ),
        ];
        for (i, (label, value, blurb)) in rows.iter().enumerate() {
            let on = i == self.row;
            let c = if on { ink } else { dim };
            if on {
                uifont::text(frame, label_x - 26, y, ">", body, hot);
            }
            uifont::text(frame, label_x, y, label, body, c);
            uifont::text(
                frame,
                value_x - 22,
                y,
                if on { "<" } else { " " },
                body,
                hot,
            );
            uifont::text_center(
                frame,
                value_x + 60,
                y,
                value,
                body,
                if on { hot } else { c },
            );
            uifont::text(
                frame,
                value_x + 130,
                y,
                if on { ">" } else { " " },
                body,
                hot,
            );
            uifont::text(frame, blurb_x, y, blurb, body, dim);
            y += 26;
        }

        // 병력 한 줄 요약
        y += 8;
        let summary = if self.cfg.field == Battlefield::Siege {
            format!(
                "공격 {} · 수비 {}",
                self.cfg.total * 4 / 5,
                self.cfg.total / 5
            )
        } else {
            format!("한 팀당 {per_team}")
        };
        uifont::text(frame, label_x, y, &summary, body, dim);

        // --- 안내 ---
        uifont::text_center(frame, cx, h - 56, "ENTER  전투 시작", body, hot);
        uifont::text_center(
            frame,
            cx,
            h - 32,
            "위아래 항목 고르기 · 좌우 값 바꾸기 · ESC 종료",
            body,
            dim,
        );
    }
}

fn wrap(i: usize, step: i64, n: usize) -> usize {
    (((i as i64 + step) % n as i64 + n as i64) % n as i64) as usize
}

/// 개전 배치가 다 들어오는 폭(m).
fn span_of(sc: &crate::scenario::Scenario) -> f32 {
    let mut lo = [f32::MAX; 2];
    let mut hi = [f32::MIN; 2];
    for f in &sc.formations {
        for k in 0..2 {
            lo[k] = lo[k].min(f.center[k] - f.width * 0.5);
            hi[k] = hi[k].max(f.center[k] + f.width * 0.5);
        }
    }
    if lo[0] > hi[0] {
        return WORLD_SIZE * 0.5;
    }
    ((hi[0] - lo[0]).max(hi[1] - lo[1]) * 1.35).clamp(400.0, WORLD_SIZE)
}

/// 편성이 실제로 어떤 병종 몇 명이 되는지 — 설정 화면에서 쓸 수도 있고, 테스트가 쓴다.
pub fn breakdown(cfg: &BattleConfig) -> Vec<(&'static str, u32)> {
    use crate::sim::unit_types::stats;
    let sc = cfg.scenario();
    let mut by: Vec<(&'static str, u32)> = Vec::new();
    for f in &sc.formations {
        if f.team != 0 {
            continue;
        }
        let name = stats(f.type_id).name;
        match by.iter_mut().find(|(n, _)| *n == name) {
            Some((_, c)) => *c += f.count,
            None => by.push((name, f.count)),
        }
    }
    by
}

/// 편성 이름표 — `Doctrine` 를 화면 밖에서도 쓰기 좋게.
pub fn doctrine_label(d: Doctrine) -> String {
    format!("{} 편성", d.name())
}
