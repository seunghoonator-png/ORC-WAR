//! 발사체 — 화살, 볼트, 투사물.
//!
//! 30만 전장에서 화살은 수만 발이 동시에 날아간다. 매 틱 궤적을 적분하면
//! 감당이 안 되므로, 쏘는 순간 낙하 지점과 비행 시간을 확정하고 그 사이에는
//! 아무것도 하지 않는다. 착탄하는 틱에만 그 지점을 조회한다.
//!
//! 정확도를 산포로 표현하는 것이 핵심이다. 빗나간 화살도 그 자리에 누군가
//! 있으면 맞는다. 그래서 밀집 대형일수록 화살비가 재앙이 된다.

use rayon::prelude::*;

use crate::sim::pool::{UnitState, NO_TARGET};
use crate::sim::unit_types::stats;
use crate::sim::{World, CHUNK, DT};

/// 동시에 날 수 있는 발사체 상한
pub const MAX_PROJECTILES: usize = 200_000;
/// 착탄 지점에서 맞을 수 있는 반경(m)
const HIT_RADIUS: f32 = 0.6;
/// 최대 사거리에서의 산포(m). 가까울수록 정확해진다
const SPREAD_AT_MAX: f32 = 7.0;
/// 화살이 나는 속도(m/s) — 비행 시간 계산용
const ARROW_SPEED: f32 = 55.0;
/// 사격 판단 주기(틱). 실제 발사 속도는 병종의 공격 주기가 정한다 —
/// 이 값만으로 쏘게 두면 궁수가 초당 다섯 발을 날려 전장을 혼자 정리한다.
const FIRE_PERIOD: u64 = 2;
/// 이 거리 안으로 적이 들어오면 활을 버리고 근접전으로 넘어간다(m)
const MELEE_THRESHOLD: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Shot {
    Arrow = 0,
    Bolt = 1,
}

pub struct ProjectilePool {
    /// 착탄 지점
    pub landing: Vec<[f32; 2]>,
    /// 발사 지점 — 렌더가 궤적을 그릴 때 쓴다
    pub origin: Vec<[f32; 2]>,
    /// 착탄하는 틱
    pub land_tick: Vec<u64>,
    /// 발사 틱 — 비행 진행률 계산용
    pub fire_tick: Vec<u64>,
    pub kind: Vec<u8>,
    pub team: Vec<u8>,
    pub damage: Vec<f32>,
    /// 갑옷 관통력 — 감쇄를 이만큼 무시한다
    pub pierce: Vec<f32>,
    live: Vec<bool>,
    free: Vec<u32>,
}

impl ProjectilePool {
    pub fn new() -> Self {
        Self {
            landing: Vec::new(),
            origin: Vec::new(),
            land_tick: Vec::new(),
            fire_tick: Vec::new(),
            kind: Vec::new(),
            team: Vec::new(),
            damage: Vec::new(),
            pierce: Vec::new(),
            live: Vec::new(),
            free: Vec::new(),
        }
    }

