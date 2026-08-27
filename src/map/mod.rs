//! 지형.
//!
//! 전장은 판판한 초원이 아니다. 언덕은 사거리를 늘려 주고 오르막은 다리를
//! 무겁게 하며, 숲은 기병의 돌격을 죽이고 강은 건널 곳을 강요한다.
//! 이 모든 것이 "어디서 싸울 것인가"를 병력 구성만큼이나 중요하게 만든다.

pub mod gen;

/// 지형 격자 해상도(m)
pub const TERRAIN_CELL: f32 = 8.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Terrain {
    Plain = 0,
    /// 나무 밑 — 대열이 풀리고 기병이 속도를 못 낸다
    Forest = 1,
    /// 바위 지대 — 통행 불가
    Rock = 2,
    /// 깊은 물 — 통행 불가
    Water = 3,
    /// 여울 — 건널 수는 있지만 느리고 취약하다
    Ford = 4,
    /// 진창
    Marsh = 5,
}

impl Terrain {
    /// 이동 속도 배수
    #[inline(always)]
    pub fn speed_mult(self) -> f32 {
        match self {
            Terrain::Plain => 1.0,
            Terrain::Forest => 0.55,
            Terrain::Marsh => 0.6,
            Terrain::Ford => 0.5,
            Terrain::Rock | Terrain::Water => 0.15,
        }
    }

    /// 경로탐색 비용. 255 는 통행 불가.
    #[inline(always)]
    pub fn path_cost(self) -> u8 {
        match self {
            Terrain::Plain => 1,
            Terrain::Forest => 3,
            Terrain::Marsh => 3,
            Terrain::Ford => 4,
            Terrain::Rock | Terrain::Water => 255,
        }
    }

    #[inline(always)]
    pub fn passable(self) -> bool {
        !matches!(self, Terrain::Rock | Terrain::Water)
    }

    /// 기병이 속도를 붙일 수 있는 땅인가
    #[inline(always)]
    pub fn allows_charge(self) -> bool {
        matches!(self, Terrain::Plain)
    }

    /// 화살이 가지를 뚫지 못하고 걸릴 확률
    #[inline(always)]
    pub fn arrow_block(self) -> f32 {
        match self {
            Terrain::Forest => 0.55,
            _ => 0.0,
        }
    }
}

pub struct TerrainMap {
    pub w: usize,
    pub h: usize,
    pub cell: f32,
    pub kind: Vec<Terrain>,
    /// 해발 고도(m)
    pub height: Vec<f32>,
}

impl TerrainMap {
    pub fn flat(world_size: f32) -> Self {
        let w = (world_size / TERRAIN_CELL).ceil() as usize;
        Self {
            w,
            h: w,
            cell: TERRAIN_CELL,
            kind: vec![Terrain::Plain; w * w],
            height: vec![0.0; w * w],
        }
    }

    #[inline(always)]
    pub fn idx(&self, p: [f32; 2]) -> usize {
        let cx = ((p[0] / self.cell) as isize).clamp(0, self.w as isize - 1) as usize;
        let cy = ((p[1] / self.cell) as isize).clamp(0, self.h as isize - 1) as usize;
        cy * self.w + cx
    }

    #[inline(always)]
    pub fn at(&self, p: [f32; 2]) -> Terrain {
        self.kind[self.idx(p)]
    }

    /// 이중선형 보간한 고도. 칸 경계에서 지형이 계단처럼 튀지 않게 한다.
    #[inline(always)]
    pub fn height_at(&self, p: [f32; 2]) -> f32 {
        let fx = (p[0] / self.cell - 0.5).clamp(0.0, self.w as f32 - 1.001);
        let fy = (p[1] / self.cell - 0.5).clamp(0.0, self.h as f32 - 1.001);
        let x0 = fx as usize;
        let y0 = fy as usize;
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let a = self.height[y0 * self.w + x0];
        let b = self.height[y0 * self.w + x1];
        let c = self.height[y1 * self.w + x0];
        let d = self.height[y1 * self.w + x1];
        let top = a + (b - a) * tx;
        let bot = c + (d - c) * tx;
        top + (bot - top) * ty
    }

    /// 진행 방향으로의 경사 (양수면 오르막). 단위는 m/m.
    #[inline(always)]
    pub fn slope_along(&self, p: [f32; 2], dir: [f32; 2]) -> f32 {
        const STEP: f32 = 6.0;
        let ahead = [p[0] + dir[0] * STEP, p[1] + dir[1] * STEP];
        (self.height_at(ahead) - self.height_at(p)) / STEP
    }
}
