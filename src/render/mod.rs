//! 전장을 화소 버퍼에 그린다.
//!
//! GPU 를 쓰지 않는다. 30만 개의 점을 찍는 일은 CPU 로도 충분히 빠르고,
//! 무엇보다 외장/내장 GPU 를 잘못 잡는 하이브리드 노트북 문제를 통째로 비켜간다.
//! 그림 자체가 점과 사각형뿐이라 셰이더로 얻을 것이 많지도 않다.

pub mod font;

use rayon::prelude::*;

use crate::map::Terrain;
use crate::sim::pool::UnitState;
use crate::sim::unit_types::{is_engine, stats};
use crate::sim::{Outcome, World, DT};

/// 화면과 월드 사이의 변환.
#[derive(Clone, Copy)]
pub struct Camera {
    /// 화면 한복판이 보고 있는 월드 좌표
    pub center: [f32; 2],
    /// 화소 하나가 덮는 미터. 작을수록 확대된 것이다
    pub mpp: f32,
}

impl Camera {
    #[inline(always)]
    pub fn to_screen(&self, p: [f32; 2], w: usize, h: usize) -> (f32, f32) {
        let sx = (p[0] - self.center[0]) / self.mpp + w as f32 * 0.5;
        // 화면 y 는 아래로 증가하므로 뒤집는다
        let sy = h as f32 * 0.5 - (p[1] - self.center[1]) / self.mpp;
        (sx, sy)
    }

    #[inline(always)]
    pub fn to_world(&self, sx: f32, sy: f32, w: usize, h: usize) -> [f32; 2] {
        [
            self.center[0] + (sx - w as f32 * 0.5) * self.mpp,
            self.center[1] + (h as f32 * 0.5 - sy) * self.mpp,
        ]
    }

    /// 화면에 담기는 월드 범위 (x0, y0, x1, y1)
    pub fn view_bounds(&self, w: usize, h: usize) -> [f32; 4] {
        let hw = w as f32 * 0.5 * self.mpp;
        let hh = h as f32 * 0.5 * self.mpp;
        [
            self.center[0] - hw,
            self.center[1] - hh,
            self.center[0] + hw,
            self.center[1] + hh,
        ]
    }
}

/// 개전 배치와 성곽이 모두 담기도록 카메라를 맞춘다.
pub fn fit_camera(w: &World, win_w: usize, win_h: usize) -> Camera {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for i in 0..w.pool.len() {
        let p = w.pool.pos[i];
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    // 성이 있으면 성벽 바깥까지 담아야 한다. 병력만 보고 맞추면 성이 잘린다
    if let Some(c) = &w.castle {
        let margin = 70.0;
        x0 = x0.min(c.center[0] - c.half[0] - margin);
        x1 = x1.max(c.center[0] + c.half[0] + margin);
        y0 = y0.min(c.center[1] - c.half[1] - margin);
        y1 = y1.max(c.center[1] + c.half[1] + margin);
    }
    if x0 > x1 {
        return Camera {
            center: [crate::sim::WORLD_SIZE * 0.5, crate::sim::WORLD_SIZE * 0.5],
            mpp: 2.0,
        };
    }
    let pad = 50.0;
    let span_x = (x1 - x0 + pad * 2.0).max(50.0);
    let span_y = (y1 - y0 + pad * 2.0).max(50.0);
    Camera {
        center: [(x0 + x1) * 0.5, (y0 + y1) * 0.5],
        mpp: (span_x / win_w as f32).max(span_y / win_h as f32),
    }
}

/// 전장에 쌓이는 시체와 핏자국.
///
/// 카메라가 움직이므로 화면이 아니라 월드 격자에 누적한다.
pub struct Decals {
    pub cell: f32,
    pub w: usize,
    pub h: usize,
    pub density: Vec<u16>,
}

impl Decals {
    pub fn new(world_size: f32, cell: f32) -> Self {
        let w = (world_size / cell).ceil() as usize;
        Self {
            cell,
            w,
            h: w,
            density: vec![0; w * w],
        }
    }

    pub fn absorb(&mut self, world: &World) {
        for (p, _, _) in &world.death_events {
            let cx = ((p[0] / self.cell) as isize).clamp(0, self.w as isize - 1) as usize;
            let cy = ((p[1] / self.cell) as isize).clamp(0, self.h as isize - 1) as usize;
            let i = cy * self.w + cx;
            self.density[i] = self.density[i].saturating_add(1);
        }
    }

    #[inline(always)]
    pub fn at(&self, p: [f32; 2]) -> u16 {
        let cx = ((p[0] / self.cell) as isize).clamp(0, self.w as isize - 1) as usize;
        let cy = ((p[1] / self.cell) as isize).clamp(0, self.h as isize - 1) as usize;
        self.density[cy * self.w + cx]
    }

    /// 이웃 칸까지 섞어 읽는다.
    ///
    /// 칸 값을 그대로 쓰면 확대했을 때 핏자국이 격자 모양 사각형으로 보인다.
    /// 시체가 격자에 맞춰 쓰러질 리는 없으므로 경계를 뭉갠다.
    #[inline(always)]
    pub fn smooth_at(&self, p: [f32; 2]) -> f32 {
        let fx = (p[0] / self.cell - 0.5).clamp(0.0, self.w as f32 - 1.001);
        let fy = (p[1] / self.cell - 0.5).clamp(0.0, self.h as f32 - 1.001);
        let x0 = fx as usize;
        let y0 = fy as usize;
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let a = self.density[y0 * self.w + x0] as f32;
        let b = self.density[y0 * self.w + x1] as f32;
        let c = self.density[y1 * self.w + x0] as f32;
        let d = self.density[y1 * self.w + x1] as f32;
        let top = a + (b - a) * tx;
        let bot = c + (d - c) * tx;
        top + (bot - top) * ty
    }
}

pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

impl Frame {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![0; w * h],
        }
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        if self.w != w || self.h != h {
            self.w = w;
            self.h = h;
            self.px.resize(w * h, 0);
        }
    }

    #[inline(always)]
    pub fn put(&mut self, x: i32, y: i32, c: u32) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }

    #[inline(always)]
    pub fn blot(&mut self, x: i32, y: i32, size: i32, c: u32) {
        for dy in 0..size {
            for dx in 0..size {
                self.put(x + dx, y + dy, c);
            }
        }
    }
}

