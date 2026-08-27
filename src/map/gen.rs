//! 지형 절차 생성.
//!
//! 같은 씨앗이면 같은 전장이 나와야 하므로 난수는 전부 좌표 해시로 만든다.

use crate::map::{Terrain, TerrainMap};
use crate::rng::unit_f32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapKind {
    /// 개활지. 순수한 병력 대결
    Plains,
    /// 완만한 언덕 — 고지를 쥔 쪽이 유리하다
    Hills,
    /// 절벽과 좁은 협곡. 소수가 다수를 막을 수 있는 유일한 지형
    Mountain,
}

#[derive(Clone, Copy, Debug)]
pub struct MapOptions {
    pub kind: MapKind,
    /// 전장을 가로지르는 강 (여울 두세 곳으로만 건널 수 있다)
    pub river: bool,
    pub forest: bool,
    pub rocks: bool,
}

impl Default for MapOptions {
    fn default() -> Self {
        Self {
            kind: MapKind::Plains,
            river: false,
            forest: false,
            rocks: false,
        }
    }
}

/// 격자 값 노이즈 — 격자점마다 난수를 두고 이중선형으로 잇는다.
fn value_noise(x: f32, y: f32, seed: u64, freq: f32) -> f32 {
    let fx = x * freq;
    let fy = y * freq;
    let x0 = fx.floor();
    let y0 = fy.floor();
    let tx = fx - x0;
    let ty = fy - y0;
    // 부드럽게 잇는다 (smoothstep)
    let sx = tx * tx * (3.0 - 2.0 * tx);
    let sy = ty * ty * (3.0 - 2.0 * ty);
    let at = |ix: f32, iy: f32| -> f32 {
        unit_f32(
            seed,
            (ix as i64 as u64).wrapping_mul(0x9E37),
            iy as i64 as u64,
        )
    };
    let a = at(x0, y0);
    let b = at(x0 + 1.0, y0);
    let c = at(x0, y0 + 1.0);
    let d = at(x0 + 1.0, y0 + 1.0);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

/// 여러 배율을 겹쳐 자연스러운 기복을 만든다.
fn fbm(x: f32, y: f32, seed: u64, base_freq: f32, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = base_freq;
    let mut norm = 0.0;
    for o in 0..octaves {
        sum += value_noise(x, y, seed ^ (o as u64 * 0x51ED), freq) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

pub fn generate(world_size: f32, opts: MapOptions, seed: u64) -> TerrainMap {
    let mut m = TerrainMap::flat(world_size);
    let mid = world_size * 0.5;

    // --- 고도 ---
    match opts.kind {
        MapKind::Plains => {
            // 미세한 기복만. 눈에 띄지 않지만 완전히 평평하지도 않다
            for cy in 0..m.h {
                for cx in 0..m.w {
                    let (x, y) = (cx as f32 * m.cell, cy as f32 * m.cell);
                    m.height[cy * m.w + cx] = fbm(x, y, seed, 0.0016, 3) * 6.0;
                }
            }
        }
        MapKind::Hills => {
            for cy in 0..m.h {
                for cx in 0..m.w {
                    let (x, y) = (cx as f32 * m.cell, cy as f32 * m.cell);
                    // 전장 한복판에 능선 하나를 세우고 기복을 얹는다.
                    // 고지를 먼저 쥐는 쪽이 유리해지는 것이 이 지형의 요점이다.
                    let ridge = (-(((y - mid) / 220.0).powi(2))).exp() * 42.0;
                    m.height[cy * m.w + cx] = ridge + fbm(x, y, seed, 0.0022, 4) * 22.0;
                }
            }
        }
        MapKind::Mountain => {
            for cy in 0..m.h {
                for cx in 0..m.w {
                    let (x, y) = (cx as f32 * m.cell, cy as f32 * m.cell);
                    let n = fbm(x, y, seed, 0.0018, 4);
                    m.height[cy * m.w + cx] = n * 260.0;
                }
            }
            carve_passes(&mut m, seed);
        }
    }

    // --- 통행 불가 지대 ---
    if opts.kind == MapKind::Mountain {
        // 일정 고도 위는 절벽이다
        for i in 0..m.kind.len() {
            if m.height[i] > 150.0 {
                m.kind[i] = Terrain::Rock;
            }
        }
    }

    if opts.river {
        carve_river(&mut m, seed);
    }
    if opts.forest {
        scatter_forest(&mut m, seed);
    }
    if opts.rocks {
        scatter_rocks(&mut m, seed);
    }

    // 양측 배치 구역은 반드시 열어 둔다. 개전하자마자 절벽에 갇히면 곤란하다
    clear_deployment(&mut m, world_size);
    m
}

/// 산악 지형에 협곡 통로를 낸다. 통로가 없으면 그냥 벽 두 개일 뿐이다.
fn carve_passes(m: &mut TerrainMap, seed: u64) {
    let n_pass = 3;
    let span = m.w as f32 * m.cell;
    for k in 0..n_pass {
        let t = (k as f32 + 0.5) / n_pass as f32;
        let cx = span * t + (unit_f32(seed ^ 0x9A54, k, 0) - 0.5) * span * 0.12;
        let width = 90.0 + unit_f32(seed ^ 0x9A55, k, 1) * 60.0;
        for cyi in 0..m.h {
            for cxi in 0..m.w {
                let x = cxi as f32 * m.cell;
                let d = (x - cx).abs();
                if d < width {
                    let i = cyi * m.w + cxi;
                    // 통로 한가운데일수록 깊게 깎는다
                    let k = 1.0 - (d / width);
                    m.height[i] *= 1.0 - k * 0.92;
                }
            }
        }
    }
}

/// 전장을 가로지르는 강과 건널목.
fn carve_river(m: &mut TerrainMap, seed: u64) {
    let span = m.h as f32 * m.cell;
    let mid = span * 0.5;
    let n_ford = 3;
    for cy in 0..m.h {
        for cx in 0..m.w {
            let x = cx as f32 * m.cell;
            let y = cy as f32 * m.cell;
            // 굽이치는 물길
            let course = mid + (fbm(x, 0.0, seed ^ 0x8172, 0.0009, 3) - 0.5) * 150.0;
            let half_width = 26.0 + fbm(x, 100.0, seed ^ 0x3311, 0.002, 2) * 18.0;
            if (y - course).abs() < half_width {
                let i = cy * m.w + cx;
                m.kind[i] = Terrain::Water;
                m.height[i] -= 4.0;
            }
        }
    }
    // 건널목을 몇 군데 뚫는다
    for k in 0..n_ford {
        let t = (k as f32 + 0.5) / n_ford as f32;
        let fx = span * t + (unit_f32(seed ^ 0xF0DD, k, 0) - 0.5) * span * 0.2;
        let fw = 55.0 + unit_f32(seed ^ 0xF0DE, k, 1) * 45.0;
        for cy in 0..m.h {
            for cx in 0..m.w {
                let x = cx as f32 * m.cell;
                if (x - fx).abs() < fw {
                    let i = cy * m.w + cx;
                    if m.kind[i] == Terrain::Water {
                        m.kind[i] = Terrain::Ford;
                    }
                }
            }
        }
    }
}

fn scatter_forest(m: &mut TerrainMap, seed: u64) {
    for cy in 0..m.h {
        for cx in 0..m.w {
            let i = cy * m.w + cx;
            if m.kind[i] != Terrain::Plain {
                continue;
            }
            let (x, y) = (cx as f32 * m.cell, cy as f32 * m.cell);
            if fbm(x, y, seed ^ 0xF07E, 0.0035, 3) > 0.62 {
                m.kind[i] = Terrain::Forest;
            }
        }
    }
}

fn scatter_rocks(m: &mut TerrainMap, seed: u64) {
    for cy in 0..m.h {
        for cx in 0..m.w {
            let i = cy * m.w + cx;
            if m.kind[i] != Terrain::Plain {
                continue;
            }
            let (x, y) = (cx as f32 * m.cell, cy as f32 * m.cell);
            if fbm(x, y, seed ^ 0x8043, 0.006, 2) > 0.80 {
                m.kind[i] = Terrain::Rock;
            }
        }
    }
}

/// 양 진영이 늘어서는 띠는 평지로 비워 둔다.
fn clear_deployment(m: &mut TerrainMap, world_size: f32) {
    let mid = world_size * 0.5;
    let band = 130.0;
    for cy in 0..m.h {
        let y = cy as f32 * m.cell;
        let d = (y - mid).abs();
        if !(180.0..=180.0 + band).contains(&d) {
            continue;
        }
        for cx in 0..m.w {
            let i = cy * m.w + cx;
            if !m.kind[i].passable() {
                m.kind[i] = Terrain::Plain;
            }
        }
    }
}
