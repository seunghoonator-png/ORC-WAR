//! 전장 스냅샷 — 전투를 돌리며 일정 간격으로 화면을 이미지로 굽는다.
//!
//! GUI 없이 시뮬레이션이 제대로 보이는지 확인하는 용도이자, 나중에 결과
//! 리포트의 전황 축소도로도 쓴다. 화소 하나에 유닛 여럿이 겹치므로 밀도를
//! 밝기로 환산하는데, 이는 실제 렌더러가 최대 줌아웃에서 쓸 방식과 같다.

use std::fs::File;
use std::io::{BufWriter, Write};

use orc_war::scenario::Scenario;
use orc_war::sim::pool::UnitState;
use orc_war::sim::unit_types::INF_SWORD;
use orc_war::sim::World;

const W: usize = 1000;
const H: usize = 460;

struct Canvas {
    /// 팀별 화소당 유닛 수
    density: Vec<[u16; 2]>,
    /// 시체 누적 — 전투가 지나간 자리는 지워지지 않는다
    corpses: Vec<u16>,
    view: [f32; 4],
}

impl Canvas {
    fn new(view: [f32; 4]) -> Self {
        Self {
            density: vec![[0; 2]; W * H],
            corpses: vec![0; W * H],
            view,
        }
    }

    fn to_px(&self, p: [f32; 2]) -> Option<usize> {
        let [x0, y0, x1, y1] = self.view;
        let u = (p[0] - x0) / (x1 - x0);
        // 이미지 y축은 아래로 증가하므로 뒤집는다
        let v = 1.0 - (p[1] - y0) / (y1 - y0);
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return None;
        }
        Some((v * H as f32) as usize * W + (u * W as f32) as usize)
    }

    fn accumulate_corpses(&mut self, w: &World) {
        for (p, _, _) in &w.death_events {
            if let Some(i) = self.to_px(*p) {
                self.corpses[i] = self.corpses[i].saturating_add(1);
            }
        }
    }

    fn snap_units(&mut self, w: &World) {
        self.density.iter_mut().for_each(|d| *d = [0; 2]);
        for i in 0..w.pool.len() {
            if w.pool.state[i] == UnitState::Dead {
                continue;
            }
            if let Some(px) = self.to_px(w.pool.pos[i]) {
                let t = w.pool.team[i] as usize;
                self.density[px][t] = self.density[px][t].saturating_add(1);
            }
        }
    }

    fn write_ppm(&self, path: &str) -> std::io::Result<()> {
        let f = File::create(path)?;
        let mut out = BufWriter::new(f);
        write!(out, "P6\n{} {}\n255\n", W, H)?;
        let mut buf = Vec::with_capacity(W * H * 3);
        for i in 0..W * H {
            let [a, b] = self.density[i];
            let corpse = self.corpses[i];
            // 초원 바탕
            let mut rgb = [26u8, 34, 24];
            if corpse > 0 {
                // 시체가 쌓일수록 검붉게 물든다
                let k = ((corpse as f32 / 3.0).min(1.0) * 255.0) as u8;
                rgb = [
                    26 + (k as u16 * 46 / 255) as u8,
                    34u8.saturating_sub(k / 12),
                    24u8.saturating_sub(k / 14),
                ];
            }
            let ramp = |n: u16| -> f32 { (n as f32 / 2.5).min(1.0) };
            if a > 0 || b > 0 {
                let (ia, ib) = (ramp(a), ramp(b));
                if a >= b {
                    rgb = [
                        (90.0 + 165.0 * ia) as u8,
                        (30.0 + 30.0 * ia) as u8,
                        (28.0 + 24.0 * ia) as u8,
                    ];
                } else {
                    rgb = [
                        (28.0 + 30.0 * ib) as u8,
                        (60.0 + 70.0 * ib) as u8,
                        (110.0 + 145.0 * ib) as u8,
                    ];
                }
            }
            buf.extend_from_slice(&rgb);
        }
        out.write_all(&buf)?;
        Ok(())
    }
}

fn main() -> std::io::Result<()> {
    let mut a = std::env::args().skip(1);
    let units: u32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(60_000);
    let seed: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let out_dir = a.next().unwrap_or_else(|| ".".into());

    let sc = Scenario::head_on(units, INF_SWORD, seed, 20_000);
    let mut w = sc.build();

    // 개전 시점 배치를 다 담도록 시야를 잡고, 전투 내내 고정한다
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in &w.pool.pos {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    let pad = 30.0;
    // 이미지 종횡비에 맞춰 시야를 넓힌다
    let (mut cx, mut cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let mut half_w = (x1 - x0) * 0.5 + pad;
    let mut half_h = (y1 - y0) * 0.5 + pad;
    let aspect = W as f32 / H as f32;
    if half_w / half_h < aspect {
        half_w = half_h * aspect;
    } else {
        half_h = half_w / aspect;
    }
    let _ = (&mut cx, &mut cy);
    let mut canvas = Canvas::new([cx - half_w, cy - half_h, cx + half_w, cy + half_h]);

    let shots = [0u64, 400, 800, 1200, 1800, 2600];
    let mut next = 0usize;
    for t in 0..=*shots.last().unwrap() {
        if next < shots.len() && t == shots[next] {
            canvas.snap_units(&w);
            let path = format!("{out_dir}/frame_{:04}.ppm", shots[next]);
            canvas.write_ppm(&path)?;
            println!(
                "{path}  t={:<5} 생존 {:>7} / {:>7}",
                t, w.stats.alive[0], w.stats.alive[1]
            );
            next += 1;
        }
        w.step();
        canvas.accumulate_corpses(&w);
    }
    Ok(())
}
