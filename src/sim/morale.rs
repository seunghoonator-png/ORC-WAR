//! 사기와 패주.
//!
//! 고대 전투에서 병력의 대부분은 칼에 맞아 죽지 않는다. 전열이 무너지고
//! 등을 보인 뒤에 죽는다. 그래서 이 시스템이 전투 결과를 실제로 결정한다.
//!
//! 유닛마다 주변을 훑어 사기를 계산하면 30만 규모에서 감당이 안 되므로,
//! 전장에 격자를 깔고 "이 구역에서 최근 무슨 일이 있었는가"를 누적한다.
//! 유닛은 자기 발밑 칸의 값만 읽는다. 국소적 붕괴가 이웃 칸으로 번지는
//! 전염 현상도 이 격자 위에서 자연스럽게 나온다.

use rayon::prelude::*;

use crate::sim::pool::{UnitState, MORALE_SCALE};
use crate::sim::unit_types::stats;
use crate::sim::{World, CHUNK};

/// 사기 격자 해상도(m)
pub const MORALE_CELL: f32 = 16.0;
/// 충격이 가라앉는 속도 (틱당 잔존 비율)
const SHOCK_DECAY: f32 = 0.97;
/// 이웃 칸으로 번지는 비율
const SHOCK_SPREAD: f32 = 0.16;
/// 아군 사망 한 명이 남기는 충격
const SHOCK_PER_DEATH: f32 = 1.0;
/// 아군 패주 한 명이 남기는 충격 — 죽는 것보다 전염력이 크다
const SHOCK_PER_ROUT: f32 = 2.6;

/// 사기 갱신 주기(틱). 유닛마다 위상을 어긋나게 해서 부하를 고르게 편다.
const UPDATE_PERIOD: u64 = 8;

// --- 사기 증감 계수 (UPDATE_PERIOD 틱마다 한 번 적용) ---
/// 아군이 죽어 나가는 구역에서 받는 압박
const W_OWN_SHOCK: f32 = -20.0;
/// 적이 무너지는 것을 보는 고양감
const W_ENEMY_SHOCK: f32 = 14.0;
/// 국지 수적 우세/열세
const W_ODDS: f32 = 26.0;
/// 아군 전체 손실률이 주는 압박
const W_ATTRITION: f32 = -26.0;
/// 교전에서 벗어나 있을 때의 회복
const RECOVER: f32 = 20.0;
/// 교전 중에도 이만큼은 버틴다. 이게 없으면 붙어 싸우는 유닛의 사기는
/// 내려가기만 해서, 어느 쪽도 이기지 못한 채 전군이 함께 무너진다.
const HOLD: f32 = 11.0;
/// 후방·측면에서 맞았을 때 즉시 깎이는 사기 (combat 에서 직접 적용)
pub const FLANK_SHOCK: i16 = 22;
/// 기병 돌격을 정면으로 받았을 때
pub const CHARGE_SHOCK: i16 = 150;

/// 패주한 유닛이 다시 싸우러 돌아오는 사기 기준 (기준치 대비 비율).
/// 한 번 등을 보인 부대는 쉽게 돌아오지 않는다.
const RALLY_RATIO: f32 = 0.75;
/// 패주 중에는 사기가 이만큼만 회복된다
const ROUT_RECOVER_SCALE: f32 = 0.35;
/// 이만큼 도망치면 전장을 아주 벗어난 것으로 본다 (틱)
const FLEE_TICKS: u16 = 500;

pub struct MoraleField {
    pub w: usize,
    pub h: usize,
    pub cell: f32,
    /// 팀별 최근 충격량 (사망·패주가 남기고 시간이 지나면 가라앉는다)
    pub(crate) shock: [Vec<f32>; 2],
    /// 팀별 유닛 수 — 국지 수적 우세 판정에 쓴다
    pub(crate) presence: [Vec<u32>; 2],
    scratch: Vec<f32>,
}

impl MoraleField {
    pub fn new(world_size: f32) -> Self {
        let w = (world_size / MORALE_CELL).ceil() as usize;
        let n = w * w;
        Self {
            w,
            h: w,
            cell: MORALE_CELL,
            shock: [vec![0.0; n], vec![0.0; n]],
            presence: [vec![0; n], vec![0; n]],
            scratch: vec![0.0; n],
        }
    }

    /// 이 지점 칸에 있는 해당 팀 인원. 사격이 겨눌 곳을 고르는 데 쓴다.
    #[inline(always)]
    pub fn presence_at(&self, p: [f32; 2], team: usize) -> u32 {
        self.presence[team][self.cell_of(p)]
    }

    #[inline(always)]
    pub fn cell_of(&self, p: [f32; 2]) -> usize {
        let cx = ((p[0] / self.cell) as isize).clamp(0, self.w as isize - 1) as usize;
        let cy = ((p[1] / self.cell) as isize).clamp(0, self.h as isize - 1) as usize;
        cy * self.w + cx
    }

