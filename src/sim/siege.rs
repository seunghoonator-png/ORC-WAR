//! 공성 — 성벽을 부수거나, 넘거나, 문을 여는 일.
//!
//! 성벽은 세 가지 방법으로 뚫린다. 투석기로 구간을 무너뜨려 잔해 경사로를
//! 만들거나, 파성추로 성문을 부수거나, 사다리를 걸고 기어오르는 것이다.
//! 앞의 둘은 느리지만 안전하고, 마지막 하나는 빠르지만 사람이 갈려 나간다.

use rayon::prelude::*;

use crate::map::Terrain;
use crate::sim::pool::{UnitState, NO_TARGET};
use crate::sim::unit_types::{is_engine, stats};
use crate::sim::{World, CHUNK, DT};

/// 성벽에 붙어 기어오르는 데 걸리는 시간(틱)
const CLIMB_TICKS: u16 = 200;
/// 사다리가 곁에 있으면 이만큼 빨라진다
const LADDER_SPEEDUP: u16 = 4;
/// 사다리가 등반을 도와주는 거리(m)
const LADDER_RANGE: f32 = 18.0;
/// 구조물을 때릴 수 있는 거리(m)
const REACH_MARGIN: f32 = 2.5;
/// 성 둘레 이 거리 밖은 공성과 무관하다 — 전 유닛을 훑지 않기 위한 울타리(m)
const CASTLE_MARGIN: f32 = 70.0;
/// 성벽 위에서 아래로 돌과 기름을 붓는 주기(틱)
const DROP_PERIOD: u64 = 50;
/// 한 사람이 한 번에 맞힐 수 있는 인원.
///
/// 반경 안을 전부 때리게 두면 수비 천 명이 매번 수천 명을 쓸어버려, 성문
/// 근처에 발도 못 붙인다. 돌 하나는 한둘을 맞힌다.
const DROP_TARGETS: u32 = 1;
/// 성벽에 붙어 지키는 것으로 보는 거리(m).
///
/// 흉벽에 바짝 붙어야만 인정하면 아무도 해당되지 않는다. 수비 대열은 성벽
/// 안쪽으로 십수 미터 물러나 서기 때문이다.
const WALL_GUARD_RANGE: f32 = 20.0;
/// 낙하물이 닿는 거리(m)
const DROP_RANGE: f32 = 14.0;
/// 낙하물 피해
const DROP_DAMAGE: f32 = 26.0;
/// 기어오르는 중에는 몸을 가릴 수 없다
const CLIMBING_DROP_MULT: f32 = 3.0;
/// 목표 구역을 이만큼 붙들고 있으면 성이 떨어진다(틱)
pub const HOLD_TO_WIN: u32 = 200;
/// 점령으로 인정하는 최소 인원
const HOLD_MIN_UNITS: u32 = 30;

/// 성벽 안쪽 면까지의 거리. 음수면 성 밖이다.
#[inline(always)]
fn wall_gap(p: [f32; 2], c: &crate::map::castle::Castle) -> f32 {
    let dx = c.half[0] - (p[0] - c.center[0]).abs();
    let dy = c.half[1] - (p[1] - c.center[1]).abs();
    dx.min(dy)
}

/// 성 둘레에 붙어 있는가 — 값싼 사각형 판정으로 대부분을 걸러낸다.
#[inline(always)]
fn near_castle(p: [f32; 2], c: &crate::map::castle::Castle) -> bool {
    (p[0] - c.center[0]).abs() < c.half[0] + CASTLE_MARGIN
        && (p[1] - c.center[1]).abs() < c.half[1] + CASTLE_MARGIN
}

pub fn step(w: &mut World) {
    if w.castle.is_none() {
        return;
    }
    batter_structures(w);
    climb_walls(w);
    pour_from_walls(w);
    check_objective(w);
}

