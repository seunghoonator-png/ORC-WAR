//! 근접 전투 — 대상 획득과 피해 판정.
//!
//! 피해는 병렬 단계에서 곧바로 적용하지 않고 청크별 목록에 모았다가 인덱스 순서로
//! 합친다. 쓰기 경합이 사라지는 것은 물론, 결과가 스레드 개수와 무관해진다.
//! (같은 씨앗 → 같은 전투. docs/IMPLEMENTATION.md §6 결정론 테스트의 전제)

use rayon::prelude::*;

use crate::sim::pool::{UnitState, NO_TARGET};
use crate::sim::unit_types::stats;
use crate::sim::{World, CHUNK};

/// 대상 재탐색 주기(틱). 유닛마다 위상을 어긋나게 해서 부하를 고르게 편다.
const RETARGET_PERIOD: u64 = 4;
/// 대상 탐색 시 훑어볼 이웃 수 상한 — 낮으면 그리드 순회 방향으로 편향된다
const MAX_SCAN: u32 = 128;
/// 근접 대상 탐색 여유 거리(m)
const SEEK_MARGIN: f32 = 2.5;
/// 방패가 막아주는 정면 각도(라디안, 편측)
const SHIELD_ARC: f32 = std::f32::consts::FRAC_PI_3;

struct Hit {
    target: u32,
    dmg: f32,
}

pub fn step(w: &mut World) {
    let n = w.pool.len();
    if n == 0 {
        return;
    }

    let grid = &w.grid;
    let tick = w.tick;
    let seed = w.seed;
    let pos = &w.pool.pos;
    let facing = &w.pool.facing;
    let type_id = &w.pool.type_id;
    let team = &w.pool.team;
    let hp = &w.pool.hp;

    // --- A/B. 대상 갱신 + 공격 판정 (병렬) ---
    let hits: Vec<Vec<Hit>> = w
        .pool
        .target
        .par_chunks_mut(CHUNK)
        .zip(w.pool.state.par_chunks_mut(CHUNK))
        .zip(w.pool.cooldown.par_chunks_mut(CHUNK))
        .enumerate()
        .map(|(ci, ((tchunk, schunk), cchunk))| {
            let base = ci * CHUNK;
            let mut out: Vec<Hit> = Vec::new();

            for k in 0..tchunk.len() {
                let i = base + k;
                if schunk[k] == UnitState::Dead || schunk[k] == UnitState::Rout {
                    tchunk[k] = NO_TARGET;
                    continue;
                }
                if cchunk[k] > 0 {
                    cchunk[k] -= 1;
                }

                let s = stats(type_id[i]);
                let p = pos[i];
                let my_team = team[i];

                // 현재 대상이 여전히 유효한가
                let mut tgt = tchunk[k];
                if tgt != NO_TARGET {
                    let t = tgt as usize;
                    let dead = hp[t] <= 0.0;
                    let d = [pos[t][0] - p[0], pos[t][1] - p[1]];
                    let far = d[0] * d[0] + d[1] * d[1]
                        > (s.reach + SEEK_MARGIN * 2.0) * (s.reach + SEEK_MARGIN * 2.0);
                    if dead || far {
                        tgt = NO_TARGET;
                    }
                }

                // 재탐색 (위상 분산)
                if tgt == NO_TARGET && (tick + i as u64) % RETARGET_PERIOD == 0 {
                    let seek = s.reach + SEEK_MARGIN;
                    let mut best = f32::MAX;
                    let mut best_j = NO_TARGET;
                    let mut scanned = 0u32;
                    grid.for_each_near(p, seek, |j| {
                        if scanned >= MAX_SCAN {
                            return;
                        }
                        let ju = j as usize;
                        if team[ju] == my_team || hp[ju] <= 0.0 {
                            return;
                        }
                        scanned += 1;
                        let d = [pos[ju][0] - p[0], pos[ju][1] - p[1]];
                        let d2 = d[0] * d[0] + d[1] * d[1];
                        // 동점이면 인덱스가 작은 쪽으로 — 순서 의존성을 없앤다
                        if d2 < best || (d2 == best && j < best_j) {
                            best = d2;
                            best_j = j;
                        }
                    });
                    tgt = best_j;
                }
                tchunk[k] = tgt;

                // 상태 전이 + 공격
                if tgt == NO_TARGET {
                    if schunk[k] == UnitState::Fight {
                        schunk[k] = UnitState::Advance;
                    }
                    continue;
                }
                let t = tgt as usize;
                let d = [pos[t][0] - p[0], pos[t][1] - p[1]];
                let dist2 = d[0] * d[0] + d[1] * d[1];
                let reach = s.reach + stats(type_id[t]).radius;

                if dist2 > reach * reach {
                    schunk[k] = UnitState::Advance;
                    continue;
                }
                schunk[k] = UnitState::Fight;
                if cchunk[k] > 0 {
                    continue;
                }
                cchunk[k] = s.attack_period;

                out.push(Hit {
                    target: tgt,
                    dmg: resolve_damage(i, t, seed, tick, type_id, facing, pos),
                });
            }
            out
        })
        .collect();

    // --- C. 피해 적용 (순차, 결정론) ---
    //
    // 피해 차감과 사망 판정을 두 패스로 분리한다. 한 패스로 처리하면 "이미 죽은
    // 대상에 대한 타격"이 버려지는데, 그 낭비량이 청크 순서(=팀 번호 순서)에
    // 따라 달라져 한쪽 진영에 체계적으로 유리해진다. 나눠 처리하면 같은 틱의
    // 타격은 누가 먼저 굴렀든 전부 들어간다.
    let mut deaths: [u32; 2] = [0, 0];
    for chunk in &hits {
        for h in chunk {
            w.pool.hp[h.target as usize] -= h.dmg;
        }
    }
    for chunk in &hits {
        for h in chunk {
            let t = h.target as usize;
            if w.pool.hp[t] <= 0.0 && w.pool.state[t] != UnitState::Dead {
                w.pool.state[t] = UnitState::Dead;
                w.pool.target[t] = NO_TARGET;
                deaths[w.pool.team[t] as usize] += 1;
                w.death_events.push((w.pool.pos[t], w.pool.team[t], w.pool.type_id[t]));
            }
        }
    }
    w.stats.dead[0] += deaths[0];
    w.stats.dead[1] += deaths[1];
}

