//! 전투 시뮬레이션 코어.
//!
//! 고정 스텝 20Hz. 렌더와 완전히 분리되어 있어 헤드리스로도 그대로 돌아간다.

pub mod charge;
pub mod combat;
pub mod flow;
pub mod grid;
pub mod morale;
pub mod movement;
pub mod pool;
pub mod projectile;
pub mod siege;
pub mod unit_types;

use rayon::prelude::*;

use crate::map::{castle::Castle, gen::MapOptions, TerrainMap};
use flow::{CostField, FlowField};
use grid::Grid;
use morale::MoraleField;
use pool::{UnitPool, UnitState};
use projectile::ProjectilePool;
use unit_types::N_TYPES;

pub const SIM_HZ: u32 = 20;
pub const DT: f32 = 1.0 / SIM_HZ as f32;
pub const WORLD_SIZE: f32 = 3072.0;
/// 근접 판정·분리에 쓰는 공간 해시 셀 크기(m)
pub const GRID_CELL: f32 = 2.0;
/// 플로우 필드 해상도(m)
pub const FLOW_CELL: f32 = 12.0;
/// 병렬 작업 청크 크기 — 고정값이어야 결과가 스레드 수와 무관해진다
pub const CHUNK: usize = 4096;
/// 목표 재평가 주기(틱)
const AI_PERIOD: u64 = 10;
/// 성벽이 무너진 뒤 길을 다시 굽기까지 기다리는 최소 틱
const PATH_REBUILD_COOLDOWN: u64 = 60;
/// 적 전열이 깔린 칸의 통행 비용 상한. 크게 잡을수록 기병이 멀리 돌아간다.
const LINE_COST: u8 = 10;

/// 무엇에 죽었는가. 결과 리포트에서 "실제로 사람을 줄인 것"을 보여준다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cause {
    /// 칼과 창이 닿아서
    Melee = 0,
    /// 화살·쇠뇌·투석
    Missile = 1,
    /// 기병 충격에 깔려서
    Charge = 2,
    /// 성벽 위에서 쏟아진 것에
    Drop = 3,
}

pub const N_CAUSES: usize = 4;

pub static CAUSE_NAMES: [&str; N_CAUSES] = ["근접", "사격", "돌격", "낙하물"];

/// 한 명의 죽음.
#[derive(Clone, Copy)]
pub struct Death {
    pub pos: [f32; 2],
    pub team: u8,
    pub type_id: u8,
    pub cause: Cause,
}

#[derive(Clone)]
pub struct BattleStats {
    pub alive: [u32; 2],
    pub dead: [u32; 2],
    pub routed: [u32; 2],
    /// 전장을 아주 벗어난 병력 — 죽지는 않았지만 이 전투에는 없다
    pub fled: [u32; 2],
    /// 명중한 발사체 수
    pub shots_landed: u64,
    /// 기병 충격이 실제로 꽂힌 횟수
    pub charge_impacts: u64,
    /// 성벽을 기어올라 넘어간 인원
    pub wall_breaches_climbed: u64,
    /// 성 한복판을 밟고 있는 공격측 인원
    pub objective_holders: u32,
    /// 성벽 위에서 쏟아부은 돌과 기름이 맞은 횟수
    pub drops_landed: u64,
    /// 틱별 생존 수 — 결과 리포트 그래프용
    pub history: Vec<[u32; 2]>,
    /// 개전 시 병종별 인원 [팀][병종]
    pub start_by_type: [[u32; N_TYPES]; 2],
    /// 병종별 전사자 [팀][병종]
    pub dead_by_type: [[u32; N_TYPES]; 2],
    /// 사인별 전사자 [팀][사인]
    pub dead_by_cause: [[u32; N_CAUSES]; 2],
}

impl Default for BattleStats {
    fn default() -> Self {
        Self {
            alive: [0; 2],
            dead: [0; 2],
            routed: [0; 2],
            fled: [0; 2],
            shots_landed: 0,
            charge_impacts: 0,
            wall_breaches_climbed: 0,
            objective_holders: 0,
            drops_landed: 0,
            history: Vec::new(),
            start_by_type: [[0; N_TYPES]; 2],
            dead_by_type: [[0; N_TYPES]; 2],
            dead_by_cause: [[0; N_CAUSES]; 2],
        }
    }
}

