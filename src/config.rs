//! 유저가 정하는 것 전부.
//!
//! 이 시뮬레이터에서 사람이 하는 일은 여기까지다. 지형을 고르고, 병력을 정하고,
//! 편성을 고른 뒤에는 손을 뗀다 — 그 다음은 전장이 알아서 굴러간다.
//!
//! 화면(설정 화면)과 콘솔(`--field hills --units 300000`)이 같은 구조체를 만든다.

use crate::map::gen::{MapKind, MapOptions};
use crate::scenario::Scenario;
use crate::sim::unit_types::{
    ARCHER, CAV_ARCHER, CAV_HEAVY, CAV_LIGHT, CROSSBOW, INF_AXE, INF_SPEAR, INF_SWORD,
};

/// 어디서 싸우는가.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Battlefield {
    Plains,
    Hills,
    Mountain,
    River,
    Forest,
    Siege,
}

pub const BATTLEFIELDS: [Battlefield; 6] = [
    Battlefield::Plains,
    Battlefield::Hills,
    Battlefield::Mountain,
    Battlefield::River,
    Battlefield::Forest,
    Battlefield::Siege,
];

impl Battlefield {
    pub fn name(self) -> &'static str {
        match self {
            Battlefield::Plains => "평지",
            Battlefield::Hills => "언덕",
            Battlefield::Mountain => "산악",
            Battlefield::River => "도하",
            Battlefield::Forest => "삼림",
            Battlefield::Siege => "공성",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Battlefield::Plains => "숨을 곳도 막을 곳도 없다. 순수한 물량 대결",
            Battlefield::Hills => "능선이 전장을 가른다. 고지의 사수가 멀리 쏜다",
            Battlefield::Mountain => "협곡이 전부다. 소수가 다수를 막을 수 있는 유일한 지형",
            Battlefield::River => "강은 여울로만 건넌다. 양군이 좁은 건널목으로 몰린다",
            Battlefield::Forest => "대열이 풀리고 화살이 가지에 걸린다. 말은 속도를 못 낸다",
            Battlefield::Siege => "해자와 성벽. 공격측 4, 수비측 1 로 나뉜다",
        }
    }

    pub fn map(self) -> MapOptions {
        match self {
            Battlefield::Plains | Battlefield::Siege => MapOptions::default(),
            Battlefield::Hills => MapOptions {
                kind: MapKind::Hills,
                ..Default::default()
            },
            Battlefield::Mountain => MapOptions {
                kind: MapKind::Mountain,
                ..Default::default()
            },
            Battlefield::River => MapOptions {
                river: true,
                forest: true,
                ..Default::default()
            },
            Battlefield::Forest => MapOptions {
                forest: true,
                rocks: true,
                ..Default::default()
            },
        }
    }
}

/// 무엇으로 싸우는가. 양측이 같은 편성을 쓴다 — 고증이 아니라 강함을 보는 것이 목적이다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Doctrine {
    Balanced,
    Infantry,
    Cavalry,
    Missile,
}

pub const DOCTRINES: [Doctrine; 4] = [
    Doctrine::Balanced,
    Doctrine::Infantry,
    Doctrine::Cavalry,
    Doctrine::Missile,
];

impl Doctrine {
    pub fn name(self) -> &'static str {
        match self {
            Doctrine::Balanced => "균형",
            Doctrine::Infantry => "보병",
            Doctrine::Cavalry => "기병",
            Doctrine::Missile => "사수",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Doctrine::Balanced => "전열 보병에 창병과 사수, 양익에 기병",
            Doctrine::Infantry => "밀집한 방패와 창. 두껍고 느리다",
            Doctrine::Cavalry => "얇은 전열과 두꺼운 양익. 돌격이 전부다",
            Doctrine::Missile => "창벽 뒤에 사수를 쌓는다. 화살통이 비면 약해진다",
        }
    }

    /// 병종별 비율. 야전 편성이다.
    pub fn mix(self) -> &'static [(u8, f32)] {
        match self {
            Doctrine::Balanced => &[
                (INF_SWORD, 0.42),
                (INF_SPEAR, 0.18),
                (ARCHER, 0.22),
                (CAV_HEAVY, 0.10),
                (CAV_LIGHT, 0.08),
            ],
            Doctrine::Infantry => &[
                (INF_SWORD, 0.46),
                (INF_SPEAR, 0.28),
                (INF_AXE, 0.14),
                (ARCHER, 0.12),
            ],
            Doctrine::Cavalry => &[
                (INF_SWORD, 0.22),
                (INF_SPEAR, 0.12),
                (ARCHER, 0.12),
                (CAV_HEAVY, 0.32),
                (CAV_LIGHT, 0.14),
                (CAV_ARCHER, 0.08),
            ],
            Doctrine::Missile => &[
                (INF_SWORD, 0.20),
                (INF_SPEAR, 0.22),
                (ARCHER, 0.32),
                (CROSSBOW, 0.18),
                (CAV_LIGHT, 0.08),
            ],
        }
    }

    /// 공성전 공격측 보병 편성.
    ///
    /// 말은 성벽을 오르지 못한다. 기병 몫은 걸어서 성벽에 붙을 병종으로 돌린다 —
    /// 기병 편성으로 성을 치면 그만큼 사람이 적어지고, 그것이 이 편성의 대가다.
    pub fn siege_mix(self) -> Vec<(u8, f32)> {
        use crate::sim::unit_types::stats;
        let mut foot: Vec<(u8, f32)> = self
            .mix()
            .iter()
            .copied()
            .filter(|(t, _)| !stats(*t).is_cavalry)
            .collect();
        let sum: f32 = foot.iter().map(|(_, s)| s).sum();
        for (_, s) in foot.iter_mut() {
            *s /= sum;
        }
        foot
    }
}

pub const ARMY_SIZES: [u32; 6] = [20_000, 50_000, 100_000, 200_000, 300_000, 500_000];

/// 한 판의 설정.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BattleConfig {
    pub field: Battlefield,
    /// 양팀 합계 병력
    pub total: u32,
    pub doctrine: Doctrine,
    pub seed: u64,
}

impl Default for BattleConfig {
    fn default() -> Self {
        Self {
            field: Battlefield::Plains,
            total: 100_000,
            doctrine: Doctrine::Balanced,
            seed: 1,
        }
    }
}

impl BattleConfig {
    /// 제한 틱. 20Hz 이므로 40,000 틱은 전장 시간으로 33분이다.
    pub const MAX_TICKS: u64 = 40_000;

    pub fn scenario(&self) -> Scenario {
        match self.field {
            Battlefield::Siege => Scenario::siege_with_mix(
                self.total * 4 / 5,
                self.total / 5,
                self.seed,
                Self::MAX_TICKS,
                true,
                &self.doctrine.siege_mix(),
            ),
            _ => Scenario::field_with_mix(
                self.total,
                self.doctrine.mix(),
                self.seed,
                Self::MAX_TICKS,
            )
            .on_map(self.field.map()),
        }
    }

    /// 창 제목에 실을 한 줄.
    pub fn title(&self) -> String {
        format!(
            "{} · {} 편성 · {}명 · 씨앗 {}",
            self.field.name(),
            self.doctrine.name(),
            self.total,
            self.seed
        )
    }
}
