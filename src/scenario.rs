//! 전투 설정 — 어떤 병종이 어디에 몇 명 서는가.
//!
//! 유저가 UI로 만드는 값이자, 헤드리스 회귀 테스트가 코드로 만드는 값이다.

use crate::map::castle::Castle;
use crate::map::gen::MapOptions;
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
    pub map: MapOptions,
    /// 공성전이면 성곽 크기(반폭)와 해자 여부
    pub castle: Option<([f32; 2], bool)>,
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
            map: MapOptions::default(),
            castle: None,
        }
    }

    /// 공성전 — 성에 틀어박힌 수비군과 그것을 뜯어내려는 공격군.
    ///
    /// 공격측은 남쪽에서 올라오며 성문을 마주한다. 성벽을 부술 병기가 없으면
    /// 아무리 많아도 기어오르다 갈릴 뿐이다.
    pub fn siege(attackers: u32, defenders: u32, seed: u64, max_ticks: u64, moat: bool) -> Self {
        use crate::sim::unit_types::{ARCHER, INF_SPEAR, INF_SWORD};
        let mix = [(INF_SWORD, 0.62), (ARCHER, 0.24), (INF_SPEAR, 0.14)];
        Self::siege_with_mix(attackers, defenders, seed, max_ticks, moat, &mix)
    }

    /// 공격측 보병 편성을 직접 주는 공성전.
    pub fn siege_with_mix(
        attackers: u32,
        defenders: u32,
        seed: u64,
        max_ticks: u64,
        moat: bool,
        foot_mix: &[(u8, f32)],
    ) -> Self {
        use crate::sim::unit_types::{ARCHER, CATAPULT, INF_SPEAR, INF_SWORD, LADDER, RAM};
        let mid = WORLD_SIZE * 0.5;
        // 성은 안에 들어설 병력에 맞춰 커진다. 크기를 고정해 두면 수비가 늘어날수록
        // 성 안에서 서로 겹쳐 서고, 밀어내기 계산이 폭발한다.
        let area = (defenders as f32 * 4.0).max(60_000.0);
        let hx = (area * 1.35 / 4.0).sqrt().max(140.0);
        let hy = (hx / 1.35).max(100.0);
        let half = [hx, hy];
        let mut formations = Vec::new();

        // --- 공격측: 성 남쪽에서 올라온다 ---
        let siege_train = 26u32; // 파성추 6, 투석기 8, 사다리 12
        let foot = attackers.saturating_sub(siege_train);
        let line_w = (foot as f32).sqrt() * 4.0;
        let mut depth_off = 0.0f32;
        for &(type_id, share) in foot_mix {
            let count = (foot as f32 * share) as u32;
            if count == 0 {
                continue;
            }
            let st = stats(type_id);
            let spacing = st.radius * 2.3;
            let cols = ((line_w / spacing).floor() as u32).max(1);
            let depth = (count.div_ceil(cols)) as f32 * spacing;
            formations.push(Formation {
                type_id,
                team: 0,
                count,
                center: [mid, mid - half[1] - 150.0 - depth_off - depth * 0.5],
                width: line_w,
                front: [0.0, 1.0],
            });
            depth_off += depth;
        }
        // 공성 열차는 보병 뒤에 선다
        for (type_id, count, back) in [
            (RAM, 6u32, 40.0f32),
            (LADDER, 12, 70.0),
            (CATAPULT, 8, 210.0),
        ] {
            formations.push(Formation {
                type_id,
                team: 0,
                count,
                center: [mid, mid - half[1] - 150.0 - back],
                width: 160.0,
                front: [0.0, 1.0],
            });
        }

        // --- 방어측: 성벽에 붙어 지키고, 안뜰에 예비를 둔다 ---
        // 성벽 수비는 궁수 위주다. 위에서 쏘는 화살이 가장 값싸게 사람을 줄인다
        let wall_share = 0.62;
        let wall_n = (defenders as f32 * wall_share) as u32;
        let yard_n = defenders - wall_n;
        let sides: [([f32; 2], [f32; 2], f32); 4] = [
            ([mid, mid - half[1] + 18.0], [0.0, -1.0], half[0] * 1.7),
            ([mid, mid + half[1] - 18.0], [0.0, 1.0], half[0] * 1.7),
            ([mid - half[0] + 18.0, mid], [-1.0, 0.0], half[1] * 1.7),
            ([mid + half[0] - 18.0, mid], [1.0, 0.0], half[1] * 1.7),
        ];
        // 성문이 있는 남쪽에 절반, 나머지 세 면에 나눠 세운다
        let weights = [0.4f32, 0.2, 0.2, 0.2];
        for (k, (center, front, width)) in sides.iter().enumerate() {
            let n = (wall_n as f32 * weights[k]) as u32;
            if n == 0 {
                continue;
            }
            let archers = n * 3 / 4;
            formations.push(Formation {
                type_id: ARCHER,
                team: 1,
                count: archers,
                center: *center,
                width: *width,
                front: *front,
            });
            formations.push(Formation {
                type_id: INF_SWORD,
                team: 1,
                count: n - archers,
                center: [center[0] - front[0] * 14.0, center[1] - front[1] * 14.0],
                width: *width,
                front: *front,
            });
        }
        // 안뜰 예비대 — 돌파구를 막으러 달려간다
        formations.push(Formation {
            type_id: INF_SPEAR,
            team: 1,
            count: yard_n,
            center: [mid, mid],
            width: half[0] * 1.2,
            front: [0.0, -1.0],
        });

        Self {
            name: format!("siege_{attackers}v{defenders}"),
            seed,
            formations,
            max_ticks,
            map: MapOptions::default(),
            castle: Some((half, moat)),
        }
    }

    /// 지형을 지정한다.
    pub fn on_map(mut self, opts: MapOptions) -> Self {
        self.map = opts;
        self
    }

    /// 여러 병종을 섞은 전형적인 야전 편성. 앞에 보병, 뒤에 사수, 양익에 기병.
    pub fn combined_arms(total: u32, seed: u64, max_ticks: u64) -> Self {
        use crate::config::Doctrine;
        Self::field_with_mix(total, Doctrine::Balanced.mix(), seed, max_ticks)
    }

    /// 병종 비율을 직접 주는 야전 편성.
    ///
    /// 양측이 같은 편성을 쓴다. 서로 다른 편성을 붙이면 재미는 있겠지만, 이
    /// 시뮬레이터가 보려는 것은 "어느 쪽이 유리한 설정인가"가 아니라 "같은 조건에서
    /// 무엇이 얼마나 강한가"다.
    pub fn field_with_mix(total: u32, mix: &[(u8, f32)], seed: u64, max_ticks: u64) -> Self {
        let half = total / 2;
        let mid = WORLD_SIZE * 0.5;
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
            name: format!("field_{total}"),
            seed,
            formations,
            max_ticks,
            map: MapOptions::default(),
            castle: None,
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
            map: MapOptions::default(),
            castle: None,
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
        w.set_terrain(self.map, self.seed);
        if let Some((half, moat)) = self.castle {
            w.place_castle(Castle::square(
                [WORLD_SIZE * 0.5, WORLD_SIZE * 0.5],
                half,
                moat,
            ));
        }
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
