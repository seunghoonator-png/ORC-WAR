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
/// 근접 대상 탐색 여유 거리(m).
///
/// 넓게 잡으면 안 된다. 대상을 잡은 유닛은 진형을 따르는 대신 그 적에게 직진하는데,
/// 몇 미터 떨어진 적까지 쫓아가면 대형이 풀리면서 접적 순간의 미세한 차이가
/// 눈덩이처럼 불어난다. (실측: 2.5m 일 때 거울 대칭 전투가 z=-5.0 으로 한쪽에
/// 쏠렸고, 0.6m 로 좁히자 z=-0.7 로 사라졌다.) 실제 병사도 눈앞의 적과 싸우지
/// 옆의 적을 쫓아가지 않는다.
const SEEK_MARGIN: f32 = 0.6;
/// 방패가 막아주는 정면 각도(라디안, 편측)
const SHIELD_ARC: f32 = std::f32::consts::FRAC_PI_3;

struct Hit {
    target: u32,
    dmg: f32,
    /// 방어자가 등이나 옆구리를 내준 타격
    flanked: bool,
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
    let stagger = &w.pool.stagger;

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
                if matches!(
                    schunk[k],
                    UnitState::Dead | UnitState::Fled | UnitState::Rout
                ) {
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
                if tgt == NO_TARGET && (tick + i as u64).is_multiple_of(RETARGET_PERIOD) {
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

                // 돌격 중인 기병의 상태는 charge 단계가 쥐고 있다. 여기서
                // 덮어쓰면 가속 카운터가 매 틱 0으로 되돌아가 충격이 영영
                // 실리지 않는다.
                let charging = schunk[k] == UnitState::Charge;
                if dist2 > reach * reach {
                    if !charging {
                        schunk[k] = UnitState::Advance;
                    }
                    continue;
                }
                if !charging {
                    schunk[k] = UnitState::Fight;
                }
                if cchunk[k] > 0 || stagger[i] > 0 {
                    continue;
                }
                cchunk[k] = s.attack_period;

                let (mut dmg, flanked) = resolve_damage(i, t, seed, tick, type_id, facing, pos);
                if stagger[t] > 0 {
                    // 넘어진 상대는 막지도 피하지도 못한다
                    dmg *= 2.0;
                }
                out.push(Hit {
                    target: tgt,
                    dmg,
                    flanked,
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
            let t = h.target as usize;
            w.pool.hp[t] -= h.dmg;
            if h.flanked {
                // 옆이나 뒤를 잡히면 사기가 먼저 부러진다
                w.pool.morale[t] = (w.pool.morale[t] - crate::sim::morale::FLANK_SHOCK).max(0);
            }
        }
    }
    for chunk in &hits {
        for h in chunk {
            let t = h.target as usize;
            if w.pool.hp[t] <= 0.0 && w.pool.state[t] != UnitState::Dead {
                w.pool.state[t] = UnitState::Dead;
                w.pool.target[t] = NO_TARGET;
                w.pool.vel[t] = [0.0, 0.0];
                deaths[w.pool.team[t] as usize] += 1;
                w.death_events
                    .push((w.pool.pos[t], w.pool.team[t], w.pool.type_id[t]));
            }
        }
    }
    w.stats.dead[0] += deaths[0];
    w.stats.dead[1] += deaths[1];
}

/// 냉병기 피해 공식: 갑옷은 비율 감쇄, 방패는 정면 한정 확률 차단.
///
/// 반환값의 두 번째는 방어자가 옆이나 뒤를 내줬는지 여부다.
#[inline]
fn resolve_damage(
    attacker: usize,
    defender: usize,
    seed: u64,
    tick: u64,
    type_id: &[u8],
    facing: &[f32],
    pos: &[[f32; 2]],
) -> (f32, bool) {
    let a = stats(type_id[attacker]);
    let d = stats(type_id[defender]);

    // 공격자가 방어자의 어느 쪽에 서 있는가
    let to_att = [
        pos[attacker][0] - pos[defender][0],
        pos[attacker][1] - pos[defender][1],
    ];
    let ang = to_att[1].atan2(to_att[0]);
    let mut off_axis = (ang - facing[defender]).abs();
    while off_axis > std::f32::consts::PI {
        off_axis = (std::f32::consts::TAU - off_axis).abs();
    }
    let frontal = off_axis < SHIELD_ARC;
    // 등 뒤 90도는 무방비다
    let from_behind = off_axis > std::f32::consts::PI * 0.75;

    // 방패는 정면으로 마주 본 공격만 막는다.
    // 말 위에서 내려찍는 타격은 방패 위쪽을 넘어오므로 절반만 막힌다.
    if frontal && d.shield > 0.0 {
        let block = if a.is_cavalry {
            d.shield * 0.5
        } else {
            d.shield
        };
        let roll = crate::rng::unit_f32(seed ^ 0xB10C, tick, attacker as u64);
        if roll < block {
            return (0.0, false);
        }
    }

    // 명중 편차 ±15% — 같은 대형이 완전히 동시에 죽는 걸 막는다
    let jitter = 1.0 + crate::rng::signed_f32(seed ^ 0xDA11, tick, attacker as u64) * 0.15;
    let mut dmg = a.melee_dmg * jitter * (1.0 - d.armor);
    if from_behind {
        dmg *= 1.6;
    }
    (dmg, !frontal)
}
