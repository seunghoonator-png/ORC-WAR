//! 기병 돌격과 장창 브레이스.
//!
//! 냉병기 전장에서 기병의 값어치는 칼질이 아니라 충돌 그 자체에서 나온다.
//! 가속을 붙여 부딪히면 대비 안 된 전열은 그냥 갈려 나간다. 반대로 자리를
//! 잡고 창을 세운 방진에 정면으로 뛰어들면 말이 먼저 꿰뚫린다.
//! 이 두 결과가 규칙으로 갈리는 곳이 여기다.

use rayon::prelude::*;

use crate::sim::pool::{UnitState, NO_TARGET};
use crate::sim::unit_types::stats;
use crate::sim::{World, CHUNK};

/// 이 거리 안에 목표가 들어오면 가속을 시작한다(m)
const CHARGE_TRIGGER: f32 = 28.0;
/// 돌격 중 속도 배수
pub const CHARGE_SPEED: f32 = 1.35;
/// 가속이 다 붙는 데 걸리는 틱 — 이 값에 도달해야 충격이 최대가 된다
const CHARGE_WINDUP: u16 = 24;
/// 충격이 소진되기까지의 최대 틱
const CHARGE_MAX: u16 = 90;
/// 운동량이 충격 대미지에 실리는 비율
const IMPACT_SCALE: f32 = 0.15;
/// 충격에 나가떨어져 무방비가 되는 시간(틱)
pub const KNOCKDOWN: u8 = 30;
/// 넉백 거리(m)
const KNOCKBACK: f32 = 1.6;
/// 충돌 한 번이 휩쓰는 반경(m).
///
/// 돌격의 값어치는 한 명을 베는 데 있지 않고 대열을 뚫고 지나가는 데 있다.
/// 표적 하나만 치도록 두면 중장기병이 동수의 방패 보병에게 지는, 기획 의도와
/// 정반대의 결과가 나온다.
const IMPACT_RADIUS: f32 = 2.2;
/// 휩쓸리는 인원 상한 — 밀집도가 극단적일 때의 안전장치
const MAX_TRAMPLED: u32 = 6;
/// 브레이스 정면으로 뛰어든 기병이 되받는 배수.
/// 풀차지 중장기병이 한 번에 꿰뚫리도록 잡았다.
const BRACE_REFLECT: f32 = 2.0;
/// 브레이스로 인정하는 정면 각도(라디안, 편측) — 전방 120도
const BRACE_ARC: f32 = std::f32::consts::FRAC_PI_3;
/// 이 속도 아래로 떨어져 있어야 창을 세운 것으로 본다(m/s)
const BRACE_SPEED: f32 = 0.35;

struct Impact {
    /// 충격을 받는 쪽
    target: u32,
    /// 되받아 꿰뚫리는 기병 (브레이스에 걸렸을 때)
    attacker: u32,
    dmg: f32,
    /// 기병이 되받는 피해
    reflect: f32,
    push: [f32; 2],
}

