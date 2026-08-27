//! 플로우 필드 경로탐색.
//!
//! 30만 유닛이 각자 A*를 돌리는 것은 불가능하다. 대신 목표마다 격자 하나를
//! Dijkstra로 적분해 방향장을 만들고, 모든 유닛이 자기 발밑 셀의 화살표만 읽는다.
//! 유닛당 비용은 배열 조회 한 번이다.

use std::collections::BinaryHeap;

/// 통행 불가 비용
pub const IMPASSABLE: u8 = 255;
/// 직선 이동 비용(대각선과 비교하려고 10배 스케일로 쓴다)
const STEP_STRAIGHT: u32 = 10;
const STEP_DIAG: u32 = 14;
/// 도달 불가 표식
pub const UNREACHABLE: u16 = u16::MAX;
pub const NO_DIR: u8 = 255;

/// 8방향 단위 벡터 (정규화됨)
pub const DIRS: [[f32; 2]; 8] = [
    [1.0, 0.0],
    [0.707_106_8, 0.707_106_8],
    [0.0, 1.0],
    [-0.707_106_8, 0.707_106_8],
    [-1.0, 0.0],
    [-0.707_106_8, -0.707_106_8],
    [0.0, -1.0],
    [0.707_106_8, -0.707_106_8],
];

const OFFSETS: [(isize, isize, u32); 8] = [
    (1, 0, STEP_STRAIGHT),
    (1, 1, STEP_DIAG),
    (0, 1, STEP_STRAIGHT),
    (-1, 1, STEP_DIAG),
    (-1, 0, STEP_STRAIGHT),
    (-1, -1, STEP_DIAG),
    (0, -1, STEP_STRAIGHT),
    (1, -1, STEP_DIAG),
];

/// 지형 통행 비용 격자 (M1은 전부 평지)
pub struct CostField {
    pub w: usize,
    pub h: usize,
    pub cell: f32,
    pub cost: Vec<u8>,
}

impl CostField {
    pub fn flat(world_size: f32, cell: f32) -> Self {
        let w = (world_size / cell).ceil() as usize;
        Self {
            w,
            h: w,
            cell,
            cost: vec![1; w * w],
        }
    }
}

pub struct FlowField {
    pub w: usize,
    pub h: usize,
    pub cell: f32,
    pub integration: Vec<u16>,
    /// 각 셀에서 목표로 향하는 방향(DIRS 인덱스), 255 = 없음
    pub dir: Vec<u8>,
    heap: BinaryHeap<std::cmp::Reverse<(u32, u32)>>,
}

impl FlowField {
    pub fn new(cf: &CostField) -> Self {
        Self {
            w: cf.w,
            h: cf.h,
            cell: cf.cell,
            integration: vec![UNREACHABLE; cf.w * cf.h],
            dir: vec![NO_DIR; cf.w * cf.h],
            heap: BinaryHeap::new(),
        }
    }

    #[inline(always)]
    pub fn cell_of(&self, p: [f32; 2]) -> usize {
        let cx = ((p[0] / self.cell) as isize).clamp(0, self.w as isize - 1) as usize;
        let cy = ((p[1] / self.cell) as isize).clamp(0, self.h as isize - 1) as usize;
        cy * self.w + cx
    }

    /// 유닛 위치에서 읽는 이동 방향. 목표에 도달했거나 길이 없으면 None.
    #[inline(always)]
    pub fn dir_at(&self, p: [f32; 2]) -> Option<[f32; 2]> {
        let d = self.dir[self.cell_of(p)];
        if d == NO_DIR {
            None
        } else {
            Some(DIRS[d as usize])
        }
    }

    /// 목표 셀들로부터 거리장을 적분하고 방향장을 굽는다.
    pub fn compute(&mut self, cf: &CostField, sources: &[usize]) {
        debug_assert_eq!(cf.w, self.w);
        self.integration.iter_mut().for_each(|v| *v = UNREACHABLE);
        self.heap.clear();

        for &s in sources {
            if s < self.integration.len() && cf.cost[s] != IMPASSABLE {
                self.integration[s] = 0;
                self.heap.push(std::cmp::Reverse((0u32, s as u32)));
            }
        }

        while let Some(std::cmp::Reverse((d, c))) = self.heap.pop() {
            let c = c as usize;
            if d > self.integration[c] as u32 {
                continue; // 낡은 항목
            }
            let cx = (c % self.w) as isize;
            let cy = (c / self.w) as isize;
            for &(dx, dy, step) in &OFFSETS {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= self.w as isize || ny >= self.h as isize {
                    continue;
                }
                let n = ny as usize * self.w + nx as usize;
                let terrain = cf.cost[n];
                if terrain == IMPASSABLE {
                    continue;
                }
                let nd = d + step * terrain as u32;
                if nd < self.integration[n] as u32 && nd < UNREACHABLE as u32 {
                    self.integration[n] = nd as u16;
                    self.heap.push(std::cmp::Reverse((nd, n as u32)));
                }
            }
        }

        self.bake_directions(cf);
    }

    /// 적분장에서 각 셀의 내리막 방향을 찾아 방향장으로 굽는다.
    fn bake_directions(&mut self, cf: &CostField) {
        for cy in 0..self.h {
            for cx in 0..self.w {
                let c = cy * self.w + cx;
                if cf.cost[c] == IMPASSABLE || self.integration[c] == UNREACHABLE {
                    self.dir[c] = NO_DIR;
                    continue;
                }
                if self.integration[c] == 0 {
                    self.dir[c] = NO_DIR; // 목표 도착
                    continue;
                }
                let mut best = self.integration[c];
                let mut best_dir = NO_DIR;
                for (k, &(dx, dy, _)) in OFFSETS.iter().enumerate() {
                    let nx = cx as isize + dx;
                    let ny = cy as isize + dy;
                    if nx < 0 || ny < 0 || nx >= self.w as isize || ny >= self.h as isize {
                        continue;
                    }
                    let n = ny as usize * self.w + nx as usize;
                    let v = self.integration[n];
                    if v < best {
                        best = v;
                        best_dir = k as u8;
                    }
                }
                self.dir[c] = best_dir;
            }
        }
    }
}
