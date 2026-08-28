//! 병종 상성 확인 — 두 병종을 붙여놓고 결과를 본다.
//!
//! `orc-war --matchup <병종A> <수A> <병종B> <수B> [간격] [씨앗] [지형]`

use crate::scenario::Scenario;
use crate::sim::unit_types::{stats, UNIT_STATS};
use crate::sim::Outcome;

pub fn run(argv: &[String]) {
    let mut a = argv.iter().cloned();
    let ta: u8 = a.next().and_then(|s| s.parse().ok()).unwrap_or(6);
    let na: u32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(500);
    let tb: u8 = a.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let nb: u32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(2000);
    let gap: f32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let seed: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(1);

    if ta as usize >= UNIT_STATS.len() || tb as usize >= UNIT_STATS.len() {
        eprintln!("병종 번호는 0..{} 입니다", UNIT_STATS.len() - 1);
        for (i, u) in UNIT_STATS.iter().enumerate() {
            eprintln!("  {i}  {}", u.name);
        }
        return;
    }

    let max_ticks = 6_000;
    use crate::map::gen::{MapKind, MapOptions};
    let map = argv.get(6).cloned().unwrap_or_else(|| "plains".into());
    let opts = match map.as_str() {
        "hills" => MapOptions {
            kind: MapKind::Hills,
            ..Default::default()
        },
        "mountain" => MapOptions {
            kind: MapKind::Mountain,
            ..Default::default()
        },
        "river" => MapOptions {
            kind: MapKind::Plains,
            river: true,
            ..Default::default()
        },
        "forest" => MapOptions {
            kind: MapKind::Plains,
            forest: true,
            rocks: true,
            ..Default::default()
        },
        _ => MapOptions::default(),
    };
    let sc = Scenario::matchup((ta, na), (tb, nb), seed, max_ticks, gap).on_map(opts);
    let mut w = sc.build();
    let outcome = loop {
        w.step();
        match w.outcome(max_ticks) {
            Outcome::Ongoing => {}
            done => break done,
        }
    };

    let verdict = match outcome {
        Outcome::Victory(0) => stats(ta).name,
        Outcome::Victory(_) => stats(tb).name,
        _ => "결판 안 남",
    };
    println!(
        "{:>12} {:>6} vs {:>12} {:>6}  |  {:>4}틱  승자 {:>12}  |  잔존 {:>5} / {:>5}  전사 {:>5} / {:>5}  이탈 {:>5} / {:>5}",
        stats(ta).name,
        na,
        stats(tb).name,
        nb,
        w.tick,
        verdict,
        w.stats.alive[0],
        w.stats.alive[1],
        w.stats.dead[0],
        w.stats.dead[1],
        w.stats.fled[0],
        w.stats.fled[1],
    );
}