/// 페이즈별 소요 시간(ms) — 성능 예산 검증용
#[derive(Default, Clone, Copy)]
pub struct PhaseTimes {
    pub flow: f64,
    pub movement: f64,
    pub grid: f64,
    pub combat: f64,
    pub shooting: f64,
    pub siege: f64,
    pub morale: f64,
}

impl PhaseTimes {
    pub fn total(&self) -> f64 {
        self.flow
            + self.movement
            + self.grid
            + self.combat
            + self.shooting
            + self.siege
            + self.morale
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Ongoing,
    /// 해당 팀의 승리
    Victory(u8),
    /// 제한 틱 초과
    Timeout,
}

pub struct World {
    pub pool: UnitPool,
    pub grid: Grid,
    pub cost: CostField,
    pub terrain: TerrainMap,
    /// 공성전이면 성곽이 있다
    pub castle: Option<Castle>,
    /// 공격측이 성 한복판을 붙들고 있은 틱
    pub objective_hold: u32,
    /// 사다리가 걸려 있는 자리 (드물게 갱신)
    pub ladder_spots: Vec<[f32; 2]>,
    /// 플로우 필드.
    ///
    /// 0,1 = 팀별 전면(모든 적)  ·  2,3 = 팀별 무른 표적(적 사수와 무너진 부대)
    ///
    /// 병종마다 다른 장을 읽게 하는 것이 교리의 핵심이다. 전부 같은 장을 보면
    /// 경기병이 방패벽에 정면으로 박아 전멸한다.
    pub flows: Vec<FlowField>,
    pub tick: u64,
    pub seed: u64,
    pub stats: BattleStats,

    pub morale_field: MoraleField,
    pub projectiles: ProjectilePool,

    /// 이번 틱의 사망 — 렌더가 시체 데칼로 굽고, 통계가 집계한다
    pub death_events: Vec<Death>,
    /// 이번 틱에 무너진 유닛의 위치 — 사기 격자에 충격으로 들어간다
    pub rout_events: Vec<([f32; 2], u8)>,
    /// 이번 틱에 무너진 성벽 구간의 위치
    pub breach_events: Vec<[f32; 2]>,
    /// 개전 병력 — 손실률 계산의 분모
    pub start_strength: [u32; 2],
    /// 직전 틱의 페이즈별 소요 시간
    pub phase: PhaseTimes,

    // 재사용 스크래치 (매 틱 할당을 피한다)
    alive: Vec<bool>,
    pos_next: Vec<[f32; 2]>,
    vel_next: Vec<[f32; 2]>,
    facing_next: Vec<f32>,
    flow_sources: Vec<usize>,
    flow_mark: Vec<bool>,
    /// 기병용 경로 비용 — 지형에 적 전열의 두께를 얹은 것
    flow_cost_scratch: CostField,
    /// 다시 구워야 할 플로우 필드 대기열 — 한 틱에 한 장씩 뺀다
    flow_queue: Vec<usize>,
    /// 성벽이 무너져 길이 바뀌었다
    flows_dirty: bool,
    /// 마지막으로 길을 다시 구운 틱 — 연속 붕괴에 매번 반응하지 않기 위해
    last_path_rebuild: u64,
}

impl World {
    pub fn new(seed: u64, capacity: usize) -> Self {
        let cost = CostField::flat(WORLD_SIZE, FLOW_CELL);
        let flows = vec![
            FlowField::new(&cost),
            FlowField::new(&cost),
            FlowField::new(&cost),
            FlowField::new(&cost),
        ];
        let ncells = cost.w * cost.h;
        Self {
            pool: UnitPool::with_capacity(capacity),
            grid: Grid::new(WORLD_SIZE, GRID_CELL),
            cost,
            terrain: TerrainMap::flat(WORLD_SIZE),
            castle: None,
            objective_hold: 0,
            ladder_spots: Vec::new(),
            flows,
            tick: 0,
            seed,
            stats: BattleStats::default(),
            morale_field: MoraleField::new(WORLD_SIZE),
            projectiles: ProjectilePool::new(),
            flow_queue: Vec::new(),
            death_events: Vec::new(),
            rout_events: Vec::new(),
            breach_events: Vec::new(),
            start_strength: [0, 0],
            phase: PhaseTimes::default(),
            alive: Vec::with_capacity(capacity),
            pos_next: Vec::with_capacity(capacity),
            vel_next: Vec::with_capacity(capacity),
            facing_next: Vec::with_capacity(capacity),
            flow_sources: Vec::new(),
            flow_mark: vec![false; ncells],
            flow_cost_scratch: CostField::flat(WORLD_SIZE, FLOW_CELL),
            flows_dirty: false,
            last_path_rebuild: 0,
        }
    }

