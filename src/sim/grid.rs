//! 균일 공간 해시 그리드 — 매 틱 counting sort 로 전체 재구축한다.
//!
//! 증분 갱신보다 전체 재구축이 빠르다: 30만 유닛이 매 틱 전부 움직이므로 어차피
//! 대부분이 갱신 대상이고, 재구축은 분기 없는 O(n) 선형 패스라 캐시 친화적이다.
//!
//! 격자는 월드 전체가 아니라 **살아있는 유닛의 경계 상자**에만 친다. 월드는 3km²
//! 지만 실제 교전은 그중 일부에서만 벌어지고, 빈 셀도 prefix sum 비용은 똑같이
//! 물기 때문이다. (실측: 고정 격자 6.6ms → 적응 격자 0.2ms)

use rayon::prelude::*;

/// 격자에 담기는 한 항목.
///
/// 인덱스만 담고 위치는 원본 배열에서 찾아 읽으면, 이웃을 훑을 때마다 무작위
/// 접근이 일어나 캐시가 계속 어긋난다. 순회에 필요한 값을 셀 순서대로 함께
/// 담아 두면 이웃 순회가 통째로 순차 접근이 된다.
#[derive(Clone, Copy)]
pub struct GridItem {
    pub idx: u32,
    pub pos: [f32; 2],
    pub type_id: u8,
    pub team: u8,
}

pub struct Grid {
    /// 현재 셀 크기(m). 유닛이 넓게 퍼지면 자동으로 커진다
    pub cell_size: f32,
    /// 격자 좌상단의 월드 좌표
    pub origin: [f32; 2],
    pub w: usize,
    pub h: usize,
    /// 셀별 items 시작 오프셋. len = w*h + 1
    pub cell_start: Vec<u32>,
    /// 셀 순으로 정렬된 항목
    pub items: Vec<GridItem>,
    /// 재구축 중 쓰는 커서
    cursor: Vec<u32>,
    base_cell: f32,
}

impl Grid {
    pub fn new(_world_size: f32, cell_size: f32) -> Self {
        Self {
            cell_size,
            origin: [0.0, 0.0],
            w: 1,
            h: 1,
            cell_start: vec![0; 2],
            items: Vec::new(),
            cursor: vec![0; 1],
            base_cell: cell_size,
        }
    }

    #[inline(always)]
    pub fn cell_of(&self, p: [f32; 2]) -> usize {
        let (cx, cy) = self.cell_coords(p);
        cy * self.w + cx
    }

    #[inline(always)]
    pub fn cell_coords(&self, p: [f32; 2]) -> (usize, usize) {
        let cx = (((p[0] - self.origin[0]) / self.cell_size) as isize).clamp(0, self.w as isize - 1)
            as usize;
        let cy = (((p[1] - self.origin[1]) / self.cell_size) as isize).clamp(0, self.h as isize - 1)
            as usize;
        (cx, cy)
    }