    pub fn live_count(&self) -> usize {
        self.live.iter().filter(|l| **l).count()
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn(
        &mut self,
        origin: [f32; 2],
        landing: [f32; 2],
        fire_tick: u64,
        land_tick: u64,
        kind: Shot,
        team: u8,
        damage: f32,
        pierce: f32,
    ) {
        if let Some(idx) = self.free.pop() {
            let i = idx as usize;
            self.origin[i] = origin;
            self.landing[i] = landing;
            self.fire_tick[i] = fire_tick;
            self.land_tick[i] = land_tick;
            self.kind[i] = kind as u8;
            self.team[i] = team;
            self.damage[i] = damage;
            self.pierce[i] = pierce;
            self.live[i] = true;
        } else if self.live.len() < MAX_PROJECTILES {
            self.origin.push(origin);
            self.landing.push(landing);
            self.fire_tick.push(fire_tick);
            self.land_tick.push(land_tick);
            self.kind.push(kind as u8);
            self.team.push(team);
            self.damage.push(damage);
            self.pierce.push(pierce);
            self.live.push(true);
        }
    }

    /// 렌더용: 살아있는 발사체의 현재 위치와 비행 진행률(0~1).
    pub fn for_each_in_flight<F: FnMut([f32; 2], f32, u8)>(&self, tick: u64, mut f: F) {
        for i in 0..self.live.len() {
            if !self.live[i] || self.land_tick[i] <= tick {
                continue;
            }
            let total = (self.land_tick[i] - self.fire_tick[i]).max(1) as f32;
            let t = (tick - self.fire_tick[i]) as f32 / total;
            let p = [
                self.origin[i][0] + (self.landing[i][0] - self.origin[i][0]) * t,
                self.origin[i][1] + (self.landing[i][1] - self.origin[i][1]) * t,
            ];
            f(p, t, self.kind[i]);
        }
    }
}

impl Default for ProjectilePool {
    fn default() -> Self {
        Self::new()
    }
}

pub fn step(w: &mut World) {
    fire(w);
    land(w);
}

/// 사거리 안에 표적이 있는 사수들이 쏜다.
fn fire(w: &mut World) {
    let n = w.pool.len();
    let tick = w.tick;
    let seed = w.seed;
    let grid = &w.grid;
    let pos = &w.pool.pos;
    let team = &w.pool.team;
    let type_id = &w.pool.type_id;
    let state = &w.pool.state;
    let stagger = &w.pool.stagger;
    let cooldown = w.pool.cooldown.clone();
    let ammo = w.pool.ammo.clone();

    // 각 사수가 어디를 겨눌지 고른다
    let shots: Vec<Vec<(u32, [f32; 2])>> = (0..n.div_ceil(CHUNK))
        .into_par_iter()
        .map(|ci| {
            let lo = ci * CHUNK;
            let hi = ((ci + 1) * CHUNK).min(n);
            let mut out = Vec::new();
            for i in lo..hi {
                let s = stats(type_id[i]);
                if s.range <= 0.0 || ammo[i] == 0 || !(tick + i as u64).is_multiple_of(FIRE_PERIOD) {
                    continue;
                }
                if !matches!(state[i], UnitState::Advance | UnitState::Fight)
                    || stagger[i] > 0
                    || cooldown[i] > 0
                {
                    continue;
                }
                let p = pos[i];
                let my_team = team[i];

                // 코앞에 적이 있으면 활을 쏠 계제가 아니다
                let mut threatened = false;
                grid.for_each_near(p, MELEE_THRESHOLD, |j| {
                    let ju = j as usize;
                    if team[ju] != my_team {
                        let d = [pos[ju][0] - p[0], pos[ju][1] - p[1]];
                        if d[0] * d[0] + d[1] * d[1] < MELEE_THRESHOLD * MELEE_THRESHOLD {
                            threatened = true;
                        }
                    }
                });
                if threatened {
                    continue;
                }

                // 사거리 안에서 표적을 하나 고른다. 굳이 가장 가까운 적을
                // 찾지 않는다 — 화살은 사람을 조준하기보다 대열을 향해 쏜다.
                let r = s.range;
                let mut pick: Option<[f32; 2]> = None;
                let mut seen = 0u32;
                let want = 1 + crate::rng::below(seed ^ 0xA110, tick, i as u64, 8);
                grid.for_each_near(p, r, |j| {
                    let ju = j as usize;
                    if team[ju] == my_team {
                        return;
                    }
                    let d = [pos[ju][0] - p[0], pos[ju][1] - p[1]];
                    let d2 = d[0] * d[0] + d[1] * d[1];
                    if d2 > r * r || d2 < MELEE_THRESHOLD * MELEE_THRESHOLD {
                        return;
                    }
                    seen += 1;
                    if seen <= want || pick.is_none() {
                        pick = Some(pos[ju]);
                    }
                });
                if let Some(aim) = pick {
                    out.push((i as u32, aim));
                }
            }
            out
        })
        .collect();

    // 발사체 생성 (순차 — 풀 인덱스 배분이 결정론이어야 한다)
    for chunk in &shots {
        for &(i, aim) in chunk {
            let iu = i as usize;
            let s = stats(type_id[iu]);
            let p = pos[iu];
            let d = [aim[0] - p[0], aim[1] - p[1]];
            let dist = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1.0);

            // 멀수록 크게 흩어진다
            let spread = SPREAD_AT_MAX * (dist / s.range).min(1.0);
            let sx = crate::rng::signed_f32(seed ^ 0x5A17, tick, i as u64) * spread;
            let sy = crate::rng::signed_f32(seed ^ 0x5A18, tick, i as u64) * spread;
            let landing = [aim[0] + sx, aim[1] + sy];

            let flight = (dist / ARROW_SPEED / DT).ceil() as u64;
            let kind = if s.attack_period > 40 {
                Shot::Bolt
            } else {
                Shot::Arrow
            };
            // 석궁 볼트는 갑옷을 잘 뚫는다
            let (dmg, pierce) = match kind {
                Shot::Bolt => (34.0, 0.45),
                Shot::Arrow => (20.0, 0.1),
            };
            w.projectiles.spawn(
                p,
                landing,
                tick,
                tick + flight.max(1),
                kind,
                team[iu],
                dmg,
                pierce,
            );
            w.pool.cooldown[iu] = s.attack_period;
            w.pool.ammo[iu] = w.pool.ammo[iu].saturating_sub(1);
        }
    }
}

