//! 대칭성 진단 — 동일 조건 양군에서 어느 쪽이, 언제부터, 왜 밀리는지 계측한다.

use orc_war::scenario::Scenario;
use orc_war::sim::pool::{UnitState, NO_TARGET};
use orc_war::sim::unit_types::INF_SWORD;

fn main() {
    let units: u32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(5_000);
    let seed: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(1);

    let sc = Scenario::head_on(units, INF_SWORD, seed, 20_000);
    let mut w = sc.build();

    println!(
        "{:>6} {:>7} {:>7} {:>8} {:>8} {:>7} {:>7} {:>8} {:>8}",
        "tick", "alive0", "alive1", "front0", "front1", "fight0", "fight1", "cgy0", "cgy1"
    );

    for t in 0..20_000u64 {
        w.step();
        if t % 250 != 0 {
            continue;
        }
        let mut alive = [0u32; 2];
        let mut fighting = [0u32; 2];
        let mut cg = [0.0f64; 2];
        let mut front0 = f32::MIN;
        let mut front1 = f32::MAX;

        for i in 0..w.pool.len() {
            if w.pool.state[i] == UnitState::Dead {
                continue;
            }
            let team = w.pool.team[i] as usize;
            alive[team] += 1;
            cg[team] += w.pool.pos[i][1] as f64;
            if w.pool.target[i] != NO_TARGET {
                fighting[team] += 1;
            }
            if team == 0 {
                front0 = front0.max(w.pool.pos[i][1]);
            } else {
                front1 = front1.min(w.pool.pos[i][1]);
            }
        }
        if alive[0] == 0 || alive[1] == 0 {
            println!("--- t={t} 종료 (alive {} / {}) ---", alive[0], alive[1]);
            break;
        }
        println!(
            "{:>6} {:>7} {:>7} {:>8.1} {:>8.1} {:>7} {:>7} {:>8.1} {:>8.1}",
            t, alive[0], alive[1], front0, front1, fighting[0], fighting[1],
            cg[0] / alive[0] as f64, cg[1] / alive[1] as f64
        );
    }
}
