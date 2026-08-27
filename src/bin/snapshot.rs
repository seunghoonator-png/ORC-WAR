//! 전장 스냅샷 — 전투를 돌리며 일정 간격으로 화면을 이미지로 굽는다.
//!
//! GUI 없이 시뮬레이션이 제대로 보이는지 확인하는 용도이자, 나중에 결과
//! 리포트의 전황 축소도로도 쓴다. 화소 하나에 유닛 여럿이 겹치므로 밀도를
//! 밝기로 환산하는데, 이는 실제 렌더러가 최대 줌아웃에서 쓸 방식과 같다.

use std::fs::File;
use std::io::{BufWriter, Write};

use orc_war::map::Terrain;
use orc_war::scenario::Scenario;
use orc_war::sim::unit_types::stats;
use orc_war::sim::World;

const W: usize = 1000;
const H: usize = 460;

struct Canvas {
    /// 팀별 화소당 유닛 수
    density: Vec<[u16; 2]>,
    /// 기병이 있는 화소 — 돌격이 눈에 보이도록 밝게 찍는다
    horse: Vec<[u16; 2]>,
    /// 날고 있는 발사체
    shots: Vec<u8>,
    /// 지형 바탕색 — 전투 전에 한 번만 굽는다
    ground: Vec<[u8; 3]>,
    /// 가까이 당겨 볼 때 유닛을 두껍게 찍는다
    fat: bool,
    /// 시체 누적 — 전투가 지나간 자리는 지워지지 않는다
    corpses: Vec<u16>,
    view: [f32; 4],
}