    /// 지형을 깔고 경로 비용을 다시 굽는다. 스폰 전에 호출해야 한다.
    pub fn set_terrain(&mut self, opts: MapOptions, seed: u64) {
        self.terrain = crate::map::gen::generate(WORLD_SIZE, opts, seed);
        self.rebuild_cost();
    }

    /// 성곽을 세우고 지형에 찍는다.
    pub fn place_castle(&mut self, castle: Castle) {
        castle.stamp(&mut self.terrain);
        self.terrain.bake_gradients();
        self.castle = Some(castle);
        self.rebuild_cost();
    }

    /// 성벽이 무너진 뒤 경로 비용을 다시 굽는다.
    pub fn rebuild_cost_public(&mut self) {
        self.rebuild_cost();
    }

    /// 다음 틱에 모든 플로우 필드를 다시 굽게 한다.
    pub fn mark_flows_dirty(&mut self) {
        self.flows_dirty = true;
    }

    /// 지형에서 경로탐색 비용을 만든다.
    fn rebuild_cost(&mut self) {
        let cf = &mut self.cost;
        for cy in 0..cf.h {
            for cx in 0..cf.w {
                let p = [(cx as f32 + 0.5) * cf.cell, (cy as f32 + 0.5) * cf.cell];
                let t = self.terrain.at(p);
                let mut c = t.path_cost();
                if c != crate::sim::flow::IMPASSABLE {
                    // 가파른 비탈은 돌아가는 편이 빠르다
                    let gx = self.terrain.slope_along(p, [1.0, 0.0]).abs();
                    let gy = self.terrain.slope_along(p, [0.0, 1.0]).abs();
                    let steep = gx.max(gy);
                    c = c.saturating_add((steep * 9.0) as u8);
                }
                cf.cost[cy * cf.w + cx] = c;
            }
        }
    }

    /// 스폰이 끝난 뒤 한 번 호출해 스크래치 배열 길이를 맞춘다.
    pub fn finalize_spawns(&mut self) {
        let n = self.pool.len();
        self.alive.resize(n, true);
        self.pos_next.resize(n, [0.0, 0.0]);
        self.vel_next.resize(n, [0.0, 0.0]);
        self.facing_next.resize(n, 0.0);
        self.refresh_alive();
        self.grid.rebuild(
            &self.pool.pos,
            &self.alive,
            &self.pool.team,
            &self.pool.type_id,
        );
        self.recount();
        self.start_strength = self.stats.alive;
        // 병종별 개전 인원 — 결과 리포트의 손실률 분모
        for i in 0..n {
            self.stats.start_by_type[self.pool.team[i] as usize][self.pool.type_id[i] as usize] +=
                1;
        }
        // 첫 틱부터 유닛이 갈 곳을 알도록 모든 장을 미리 굽는다
        for i in 0..self.flows.len() {
            self.rebuild_flow(i);
        }
    }

