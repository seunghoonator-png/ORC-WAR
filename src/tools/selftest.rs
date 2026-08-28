//! 자체 점검 — 받은 사람이 "제대로 도는가"를 30초 안에 확인하는 수단.
//!
//! 개발 기계에서는 `cargo test` 가 있지만, 실행파일만 받은 쪽에는 없다.
//! 창이 안 떠도, 성능이 안 나와도, **무엇이 어긋났는지**는 알 수 있어야 한다.

use std::time::Instant;

use crate::config::{BattleConfig, Battlefield, Doctrine};
use crate::sim::{Outcome, SIM_HZ};

fn check(name: &str, ok: bool, detail: String) -> bool {
    println!("  [{}] {name}  {detail}", if ok { "통과" } else { "실패" });
    ok
}

pub fn run(_argv: &[String]) -> bool {
    println!("ORC-WAR 자체 점검");
    println!("스레드 {}개\n", rayon::current_num_threads());
    let mut all = true;

    // 1. 같은 씨앗이면 같은 전투여야 한다
    let cfg = BattleConfig {
        field: Battlefield::Plains,
        total: 8_000,
        doctrine: Doctrine::Balanced,
        seed: 5,
    };
    let sc = cfg.scenario();
    let hash = |ticks: u64| {
        let mut w = sc.build();
        for _ in 0..ticks {
            w.step();
        }
        w.state_hash()
    };
    let (a, b) = (hash(400), hash(400));
    all &= check("결정론", a == b, format!("{a:016x} / {b:016x}"));

    // 2. 전투가 결판까지 간다
    let mut w = sc.build();
    let outcome = loop {
        w.step();
        match w.outcome(sc.max_ticks) {
            Outcome::Ongoing => {}
            done => break done,
        }
    };
    all &= check(
        "전투 완주",
        !matches!(outcome, Outcome::Ongoing),
        format!("{}틱에 결판 ({:?})", w.tick, outcome),
    );

    // 3. 공성전이 성립한다 — 성벽이 실제로 부서지는가
    let siege = BattleConfig {
        field: Battlefield::Siege,
        total: 30_000,
        ..cfg
    }
    .scenario();
    let mut w = siege.build();
    for _ in 0..4_000 {
        w.step();
        if !matches!(w.outcome(siege.max_ticks), Outcome::Ongoing) {
            break;
        }
    }
    let broken = w
        .castle
        .as_ref()
        .map(|c| c.segments.iter().filter(|s| s.breached).count())
        .unwrap_or(0);
    all &= check("공성", broken > 0, format!("{broken}구간 붕괴"));

    // 4. 이 기계가 30만을 실시간으로 감당하는가
    let big = BattleConfig {
        field: Battlefield::Plains,
        total: 300_000,
        doctrine: Doctrine::Balanced,
        seed: 1,
    }
    .scenario();
    let mut w = big.build();
    let t0 = Instant::now();
    const N: u64 = 60;
    for _ in 0..N {
        w.step();
    }
    let ms = t0.elapsed().as_secs_f64() * 1e3 / N as f64;
    let budget = 1000.0 / SIM_HZ as f64;
    all &= check(
        "30만 실시간",
        ms <= budget,
        format!("{ms:.1} ms/틱  (예산 {budget:.0})"),
    );

    println!(
        "\n{}",
        if all {
            "모두 통과했습니다."
        } else {
            "어긋난 항목이 있습니다. 위 줄을 그대로 알려주세요."
        }
    );
    all
}