impl Canvas {
    fn new(view: [f32; 4]) -> Self {
        Self {
            density: vec![[0; 2]; W * H],
            horse: vec![[0; 2]; W * H],
            shots: vec![0; W * H],
            ground: vec![[26, 34, 24]; W * H],
            fat: false,
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

    /// 지형을 화소마다 미리 칠해 둔다.
    fn bake_ground(&mut self, w: &World) {
        let [x0, y0, x1, y1] = self.view;
        for py in 0..H {
            for px in 0..W {
                let u = (px as f32 + 0.5) / W as f32;
                let v = 1.0 - (py as f32 + 0.5) / H as f32;
                let p = [x0 + (x1 - x0) * u, y0 + (y1 - y0) * v];
                let t = w.terrain.at(p);
                // 고도에 따라 밝기를 줘서 기복이 보이게 한다
                let h = w.terrain.height_at(p);
                let shade = (h * 0.5).clamp(-20.0, 70.0);
                let base = match t {
                    Terrain::Plain => [30.0, 42.0, 28.0],
                    Terrain::Forest => [18.0, 40.0, 20.0],
                    Terrain::Rock => [64.0, 62.0, 58.0],
                    Terrain::Water => [22.0, 42.0, 78.0],
                    Terrain::Ford => [58.0, 82.0, 96.0],
                    Terrain::Marsh => [40.0, 44.0, 30.0],
                };
                self.ground[py * W + px] = [
                    (base[0] + shade).clamp(0.0, 255.0) as u8,
                    (base[1] + shade).clamp(0.0, 255.0) as u8,
                    (base[2] + shade * 0.7).clamp(0.0, 255.0) as u8,
                ];
            }
        }
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
        self.horse.iter_mut().for_each(|d| *d = [0; 2]);
        self.shots.iter_mut().for_each(|s| *s = 0);
        for i in 0..w.pool.len() {
            if !w.pool.is_alive(i) {
                continue;
            }
            if let Some(px) = self.to_px(w.pool.pos[i]) {
                let t = w.pool.team[i] as usize;
                let cav = stats(w.pool.type_id[i]).is_cavalry;
                self.density[px][t] = self.density[px][t].saturating_add(1);
                if cav {
                    self.horse[px][t] = self.horse[px][t].saturating_add(1);
                }
                // 화소당 미터가 작을 때는 한 점으로는 보이지 않는다
                if self.fat {
                    for (dx, dy) in [(1i32, 0i32), (0, 1), (1, 1)] {
                        let q = px as i64 + dy as i64 * W as i64 + dx as i64;
                        if q >= 0 && (q as usize) < W * H {
                            let q = q as usize;
                            self.density[q][t] = self.density[q][t].saturating_add(1);
                            if cav {
                                self.horse[q][t] = self.horse[q][t].saturating_add(1);
                            }
                        }
                    }
                }
            }
        }
        let tick = w.tick;
        let shots = &mut self.shots;
        let view = self.view;
        w.projectiles.for_each_in_flight(tick, |p, _, _| {
            let [x0, y0, x1, y1] = view;
            let u = (p[0] - x0) / (x1 - x0);
            let v = 1.0 - (p[1] - y0) / (y1 - y0);
            if (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v) {
                let px = (v * H as f32) as usize * W + (u * W as f32) as usize;
                shots[px] = shots[px].saturating_add(1);
            }
        });
    }

    fn write_ppm(&self, path: &str) -> std::io::Result<()> {
        let f = File::create(path)?;
        let mut out = BufWriter::new(f);
        write!(out, "P6\n{} {}\n255\n", W, H)?;
        let mut buf = Vec::with_capacity(W * H * 3);
        for i in 0..W * H {
            let [a, b] = self.density[i];
            let corpse = self.corpses[i];
            let mut rgb = self.ground[i];
            if corpse > 0 {
                // 시체가 쌓일수록 검붉게 물든다
                let k = ((corpse as f32 / 3.0).min(1.0) * 255.0) as u8;
                rgb = [
                    rgb[0].saturating_add(k / 5),
                    rgb[1].saturating_sub(k / 8),
                    rgb[2].saturating_sub(k / 9),
                ];
            }
            let ramp = |n: u16| -> f32 { (n as f32 / 2.5).min(1.0) };
            let cav = self.horse[i];
            if a > 0 || b > 0 {
                let (ia, ib) = (ramp(a), ramp(b));
                if a >= b {
                    let boost = if cav[0] > 0 { 60.0 } else { 0.0 };
                    rgb = [
                        (90.0 + 165.0 * ia) as u8,
                        (30.0 + 30.0 * ia + boost) as u8,
                        (28.0 + 24.0 * ia + boost * 0.6) as u8,
                    ];
                } else {
                    let boost = if cav[1] > 0 { 70.0 } else { 0.0 };
                    rgb = [
                        (28.0 + 30.0 * ib + boost) as u8,
                        (60.0 + 70.0 * ib + boost) as u8,
                        (110.0 + 145.0 * ib) as u8,
                    ];
                }
            }
            if self.shots[i] > 0 {
                // 날고 있는 화살
                let k = (self.shots[i].min(3) as f32 / 3.0 * 130.0) as u8;
                rgb = [
                    rgb[0].saturating_add(k + 60),
                    rgb[1].saturating_add(k + 55),
                    rgb[2].saturating_add(k + 40),
                ];
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
    let map_name = a.next().unwrap_or_else(|| "plains".into());
    // 몇 틱마다, 몇 장을 뽑을지 — 애니메이션용
    let every: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let count: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    // 전장 한복판을 이 폭(m)으로 잘라 본다. 0이면 전체를 담는다.
    let zoom_w: f32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);

    use orc_war::map::gen::{MapKind, MapOptions};
    let opts = match map_name.as_str() {
        "hills" => MapOptions {
            kind: MapKind::Hills,
            ..Default::default()
        },
        "mountain" => MapOptions {
            kind: MapKind::Mountain,
            ..Default::default()
        },
        "river" => MapOptions {
            kind: MapKind::Plains,
            river: true,
            forest: true,
            ..Default::default()
        },
        "forest" => MapOptions {
            kind: MapKind::Plains,
            forest: true,
            rocks: true,
            ..Default::default()
        },
        _ => MapOptions::default(),
    };
    let sc = Scenario::combined_arms(units, seed, 20_000).on_map(opts);
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
    if zoom_w > 0.0 {
        half_w = zoom_w * 0.5;
        half_h = half_w / aspect;
    }
    let mut canvas = Canvas::new([cx - half_w, cy - half_h, cx + half_w, cy + half_h]);
    canvas.fat = (half_w * 2.0 / W as f32) < 0.6;
    canvas.bake_ground(&w);

    let shots: Vec<u64> = if every > 0 && count > 0 {
        (0..count).map(|i| i * every).collect()
    } else {
        vec![0, 500, 800, 1000, 1400, 2200]
    };
    let mut next = 0usize;
    for t in 0..=*shots.last().unwrap() {
        if next < shots.len() && t == shots[next] {
            canvas.snap_units(&w);
            let path = format!("{out_dir}/frame_{:05}.ppm", shots[next]);
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