#[inline(always)]
pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// 지형과 시체를 그린다. 카메라가 그대로면 다시 그릴 필요가 없어 따로 둔다.
pub fn draw_ground(frame: &mut Frame, world: &World, decals: &Decals, cam: &Camera) {
    let (w, h) = (frame.w, frame.h);
    let mpp = cam.mpp;
    let x0 = cam.center[0] - w as f32 * 0.5 * mpp;
    let y_top = cam.center[1] + h as f32 * 0.5 * mpp;
    let terrain = &world.terrain;

    frame
        .px
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(row, line)| {
            let wy = y_top - row as f32 * mpp;
            let mut wx = x0;
            for px in line.iter_mut() {
                let p = [wx, wy];
                let t = terrain.at(p);
                // 화소마다 고도를 보간하면 너무 비싸다. 칸 값을 그대로 쓴다
                let elev = terrain.height[terrain.idx(p)];
                // 고도에 따라 밝기를 줘서 기복이 보이게 한다
                let shade = (elev * 0.42).clamp(-22.0, 74.0);
                let base = match t {
                    Terrain::Plain => [30.0, 42.0, 28.0],
                    Terrain::Forest => [17.0, 39.0, 19.0],
                    Terrain::Rock => [64.0, 62.0, 58.0],
                    Terrain::Water => [22.0, 42.0, 78.0],
                    Terrain::Ford => [58.0, 82.0, 96.0],
                    Terrain::Marsh => [40.0, 44.0, 30.0],
                    Terrain::Wall => [150.0, 146.0, 136.0],
                    Terrain::Gate => [112.0, 84.0, 48.0],
                    Terrain::Rubble => [96.0, 90.0, 82.0],
                    Terrain::Moat => [26.0, 46.0, 72.0],
                };
                let mut r = base[0] + shade;
                let mut g = base[1] + shade;
                let mut b = base[2] + shade * 0.7;
                // 시체가 쌓인 자리는 검붉게 물든다
                let d = decals.smooth_at(p);
                if d > 0.05 {
                    let k = (d / 3.5).min(1.0);
                    r += 58.0 * k;
                    g -= 16.0 * k;
                    b -= 14.0 * k;
                }
                *px = rgb(
                    r.clamp(0.0, 255.0) as u8,
                    g.clamp(0.0, 255.0) as u8,
                    b.clamp(0.0, 255.0) as u8,
                );
                wx += mpp;
            }
        });
}