/// 성벽과 성문을 두들긴다.
fn batter_structures(w: &mut World) {
    let n = w.pool.len();
    let castle = w.castle.as_ref().unwrap();
    let terrain = &w.terrain;
    let pool = &w.pool;

    // (구간 번호, 피해) 를 모은다
    let hits: Vec<Vec<(usize, f32)>> = (0..n.div_ceil(CHUNK))
        .into_par_iter()
        .map(|ci| {
            let lo = ci * CHUNK;
            let hi = ((ci + 1) * CHUNK).min(n);
            let mut out = Vec::new();
            for i in lo..hi {
                if !pool.is_alive(i) || pool.state[i] == UnitState::Rout {
                    continue;
                }
                // 공격측만 성을 부순다
                if pool.team[i] != 0 {
                    continue;
                }
                let s = stats(pool.type_id[i]);
                if s.siege_dmg <= 0.0 || pool.cooldown[i] > 0 {
                    continue;
                }
                // 투석기가 아니면 성에 붙어 있어야 한다
                if s.range <= 0.0 && !near_castle(pool.pos[i], castle) {
                    continue;
                }

                let probe = if s.range > 0.0 {
                    // 투석기는 멀리서 가장 가까운 성벽 구간을 노린다
                    let mut best = f32::MAX;
                    let mut aim = None;
                    for (k, seg) in castle.segments.iter().enumerate() {
                        if seg.breached {
                            continue;
                        }
                        let d = [
                            seg.center[0] - pool.pos[i][0],
                            seg.center[1] - pool.pos[i][1],
                        ];
                        let d2 = d[0] * d[0] + d[1] * d[1];
                        if d2 < best && d2 < s.range * s.range {
                            best = d2;
                            aim = Some(k);
                        }
                    }
                    match aim {
                        Some(k) => {
                            out.push((k, s.siege_dmg));
                            continue;
                        }
                        None => continue,
                    }
                } else {
                    // 근접 병기와 보병은 코앞의 구조물만 팬다
                    let f = pool.facing[i];
                    [
                        pool.pos[i][0] + f.cos() * (s.reach + REACH_MARGIN),
                        pool.pos[i][1] + f.sin() * (s.reach + REACH_MARGIN),
                    ]
                };

                if !terrain.at(probe).is_structure() {
                    continue;
                }
                if let Some(k) = castle.segment_at(probe) {
                    out.push((k, s.siege_dmg));
                }
            }
            out
        })
        .collect();

    // 피해 적용과 붕괴 (순차)
    let mut breached: Vec<usize> = Vec::new();
    {
        let castle = w.castle.as_mut().unwrap();
        for chunk in &hits {
            for &(k, dmg) in chunk {
                let seg = &mut castle.segments[k];
                if seg.breached {
                    continue;
                }
                seg.hp -= dmg;
                if seg.hp <= 0.0 {
                    seg.breached = true;
                    breached.push(k);
                }
            }
        }
    }

    // 쿨다운을 다시 채운다
    let pool = &mut w.pool;
    pool.cooldown
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, cd)| {
            let s = stats(pool_type(i, &pool.type_id));
            if s.siege_dmg > 0.0 && *cd == 0 && is_engine(pool_type(i, &pool.type_id)) {
                *cd = s.attack_period;
            }
        });

    if breached.is_empty() {
        return;
    }
    // 무너진 자리를 잔해로 바꾸고 길을 다시 계산하게 한다
    for k in &breached {
        let castle = w.castle.as_ref().unwrap();
        let mut terrain = std::mem::replace(&mut w.terrain, crate::map::TerrainMap::flat(1.0));
        castle.restamp_segment(&mut terrain, *k);
        w.terrain = terrain;
    }
    // 비용과 경로는 곧바로가 아니라 묶어서 다시 굽는다(World::step 참고)
    w.mark_flows_dirty();
    for k in breached {
        let seg = &w.castle.as_ref().unwrap().segments[k];
        w.breach_events.push(seg.center);
    }
}