    pub fn step(&mut self) {
        use std::time::Instant;
        self.death_events.clear();
        self.breach_events.clear();

        // 1. 목표 재평가 — 한 틱에 한 장씩 돌아가며 갱신한다.
        //    전선은 천천히 움직이므로 매 틱 전부 다시 굽는 건 낭비다.
        let t0 = Instant::now();
        if self.flows_dirty && self.tick >= self.last_path_rebuild + PATH_REBUILD_COOLDOWN {
            // 성벽이 무너지면 길이 통째로 달라지므로 네 장을 전부 다시 구워야 한다.
            // 다만 투석기가 구간을 연달아 무너뜨리는 동안 매 틱 이 짓을 하면
            // 경로탐색만으로 예산을 다 쓴다. 잠깐 묵혔다가 처리한다.
            self.flows_dirty = false;
            self.last_path_rebuild = self.tick;
            self.rebuild_cost();
            // 네 장을 한 틱에 몰아 구우면 **그 틱만** 예산을 세 배로 넘긴다.
            // 화면에서는 프레임 하나가 통째로 멈추는 것으로 보인다.
            // 한 틱에 한 장씩 굽는다 — 뒤늦게 굽히는 장은 두어 틱(0.1초) 낡은
            // 길을 들고 있지만, 그 사이 사람이 걷는 거리는 한 걸음이 못 된다.
            self.flow_queue = (0..self.flows.len()).rev().collect();
        }
        if let Some(which) = self.flow_queue.pop() {
            self.rebuild_flow(which);
        } else if self.tick.is_multiple_of(AI_PERIOD) {
            let n = self.flows.len() as u64;
            let which = ((self.tick / AI_PERIOD) % n) as usize;
            self.rebuild_flow(which);
        }
        let t1 = Instant::now();

        // 2. 이동
        movement::step(self);
        let t2 = Instant::now();

        // 3. 그리드 재구축 (이동 후 최신 위치 기준)
        self.refresh_alive();
        self.grid.rebuild(
            &self.pool.pos,
            &self.alive,
            &self.pool.team,
            &self.pool.type_id,
        );
        let t3 = Instant::now();

        // 4. 돌격 충돌과 창벽
        charge::step(self);

        // 5. 전투
        combat::step(self);
        let t4 = Instant::now();

        // 5b. 원거리
        projectile::step(self);
        let t4a = Instant::now();
        // 5c. 공성 — 성벽을 부수고, 넘고, 위에서 들이붓는다
        siege::step(self);
        let t4b = Instant::now();

        // 6. 사기 — 실제로 전투의 승패를 가르는 단계
        morale::step(self);
        let t5 = Instant::now();

        self.phase = PhaseTimes {
            flow: (t1 - t0).as_secs_f64() * 1e3,
            movement: (t2 - t1).as_secs_f64() * 1e3,
            grid: (t3 - t2).as_secs_f64() * 1e3,
            combat: (t4 - t3).as_secs_f64() * 1e3,
            shooting: (t4a - t4).as_secs_f64() * 1e3,
            siege: (t4b - t4a).as_secs_f64() * 1e3,
            morale: (t5 - t4b).as_secs_f64() * 1e3,
        };

        // 7. 정리 — 이번 틱의 사망을 병종·사인별로 적는다
        for d in &self.death_events {
            let t = d.team as usize;
            self.stats.dead_by_type[t][d.type_id as usize] += 1;
            self.stats.dead_by_cause[t][d.cause as usize] += 1;
        }
        self.tick += 1;
        if self.tick.is_multiple_of(5) {
            self.recount();
            let a = self.stats.alive;
            self.stats.history.push(a);
        }
    }

