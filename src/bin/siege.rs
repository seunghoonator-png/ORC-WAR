//! 공성전 실행 — 성을 두고 벌어지는 전투를 끝까지 돌린다.

use std::time::Instant;

use orc_war::scenario::Scenario;
use orc_war::sim::{Outcome, SIM_HZ};

fn main() {
    let mut a = std::env::args().skip(1);
    let attackers: u32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(60_000);
    let defenders: u32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(20_000);
    let seed: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let moat = a.next().map(|s| s != "nomoat").unwrap_or(true);
    let max_ticks: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(12_000);

    let sc = Scenario::siege(attackers, defenders, seed, max_ticks, moat);
    let mut w = sc.build();
    let walls = w.castle.as_ref().unwrap().segments.len();
    println!(
        "공성전  공격 {} vs 수비 {}   성벽 {}구간  해자 {}",
        attackers,
        defenders,
        walls,
        if moat { "있음" } else { "없음" }
    );

    let t0 = Instant::now();
    let mut acc = orc_war::sim::PhaseTimes::default();
    let outcome = loop {
        w.step();
        let p = w.phase;
        acc.movement += p.movement;
        acc.grid += p.grid;
        acc.combat += p.combat;
        acc.shooting += p.shooting;
        acc.siege += p.siege;
        acc.morale += p.morale;
        acc.flow += p.flow;
        if w.tick.is_multiple_of(500) {
            let c = w.castle.as_ref().unwrap();
            let breached = c.segments.iter().filter(|s| s.breached).count();
            let gate = c
                .segments
                .iter()
                .find(|s| s.is_gate)
                .map(|s| (s.hp.max(0.0) / 2500.0 * 100.0) as i32)
                .unwrap_or(0);
            println!(
                "  t={:>5} ({:>4.0}s)  병력 {:>6} / {:>6}   무너진 성벽 {:>2}  성문 {:>3}%  \
                 성벽 넘음 {:>5}  중앙 {:>4}",
                w.tick,
                w.tick as f64 / SIM_HZ as f64,
                w.stats.alive[0],
                w.stats.alive[1],
                breached,
                gate,
                w.stats.wall_breaches_climbed,
                w.stats.objective_holders,
            );
        }
        match w.outcome(max_ticks) {
            Outcome::Ongoing => {}
            done => break done,
        }
    };

    println!("\n=== 결과 ===");
    match outcome {
        Outcome::Victory(0) => println!("성 함락 ({}틱 / {:.0}초)", w.tick, w.tick as f64 / 20.0),
        Outcome::Victory(_) => println!("수비 성공 — 공격군 붕괴"),
        Outcome::Timeout => println!("공성 실패 — 시간 초과"),
        Outcome::Ongoing => unreachable!(),
    }
    let c = w.castle.as_ref().unwrap();
    println!(
        "성벽  {}/{} 구간 붕괴   성문 {}",
        c.segments.iter().filter(|s| s.breached).count(),
        c.segments.len(),
        if c.segments.iter().any(|s| s.is_gate && s.breached) {
            "파괴됨"
        } else {
            "버팀"
        }
    );
    println!(
        "전사  공격 {:>7}  수비 {:>7}",
        w.stats.dead[0], w.stats.dead[1]
    );
    println!(
        "이탈  공격 {:>7}  수비 {:>7}",
        w.stats.fled[0], w.stats.fled[1]
    );
    println!(
        "성벽을 넘어간 인원 {}   위에서 들이부은 타격 {}",
        w.stats.wall_breaches_climbed, w.stats.drops_landed
    );
    println!("소요 {:.1}초", t0.elapsed().as_secs_f64());
    let t = w.tick as f64;
    println!(
        "평균/틱  이동 {:.2}  격자 {:.2}  전투 {:.2}  사격 {:.2}  공성 {:.2}  사기 {:.2}  경로 {:.2}  합 {:.2} ms",
        acc.movement / t,
        acc.grid / t,
        acc.combat / t,
        acc.shooting / t,
        acc.siege / t,
        acc.morale / t,
        acc.flow / t,
        acc.total() / t
    );
    println!("실측 {:.2} ms/틱", t0.elapsed().as_secs_f64() * 1e3 / t);
}