/// 병력과 날아가는 것들을 그린다.
pub fn draw_units(frame: &mut Frame, world: &World, cam: &Camera) {
    let (w, h) = (frame.w, frame.h);
    let mpp = cam.mpp;
    // 화소당 미터가 작을수록(확대될수록) 한 사람을 크게 찍는다
    let size = if mpp > 1.2 {
        1
    } else if mpp > 0.55 {
        2
    } else if mpp > 0.28 {
        3
    } else {
        4
    };

    let bounds = cam.view_bounds(w, h);
    let pool = &world.pool;
    for i in 0..pool.len() {
        let p = pool.pos[i];
        if p[0] < bounds[0] || p[0] > bounds[2] || p[1] < bounds[1] || p[1] > bounds[3] {
            continue;
        }
        let st = pool.state[i];
        if matches!(st, UnitState::Dead | UnitState::Fled) {
            continue;
        }
        let (sx, sy) = cam.to_screen(p, w, h);
        let ty = pool.type_id[i];
        let s = stats(ty);
        let team = pool.team[i];

        let c = if is_engine(ty) {
            rgb(196, 168, 96) // 공성병기는 나무빛
        } else if st == UnitState::Rout {
            // 등을 보인 병력은 흐릿하게
            if team == 0 {
                rgb(120, 62, 58)
            } else {
                rgb(58, 78, 120)
            }
        } else if s.is_cavalry {
            if team == 0 {
                rgb(255, 150, 90)
            } else {
                rgb(120, 210, 255)
            }
        } else if s.range > 0.0 {
            if team == 0 {
                rgb(226, 96, 74)
            } else {
                rgb(96, 150, 236)
            }
        } else if team == 0 {
            rgb(214, 52, 44)
        } else {
            rgb(58, 106, 214)
        };

        let engine_size = if is_engine(ty) { size + 2 } else { size };
        frame.blot(sx as i32, sy as i32, engine_size, c);
    }

    // 날아가는 화살
    let tick = world.tick;
    let mut shots: Vec<([f32; 2], u8)> = Vec::new();
    world
        .projectiles
        .for_each_in_flight(tick, |p, _, kind| shots.push((p, kind)));
    for (p, _) in shots {
        if p[0] < bounds[0] || p[0] > bounds[2] || p[1] < bounds[1] || p[1] > bounds[3] {
            continue;
        }
        let (sx, sy) = cam.to_screen(p, w, h);
        frame.put(sx as i32, sy as i32, rgb(236, 230, 196));
    }
}

/// 전황과 조작 안내를 화면 위에 얹는다.
pub fn draw_hud(
    frame: &mut Frame,
    w: &World,
    speed: f32,
    paused: bool,
    fps: f32,
    outcome: Outcome,
) {
    let pad = 12i32;
    let ink = rgb(232, 232, 220);
    let dim = rgb(150, 150, 140);
    let red = rgb(226, 84, 72);
    let blue = rgb(92, 150, 236);

    // 위쪽 전황 줄
    let mut y = pad;
    font::text(frame, pad, y, "ATTACK", 2, dim);
    font::text(
        frame,
        pad + font::width("ATTACK ", 2),
        y,
        &format!("{}", w.stats.alive[0]),
        2,
        red,
    );
    let mid = frame.w as i32 / 2;
    font::text(frame, mid, y, "DEFEND", 2, dim);
    font::text(
        frame,
        mid + font::width("DEFEND ", 2),
        y,
        &format!("{}", w.stats.alive[1]),
        2,
        blue,
    );

    y += 22;
    font::text(
        frame,
        pad,
        y,
        &format!("DEAD {} / {}", w.stats.dead[0], w.stats.dead[1]),
        1,
        dim,
    );
    font::text(
        frame,
        mid,
        y,
        &format!("ROUT {} / {}", w.stats.routed[0], w.stats.routed[1]),
        1,
        dim,
    );

    y += 14;
    font::text(
        frame,
        pad,
        y,
        &format!(
            "T {:.0}S   X{}   {}FPS",
            w.tick as f32 * DT,
            speed,
            fps as i32
        ),
        1,
        dim,
    );
    if paused {
        font::text(frame, mid, y, "PAUSED", 1, ink);
    }

    // 아래쪽 병력 막대 — 양쪽이 서로를 밀어내는 모양으로
    let total = (w.stats.alive[0] + w.stats.alive[1]).max(1) as f32;
    let bar_y = frame.h as i32 - 26;
    let bar_w = frame.w as i32 - pad * 2;
    let split = (w.stats.alive[0] as f32 / total * bar_w as f32) as i32;
    for x in 0..bar_w {
        let c = if x < split { red } else { blue };
        frame.blot(pad + x, bar_y, 1, c);
        frame.blot(pad + x, bar_y + 1, 1, c);
        frame.blot(pad + x, bar_y + 2, 1, c);
    }

    // 조작 안내
    font::text(
        frame,
        pad,
        frame.h as i32 - 16,
        "SPACE PAUSE   BRACKETS SPEED   WASD PAN   WHEEL ZOOM   F FIT   R RESET",
        1,
        rgb(96, 96, 90),
    );

    // 결판이 났으면 크게 알린다
    let verdict = match outcome {
        Outcome::Victory(0) => Some("ATTACKER WINS"),
        Outcome::Victory(_) => Some("DEFENDER WINS"),
        Outcome::Timeout => Some("NO DECISION"),
        Outcome::Ongoing => None,
    };
    if let Some(v) = verdict {
        let scale = 4;
        let tw = font::width(v, scale);
        font::text(
            frame,
            (frame.w as i32 - tw) / 2,
            frame.h as i32 / 2 - 20,
            v,
            scale,
            ink,
        );
    }
}