/// 이번 틱에 떨어지는 발사체를 판정한다.
fn land(w: &mut World) {
    let tick = w.tick;
    let seed = w.seed;

    let mut hits: Vec<(u32, f32)> = Vec::new();
    let mut spent: Vec<u32> = Vec::new();

    for i in 0..w.projectiles.live.len() {
        if !w.projectiles.live[i] || w.projectiles.land_tick[i] != tick {
            continue;
        }
        spent.push(i as u32);

        let at = w.projectiles.landing[i];
        let shooter_team = w.projectiles.team[i];
        let dmg = w.projectiles.damage[i];
        let pierce = w.projectiles.pierce[i];

        // 착탄 지점에서 가장 가까운 하나만 맞는다. 아군이 서 있으면 아군이 맞는다.
        let mut best = f32::MAX;
        let mut victim = NO_TARGET;
        w.grid.for_each_near(at, HIT_RADIUS, |j| {
            let ju = j as usize;
            let d = [w.pool.pos[ju][0] - at[0], w.pool.pos[ju][1] - at[1]];
            let d2 = d[0] * d[0] + d[1] * d[1];
            if d2 <= HIT_RADIUS * HIT_RADIUS && (d2 < best || (d2 == best && j < victim)) {
                best = d2;
                victim = j;
            }
        });
        if victim == NO_TARGET {
            continue;
        }
        let v = victim as usize;
        let s = stats(w.pool.type_id[v]);

        // 방패는 화살을 상당히 막아준다. 다만 곡사로 떨어지는 것은 절반만.
        let mut d = dmg;
        if s.shield > 0.0 {
            let roll = crate::rng::unit_f32(seed ^ 0xB0B0, tick, victim as u64);
            if roll < s.shield * 0.8 {
                continue;
            }
        }
        d *= 1.0 - (s.armor - pierce).max(0.0);
        // 아군 오사도 그대로 들어간다
        let _ = shooter_team;
        hits.push((victim, d));
    }

    for i in spent {
        w.projectiles.live[i as usize] = false;
        w.projectiles.free.push(i);
    }

    let mut deaths: [u32; 2] = [0, 0];
    for &(v, d) in &hits {
        w.pool.hp[v as usize] -= d;
    }
    for &(v, _) in &hits {
        let vu = v as usize;
        if w.pool.hp[vu] <= 0.0 && w.pool.state[vu] != UnitState::Dead {
            w.pool.state[vu] = UnitState::Dead;
            w.pool.target[vu] = NO_TARGET;
            deaths[w.pool.team[vu] as usize] += 1;
            w.death_events
                .push((w.pool.pos[vu], w.pool.team[vu], w.pool.type_id[vu]));
        }
    }
    w.stats.dead[0] += deaths[0];
    w.stats.dead[1] += deaths[1];
    w.stats.shots_landed += hits.len() as u64;
}
