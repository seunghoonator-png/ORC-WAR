//! 전투 설정 — 어떤 병종이 어디에 몇 명 서는가.
//!
//! 유저가 UI로 만드는 값이자, 헤드리스 회귀 테스트가 코드로 만드는 값이다.

use crate::sim::pool::UnitPool;
use crate::sim::unit_types::stats;
use crate::sim::{World, WORLD_SIZE};

/// 한 덩어리의 병력 배치
#[derive(Clone, Debug)]
pub struct Formation {
    pub type_id: u8,
    pub team: u8,
    pub count: u32,
    /// 대형 중심 (m)
    pub center: [f32; 2],
    /// 대형 정면 폭 (m)
    pub width: f32,
    /// 대형이 바라보는 방향(적 쪽) 단위 벡터.
    ///
    /// 병력 수가 열 수로 나누어떨어지지 않으면 마지막 행이 빈다. 그 빈 행이
    /// 어느 쪽에 놓이는지가 승패를 가른다 — 최전선에 놓이면 개전 즉시 전선에
    /// 구멍이 뚫린 채 시작하기 때문이다. 이 방향을 알아야 빈 행을 후방으로 보낼 수 있다.
    pub front: [f32; 2],
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pub name: String,
    pub seed: u64,
    pub formations: Vec<Formation>,
    pub max_ticks: u64,
}

impl Scenario {
    pub fn total_units(&self) -> u32 {
        self.formations.iter().map(|f| f.count).sum()
    }

    /// 평지에서 두 진영이 정면으로 맞붙는 기본 시나리오.
    pub fn head_on(total: u32, type_id: u8, seed: u64, max_ticks: u64) -> Self {
        let half = total / 2;
        let mid = WORLD_SIZE * 0.5;
        // 병력이 늘수록 전선도 넓어져야 밀도가 유지된다
        let width = (half as f32).sqrt() * 4.0;
        // 대형 깊이를 재서 진영 간격을 잡는다. 고정 간격을 쓰면 소규모 전투는
        // 접적까지 몇 분씩 걸리고 대규모는 시작하자마자 겹친다.
        let s = stats(type_id);
        let spacing = s.radius * 2.3;
        let cols = ((width / spacing).floor() as u32).max(1);
        let depth = (half.div_ceil(cols)) as f32 * spacing;
        let offset = depth * 0.5 + 40.0;
        Self {
            name: format!("head_on_{}", total),
            seed,
            formations: vec![
                Formation {
                    type_id,
                    team: 0,
                    count: half,
                    center: [mid, mid - offset],
                    width,
                    front: [0.0, 1.0],
                },
                Formation {
                    type_id,
                    team: 1,
                    count: total - half,
                    center: [mid, mid + offset],
                    width,
                    front: [0.0, -1.0],
                },
            ],
            max_ticks,
        }
    }

    /// 여러 병종을 섞은 전형적인 야전 편성. 앞에 보병, 뒤에 사수, 양익에 기병.
    pub fn combined_arms(total: u32, seed: u64, max_ticks: u64) -> Self {
        use crate::sim::unit_types::{ARCHER, CAV_HEAVY, CAV_LIGHT, INF_SPEAR, INF_SWORD};
        let half = total / 2;
        let mid = WORLD_SIZE * 0.5;
        // 전열 / 창병 / 사수 / 중기병 / 경기병
        let mix: [(u8, f32); 5] = [
            (INF_SWORD, 0.42),
            (INF_SPEAR, 0.18),
            (ARCHER, 0.22),
            (CAV_HEAVY, 0.10),
            (CAV_LIGHT, 0.08),
        ];
        let line_width = (half as f32).sqrt() * 4.0;
        let mut formations = Vec::new();
        for team in 0..2u8 {
            let sign = if team == 0 { 1.0 } else { -1.0 };
            let front = [0.0, sign];
            // 전열에서 뒤로 물러날수록 깊이가 쌓인다
            let mut depth_off = 0.0f32;
            for (slot, (type_id, share)) in mix.iter().enumerate() {
                let count = (half as f32 * share) as u32;
                if count == 0 {
                    continue;
                }
                let st = stats(*type_id);
                let spacing = st.radius * 2.3;
                let is_cav = st.is_cavalry;
                // 기병은 양익으로 빠진다
                let (width, lateral) = if is_cav {
                    let w = line_width * 0.22;
                    let side = if slot % 2 == 0 { 1.0 } else { -1.0 };
                    (w, side * (line_width * 0.5 + w * 0.6))
                } else {
                    (line_width, 0.0)
                };
                let cols = ((width / spacing).floor() as u32).max(1);
                let depth = (count.div_ceil(cols)) as f32 * spacing;
                let center_y = if is_cav {
                    mid - sign * 45.0
                } else {
                    mid - sign * (45.0 + depth_off + depth * 0.5)
                };
                if !is_cav {
                    depth_off += depth;
                }
                formations.push(Formation {
                    type_id: *type_id,
                    team,
                    count,
                    center: [mid + lateral, center_y],
                    width,
                    front,
                });
            }
        }
        Self {
            name: format!("combined_arms_{total}"),
            seed,
            formations,
            max_ticks,
        }
    }

