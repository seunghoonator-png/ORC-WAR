//! 거울 대칭 검사 — 동일 조건 양군의 승패가 한쪽으로 쏠리는지 통계로 본다.

use orc_war::scenario::Scenario;
use orc_war::sim::unit_types::INF_SWORD;

fn main() {
    let mut a = std::env::args().skip(1);
    let units: u32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(3_000);
    let ticks: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(1_500);
    let seeds: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(24);
    // 남북 배치를 맞바꾼다. 편향이 팀 번호를 따라가면 처리 순서 문제,
    // 위치를 따라가면 진영 배치 쪽 문제다.
    let mode = a.next().unwrap_or_default();
    let flip = mode == "flip";
    let x_axis = mode == "x";

    let mut diffs = Vec::new();
    let mut wins = [0u32; 2];
    for seed in 1..=seeds {
        let mut sc = if x_axis {
            Scenario::head_on_x(units, INF_SWORD, seed, ticks)
        } else {
            Scenario::head_on(units, INF_SWORD, seed, ticks)
        };
        if flip {
            let (c0, f0) = (sc.formations[0].center, sc.formations[0].front);
            sc.formations[0].center = sc.formations[1].center;
            sc.formations[0].front = sc.formations[1].front;
            sc.formations[1].center = c0;
            sc.formations[1].front = f0;
        }
        let mut w = sc.build();
        for _ in 0..ticks {
            w.step();
        }
        let (x, y) = (w.stats.alive[0] as f64, w.stats.alive[1] as f64);
        let d = (x - y) / (x + y).max(1.0);
        if x >= y {
            wins[0] += 1;
        } else {
            wins[1] += 1;
        }
        diffs.push(d);
        print!("{:+.0} ", d * 100.0);
    }
    let n = diffs.len() as f64;
    let mean = diffs.iter().sum::<f64>() / n;
    let var = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / n;
    let sd = var.sqrt();
    // 평균의 표준오차 대비 몇 배나 벗어났는가 (2를 넘으면 편향 의심)
    let se = sd / n.sqrt();
    println!(
        "\n\n{units} 유닛 x {seeds} 시드 x {ticks} 틱\n\
         우세  공격측 {} / 방어측 {}\n\
         평균 편차 {:+.1}%   표준편차 {:.1}%p   표준오차 {:.1}%p   z = {:+.2}",
        wins[0],
        wins[1],
        mean * 100.0,
        sd * 100.0,
        se * 100.0,
        mean / se.max(1e-9)
    );
}