pub fn step(w: &mut World) {
    let n = w.pool.len();
    if n == 0 {
        return;
    }

    let pos = &w.pool.pos;
    let vel = &w.pool.vel;
    let terrain = &w.terrain;
    let facing = &w.pool.facing;
    let type_id = &w.pool.type_id;
    let target = &w.pool.target;

    // --- 패스 A: 상태 전이 ---
    w.pool
        .state
        .par_chunks_mut(CHUNK)
        .zip(w.pool.charge_t.par_chunks_mut(CHUNK))
        .enumerate()
        .for_each(|(ci, (schunk, cchunk))| {
            let base = ci * CHUNK;
            for k in 0..schunk.len() {
                let i = base + k;
                if matches!(
                    schunk[k],
                    UnitState::Dead | UnitState::Fled | UnitState::Rout
                ) {
                    cchunk[k] = 0;
                    continue;
                }
                let s = stats(type_id[i]);
                let speed2 = vel[i][0] * vel[i][0] + vel[i][1] * vel[i][1];

                // --- 장창: 자리를 지키고 있으면 창을 세운다 ---
                if s.can_brace {
                    let planted = speed2 < BRACE_SPEED * BRACE_SPEED;
                    if planted && target[i] != NO_TARGET {
                        schunk[k] = UnitState::Brace;
                    } else if schunk[k] == UnitState::Brace {
                        schunk[k] = UnitState::Advance;
                    }
                    continue;
                }

                if !s.is_cavalry || s.charge_power <= 0.0 {
                    continue;
                }

                // --- 기병: 목표가 사정권에 들면 가속을 붙인다 ---
                let tgt = target[i];
                if tgt == NO_TARGET {
                    if schunk[k] == UnitState::Charge {
                        schunk[k] = UnitState::Advance;
                    }
                    cchunk[k] = 0;
                    continue;
                }
                let t = tgt as usize;
                let d = [pos[t][0] - pos[i][0], pos[t][1] - pos[i][1]];
                let dist2 = d[0] * d[0] + d[1] * d[1];
                let reach = s.reach + stats(type_id[t]).radius;

                if schunk[k] == UnitState::Charge {
                    if !terrain.at(pos[i]).allows_charge() {
                        // 나무 사이나 진창에 들어서면 그 자리에서 속도가 죽는다.
                        // 진입만 막고 유지를 두면, 숲에서 오히려 돌격 상태가
                        // 길어지는 뒤집힌 결과가 나온다.
                        schunk[k] = UnitState::Advance;
                        cchunk[k] = 0;
                        continue;
                    }
                    cchunk[k] = cchunk[k].saturating_add(1);
                    if cchunk[k] > CHARGE_MAX {
                        // 관성이 다 떨어졌다. 이제부터는 그냥 말 탄 보병이다
                        schunk[k] = UnitState::Fight;
                        cchunk[k] = 0;
                    }
                } else if dist2 < CHARGE_TRIGGER * CHARGE_TRIGGER
                    && dist2 > reach * reach
                    && terrain.at(pos[i]).allows_charge()
                {
                    // 나무 사이나 진창에서는 말이 속도를 낼 수 없다
                    schunk[k] = UnitState::Charge;
                    cchunk[k] = 0;
                }
                let _ = d;
            }
        });

    // --- 패스 B: 충돌 판정 ---
    let state = &w.pool.state;
    let charge_t = &w.pool.charge_t;
    let grid = &w.grid;
    let team = &w.pool.team;
    let hp = &w.pool.hp;
    let impacts: Vec<Vec<Impact>> = (0..n.div_ceil(CHUNK))
        .into_par_iter()
        .map(|ci| {
            let lo = ci * CHUNK;
            let hi = ((ci + 1) * CHUNK).min(n);
            let mut out = Vec::new();
            for i in lo..hi {
                if state[i] != UnitState::Charge {
                    continue;
                }
                let s = stats(type_id[i]);
                let tgt = target[i];
                if tgt == NO_TARGET {
                    continue;
                }
                let t = tgt as usize;
                let d = [pos[t][0] - pos[i][0], pos[t][1] - pos[i][1]];
                let dist2 = d[0] * d[0] + d[1] * d[1];
                let reach = s.reach + stats(type_id[t]).radius;
                if dist2 > reach * reach {
                    continue;
                }

                let speed2 = vel[i][0] * vel[i][0] + vel[i][1] * vel[i][1];
                let momentum = speed2.sqrt() * s.mass;
                // 가속이 덜 붙었으면 그만큼만 실린다
                let windup = (charge_t[i] as f32 / CHARGE_WINDUP as f32).min(1.0);
                let dist = dist2.sqrt().max(1e-4);
                let dir = [d[0] / dist, d[1] / dist];

                // 방어자가 창을 세우고 이쪽을 마주 보고 있는가
                let braced = state[t] == UnitState::Brace && {
                    let ang = (-dir[1]).atan2(-dir[0]);
                    let mut off = (ang - facing[t]).abs();
                    while off > std::f32::consts::PI {
                        off = (std::f32::consts::TAU - off).abs();
                    }
                    off < BRACE_ARC
                };

                if braced {
                    // 창끝에 스스로 뛰어든 셈이다
                    out.push(Impact {
                        target: tgt,
                        attacker: i as u32,
                        dmg: 0.0,
                        reflect: stats(type_id[t]).melee_dmg
                            * (1.0 + momentum * IMPACT_SCALE)
                            * BRACE_REFLECT
                            * windup,
                        push: [0.0, 0.0],
                    });
                    continue;
                }

                // 접촉 지점 주변을 통째로 휩쓴다
                let power = s.melee_dmg * (1.0 + momentum * IMPACT_SCALE * s.charge_power) * windup;
                let my_team = team[i];
                let mut trampled = 0u32;
                grid.for_each_near(pos[i], IMPACT_RADIUS, |it| {
                    if trampled >= MAX_TRAMPLED {
                        return;
                    }
                    let ju = it.idx as usize;
                    if it.team == my_team || hp[ju] <= 0.0 {
                        return;
                    }
                    let dd = [it.pos[0] - pos[i][0], it.pos[1] - pos[i][1]];
                    if dd[0] * dd[0] + dd[1] * dd[1] > IMPACT_RADIUS * IMPACT_RADIUS {
                        return;
                    }
                    // 창벽에 걸린 상대는 이 휩쓸기에 들어가지 않는다
                    if state[ju] == UnitState::Brace {
                        return;
                    }
                    trampled += 1;
                    out.push(Impact {
                        target: it.idx,
                        attacker: i as u32,
                        dmg: power,
                        reflect: 0.0,
                        push: [dir[0] * KNOCKBACK, dir[1] * KNOCKBACK],
                    });
                });
            }
            out
        })
        .collect();

    // --- 충격 적용 (순차, 결정론) ---
    let mut deaths: [u32; 2] = [0, 0];
    for chunk in &impacts {
        for im in chunk {
            let t = im.target as usize;
            let a = im.attacker as usize;
            if im.dmg > 0.0 {
                w.stats.charge_impacts += 1;
                let armor = stats(w.pool.type_id[t]).armor;
                w.pool.hp[t] -= im.dmg * (1.0 - armor);
                w.pool.stagger[t] = KNOCKDOWN;
                w.pool.pos[t][0] += im.push[0];
                w.pool.pos[t][1] += im.push[1];
                w.pool.morale[t] = (w.pool.morale[t] - crate::sim::morale::CHARGE_SHOCK).max(0);
            }
            if im.reflect > 0.0 {
                let armor = stats(w.pool.type_id[a]).armor;
                w.pool.hp[a] -= im.reflect * (1.0 - armor);
            }
            w.pool.state[a] = UnitState::Fight;
            w.pool.charge_t[a] = 0;
        }
    }
    for chunk in &impacts {
        for im in chunk {
            for u in [im.target as usize, im.attacker as usize] {
                if w.pool.hp[u] <= 0.0 && w.pool.state[u] != UnitState::Dead {
                    w.pool.state[u] = UnitState::Dead;
                    w.pool.target[u] = NO_TARGET;
                    w.pool.vel[u] = [0.0, 0.0];
                    deaths[w.pool.team[u] as usize] += 1;
                    w.death_events
                        .push((w.pool.pos[u], w.pool.team[u], w.pool.type_id[u]));
                }
            }
        }
    }
    w.stats.dead[0] += deaths[0];
    w.stats.dead[1] += deaths[1];

    // 넘어진 유닛이 일어나는 시간
    w.pool
        .stagger
        .par_iter_mut()
        .for_each(|s| *s = s.saturating_sub(1));
}
