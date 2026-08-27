//! 성곽.
//!
//! 성벽과 해자는 지형에 직접 찍어 넣는다. 그러면 기존 경로탐색이 그대로
//! "여기는 못 지나간다"를 알아듣고, 성벽이 무너지면 그 칸의 지형만 잔해로
//! 바꿔 주면 길이 열린다. 통행 규칙을 따로 만들 필요가 없다.

use crate::map::{Terrain, TerrainMap};

/// 성벽 한 구간의 내구도
pub const SEGMENT_HP: f32 = 4_000.0;
/// 성문 내구도
pub const GATE_HP: f32 = 2_500.0;
/// 성벽 구간 하나의 길이(m)
const SEGMENT_LEN: f32 = 40.0;
/// 성벽 두께(m)
const WALL_THICK: f32 = 10.0;
/// 해자 폭(m)
const MOAT_WIDTH: f32 = 22.0;
/// 성벽과 해자 사이 여유(m)
const MOAT_GAP: f32 = 14.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    South,
    North,
    West,
    East,
}

pub struct WallSegment {
    /// 구간 중심
    pub center: [f32; 2],
    pub side: Side,
    pub hp: f32,
    /// 성문이 들어앉은 구간인가
    pub is_gate: bool,
    pub breached: bool,
}

pub struct Castle {
    pub center: [f32; 2],
    /// 성벽 사각형의 반폭 (x, y)
    pub half: [f32; 2],
    pub segments: Vec<WallSegment>,
    /// 성 한복판의 목표 구역 — 공격측이 여기를 차지하면 이긴다
    pub objective: [f32; 2],
    pub objective_radius: f32,
    pub has_moat: bool,
}

impl Castle {
    /// 남쪽 성벽 한복판에 성문을 둔 사각 성곽.
    pub fn square(center: [f32; 2], half: [f32; 2], has_moat: bool) -> Self {
        let mut segments = Vec::new();
        // 남·북 성벽
        let n_x = ((half[0] * 2.0) / SEGMENT_LEN).round().max(3.0) as i32;
        for k in 0..n_x {
            let t = (k as f32 + 0.5) / n_x as f32;
            let x = center[0] - half[0] + half[0] * 2.0 * t;
            // 성문은 남쪽 한복판 — 공격측이 정면으로 마주하는 곳
            let is_gate = k == n_x / 2;
            segments.push(WallSegment {
                center: [x, center[1] - half[1]],
                side: Side::South,
                hp: if is_gate { GATE_HP } else { SEGMENT_HP },
                is_gate,
                breached: false,
            });
            segments.push(WallSegment {
                center: [x, center[1] + half[1]],
                side: Side::North,
                hp: SEGMENT_HP,
                is_gate: false,
                breached: false,
            });
        }
        // 동·서 성벽
        let n_y = ((half[1] * 2.0) / SEGMENT_LEN).round().max(3.0) as i32;
        for k in 0..n_y {
            let t = (k as f32 + 0.5) / n_y as f32;
            let y = center[1] - half[1] + half[1] * 2.0 * t;
            segments.push(WallSegment {
                center: [center[0] - half[0], y],
                side: Side::West,
                hp: SEGMENT_HP,
                is_gate: false,
                breached: false,
            });
            segments.push(WallSegment {
                center: [center[0] + half[0], y],
                side: Side::East,
                hp: SEGMENT_HP,
                is_gate: false,
                breached: false,
            });
        }
        Self {
            center,
            half,
            segments,
            objective: center,
            objective_radius: 45.0,
            has_moat,
        }
    }

    /// 성벽 구간이 덮는 지형 칸 범위.
    fn segment_bounds(&self, seg: &WallSegment) -> ([f32; 2], [f32; 2]) {
        let (hx, hy) = match seg.side {
            Side::South | Side::North => (SEGMENT_LEN * 0.5, WALL_THICK * 0.5),
            Side::West | Side::East => (WALL_THICK * 0.5, SEGMENT_LEN * 0.5),
        };
        (
            [seg.center[0] - hx, seg.center[1] - hy],
            [seg.center[0] + hx, seg.center[1] + hy],
        )
    }

    /// 지형에 성곽을 찍는다.
    pub fn stamp(&self, m: &mut TerrainMap) {
        // 성 안쪽은 평지로 정리한다
        for cy in 0..m.h {
            for cx in 0..m.w {
                let p = [cx as f32 * m.cell, cy as f32 * m.cell];
                if (p[0] - self.center[0]).abs() < self.half[0]
                    && (p[1] - self.center[1]).abs() < self.half[1]
                {
                    let i = cy * m.w + cx;
                    m.kind[i] = Terrain::Plain;
                    m.height[i] = 0.0;
                }
            }
        }

        if self.has_moat {
            self.stamp_moat(m);
        }

        for seg in &self.segments {
            self.stamp_segment(m, seg);
        }
    }