    /// 살아있는 유닛만 담아 그리드를 다시 만든다.
    pub fn rebuild(&mut self, pos: &[[f32; 2]], alive: &[bool], team: &[u8], type_id: &[u8]) {
        // --- 1. 경계 상자 (병렬 reduce, 순서와 무관하므로 결정론적) ---
        let bbox = pos
            .par_iter()
            .enumerate()
            .filter(|(i, _)| alive[*i])
            .map(|(_, p)| [p[0], p[1], p[0], p[1]])
            .reduce(
                || [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
                |a, b| {
                    [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[2].max(b[2]),
                        a[3].max(b[3]),
                    ]
                },
            );

        let live = alive.iter().filter(|a| **a).count();
        if live == 0 || bbox[0] > bbox[2] {
            self.w = 1;
            self.h = 1;
            self.cell_start.clear();
            self.cell_start.resize(2, 0);
            self.items.clear();
            return;
        }

        // --- 2. 셀 크기 결정 ---
        // 셀 수가 유닛 수의 몇 배를 넘지 않게 유지한다. 유닛이 흩어질수록
        // 셀을 키워서, 빈 셀을 훑는 비용이 폭주하지 않도록 한다.
        let span_x = (bbox[2] - bbox[0]).max(1.0);
        let span_y = (bbox[3] - bbox[1]).max(1.0);
        let budget = (live * 3).max(4096);
        let mut cs = self.base_cell;
        let (mut w, mut h);
        loop {
            w = (span_x / cs).ceil() as usize + 1;
            h = (span_y / cs).ceil() as usize + 1;
            if w.saturating_mul(h) <= budget || cs > 256.0 {
                break;
            }
            cs *= 2.0;
        }
        self.cell_size = cs;
        self.origin = [bbox[0], bbox[1]];
        self.w = w;
        self.h = h;

        let ncells = w * h;
        self.cell_start.clear();
        self.cell_start.resize(ncells + 1, 0);
        self.cursor.clear();
        self.cursor.resize(ncells, 0);

        // --- 3. counting sort 3패스 ---
        for (i, p) in pos.iter().enumerate() {
            if !alive[i] {
                continue;
            }
            let c = self.cell_of(*p);
            self.cell_start[c + 1] += 1;
        }
        for c in 0..ncells {
            self.cell_start[c + 1] += self.cell_start[c];
        }
        self.cursor.copy_from_slice(&self.cell_start[..ncells]);

        self.items.clear();
        self.items.resize(
            live,
            GridItem {
                idx: 0,
                pos: [0.0, 0.0],
                type_id: 0,
                team: 0,
            },
        );
        for (i, p) in pos.iter().enumerate() {
            if !alive[i] {
                continue;
            }
            let c = self.cell_of(*p);
            let slot = self.cursor[c];
            self.items[slot as usize] = GridItem {
                idx: i as u32,
                pos: *p,
                type_id: type_id[i],
                team: team[i],
            };
            self.cursor[c] = slot + 1;
        }
    }

    #[inline(always)]
    pub fn cell_items(&self, cx: usize, cy: usize) -> &[GridItem] {
        let c = cy * self.w + cx;
        let a = self.cell_start[c] as usize;
        let b = self.cell_start[c + 1] as usize;
        &self.items[a..b]
    }

    /// 반경 r 을 덮는 셀 범위 (포함 구간).
    #[inline(always)]
    pub fn cell_range(&self, p: [f32; 2], r: f32) -> (usize, usize, usize, usize) {
        let lo_x = ((p[0] - r - self.origin[0]) / self.cell_size).floor() as isize;
        let hi_x = ((p[0] + r - self.origin[0]) / self.cell_size).floor() as isize;
        let lo_y = ((p[1] - r - self.origin[1]) / self.cell_size).floor() as isize;
        let hi_y = ((p[1] + r - self.origin[1]) / self.cell_size).floor() as isize;
        (
            lo_x.clamp(0, self.w as isize - 1) as usize,
            hi_x.clamp(0, self.w as isize - 1) as usize,
            lo_y.clamp(0, self.h as isize - 1) as usize,
            hi_y.clamp(0, self.h as isize - 1) as usize,
        )
    }

    /// 주변 셀의 항목을 순회하되, 닫힘이 false 를 돌려주면 거기서 멈춘다.
    ///
    /// 분리력처럼 "이웃 N명까지만 본다"는 상한이 있는 계산에서, 상한에 걸린
    /// 뒤에도 남은 셀을 끝까지 훑으면 밀집한 곳에서 비용이 폭주한다.
    /// (실측: 공성 30만 성문 정체 구간에서 이동 133ms → 30ms)
    #[inline(always)]
    pub fn for_each_near_while<F: FnMut(&GridItem) -> bool>(&self, p: [f32; 2], r: f32, mut f: F) {
        if self.items.is_empty() {
            return;
        }
        let (x0, x1, y0, y1) = self.cell_range(p, r);
        for cy in y0..=y1 {
            let row = cy * self.w;
            for cx in x0..=x1 {
                let c = row + cx;
                let a = self.cell_start[c] as usize;
                let b = self.cell_start[c + 1] as usize;
                for it in &self.items[a..b] {
                    if !f(it) {
                        return;
                    }
                }
            }
        }
    }

    /// 주변 셀의 항목을 순회한다.
    #[inline(always)]
    pub fn for_each_near<F: FnMut(&GridItem)>(&self, p: [f32; 2], r: f32, mut f: F) {
        if self.items.is_empty() {
            return;
        }
        let (x0, x1, y0, y1) = self.cell_range(p, r);
        for cy in y0..=y1 {
            let row = cy * self.w;
            for cx in x0..=x1 {
                let c = row + cx;
                let a = self.cell_start[c] as usize;
                let b = self.cell_start[c + 1] as usize;
                for it in &self.items[a..b] {
                    f(it);
                }
            }
        }
    }
}
