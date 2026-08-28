//! 이동 — 플로우 필드 추종 + 국소 분리.
//!
//! 강체 물리는 쓰지 않는다. 30만 유닛에서는 쌍별 충돌 해소가 불가능하고,
//! 애초에 원하는 그림도 "밀집 대형이 서로 밀며 꾸역꾸역 흐르는" 모습이다.
//! 목표 방향 + 이웃 밀어내기 두 힘만으로 그 움직임이 나온다.

use rayon::prelude::*;

use crate::sim::pool::{UnitState, NO_TARGET};
use crate::sim::unit_types::stats;
use crate::sim::{World, CHUNK, DT, WORLD_SIZE};

/// 분리력 계산 시 볼 이웃 수 상한 — 병적인 밀집에 대한 안전장치일 뿐이다.
///
/// 이 값을 낮게 잡으면 안 된다. 그리드 순회가 항상 남쪽 셀부터 도는 탓에,
/// 상한에 걸리면 남쪽 이웃만 반영되어 모든 유닛이 북쪽으로 밀리는 편향이 생긴다.
/// (실측: 상한 16일 때 남쪽 진영이 적진으로 빨려들어가 6/6 시드 전패)
const MAX_SEP_NEIGHBORS: u32 = 64;
/// 속도 변화 상한 (m/s per tick) — 관성을 줘서 방향 전환이 튀지 않게 한다
const ACCEL: f32 = 6.0 * DT;
/// 이웃 밀어내기 세기 (속도에 실리는 부드러운 힘)
const SEP_STRENGTH: f32 = 4.0;
/// 겹친 만큼 위치를 직접 떼어놓는 비율.
///
/// 속도에만 반발력을 실으면 목표 방향 추진력을 이기지 못해 두 대형이 서로를
/// 통과해 버린다. 겹침을 위치 수준에서 즉시 해소해야 전선이 선다.
const POS_FIX: f32 = 0.5;
/// 한 틱에 위치 보정으로 움직일 수 있는 최대 거리(m) — 폭주 방지
const MAX_POS_FIX: f32 = 0.12;
/// 사수가 이 거리 안으로 적이 들어오면 물러서기 시작한다(m)
const KITE_RANGE: f32 = 14.0;
/// 적군과는 이만큼 더 떨어지려 한다.
///
/// 아군과 같은 간격만 두면 두 대형이 서로를 그냥 통과해 버린다(실측: 개전
/// 30초 만에 양군의 무게중심이 뒤바뀜). 방패를 맞대고 미는 압력을 이 계수로
/// 표현해 전선이 서게 만든다.
const ENEMY_SPACING: f32 = 1.4;

