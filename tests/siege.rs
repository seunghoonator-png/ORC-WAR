//! 공성 회귀 — 성이 실제로 성 노릇을 하는가.

use orc_war::map::castle::{Castle, Side};
use orc_war::map::{Terrain, TerrainMap};
use orc_war::scenario::Scenario;
use orc_war::sim::{Outcome, WORLD_SIZE};

struct Siege {
    outcome: Outcome,
    dead: [u32; 2],
    fled: [u32; 2],
    ticks: u64,
    breached: usize,
    gate_down: bool,
    drops: u64,
}

fn run(attackers: u32, defenders: u32, seed: u64, moat: bool, max: u64) -> Siege {
    let sc = Scenario::siege(attackers, defenders, seed, max, moat);
    let mut w = sc.build();
    let outcome = loop {
        w.step();
        match w.outcome(max) {
            Outcome::Ongoing => {}
            done => break done,
        }
    };
    let c = w.castle.as_ref().unwrap();
    Siege {
        outcome,
        dead: w.stats.dead,
        fled: w.stats.fled,
        ticks: w.tick,
        breached: c.segments.iter().filter(|s| s.breached).count(),
        gate_down: c.segments.iter().any(|s| s.is_gate && s.breached),
        drops: w.stats.drops_landed,
    }
}

#[test]
fn a_siege_reaches_a_decision() {
    let r = run(8_000, 3_000, 1, true, 9_000);
    assert!(
        !matches!(r.outcome, Outcome::Timeout),
        "공성이 결판나지 않았다 ({}틱, 무너진 구간 {})",
        r.ticks,
        r.breached
    );
    assert!(
        r.gate_down || r.breached > 0,
        "성벽도 성문도 뚫지 못한 채 끝났다"
    );
}

#[test]
fn walls_make_the_attacker_pay() {
    // 같은 병력이라도 성에 틀어박힌 쪽을 뜯어내는 것은 야전과 다른 일이다.
    let siege = run(8_000, 3_000, 1, true, 9_000);
    let attacker_loss = siege.dead[0] + siege.fled[0];
    assert!(
        attacker_loss > siege.dead[1],
        "성을 치는 쪽이 지키는 쪽보다 덜 잃었다 — 성벽이 값을 못 하고 있다 \
         (공격 {} 손실, 수비 {} 전사)",
        attacker_loss,
        siege.dead[1]
    );
}

#[test]
fn a_bigger_garrison_costs_more_to_dig_out() {
    let thin = run(8_000, 2_500, 3, true, 9_000);
    let thick = run(8_000, 5_000, 3, true, 9_000);
    let a = thin.dead[0] + thin.fled[0];
    let b = thick.dead[0] + thick.fled[0];
    assert!(
        b > a,
        "수비를 두 배로 늘려도 공격측 손실이 늘지 않는다 ({a} → {b})"
    );
}

#[test]
fn defenders_pour_things_from_the_walls() {
    let r = run(6_000, 2_500, 2, true, 6_000);
    assert!(
        r.drops > 500,
        "성벽 위에서 아무것도 떨어지지 않았다 ({}회)",
        r.drops
    );
}

#[test]
fn a_moat_only_opens_where_the_wall_falls() {
    // 해자는 성을 두르지만, 무너진 성벽의 돌더미가 그 앞을 메운다.
    // 이게 없으면 성벽을 부숴도 성문 하나만 두들기는 전투가 된다.
    let mut m = TerrainMap::flat(WORLD_SIZE);
    let mid = WORLD_SIZE * 0.5;
    let castle = Castle::square([mid, mid], [200.0, 150.0], true);
    castle.stamp(&mut m);

    // 서쪽 성벽 한 구간을 고른다 (x 좌표로 고르면 남쪽 구간이 먼저 걸린다)
    let idx = castle
        .segments
        .iter()
        .position(|s| s.side == Side::West)
        .expect("서쪽 성벽 구간이 없다");
    let seg_center = castle.segments[idx].center;
    let outside = [seg_center[0] - 30.0, seg_center[1]];
    assert_eq!(
        m.at(outside),
        Terrain::Moat,
        "성벽 바깥이 해자가 아니다 — 해자가 성을 두르지 못하고 있다"
    );

    // 그 구간을 무너뜨린다
    let mut castle = castle;
    castle.segments[idx].breached = true;
    castle.restamp_segment(&mut m, idx);
    assert_ne!(
        m.at(outside),
        Terrain::Moat,
        "성벽이 무너졌는데 해자가 그대로다 — 돌더미가 메우지 않는다"
    );
    assert!(m.at(seg_center).passable(), "무너진 구간을 지날 수 없다");
}