#[inline]
fn pool_type(i: usize, types: &[u8]) -> u8 {
    types[i]
}

/// 성벽에 달라붙어 기어오른다.
fn climb_walls(w: &mut World) {
    let n = w.pool.len();
    let castle = w.castle.as_ref().unwrap();
    let terrain = &w.terrain;
    let grid = &w.grid;
    let pos = &w.pool.pos;
    let team = &w.pool.team;
    let type_id = &w.pool.type_id;

    // 사다리가 걸린 자리를 미리 모은다. 사다리는 굼뜨게 움직이므로 매 틱
    // 다시 훑을 필요가 없다
    if w.tick.is_multiple_of(20) {
        w.ladder_spots.clear();
        for i in 0..n {
            if stats(type_id[i]).is_ladder && w.pool.is_alive(i) {
                w.ladder_spots.push(pos[i]);
            }
        }
    }
    let ladders = &w.ladder_spots;

    let inner = [castle.half[0] - 6.0, castle.half[1] - 6.0];
    let center = castle.center;

    let climbed: Vec<Vec<(u32, [f32; 2])>> = w
        .pool
        .charge_t
        .par_chunks_mut(CHUNK)
        .zip(w.pool.layer.par_chunks_mut(CHUNK))
        .enumerate()
        .map(|(ci, (tchunk, lchunk))| {
            let base = ci * CHUNK;
            let mut out = Vec::new();
            for k in 0..tchunk.len() {
                let i = base + k;
                // 성벽 위에 이미 올라섰거나, 사람이 아니거나, 공격측이 아니면 볼 것 없다
                if lchunk[k] != 0 || team[i] != 0 || is_engine(type_id[i]) {
                    continue;
                }
                if !near_castle(pos[i], castle) {
                    tchunk[k] = 0;
                    continue;
                }
                let s = stats(type_id[i]);
                if s.is_cavalry {
                    continue; // 말을 타고 성벽을 오를 수는 없다
                }
                let p = pos[i];
                // 코앞이 성벽인가
                let touching = [
                    [p[0] + 4.0, p[1]],
                    [p[0] - 4.0, p[1]],
                    [p[0], p[1] + 4.0],
                    [p[0], p[1] - 4.0],
                ]
                .iter()
                .any(|q| terrain.at(*q) == Terrain::Wall);

                if !touching {
                    tchunk[k] = 0;
                    continue;
                }
                // 사다리가 곁에 있으면 훨씬 빠르다
                let near_ladder = ladders.iter().any(|l| {
                    let d = [l[0] - p[0], l[1] - p[1]];
                    d[0] * d[0] + d[1] * d[1] < LADDER_RANGE * LADDER_RANGE
                });
                let rate = if near_ladder { LADDER_SPEEDUP } else { 1 };
                tchunk[k] = tchunk[k].saturating_add(rate);

                if tchunk[k] >= CLIMB_TICKS {
                    // 흉벽을 넘었다. 성벽 안쪽으로 내려선다
                    tchunk[k] = 0;
                    lchunk[k] = 1;
                    let to_center = [center[0] - p[0], center[1] - p[1]];
                    let len = (to_center[0] * to_center[0] + to_center[1] * to_center[1])
                        .sqrt()
                        .max(1e-3);
                    let landing = [
                        (p[0] + to_center[0] / len * 16.0)
                            .clamp(center[0] - inner[0], center[0] + inner[0]),
                        (p[1] + to_center[1] / len * 16.0)
                            .clamp(center[1] - inner[1], center[1] + inner[1]),
                    ];
                    out.push((i as u32, landing));
                }
            }
            out
        })
        .collect();

    let _ = grid;
    for chunk in climbed {
        for (i, landing) in chunk {
            w.pool.pos[i as usize] = landing;
            w.pool.target[i as usize] = NO_TARGET;
            w.stats.wall_breaches_climbed += 1;
        }
    }
}

