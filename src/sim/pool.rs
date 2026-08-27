//! UnitPool — 유닛 데이터를 배열들의 구조(SoA)로 보관한다.
//!
//! 30만 유닛에서 구조체 배열(AoS)을 쓰면 매 페이즈마다 쓰지 않는 필드까지
//! 캐시라인에 끌려온다. 페이즈별로 필요한 배열만 순회하도록 필드를 쪼갠다.

use crate::sim::unit_types::stats;

/// `target` 배열의 "대상 없음" 표식
pub const NO_TARGET: u32 = u32::MAX;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum UnitState {
    /// 목표를 향해 전진 중
    Advance = 0,
    /// 근접 교전 중
    Fight = 1,
    /// 돌격 가속 중(기병)
    Charge = 2,
    /// 브레이스(장창)
    Brace = 3,
    /// 패주
    Rout = 4,
    /// 성벽 위
    OnWall = 5,
    /// 등반 중
    Climb = 6,
    Dead = 7,
}

pub struct UnitPool {
    // --- hot: 매 틱 전 유닛 순회 ---
    pub pos: Vec<[f32; 2]>,
    /// 직전 틱 위치 — 렌더 보간용
    pub prev_pos: Vec<[f32; 2]>,
    pub vel: Vec<[f32; 2]>,
    pub hp: Vec<f32>,
    pub state: Vec<UnitState>,
    pub type_id: Vec<u8>,
    pub team: Vec<u8>,

    // --- warm ---
    pub facing: Vec<f32>,
    /// 남은 공격 쿨다운(틱)
    pub cooldown: Vec<u16>,
    pub target: Vec<u32>,
    /// 참조할 플로우 필드 id
    pub goal: Vec<u16>,
    /// 사기 0..200 (0.5 단위)
    pub morale: Vec<u8>,
    pub charge_t: Vec<u16>,
    /// 0 = 지상, 1 = 성벽 위
    pub layer: Vec<u8>,

    len: usize,
}

impl UnitPool {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            pos: Vec::with_capacity(cap),
            prev_pos: Vec::with_capacity(cap),
            vel: Vec::with_capacity(cap),
            hp: Vec::with_capacity(cap),
            state: Vec::with_capacity(cap),
            type_id: Vec::with_capacity(cap),
            team: Vec::with_capacity(cap),
            facing: Vec::with_capacity(cap),
            cooldown: Vec::with_capacity(cap),
            target: Vec::with_capacity(cap),
            goal: Vec::with_capacity(cap),
            morale: Vec::with_capacity(cap),
            charge_t: Vec::with_capacity(cap),
            layer: Vec::with_capacity(cap),
            len: 0,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn is_alive(&self, i: usize) -> bool {
        self.state[i] != UnitState::Dead
    }

    /// 전투에 참여 가능한 상태인가(패주·사망 제외).
    #[inline]
    pub fn is_fighting_fit(&self, i: usize) -> bool {
        !matches!(self.state[i], UnitState::Dead | UnitState::Rout)
    }

    pub fn spawn(&mut self, type_id: u8, team: u8, pos: [f32; 2], goal: u16) -> u32 {
        let s = stats(type_id);
        let idx = self.len as u32;
        self.pos.push(pos);
        self.prev_pos.push(pos);
        self.vel.push([0.0, 0.0]);
        self.hp.push(s.hp);
        self.state.push(UnitState::Advance);
        self.type_id.push(type_id);
        self.team.push(team);
        // 공격측(0)은 북쪽, 방어측(1)은 남쪽을 본다
        self.facing
            .push(if team == 0 { 0.0 } else { std::f32::consts::PI });
        self.cooldown.push(0);
        self.target.push(NO_TARGET);
        self.goal.push(goal);
        self.morale.push(s.morale_base);
        self.charge_t.push(0);
        self.layer.push(0);
        self.len += 1;
        idx
    }

    /// hot 배열이 차지하는 대략적 바이트 수 — 메모리 예산 검증용.
    pub fn memory_bytes(&self) -> usize {
        let n = self.len;
        n * (8 + 8 + 8 + 4 + 1 + 1 + 1 + 4 + 2 + 4 + 2 + 1 + 2 + 1)
    }
}
