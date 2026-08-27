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
    /// 각 셀에서 목표로 향하는 단위 벡터. [0,0] 이면 방향 없음(도착 또는 고립).
    ///
    /// 8방향으로 양자화하면 셀 경계마다 진행 방향이 뚝뚝 끊겨, 대형이 12m
    /// 간격의 세로 빗살로 갈라진다. 적분장의 기울기를 그대로 쓰면 연속적이다.
    pub dir: Vec<[f32; 2]>,
    heap: BinaryHeap<std::cmp::Reverse<(u32, u32)>>,
}

impl FlowField {
    pub fn new(cf: &CostField) -> Self {
        Self {
            w: cf.w,
            h: cf.h,
            cell: cf.cell,
            integration: vec![UNREACHABLE; cf.w * cf.h],
            dir: vec![[0.0, 0.0]; cf.w * cf.h],
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
    ///
    /// 인접 네 셀을 이중선형 보간해서 셀 경계를 넘을 때 방향이 튀지 않게 한다.
    #[inline(always)]
    pub fn dir_at(&self, p: [f32; 2]) -> Option<[f32; 2]> {
        let fx = (p[0] / self.cell - 0.5).clamp(0.0, self.w as f32 - 1.001);
        let fy = (p[1] / self.cell - 0.5).clamp(0.0, self.h as f32 - 1.001);
        let x0 = fx as usize;
        let y0 = fy as usize;
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);

        let a = self.dir[y0 * self.w + x0];
        let b = self.dir[y0 * self.w + x1];
        let c = self.dir[y1 * self.w + x0];
        let d = self.dir[y1 * self.w + x1];
        let mix = |p: [f32; 2], q: [f32; 2], t: f32| {
            [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]
        };
        let top = mix(a, b, tx);
        let bot = mix(c, d, tx);
        let v = mix(top, bot, ty);
        let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
        if len < 1e-3 {
            None
        } else {
            Some([v[0] / len, v[1] / len])
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

    /// 적분장의 기울기를 내리막 방향으로 구워 방향장을 만든다.
    fn bake_directions(&mut self, cf: &CostField) {
        for cy in 0..self.h {
            for cx in 0..self.w {
                let c = cy * self.w + cx;
                if cf.cost[c] == IMPASSABLE
                    || self.integration[c] == UNREACHABLE
                    || self.integration[c] == 0
                {
                    self.dir[c] = [0.0, 0.0];
                    continue;
                }
                let here = self.integration[c] as f32;
                // 갈 수 없는 이웃은 자기 값으로 대체해 벽처럼 취급한다
                let sample = |x: isize, y: isize| -> f32 {
                    if x < 0 || y < 0 || x >= self.w as isize || y >= self.h as isize {
                        return here;
                    }
                    let n = y as usize * self.w + x as usize;
                    if cf.cost[n] == IMPASSABLE || self.integration[n] == UNREACHABLE {
                        here
                    } else {
                        self.integration[n] as f32
                    }
                };
                let (x, y) = (cx as isize, cy as isize);
                let gx = sample(x + 1, y) - sample(x - 1, y);
                let gy = sample(x, y + 1) - sample(x, y - 1);
                let v = [-gx, -gy];
                let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
                self.dir[c] = if len > 1e-6 {
                    [v[0] / len, v[1] / len]
                } else {
                    [0.0, 0.0]
                };
            }
        }
    }
}