    fn stamp_moat(&self, m: &mut TerrainMap) {
        let outer = [
            self.half[0] + MOAT_GAP + MOAT_WIDTH,
            self.half[1] + MOAT_GAP + MOAT_WIDTH,
        ];
        let inner = [self.half[0] + MOAT_GAP, self.half[1] + MOAT_GAP];
        for cy in 0..m.h {
            for cx in 0..m.w {
                let p = [cx as f32 * m.cell, cy as f32 * m.cell];
                let dx = (p[0] - self.center[0]).abs();
                let dy = (p[1] - self.center[1]).abs();
                let in_outer = dx < outer[0] && dy < outer[1];
                let in_inner = dx < inner[0] && dy < inner[1];
                if in_outer && !in_inner {
                    let i = cy * m.w + cx;
                    m.kind[i] = Terrain::Moat;
                    m.height[i] = -6.0;
                }
            }
        }
        // 성문 앞 다리 — 해자를 건널 수 있는 유일한 길
        let gate = self.gate_center();
        for cy in 0..m.h {
            for cx in 0..m.w {
                let p = [cx as f32 * m.cell, cy as f32 * m.cell];
                if (p[0] - gate[0]).abs() < 16.0
                    && p[1] < gate[1]
                    && p[1] > gate[1] - (MOAT_GAP + MOAT_WIDTH + 20.0)
                {
                    let i = cy * m.w + cx;
                    if m.kind[i] == Terrain::Moat {
                        m.kind[i] = Terrain::Plain;
                        m.height[i] = 0.0;
                    }
                }
            }
        }
    }

    fn stamp_segment(&self, m: &mut TerrainMap, seg: &WallSegment) {
        let (lo, hi) = self.segment_bounds(seg);
        let kind = if seg.breached {
            Terrain::Rubble
        } else if seg.is_gate {
            Terrain::Gate
        } else {
            Terrain::Wall
        };
        let cx0 = ((lo[0] / m.cell).floor() as isize).clamp(0, m.w as isize - 1) as usize;
        let cx1 = ((hi[0] / m.cell).ceil() as isize).clamp(0, m.w as isize - 1) as usize;
        let cy0 = ((lo[1] / m.cell).floor() as isize).clamp(0, m.h as isize - 1) as usize;
        let cy1 = ((hi[1] / m.cell).ceil() as isize).clamp(0, m.h as isize - 1) as usize;
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                let i = cy * m.w + cx;
                m.kind[i] = kind;
                m.height[i] = if seg.breached { 3.0 } else { 12.0 };
            }
        }
    }

    /// 무너진 구간의 지형만 다시 찍는다.
    ///
    /// 무너진 돌더미는 그 앞의 해자도 메운다. 이게 없으면 성벽을 아무리 부숴도
    /// 해자에 막혀 들어갈 수가 없고, 결국 성문 하나만 두들기는 전투가 된다.
    pub fn restamp_segment(&self, m: &mut TerrainMap, idx: usize) {
        let seg = &self.segments[idx];
        self.stamp_segment(m, seg);
        if !seg.breached || !self.has_moat {
            return;
        }
        // 구간 바깥쪽으로 해자를 가로지르는 통로를 낸다
        let out = match seg.side {
            Side::South => [0.0f32, -1.0],
            Side::North => [0.0, 1.0],
            Side::West => [-1.0, 0.0],
            Side::East => [1.0, 0.0],
        };
        let span = SEGMENT_LEN * 0.5;
        let across = MOAT_GAP + MOAT_WIDTH + WALL_THICK + 12.0;
        let mut t = 0.0;
        while t < across {
            let base = [seg.center[0] + out[0] * t, seg.center[1] + out[1] * t];
            let mut u = -span;
            while u <= span {
                // 구간 폭 방향으로 훑는다
                let p = if out[0] == 0.0 {
                    [base[0] + u, base[1]]
                } else {
                    [base[0], base[1] + u]
                };
                let i = m.idx(p);
                if m.kind[i] == Terrain::Moat {
                    m.kind[i] = Terrain::Rubble;
                    m.height[i] = 0.0;
                }
                u += m.cell * 0.5;
            }
            t += m.cell * 0.5;
        }
    }

    pub fn gate_center(&self) -> [f32; 2] {
        self.segments
            .iter()
            .find(|s| s.is_gate)
            .map(|s| s.center)
            .unwrap_or(self.center)
    }

    /// 이 지점을 때리면 어느 구간이 상하는가.
    pub fn segment_at(&self, p: [f32; 2]) -> Option<usize> {
        for (i, seg) in self.segments.iter().enumerate() {
            if seg.breached {
                continue;
            }
            let (lo, hi) = self.segment_bounds(seg);
            if p[0] >= lo[0] - 4.0
                && p[0] <= hi[0] + 4.0
                && p[1] >= lo[1] - 4.0
                && p[1] <= hi[1] + 4.0
            {
                return Some(i);
            }
        }
        None
    }
}
