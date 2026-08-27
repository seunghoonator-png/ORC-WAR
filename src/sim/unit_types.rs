//! 병종 스탯 테이블 — docs/DESIGN.md §3 의 수치를 그대로 옮긴 것.
//!
//! 유닛은 `type_id`(u8)만 들고 다니고 스탯은 전부 이 정적 테이블에서 읽는다.

use crate::sim::SIM_HZ;

pub const N_TYPES: usize = 11;

// 병종 인덱스 상수
pub const INF_SWORD: u8 = 0;
pub const INF_SPEAR: u8 = 1;
pub const INF_AXE: u8 = 2;
pub const ARCHER: u8 = 3;
pub const CROSSBOW: u8 = 4;
pub const CAV_LIGHT: u8 = 5;
pub const CAV_HEAVY: u8 = 6;
pub const CAV_ARCHER: u8 = 7;
// --- 공성병기 ---
pub const RAM: u8 = 8;
pub const CATAPULT: u8 = 9;
pub const LADDER: u8 = 10;

#[derive(Clone, Copy, Debug)]
pub struct UnitStats {
    pub name: &'static str,
    pub hp: f32,
    /// 근접 명중 1회 대미지
    pub melee_dmg: f32,
    /// 공격 쿨다운(틱)
    pub attack_period: u16,
    /// 근접 교전 사거리(m) — 무기 길이 포함
    pub reach: f32,
    /// 원거리 사거리(m). 0이면 원거리 없음
    pub range: f32,
    /// 이동 속도(m/s)
    pub speed: f32,
    /// 피해 감쇄 0.0..1.0
    pub armor: f32,
    /// 정면 방패 차단 확률 0.0..1.0
    pub shield: f32,
    /// 충돌·돌격 계산용 질량
    pub mass: f32,
    /// 점유 반경(m) — 분리력 계산에 쓴다
    pub radius: f32,
    /// 지니고 나온 발사체 수. 다 쓰면 그 뒤로는 근접전뿐이다.
    ///
    /// 화살을 무한으로 두면 궁수가 접근 시간 내내 쏘아대 전장을 혼자 정리한다.
    /// 실제로 사수의 사격 시간을 정하는 것은 사거리가 아니라 화살통이다.
    pub ammo: u16,
    pub morale_base: u8,
    pub is_cavalry: bool,
    /// 돌격 충격에 실리는 비율.
    ///
    /// 말을 탔다고 다 같은 돌격이 아니다. 랜스를 겨누고 마갑을 두른 중장기병만
    /// 대열을 뚫는다. 사브르를 든 경기병에게 같은 계수를 주면 경기병이 방패벽을
    /// 정면으로 밀어버리는, 기획과 어긋난 결과가 나온다.
    pub charge_power: f32,
    /// 장창 브레이스 가능 여부
    pub can_brace: bool,
    /// 성벽·성문에 주는 피해. 사람을 베는 무기로는 돌벽을 어쩌지 못한다.
    pub siege_dmg: f32,
    /// 성벽 등반을 돕는 장비인가
    pub is_ladder: bool,
}

/// 공속(회/초) → 쿨다운 틱
const fn period(per_sec: f32) -> u16 {
    let t = SIM_HZ as f32 / per_sec;
    if t < 1.0 {
        1
    } else {
        t as u16
    }
}

