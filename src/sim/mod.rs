//! 전투 시뮬레이션 코어.
//!
//! 고정 스텝 20Hz. 렌더와 완전히 분리되어 있어 헤드리스로도 그대로 돌아간다.

pub mod combat;
pub mod flow;
pub mod grid;
pub mod movement;
pub mod pool;
pub mod unit_types;

use rayon::prelude::*;

use flow::{CostField, FlowField};
use grid::Grid;
use pool::{UnitPool, UnitState};

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

#[derive(Default, Clone)]
pub struct BattleStats {
    pub alive: [u32; 2],
    pub dead: [u32; 2],
    pub routed: [u32; 2],
    /// 틱별 생존 수 — 결과 리포트 그래프용
    pub history: Vec<[u32; 2]>,
}

/// 페이즈별 소요 시간(ms) — 성능 예산 검증용
#[derive(Default, Clone, Copy)]
pub struct PhaseTimes {
    pub flow: f64,
    pub movement: f64,
    pub grid: f64,
    pub combat: f64,
}

impl PhaseTimes {
    pub fn total(&self) -> f64 {
        self.flow + self.movement + self.grid + self.combat
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
    /// 팀별 플로우 필드 (인덱스 = 팀 번호)
    pub flows: Vec<FlowField>,
    pub tick: u64,
    pub seed: u64,
    pub stats: BattleStats,

    /// 이번 틱의 사망 지점 — 렌더가 시체 데칼로 굽는다
    pub death_events: Vec<([f32; 2], u8, u8)>,
    /// 직전 틱의 페이즈별 소요 시간
    pub phase: PhaseTimes,

    // 재사용 스크래치 (매 틱 할당을 피한다)
    alive: Vec<bool>,
    pos_next: Vec<[f32; 2]>,
    vel_next: Vec<[f32; 2]>,
    facing_next: Vec<f32>,
    flow_sources: Vec<usize>,
    flow_mark: Vec<bool>,
}

impl World {
    pub fn new(seed: u64, capacity: usize) -> Self {
        let cost = CostField::flat(WORLD_SIZE, FLOW_CELL);
        let flows = vec![FlowField::new(&cost), FlowField::new(&cost)];
        let ncells = cost.w * cost.h;
        Self {
            pool: UnitPool::with_capacity(capacity),
            grid: Grid::new(WORLD_SIZE, GRID_CELL),
            cost,
            flows,
            tick: 0,
            seed,
            stats: BattleStats::default(),
            death_events: Vec::new(),
            phase: PhaseTimes::default(),
            alive: Vec::with_capacity(capacity),
            pos_next: Vec::with_capacity(capacity),
            vel_next: Vec::with_capacity(capacity),
            facing_next: Vec::with_capacity(capacity),
            flow_sources: Vec::new(),
            flow_mark: vec![false; ncells],
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
        self.grid.rebuild(&self.pool.pos, &self.alive);
        self.recount();
        // 첫 틱부터 유닛이 갈 곳을 알도록 양 팀 필드를 미리 굽는다
        self.rebuild_flow(0);
        self.rebuild_flow(1);
    }

    pub fn step(&mut self) {
        use std::time::Instant;
        self.death_events.clear();

        // 1. 목표 재평가 — 한 틱에 한 팀씩 번갈아 갱신한다.
        //    전선은 천천히 움직이므로 매 틱 두 장을 다시 굽는 건 낭비다.
        let t0 = Instant::now();
        if self.tick.is_multiple_of(AI_PERIOD) {
            let t = ((self.tick / AI_PERIOD) % 2) as u8;
            self.rebuild_flow(t);
        }
        let t1 = Instant::now();

        // 2. 이동
        movement::step(self);
        let t2 = Instant::now();

        // 3. 그리드 재구축 (이동 후 최신 위치 기준)
        self.refresh_alive();
        self.grid.rebuild(&self.pool.pos, &self.alive);
        let t3 = Instant::now();

        // 4. 전투
        combat::step(self);
        let t4 = Instant::now();

        self.phase = PhaseTimes {
            flow: (t1 - t0).as_secs_f64() * 1e3,
            movement: (t2 - t1).as_secs_f64() * 1e3,
            grid: (t3 - t2).as_secs_f64() * 1e3,
            combat: (t4 - t3).as_secs_f64() * 1e3,
        };

        // 5. 정리
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
            .for_each(|(i, a)| *a = state[i] != UnitState::Dead);
    }

    fn recount(&mut self) {
        let state = &self.pool.state;
        let team = &self.pool.team;
        let (a0, a1, r0, r1) = (0..self.pool.len())
            .into_par_iter()
            .map(|i| match (state[i], team[i]) {
                (UnitState::Dead, _) => (0u32, 0u32, 0u32, 0u32),
                (UnitState::Rout, 0) => (1, 0, 1, 0),
                (UnitState::Rout, _) => (0, 1, 0, 1),
                (_, 0) => (1, 0, 0, 0),
                (_, _) => (0, 1, 0, 0),
            })
            .reduce(
                || (0, 0, 0, 0),
                |x, y| (x.0 + y.0, x.1 + y.1, x.2 + y.2, x.3 + y.3),
            );
        self.stats.alive = [a0, a1];
        self.stats.routed = [r0, r1];
    }

    /// 상대 팀이 점유한 모든 셀을 목표로 하는 플로우 필드를 굽는다.
    /// 한 점으로 모으는 대신 "가장 가까운 적"으로 향하게 되어 전선이 자연스럽게 선다.
    fn rebuild_flow(&mut self, team: u8) {
        let Self {
            pool,
            flows,
            cost,
            flow_sources,
            flow_mark,
            ..
        } = self;

        flow_sources.clear();
        flow_mark.iter_mut().for_each(|m| *m = false);

        let ff = &flows[team as usize];
        for i in 0..pool.len() {
            if pool.team[i] == team || pool.state[i] == UnitState::Dead {
                continue;
            }
            let c = ff.cell_of(pool.pos[i]);
            if !flow_mark[c] {
                flow_mark[c] = true;
                flow_sources.push(c);
            }
        }
        flows[team as usize].compute(cost, flow_sources);
    }

    pub fn outcome(&self, max_ticks: u64) -> Outcome {
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