/// 성벽 위에서 돌과 끓는 기름을 붓는다.
fn pour_from_walls(w: &mut World) {
    if !w.tick.is_multiple_of(DROP_PERIOD) {
        return;
    }
    let n = w.pool.len();
    let pos = &w.pool.pos;
    let team = &w.pool.team;
    let charge_t = &w.pool.charge_t;
    let grid = &w.grid;

    // 성벽에 붙어 있는 방어측이 아래를 때린다
    let mut hits: Vec<(u32, f32)> = Vec::new();
    let castle = w.castle.as_ref().unwrap();
    for i in 0..n {
        if team[i] != 1 || !w.pool.is_alive(i) || w.pool.state[i] == UnitState::Rout {
            continue;
        }
        if !near_castle(pos[i], castle) {
            continue;
        }
        // 성벽 바로 안쪽에 선 수비병만 해당한다
        // 성은 사각형이다. 지형을 더듬는 것보다 안쪽 면까지의 거리를 재는 편이
        // 정확하고 싸다
        if wall_gap(pos[i], castle) > WALL_GUARD_RANGE {
            continue;
        }
        let mut struck = 0u32;
        grid.for_each_near(pos[i], DROP_RANGE, |j| {
            if struck >= DROP_TARGETS {
                return;
            }
            let ju = j as usize;
            if team[ju] == 1 {
                return;
            }
            let d = [pos[ju][0] - pos[i][0], pos[ju][1] - pos[i][1]];
            if d[0] * d[0] + d[1] * d[1] > DROP_RANGE * DROP_RANGE {
                return;
            }
            struck += 1;
            // 기어오르는 중이면 몸을 가릴 수 없다
            let mult = if charge_t[ju] > 0 {
                CLIMBING_DROP_MULT
            } else {
                1.0
            };
            hits.push((j, DROP_DAMAGE * mult));
        });
    }

    w.stats.drops_landed += hits.len() as u64;
    let mut deaths: [u32; 2] = [0, 0];
    for &(v, d) in &hits {
        let vu = v as usize;
        let armor = stats(w.pool.type_id[vu]).armor;
        w.pool.hp[vu] -= d * (1.0 - armor * 0.5);
    }
    for &(v, _) in &hits {
        let vu = v as usize;
        if w.pool.hp[vu] <= 0.0 && w.pool.state[vu] != UnitState::Dead {
            w.pool.state[vu] = UnitState::Dead;
            w.pool.target[vu] = NO_TARGET;
            w.pool.vel[vu] = [0.0, 0.0];
            deaths[w.pool.team[vu] as usize] += 1;
            w.death_events
                .push((w.pool.pos[vu], w.pool.team[vu], w.pool.type_id[vu]));
        }
    }
    w.stats.dead[0] += deaths[0];
    w.stats.dead[1] += deaths[1];
}

/// 성 한복판을 누가 쥐고 있는가.
fn check_objective(w: &mut World) {
    if !w.tick.is_multiple_of(5) {
        return;
    }
    let castle = w.castle.as_ref().unwrap();
    let o = castle.objective;
    let r2 = castle.objective_radius * castle.objective_radius;
    let mut attackers = 0u32;
    let mut defenders = 0u32;
    for i in 0..w.pool.len() {
        if !w.pool.is_alive(i) || w.pool.state[i] == UnitState::Rout {
            continue;
        }
        let d = [w.pool.pos[i][0] - o[0], w.pool.pos[i][1] - o[1]];
        if d[0] * d[0] + d[1] * d[1] > r2 {
            continue;
        }
        if w.pool.team[i] == 0 {
            attackers += 1;
        } else {
            defenders += 1;
        }
    }
    w.stats.objective_holders = attackers;
    if attackers >= HOLD_MIN_UNITS && attackers > defenders * 2 {
        w.objective_hold += 5;
    } else {
        w.objective_hold = w.objective_hold.saturating_sub(10);
    }
    let _ = DT;
}