pub fn step(w: &mut World) {
    let n = w.pool.len();
    if n == 0 {
        return;
    }
    let pool = &w.pool;
    let grid = &w.grid;
    let flows = &w.flows;
    let terrain = &w.terrain;
    let tick = w.tick;
    let seed = w.seed;

    let pos = &pool.pos;
    let vel = &pool.vel;
    let state = &pool.state;
    let type_id = &pool.type_id;
    let goal = &pool.goal;
    let target = &pool.target;

    w.pos_next
        .par_chunks_mut(CHUNK)
        .zip(w.vel_next.par_chunks_mut(CHUNK))
        .zip(w.facing_next.par_chunks_mut(CHUNK))
        .enumerate()
        .for_each(|(ci, ((pchunk, vchunk), fchunk))| {
            let base = ci * CHUNK;
            for k in 0..pchunk.len() {
                let i = base + k;
                let p = pos[i];
                let v = vel[i];

                if matches!(state[i], UnitState::Dead | UnitState::Fled) {
                    pchunk[k] = p;
                    vchunk[k] = [0.0, 0.0];
                    fchunk[k] = pool.facing[i];
                    continue;
                }

                let s = stats(type_id[i]);
                let my_team = pool.team[i];

                // --- 1. 목표 방향 ---
                let mut want = [0.0f32, 0.0];
                if state[i] == UnitState::Rout {
                    // 패주병은 아군 후방(맵 가장자리)으로 도주한다
                    want = if my_team == 0 {
                        [0.0, -1.0]
                    } else {
                        [0.0, 1.0]
                    };
                } else if target[i] != NO_TARGET {
                    let t = target[i] as usize;
                    let d = [pos[t][0] - p[0], pos[t][1] - p[1]];
                    let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-4);
                    let reach = s.reach + stats(type_id[t]).radius;
                    if s.range > 0.0 && pool.ammo[i] > 0 && len < KITE_RANGE {
                        // 아직 쏠 화살이 남은 사수는 붙지 않고 물러서며 쏜다.
                        // 활을 든 채 창칼 앞으로 걸어들어갈 이유가 없다.
                        want = [-d[0] / len, -d[1] / len];
                    } else if len > reach {
                        // 교전 대상이 있으면 그쪽으로 파고든다
                        want = [d[0] / len, d[1] / len];
                    }
                } else if s.range > 0.0 && pool.ammo[i] > 0 && pool.cooldown[i] > 0 {
                    // 방금 쏘았다는 것은 사거리 안에 표적이 있다는 뜻이다.
                    // 사수는 전열 뒤에서 자리를 지킨다.
                    want = [0.0, 0.0];
                } else {
                    // 기병은 적 전열 정면이 아니라 무른 곳을 찾아 돈다.
                    // 그런 표적이 남지 않았으면 전면 장으로 되돌아온다.
                    let team_idx = pool.team[i] as usize;
                    let mut dir = None;
                    if s.is_cavalry {
                        dir = flows.get(2 + team_idx).and_then(|ff| ff.dir_at(p));
                    }
                    if dir.is_none() {
                        dir = flows.get(team_idx).and_then(|ff| ff.dir_at(p));
                    }
                    if let Some(d) = dir {
                        want = d;
                    }
                    let _ = goal;
                }

                // --- 2. 이웃 분리 ---
                let sep_r = s.radius * 2.0 * ENEMY_SPACING;
                let mut push = [0.0f32, 0.0];
                let mut fix = [0.0f32, 0.0];
                let mut seen = 0u32;
                // 상한에 걸리면 그 자리에서 순회를 끊는다. 예전에는 닫힘 안에서만
                // 빠져나와, 상한을 넘긴 뒤에도 남은 셀을 끝까지 훑고 있었다 —
                // 성문 앞처럼 사람이 뭉친 곳에서 그 비용이 폭주했다.
                // 상한 뒤로는 어차피 아무 일도 하지 않으므로 결과는 똑같다.
                grid.for_each_near_while(p, sep_r, |it| {
                    if seen >= MAX_SEP_NEIGHBORS {
                        return false;
                    }
                    let j = it.idx as usize;
                    if j == i {
                        return true;
                    }
                    let d = [p[0] - it.pos[0], p[1] - it.pos[1]];
                    let d2 = d[0] * d[0] + d[1] * d[1];
                    let mut min_d = s.radius + stats(it.type_id).radius;
                    if it.team != my_team {
                        min_d *= ENEMY_SPACING;
                    }
                    if d2 >= min_d * min_d {
                        return true;
                    }
                    seen += 1;
                    if d2 > 1e-6 {
                        let dist = d2.sqrt();
                        let strength = (min_d - dist) / min_d;
                        push[0] += d[0] / dist * strength;
                        push[1] += d[1] / dist * strength;
                        let overlap = (min_d - dist) * POS_FIX;
                        fix[0] += d[0] / dist * overlap;
                        fix[1] += d[1] / dist * overlap;
                    } else {
                        // 완전히 겹친 경우: 결정론적 지터로 떼어놓는다
                        let a = crate::rng::signed_f32(seed, i as u64, j as u64);
                        let b = crate::rng::signed_f32(seed, j as u64, i as u64);
                        push[0] += a;
                        push[1] += b;
                    }
                    true
                });

                // --- 3. 속도 적분 ---
                let speed = match state[i] {
                    UnitState::Rout => s.speed * 1.3, // 도망은 빠르다
                    UnitState::Charge => s.speed * crate::sim::charge::CHARGE_SPEED,
                    // 창을 세웠거나 나가떨어진 유닛은 제자리를 지킨다
                    UnitState::Brace => 0.0,
                    _ if pool.stagger[i] > 0 => 0.0,
                    _ => s.speed,
                };
                let desired = [
                    want[0] * speed + push[0] * SEP_STRENGTH * DT * speed,
                    want[1] * speed + push[1] * SEP_STRENGTH * DT * speed,
                ];
                let mut nv = [desired[0] - v[0], desired[1] - v[1]];
                let steer = (nv[0] * nv[0] + nv[1] * nv[1]).sqrt();
                if steer > ACCEL {
                    nv[0] *= ACCEL / steer;
                    nv[1] *= ACCEL / steer;
                }
                let mut nvel = [v[0] + nv[0], v[1] + nv[1]];
                let mut sp = (nvel[0] * nvel[0] + nvel[1] * nvel[1]).sqrt();

                // 발밑의 땅과 비탈이 속도를 정한다
                let (ground, grad) = terrain.ground_at(p);
                let mut cap = speed * ground.speed_mult();
                if sp > 1e-4 {
                    // 진행 방향으로의 기울기. 오르막은 다리를 무겁게, 내리막은 조금 가볍게
                    let slope = (grad[0] * nvel[0] + grad[1] * nvel[1]) / sp;
                    cap *= (1.0 - slope * 1.5).clamp(0.35, 1.25);
                }
                if sp > cap {
                    nvel[0] *= cap / sp;
                    nvel[1] *= cap / sp;
                    sp = cap;
                }

                let fl = (fix[0] * fix[0] + fix[1] * fix[1]).sqrt();
                if fl > MAX_POS_FIX {
                    fix[0] *= MAX_POS_FIX / fl;
                    fix[1] *= MAX_POS_FIX / fl;
                }
                let np = [
                    (p[0] + nvel[0] * DT + fix[0]).clamp(0.0, WORLD_SIZE - 0.01),
                    (p[1] + nvel[1] * DT + fix[1]).clamp(0.0, WORLD_SIZE - 0.01),
                ];

                pchunk[k] = np;
                vchunk[k] = nvel;
                fchunk[k] = if sp > 0.05 {
                    nvel[1].atan2(nvel[0])
                } else {
                    pool.facing[i]
                };

                let _ = tick;
            }
        });

    // 더블 버퍼 교체: prev_pos 는 렌더 보간용으로 남긴다
    std::mem::swap(&mut w.pool.prev_pos, &mut w.pool.pos);
    std::mem::swap(&mut w.pool.pos, &mut w.pos_next);
    std::mem::swap(&mut w.pool.vel, &mut w.vel_next);
    std::mem::swap(&mut w.pool.facing, &mut w.facing_next);
}