/// 냉병기 피해 공식: 갑옷은 비율 감쇄, 방패는 정면 한정 확률 차단.
#[inline]
fn resolve_damage(
    attacker: usize,
    defender: usize,
    seed: u64,
    tick: u64,
    type_id: &[u8],
    facing: &[f32],
    pos: &[[f32; 2]],
) -> f32 {
    let a = stats(type_id[attacker]);
    let d = stats(type_id[defender]);

    // 방패 판정 — 방어자가 공격자를 정면으로 보고 있을 때만
    if d.shield > 0.0 {
        let to_att = [
            pos[attacker][0] - pos[defender][0],
            pos[attacker][1] - pos[defender][1],
        ];
        let ang = to_att[1].atan2(to_att[0]);
        let mut diff = (ang - facing[defender]).abs();
        while diff > std::f32::consts::PI {
            diff = (std::f32::consts::TAU - diff).abs();
        }
        if diff < SHIELD_ARC {
            let roll = crate::rng::unit_f32(seed ^ 0xB10C, tick, attacker as u64);
            if roll < d.shield {
                return 0.0;
            }
        }
    }

    // 명중 편차 ±15% — 같은 대형이 완전히 동시에 죽는 걸 막는다
    let jitter = 1.0 + crate::rng::signed_f32(seed ^ 0xDA11, tick, attacker as u64) * 0.15;
    a.melee_dmg * jitter * (1.0 - d.armor)
}
