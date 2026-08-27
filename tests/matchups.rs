//! 병종 상성 회귀 — docs/DESIGN.md §4 의 규칙이 실제로 그렇게 굴러가는지 본다.
//!
//! 수치를 조금 만지면 상성이 통째로 뒤집히곤 하므로, 규칙마다 기대 결과를
//! 못 박아 둔다. 여기가 깨지면 밸런스가 아니라 규칙이 무너진 것이다.

use orc_war::scenario::Scenario;
use orc_war::sim::unit_types::{ARCHER, CAV_HEAVY, CAV_LIGHT, INF_AXE, INF_SPEAR, INF_SWORD};
use orc_war::sim::Outcome;

struct Result {
    alive: [u32; 2],
    dead: [u32; 2],
    ticks: u64,
    outcome: Outcome,
}

fn fight(a: (u8, u32), b: (u8, u32), gap: f32, seed: u64) -> Result {
    const MAX: u64 = 6_000;
    let sc = Scenario::matchup(a, b, seed, MAX, gap);
    let mut w = sc.build();
    let outcome = loop {
        w.step();
        match w.outcome(MAX) {
            Outcome::Ongoing => {}
            done => break done,
        }
    };
    Result {
        alive: w.stats.alive,
        dead: w.stats.dead,
        ticks: w.tick,
        outcome,
    }
}

/// 교환비 — 아군 하나를 잃는 동안 적을 몇이나 눕혔는가
fn trade(r: &Result) -> f64 {
    r.dead[1] as f64 / (r.dead[0].max(1)) as f64
}

#[test]
fn cavalry_annihilates_archers_in_the_open() {
    // 개활지에서 활을 든 상대에게 기병은 재앙이다.
    let r = fight((CAV_HEAVY, 600), (ARCHER, 600), 90.0, 1);
    assert!(
        trade(&r) > 3.0,
        "중기병이 궁수를 압도하지 못한다: 전사 {:?}, 교환비 {:.1}",
        r.dead,
        trade(&r)
    );
}

#[test]
fn braced_pikes_break_a_frontal_charge() {
    // 자리를 지킨 창벽에 정면으로 뛰어들면 말이 먼저 꿰뚫린다.
    let r = fight((CAV_HEAVY, 600), (INF_SPEAR, 600), 90.0, 1);
    assert!(
        r.dead[0] > r.dead[1],
        "창벽이 정면 돌격을 받아내지 못한다: 기병 전사 {}, 장창 전사 {}",
        r.dead[0],
        r.dead[1]
    );
}

#[test]
fn armour_shrugs_off_arrows() {
    // 판금을 두른 상대에게 화살은 소모전일 뿐이다.
    let vs_armoured = fight((ARCHER, 600), (INF_AXE, 600), 110.0, 1);
    assert!(
        matches!(vs_armoured.outcome, Outcome::Victory(1)),
        "중갑 보병이 화살비를 뚫지 못한다: 생존 {:?}",
        vs_armoured.alive
    );
}

#[test]
fn arrows_run_out() {
    // 화살통이 비면 사수는 그냥 약한 보병이다. 탄약이 무한이면 궁수 혼자
    // 전장을 정리해 버린다.
    let r = fight((ARCHER, 800), (INF_SWORD, 800), 130.0, 3);
    assert!(
        r.dead[0] > 200,
        "사수가 근접을 허용하지 않는다 — 탄약 제한이 듣지 않는 것으로 보인다: 전사 {:?}",
        r.dead
    );
}

#[test]
fn light_cavalry_is_not_a_battering_ram() {
    // 경기병은 전열을 뚫는 병종이 아니다. 정면으로 박으면 갈린다.
    let light = fight((CAV_LIGHT, 600), (INF_SWORD, 600), 90.0, 1);
    let heavy = fight((CAV_HEAVY, 600), (INF_SWORD, 600), 90.0, 1);
    assert!(
        trade(&light) < trade(&heavy),
        "경기병이 중기병만큼 전열을 뚫는다: 경 {:.2} / 중 {:.2}",
        trade(&light),
        trade(&heavy)
    );
}

#[test]
fn morale_ends_battles_before_annihilation() {
    // 사기가 있어야 전투가 끝난다. 없으면 마지막 한 명까지 서로 베다가
    // 제한 틱에 걸린다.
    let r = fight((INF_SWORD, 1500), (ARCHER, 1500), 110.0, 5);
    assert!(
        !matches!(r.outcome, Outcome::Timeout),
        "전투가 결판나지 않았다 ({}틱)",
        r.ticks
    );
    let survivors = r.alive[0] + r.alive[1];
    assert!(
        survivors > 100,
        "전멸할 때까지 싸웠다 — 사기 붕괴가 일어나지 않는다: 생존 {survivors}"
    );
}
