//! 시뮬레이션 기본 동작 — 스폰, 이동, 교전, 종료 판정.

use orc_war::scenario::{Formation, Scenario};
use orc_war::sim::pool::UnitState;
use orc_war::sim::unit_types::{stats, INF_SWORD};
use orc_war::sim::{Outcome, WORLD_SIZE};

#[test]
fn spawns_requested_count() {
    let sc = Scenario::head_on(10_000, INF_SWORD, 1, 100);
    let w = sc.build();
    assert_eq!(w.pool.len(), 10_000);
    assert_eq!(w.stats.alive, [5_000, 5_000]);
}

#[test]
fn units_stay_inside_world() {
    let sc = Scenario::head_on(4_000, INF_SWORD, 5, 300);
    let mut w = sc.build();
    for _ in 0..300 {
        w.step();
    }
    for p in &w.pool.pos {
        assert!(
            p[0] >= 0.0 && p[0] < WORLD_SIZE && p[1] >= 0.0 && p[1] < WORLD_SIZE,
            "유닛이 월드 밖으로 나갔다: {p:?}"
        );
        assert!(p[0].is_finite() && p[1].is_finite(), "위치가 NaN/Inf 다");
    }
}

#[test]
fn armies_close_and_fight() {
    let sc = Scenario::head_on(4_000, INF_SWORD, 1, 1200);
    let mut w = sc.build();
    let start_gap = gap_between_armies(&w);
    for _ in 0..1200 {
        w.step();
    }
    assert!(
        gap_between_armies(&w) < start_gap,
        "양군이 서로 접근하지 않았다"
    );
    assert!(
        w.stats.dead[0] + w.stats.dead[1] > 0,
        "1200틱 동안 교전이 전혀 없었다"
    );
}

#[test]
fn lopsided_fight_ends_in_victory() {
    // 40 대 4000. 소수측이 전멸하고 종료 판정이 서야 한다.
    let mid = WORLD_SIZE * 0.5;
    let sc = Scenario {
        name: "lopsided".into(),
        seed: 11,
        max_ticks: 3_000,
        map: Default::default(),
        formations: vec![
            Formation {
                type_id: INF_SWORD,
                team: 0,
                count: 40,
                center: [mid, mid - 12.0],
                width: 20.0,
                front: [0.0, 1.0],
            },
            Formation {
                type_id: INF_SWORD,
                team: 1,
                count: 4_000,
                center: [mid, mid + 30.0],
                width: 120.0,
                front: [0.0, -1.0],
            },
        ],
    };
    let mut w = sc.build();
    let mut outcome = Outcome::Ongoing;
    for _ in 0..3_000 {
        w.step();
        match w.outcome(3_000) {
            Outcome::Ongoing => {}
            done => {
                outcome = done;
                break;
            }
        }
    }
    assert_eq!(outcome, Outcome::Victory(1), "다수측이 이기지 못했다");
    assert_eq!(w.stats.alive[0], 0);
}

#[test]
fn damage_respects_armor() {
    // 갑옷 감쇄가 스탯대로 반영되는지 — HP 대비 필요 타격 수로 확인한다.
    let s = stats(INF_SWORD);
    let per_hit = s.melee_dmg * (1.0 - s.armor);
    let hits_needed = (s.hp / per_hit).ceil();
    assert!(
        (3.0..=6.0).contains(&hits_needed),
        "검방 보병끼리 {hits_needed} 대에 죽는다 — 냉병기 전투로는 비현실적"
    );
}

#[test]
fn dead_units_stop_acting() {
    let sc = Scenario::head_on(2_000, INF_SWORD, 2, 1200);
    let mut w = sc.build();
    for _ in 0..1200 {
        w.step();
    }
    for i in 0..w.pool.len() {
        if w.pool.state[i] == UnitState::Dead {
            assert!(w.pool.hp[i] <= 0.0, "죽은 유닛의 HP가 양수다");
            assert_eq!(w.pool.vel[i], [0.0, 0.0], "죽은 유닛이 움직인다");
        } else {
            assert!(w.pool.hp[i] > 0.0, "살아있는 유닛의 HP가 0 이하다");
        }
    }
}

fn gap_between_armies(w: &orc_war::sim::World) -> f32 {
    let mut front0 = f32::MIN;
    let mut front1 = f32::MAX;
    for i in 0..w.pool.len() {
        if w.pool.state[i] == UnitState::Dead {
            continue;
        }
        if w.pool.team[i] == 0 {
            front0 = front0.max(w.pool.pos[i][1]);
        } else {
            front1 = front1.min(w.pool.pos[i][1]);
        }
    }
    front1 - front0
}

#[test]
fn mirror_battle_stays_balanced() {
    // 완전히 같은 조건의 양군이라면 승패는 씨앗에 따라 갈려야 하고, 한쪽으로
    // 체계적으로 기울면 안 된다.
    //
    // 개별 전투의 편차는 원래 크다 — 전선이 한 번 무너지면 걷잡을 수 없이
    // 벌어지기 때문이다. 그래서 평균만 보지 않고 표준오차로 나눈 z 값을 본다.
    // 편차가 아무리 커도 방향이 무작위라면 z 는 0 근처에 머문다.
    //
    // 이 테스트는 실제 버그를 세 개 잡아서 들어왔다.
    //  - 피해를 한 패스로 적용해 '이미 죽은 대상에 대한 타격'이 팀 번호 순서대로
    //    버려지면서, 먼저 처리되는 진영만 공격을 낭비했다.
    //  - 대형의 모자란 마지막 행이 항상 북쪽에 놓여, 남쪽 진영만 최전선에
    //    구멍이 뚫린 채 개전했다.
    //  - 대상 탐색 반경이 넓어, 먼저 적을 포착한 쪽이 진형을 풀고 끌려나갔다.
    const SEEDS: u64 = 16;
    let mut diffs = Vec::new();
    for seed in 1..=SEEDS {
        let sc = Scenario::head_on(3_000, INF_SWORD, seed, 1_500);
        let mut w = sc.build();
        for _ in 0..1_500 {
            w.step();
        }
        let a = w.stats.alive[0] as f64;
        let b = w.stats.alive[1] as f64;
        assert!(a + b > 0.0, "양군이 모두 사라졌다");
        diffs.push((a - b) / (a + b));
    }
    let n = diffs.len() as f64;
    let mean: f64 = diffs.iter().sum::<f64>() / n;
    let sd = (diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / n).sqrt();
    let z = mean / (sd / n.sqrt()).max(1e-9);
    assert!(
        z.abs() < 3.0,
        "거울 대칭 전투가 한쪽으로 기운다: 평균 {:+.1}%, z={:+.2} (시드별 {:?})",
        mean * 100.0,
        z,
        diffs
            .iter()
            .map(|d| (d * 100.0).round())
            .collect::<Vec<_>>()
    );
}