    fn refresh_alive(&mut self) {
        let state = &self.pool.state;
        self.alive
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, a)| *a = !matches!(state[i], UnitState::Dead | UnitState::Fled));
    }

    fn recount(&mut self) {
        let state = &self.pool.state;
        let team = &self.pool.team;
        // [생존0, 생존1, 패주0, 패주1, 이탈0, 이탈1]
        let c = (0..self.pool.len())
            .into_par_iter()
            .map(|i| {
                let mut c = [0u32; 6];
                let t = team[i] as usize;
                match state[i] {
                    UnitState::Dead => {}
                    UnitState::Fled => c[4 + t] = 1,
                    UnitState::Rout => {
                        c[t] = 1;
                        c[2 + t] = 1;
                    }
                    _ => c[t] = 1,
                }
                c
            })
            .reduce(
                || [0u32; 6],
                |a, b| {
                    let mut o = [0u32; 6];
                    for k in 0..6 {
                        o[k] = a[k] + b[k];
                    }
                    o
                },
            );
        self.stats.alive = [c[0], c[1]];
        self.stats.routed = [c[2], c[3]];
        self.stats.fled = [c[4], c[5]];
    }

    /// 상대가 점유한 셀들을 목표로 하는 플로우 필드를 굽는다.
    /// 한 점으로 모으는 대신 "가장 가까운 적"으로 향하게 되어 전선이 자연스럽게 선다.
    ///
    /// `idx` 가 2 이상이면 무른 표적(사수와 무너진 부대)만 목표로 삼는다.
    fn rebuild_flow(&mut self, idx: usize) {
        let team = (idx % 2) as u8;
        let soft_only = idx >= 2;
        let Self {
            pool,
            flows,
            cost,
            flow_sources,
            flow_mark,
            flow_cost_scratch,
            ..
        } = self;

        flow_sources.clear();
        flow_mark.iter_mut().for_each(|m| *m = false);

        let ff = &flows[idx];
        for i in 0..pool.len() {
            if pool.team[i] == team || !pool.is_alive(i) {
                continue;
            }
            if soft_only {
                // 등을 보였거나, 활을 든 채 전열 뒤에 선 무리
                let soft = pool.state[i] == UnitState::Rout
                    || unit_types::stats(pool.type_id[i]).range > 0.0;
                if !soft {
                    continue;
                }
            }
            let c = ff.cell_of(pool.pos[i]);
            if !flow_mark[c] {
                flow_mark[c] = true;
                flow_sources.push(c);
            }
        }

        if !soft_only {
            flows[idx].compute(cost, flow_sources);
            return;
        }

        // 기병에게는 적 전열이 통과 비용이 큰 지형이나 마찬가지다.
        // 이걸 반영하지 않으면 뒤에 선 사수를 노리랍시고 방패벽 한복판으로
        // 걸어들어간다. 얹어 주면 비로소 옆으로 돌아 들어간다.
        let scratch = flow_cost_scratch;
        scratch.cost.copy_from_slice(&cost.cost);
        for i in 0..pool.len() {
            if pool.team[i] != team || !pool.is_alive(i) {
                continue;
            }
            let st = unit_types::stats(pool.type_id[i]);
            // 창칼을 든 채 대열을 지키는 무리만 장벽이 된다
            if st.range > 0.0 || st.is_cavalry || pool.state[i] == UnitState::Rout {
                continue;
            }
            let c = ff.cell_of(pool.pos[i]);
            scratch.cost[c] = scratch.cost[c].saturating_add(2).min(LINE_COST);
        }
        flows[idx].compute(scratch, flow_sources);
    }

    pub fn outcome(&self, max_ticks: u64) -> Outcome {
        // 성을 밟고 버텼으면 그것으로 끝이다
        if self.castle.is_some() && self.objective_hold >= siege::HOLD_TO_WIN {
            return Outcome::Victory(0);
        }
        let fighting = [
            self.stats.alive[0] - self.stats.routed[0],
            self.stats.alive[1] - self.stats.routed[1],
        ];
        match (fighting[0] == 0, fighting[1] == 0) {
            (true, false) => Outcome::Victory(1),
            (false, true) => Outcome::Victory(0),
            (true, true) => Outcome::Timeout,
            _ if self.tick >= max_ticks => Outcome::Timeout,
            _ => Outcome::Ongoing,
        }
    }

    /// 결정론 검증용 상태 해시.
    pub fn state_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for i in 0..self.pool.len() {
            let p = self.pool.pos[i];
            h ^= p[0].to_bits() as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
            h ^= p[1].to_bits() as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
            h ^= self.pool.hp[i].to_bits() as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
            h ^= self.pool.state[i] as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h
    }
}
