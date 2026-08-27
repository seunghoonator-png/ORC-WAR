//! 지형 회귀 — 땅이 전투를 바꾸는가, 그리고 병력이 지형에 갇히지는 않는가.

use orc_war::map::gen::{MapKind, MapOptions};
use orc_war::map::Terrain;
use orc_war::scenario::Scenario;
use orc_war::sim::unit_types::{CAV_HEAVY, INF_SWORD};
use orc_war::sim::{Outcome, WORLD_SIZE};

fn run(opts: MapOptions, a: (u8, u32), b: (u8, u32), seed: u64) -> (u32, u32, u64, Outcome) {
    const MAX: u64 = 5_000;
    let sc = Scenario::matchup(a, b, seed, MAX, 200.0).on_map(opts);
    let mut w = sc.build();
    let outcome = loop {
        w.step();
        match w.outcome(MAX) {
            Outcome::Ongoing => {}
            done => break done,
        }
    };
    (w.stats.dead[0], w.stats.dead[1], w.tick, outcome)
}

fn every_map() -> Vec<(&'static str, MapOptions)> {
    vec![
        ("평지", MapOptions::default()),
        (
            "언덕",
            MapOptions {
                kind: MapKind::Hills,
                ..Default::default()
            },
        ),
        (
            "산악",
            MapOptions {
                kind: MapKind::Mountain,
                ..Default::default()
            },
        ),
        (
            "강",
            MapOptions {
                river: true,
                ..Default::default()
            },
        ),
        (
            "숲",
            MapOptions {
                forest: true,
                rocks: true,
                ..Default::default()
            },
        ),
    ]
}

#[test]
fn armies_never_get_stranded() {
    // 지형이 길을 막아 버리면 양군이 서로를 찾지 못한 채 제한 틱까지 서 있게 된다.
    // 어떤 지형에서도 교전은 벌어져야 한다.
    for (name, opts) in every_map() {
        let (d0, d1, ticks, _) = run(opts, (INF_SWORD, 1_500), (INF_SWORD, 1_500), 1);
        assert!(
            d0 + d1 > 200,
            "{name} 에서 전투가 벌어지지 않았다 — 병력이 지형에 갇힌 것으로 보인다 \
             (전사 {d0}/{d1}, {ticks}틱)"
        );
    }
}

#[test]
fn deployment_zones_are_clear() {
    // 개전하자마자 절벽 한복판에 서 있으면 곤란하다.
    for (name, opts) in every_map() {
        let t = orc_war::map::gen::generate(WORLD_SIZE, opts, 7);
        let mid = WORLD_SIZE * 0.5;
        for off in [-250.0f32, -200.0, 200.0, 250.0] {
            for k in 0..40 {
                let x = WORLD_SIZE * 0.5 + (k as f32 - 20.0) * 20.0;
                let p = [x, mid + off];
                assert!(
                    t.at(p).passable(),
                    "{name}: 배치 구역 {p:?} 이 통행 불가 지형이다"
                );
            }
        }
    }
}

#[test]
fn narrow_ground_costs_fewer_lives() {
    // 협곡에서는 한 번에 맞붙을 수 있는 인원이 적다. 같은 병력이라도
    // 개활지보다 훨씬 오래, 훨씬 덜 죽으며 싸운다.
    let plains = run(
        MapOptions::default(),
        (INF_SWORD, 2_000),
        (INF_SWORD, 2_000),
        1,
    );
    let mountain = run(
        MapOptions {
            kind: MapKind::Mountain,
            ..Default::default()
        },
        (INF_SWORD, 2_000),
        (INF_SWORD, 2_000),
        1,
    );
    let plains_dead = plains.0 + plains.1;
    let mountain_dead = mountain.0 + mountain.1;
    assert!(
        mountain_dead * 2 < plains_dead,
        "산악의 사상자가 개활지와 비슷하다 — 지형이 접촉면을 좁히지 못하고 있다 \
         (평지 {plains_dead}, 산악 {mountain_dead})"
    );
}

#[test]
fn rough_ground_is_actually_rough() {
    // 지형표가 규칙과 어긋나면 아래 통합 검증들이 전부 헛돈다.
    assert!(
        Terrain::Plain.allows_charge(),
        "개활지에서 돌격이 안 걸린다"
    );
    assert!(
        !Terrain::Forest.allows_charge(),
        "나무 사이에서 말이 속도를 낸다"
    );
    assert!(!Terrain::Marsh.allows_charge(), "진창에서 말이 속도를 낸다");
    assert!(Terrain::Forest.speed_mult() < Terrain::Plain.speed_mult());
    assert!(
        Terrain::Forest.arrow_block() > 0.0,
        "숲이 화살을 전혀 걸러내지 않는다"
    );
    assert!(!Terrain::Water.passable(), "깊은 물을 걸어서 건넌다");
    assert!(Terrain::Ford.passable(), "여울조차 건널 수 없다");
    assert!(Terrain::Ford.speed_mult() < Terrain::Plain.speed_mult());
}

#[test]
fn woods_drag_a_battle_out() {
    // 나무 사이에서는 대열이 풀리고 걸음이 무거워진다. 같은 병력이라면
    // 개활지보다 오래 걸리고 덜 죽어야 한다.
    let open = run(MapOptions::default(), (CAV_HEAVY, 800), (INF_SWORD, 800), 2);
    let woods = run(
        MapOptions {
            forest: true,
            ..Default::default()
        },
        (CAV_HEAVY, 800),
        (INF_SWORD, 800),
        2,
    );
    assert!(
        woods.2 > open.2,
        "숲에서 전투가 더 빨리 끝난다: 개활지 {}틱, 숲 {}틱",
        open.2,
        woods.2
    );
}

#[test]
fn rivers_are_crossable_somewhere() {
    // 물길이 전장을 완전히 갈라 버리면 전투 자체가 성립하지 않는다.
    let t = orc_war::map::gen::generate(
        WORLD_SIZE,
        MapOptions {
            river: true,
            ..Default::default()
        },
        11,
    );
    let mid = WORLD_SIZE * 0.5;
    let mut fords = 0;
    for k in 0..300 {
        let x = k as f32 / 300.0 * WORLD_SIZE;
        // 물길이 지나는 띠를 훑어 건널목을 찾는다
        let mut crossable = true;
        let mut y = mid - 200.0;
        while y < mid + 200.0 {
            if t.at([x, y]) == Terrain::Water {
                crossable = false;
                break;
            }
            y += 4.0;
        }
        if crossable {
            fords += 1;
        }
    }
    assert!(
        fords > 10,
        "강을 건널 곳이 사실상 없다 (열린 x 표본 {fords}/300)"
    );
}