pub static UNIT_STATS: [UnitStats; N_TYPES] = [
    UnitStats {
        name: "검방 보병",
        hp: 120.0,
        melee_dmg: 35.0,
        attack_period: period(1.0),
        reach: 1.0,
        range: 0.0,
        speed: 1.6,
        armor: 0.30,
        shield: 0.60,
        mass: 1.0,
        radius: 0.45,
        ammo: 0,
        morale_base: 100,
        is_cavalry: false,
        charge_power: 0.0,
        can_brace: false,
        siege_dmg: 1.0,
        is_ladder: false,
    },
    UnitStats {
        name: "장창병",
        hp: 110.0,
        melee_dmg: 30.0,
        attack_period: period(0.8),
        reach: 2.2,
        range: 0.0,
        speed: 1.5,
        armor: 0.20,
        shield: 0.0,
        mass: 1.0,
        radius: 0.45,
        ammo: 0,
        morale_base: 100,
        is_cavalry: false,
        charge_power: 0.0,
        can_brace: true,
        siege_dmg: 1.0,
        is_ladder: false,
    },
    UnitStats {
        name: "중갑 도끼병",
        hp: 140.0,
        melee_dmg: 55.0,
        attack_period: period(0.7),
        reach: 1.2,
        range: 0.0,
        speed: 1.3,
        armor: 0.45,
        shield: 0.0,
        mass: 1.2,
        radius: 0.50,
        ammo: 0,
        morale_base: 110,
        is_cavalry: false,
        charge_power: 0.0,
        can_brace: false,
        siege_dmg: 6.0,
        is_ladder: false,
    },
    UnitStats {
        name: "궁수",
        hp: 80.0,
        melee_dmg: 10.0,
        attack_period: period(0.9),
        reach: 0.9,
        range: 120.0,
        speed: 1.7,
        armor: 0.05,
        shield: 0.0,
        mass: 0.8,
        radius: 0.42,
        ammo: 24,
        morale_base: 80,
        is_cavalry: false,
        charge_power: 0.0,
        can_brace: false,
        siege_dmg: 1.0,
        is_ladder: false,
    },
    UnitStats {
        name: "석궁수",
        hp: 85.0,
        melee_dmg: 12.0,
        attack_period: period(0.25),
        reach: 0.9,
        range: 90.0,
        speed: 1.6,
        armor: 0.10,
        shield: 0.0,
        mass: 0.8,
        radius: 0.42,
        ammo: 20,
        morale_base: 85,
        is_cavalry: false,
        charge_power: 0.0,
        can_brace: false,
        siege_dmg: 1.0,
        is_ladder: false,
    },
    UnitStats {
        name: "경기병",
        hp: 130.0,
        melee_dmg: 30.0,
        attack_period: period(1.0),
        reach: 1.4,
        range: 0.0,
        speed: 4.5,
        armor: 0.15,
        shield: 0.0,
        mass: 4.0,
        radius: 0.95,
        ammo: 0,
        morale_base: 95,
        is_cavalry: true,
        charge_power: 0.3,
        can_brace: false,
        siege_dmg: 1.0,
        is_ladder: false,
    },
    UnitStats {
        name: "중기병",
        hp: 180.0,
        melee_dmg: 45.0,
        attack_period: period(0.9),
        reach: 2.0,
        range: 0.0,
        speed: 3.8,
        armor: 0.40,
        shield: 0.0,
        mass: 5.5,
        radius: 1.10,
        ammo: 0,
        morale_base: 120,
        is_cavalry: true,
        charge_power: 1.0,
        can_brace: false,
        siege_dmg: 1.0,
        is_ladder: false,
    },
    UnitStats {
        name: "궁기병",
        hp: 110.0,
        melee_dmg: 15.0,
        attack_period: period(0.8),
        reach: 1.2,
        range: 90.0,
        speed: 4.2,
        armor: 0.10,
        shield: 0.0,
        mass: 3.5,
        radius: 0.90,
        ammo: 20,
        morale_base: 90,
        is_cavalry: true,
        charge_power: 0.2,
        can_brace: false,
        siege_dmg: 1.0,
        is_ladder: false,
    },
    UnitStats {
        name: "파성추",
        hp: 2500.0,
        melee_dmg: 0.0,
        attack_period: period(0.5),
        reach: 6.0,
        range: 0.0,
        speed: 0.6,
        armor: 0.55,
        shield: 0.0,
        mass: 8.0,
        radius: 3.0,
        ammo: 0,
        morale_base: 255,
        is_cavalry: false,
        charge_power: 0.0,
        can_brace: false,
        siege_dmg: 500.0,
        is_ladder: false,
    },
    UnitStats {
        name: "투석기",
        hp: 800.0,
        melee_dmg: 0.0,
        attack_period: period(0.09),
        reach: 4.0,
        range: 400.0,
        speed: 0.4,
        armor: 0.1,
        shield: 0.0,
        mass: 6.0,
        radius: 3.0,
        ammo: 40,
        morale_base: 255,
        is_cavalry: false,
        charge_power: 0.0,
        can_brace: false,
        siege_dmg: 260.0,
        is_ladder: false,
    },
    UnitStats {
        name: "공성 사다리",
        hp: 300.0,
        melee_dmg: 0.0,
        attack_period: period(1.0),
        reach: 5.0,
        range: 0.0,
        speed: 1.1,
        armor: 0.2,
        shield: 0.0,
        mass: 2.0,
        radius: 1.6,
        ammo: 0,
        morale_base: 255,
        is_cavalry: false,
        charge_power: 0.0,
        can_brace: false,
        siege_dmg: 0.0,
        is_ladder: true,
    },
];

/// 공성병기인가 — 사람이 아니라 장비다. 사기도 없고 도망치지도 않는다.
#[inline(always)]
pub fn is_engine(type_id: u8) -> bool {
    type_id >= RAM
}

#[inline(always)]
pub fn stats(type_id: u8) -> &'static UnitStats {
    &UNIT_STATS[type_id as usize]
}