    /// 서로 다른 병종을 맞붙인다. 상성 검증용.
    pub fn matchup(a: (u8, u32), b: (u8, u32), seed: u64, max_ticks: u64, gap: f32) -> Self {
        let mid = WORLD_SIZE * 0.5;
        let make = |(type_id, count): (u8, u32), team: u8, front: [f32; 2]| {
            let s = stats(type_id);
            let spacing = s.radius * 2.3;
            let width = (count as f32).sqrt() * spacing * 4.0;
            let cols = ((width / spacing).floor() as u32).max(1);
            let depth = (count.div_ceil(cols)) as f32 * spacing;
            let off = depth * 0.5 + gap * 0.5;
            Formation {
                type_id,
                team,
                count,
                center: [mid, mid - front[1] * off],
                width,
                front,
            }
        };
        Self {
            name: format!("{} vs {}", stats(a.0).name, stats(b.0).name),
            seed,
            formations: vec![make(a, 0, [0.0, 1.0]), make(b, 1, [0.0, -1.0])],
            max_ticks,
        }
    }

    /// 같은 대치를 동서 축으로 세운 것. 좌표축에 얽힌 편향을 가려내는 데 쓴다.
    pub fn head_on_x(total: u32, type_id: u8, seed: u64, max_ticks: u64) -> Self {
        let mut sc = Self::head_on(total, type_id, seed, max_ticks);
        let mid = WORLD_SIZE * 0.5;
        for f in sc.formations.iter_mut() {
            let off = f.center[1] - mid;
            f.center = [mid + off, mid];
            f.front = [f.front[1], f.front[0]];
        }
        sc
    }

    pub fn build(&self) -> World {
        let mut w = World::new(self.seed, self.total_units() as usize);
        for (fi, f) in self.formations.iter().enumerate() {
            spawn_block(&mut w.pool, f, self.seed, fi as u64);
        }
        w.finalize_spawns();
        w
    }
}

/// 직사각형 대형으로 채워 넣는다.
///
/// 최전방 행부터 채워서 인원이 모자란 마지막 행이 후방으로 가게 하고, 그 행은
/// 가운데 정렬한다. 격자에는 결정론적 지터를 섞어 줄이 너무 반듯해 보이지 않게 한다.
fn spawn_block(pool: &mut UnitPool, f: &Formation, seed: u64, salt: u64) {
    if f.count == 0 {
        return;
    }
    let s = stats(f.type_id);
    let spacing = s.radius * 2.3;
    let cols = ((f.width / spacing).floor() as u32).max(1);
    let rows = f.count.div_ceil(cols);

    // 전방 축과 그에 수직인 횡대 축
    let fl = (f.front[0] * f.front[0] + f.front[1] * f.front[1]).sqrt();
    let fwd = if fl > 1e-6 {
        [f.front[0] / fl, f.front[1] / fl]
    } else {
        [0.0, 1.0]
    };
    let right = [fwd[1], -fwd[0]];
    // 중심에서 최전방 행까지의 거리
    let front_off = (rows as f32 - 1.0) * spacing * 0.5;

    for k in 0..f.count {
        let r = k / cols;
        let c = k % cols;
        // 이 행에 실제로 서는 인원 — 마지막 행은 모자랄 수 있다
        let row_n = cols.min(f.count - r * cols);
        // 모자란 행은 가운데로 모은다
        let lateral = (c as f32 - (row_n as f32 - 1.0) * 0.5) * spacing;
        let depth = front_off - r as f32 * spacing;

        let jx = crate::rng::signed_f32(seed ^ 0x5EED, salt, k as u64) * spacing * 0.25;
        let jy = crate::rng::signed_f32(seed ^ 0x5EED, salt + 1, k as u64) * spacing * 0.25;
        let p = [
            (f.center[0] + fwd[0] * depth + right[0] * lateral + jx).clamp(1.0, WORLD_SIZE - 1.0),
            (f.center[1] + fwd[1] * depth + right[1] * lateral + jy).clamp(1.0, WORLD_SIZE - 1.0),
        ];
        pool.spawn(f.type_id, f.team, p, f.team as u16);
    }
}