    /// 충격을 가라앉히고 이웃 칸으로 번지게 한다.
    fn decay_and_spread(&mut self) {
        for t in 0..2 {
            self.scratch.copy_from_slice(&self.shock[t]);
            let (w, h) = (self.w, self.h);
            for cy in 0..h {
                for cx in 0..w {
                    let c = cy * w + cx;
                    let mut sum = 0.0;
                    let mut n = 0.0;
                    if cx > 0 {
                        sum += self.scratch[c - 1];
                        n += 1.0;
                    }
                    if cx + 1 < w {
                        sum += self.scratch[c + 1];
                        n += 1.0;
                    }
                    if cy > 0 {
                        sum += self.scratch[c - w];
                        n += 1.0;
                    }
                    if cy + 1 < h {
                        sum += self.scratch[c + w];
                        n += 1.0;
                    }
                    let neighbour = if n > 0.0 { sum / n } else { 0.0 };
                    let here = self.scratch[c];
                    self.shock[t][c] = (here + (neighbour - here) * SHOCK_SPREAD) * SHOCK_DECAY;
                }
            }
        }
    }
}

pub fn step(w: &mut World) {
    let n = w.pool.len();
    if n == 0 {
        return;
    }

    // 도망친 시간 누적 — 대열로 돌아가면 초기화된다
    {
        let state = &w.pool.state;
        w.pool.rout_t.par_iter_mut().enumerate().for_each(|(i, t)| {
            *t = if state[i] == UnitState::Rout {
                t.saturating_add(1)
            } else {
                0
            };
        });
    }

    // --- 1. 격자 갱신 ---
    w.morale_field.decay_and_spread();

    for (pos, team, _) in &w.death_events {
        let c = w.morale_field.cell_of(*pos);
        w.morale_field.shock[*team as usize][c] += SHOCK_PER_DEATH;
    }
    for (pos, team) in &w.rout_events {
        let c = w.morale_field.cell_of(*pos);
        w.morale_field.shock[*team as usize][c] += SHOCK_PER_ROUT;
    }
    w.rout_events.clear();

    for t in 0..2 {
        w.morale_field.presence[t].iter_mut().for_each(|v| *v = 0);
    }
    for i in 0..n {
        if w.pool.state[i] == UnitState::Dead {
            continue;
        }
        let c = w.morale_field.cell_of(w.pool.pos[i]);
        w.morale_field.presence[w.pool.team[i] as usize][c] += 1;
    }

    // --- 2. 유닛 사기 갱신 (위상 분산) ---
    // 병력의 몇 할을 잃었는가 — 전군이 함께 느끼는 압박
    let attrition = [attrition_of(w, 0), attrition_of(w, 1)];

    let tick = w.tick;
    let field = &w.morale_field;
    let pos = &w.pool.pos;
    let team = &w.pool.team;
    let type_id = &w.pool.type_id;
    let rout_t = &w.pool.rout_t;

    let routed: Vec<Vec<(u32, [f32; 2], u8)>> = w
        .pool
        .morale
        .par_chunks_mut(CHUNK)
        .zip(w.pool.state.par_chunks_mut(CHUNK))
        .enumerate()
        .map(|(ci, (mchunk, schunk))| {
            let base = ci * CHUNK;
            let mut out = Vec::new();
            for k in 0..mchunk.len() {
                let i = base + k;
                if matches!(schunk[k], UnitState::Dead | UnitState::Fled)
                    || !(tick + i as u64).is_multiple_of(UPDATE_PERIOD)
                {
                    continue;
                }
                let me = team[i] as usize;
                let foe = 1 - me;
                let c = field.cell_of(pos[i]);

                let own_shock = field.shock[me][c];
                let enemy_shock = field.shock[foe][c];
                let own_n = field.presence[me][c] as f32;
                let foe_n = field.presence[foe][c] as f32;

                // 국지 병력비: -1(완전 열세) ~ +1(완전 우세)
                let odds = if own_n + foe_n > 0.0 {
                    (own_n - foe_n) / (own_n + foe_n)
                } else {
                    0.0
                };
                let engaged = foe_n > 0.0;

                let mut delta = W_OWN_SHOCK * own_shock.min(3.0)
                    + W_ENEMY_SHOCK * enemy_shock.min(3.0)
                    + W_ODDS * odds
                    + W_ATTRITION * attrition[me];
                delta += if !engaged {
                    if schunk[k] == UnitState::Rout {
                        RECOVER * ROUT_RECOVER_SCALE
                    } else {
                        RECOVER
                    }
                } else {
                    HOLD
                };

                let base_morale = stats(type_id[i]).morale_base as f32 * MORALE_SCALE as f32;
                let m = (mchunk[k] as f32 + delta).clamp(0.0, base_morale);
                mchunk[k] = m as i16;

                match schunk[k] {
                    UnitState::Rout => {
                        if rout_t[i] >= FLEE_TICKS {
                            // 여기까지 달아났으면 이 전투에는 다시 나오지 않는다
                            schunk[k] = UnitState::Fled;
                        } else if m > base_morale * RALLY_RATIO && !engaged {
                            // 안전한 곳에서 사기를 되찾으면 다시 대열로 돌아간다
                            schunk[k] = UnitState::Advance;
                        }
                    }
                    _ if m <= 0.0 => {
                        schunk[k] = UnitState::Rout;
                        out.push((i as u32, pos[i], team[i]));
                    }
                    _ => {}
                }
            }
            out
        })
        .collect();

    for chunk in routed {
        for (i, p, t) in chunk {
            w.pool.target[i as usize] = crate::sim::pool::NO_TARGET;
            w.rout_events.push((p, t));
        }
    }
}

/// 개전 병력 대비 잃은 비율 0.0 ~ 1.0
fn attrition_of(w: &World, team: usize) -> f32 {
    let start = w.start_strength[team].max(1) as f32;
    (w.stats.dead[team] as f32 / start).clamp(0.0, 1.0)
}