#[test]
fn an_intact_castle_is_sealed() {
    let mut m = TerrainMap::flat(WORLD_SIZE);
    let mid = WORLD_SIZE * 0.5;
    let castle = Castle::square([mid, mid], [200.0, 150.0], true);
    castle.stamp(&mut m);
    // 해자를 건널 수 있는 곳은 성문 앞 다리뿐이어야 한다.
    // (성문 자체는 닫혀 있으니 성벽 선까지 보면 어디도 뚫려 있지 않다)
    let mut open_columns = 0;
    for k in 0..120 {
        let x = mid - 200.0 + k as f32 * (400.0 / 120.0);
        let mut blocked = false;
        let mut y = mid - 200.0;
        while y < mid - 158.0 {
            if !m.at([x, y]).passable() {
                blocked = true;
                break;
            }
            y += 3.0;
        }
        if !blocked {
            open_columns += 1;
        }
    }
    assert!(
        open_columns > 0,
        "성문 앞 다리조차 막혀 있다 — 공격군이 성에 닿을 수 없다"
    );
    assert!(
        open_columns < 25,
        "해자를 아무 데서나 건널 수 있다 (열린 표본 {open_columns}/120)"
    );
    // 열린 곳은 성문 앞이어야 한다
    let gate = castle.gate_center();
    let mut open_far_from_gate = 0;
    for k in 0..120 {
        let x = mid - 200.0 + k as f32 * (400.0 / 120.0);
        if (x - gate[0]).abs() < 40.0 {
            continue;
        }
        let mut blocked = false;
        let mut y = mid - 200.0;
        while y < mid - 158.0 {
            if !m.at([x, y]).passable() {
                blocked = true;
                break;
            }
            y += 3.0;
        }
        if !blocked {
            open_far_from_gate += 1;
        }
    }
    assert_eq!(
        open_far_from_gate, 0,
        "성문에서 먼 곳에도 해자를 건너는 길이 있다"
    );
}

/// 돌벽을 무너뜨리는 것은 공성병기여야 한다.
///
/// 예전에는 그렇지 않았다. 보병 하나가 벽에 주는 피해가 1이어도 수천 명이
/// 곱해지고, 게다가 사수까지 사거리 안의 성벽을 헐고 있었다. 그래서 파성추와
/// 투석기를 통째로 빼도 성이 같은 시각에 똑같이 무너졌다 — 공성병기 전체가
/// 장식이었다는 뜻이다. 눈으로는 "성이 잘 함락된다"로만 보인다.
#[test]
fn only_engines_bring_down_stone() {
    use orc_war::sim::unit_types::{CATAPULT, LADDER, RAM};

    // 공성병기를 뺀 편성으로 같은 공성을 돌린다
    let run_without = |strip: &[u8]| -> (usize, bool, u64) {
        let mut sc = Scenario::siege(24_000, 6_000, 1, 12_000, true);
        sc.formations.retain(|f| !strip.contains(&f.type_id));
        let mut w = sc.build();
        loop {
            w.step();
            if !matches!(w.outcome(sc.max_ticks), Outcome::Ongoing) {
                break;
            }
        }
        let c = w.castle.as_ref().unwrap();
        (
            c.segments
                .iter()
                .filter(|s| s.breached && !s.is_gate)
                .count(),
            c.segments.iter().any(|s| s.is_gate && s.breached),
            w.stats.wall_breaches_climbed,
        )
    };

    let (walls_bare, gate_bare, climbed_bare) = run_without(&[RAM, CATAPULT, LADDER]);
    assert_eq!(
        walls_bare, 0,
        "공성병기 없이 돌벽 {walls_bare}구간이 무너졌다 — 병기가 장식이 된다"
    );
    // 성문은 나무다. 도끼와 손으로도 결국 열린다
    assert!(gate_bare, "맨몸으로는 성문조차 열지 못한다");
    // 벽이 서 있으면 누군가는 기어올라야 한다
    assert!(
        climbed_bare > 0,
        "돌벽이 멀쩡한데 성벽을 넘어간 사람이 하나도 없다 — 등반 경로가 죽어 있다"
    );

    let (walls_full, _, _) = run_without(&[]);
    assert!(
        walls_full > 0,
        "공성병기를 다 주었는데도 돌벽이 한 구간도 무너지지 않았다"
    );
}
