//! 결정론 검증 — 같은 씨앗이면 언제나 같은 전투가 나와야 한다.
//!
//! 이게 깨지면 리플레이도, 설정 공유도, 상성 회귀 테스트도 전부 무의미해진다.
//! 특히 스레드 수에 따라 결과가 달라지는 것은 병렬 피해 적용에 경합이 있다는
//! 뜻이므로, 그 경우를 명시적으로 잡는다.

use orc_war::scenario::Scenario;
use orc_war::sim::unit_types::INF_SWORD;

fn run_hash(units: u32, ticks: u64, seed: u64) -> (u64, [u32; 2]) {
    let sc = Scenario::head_on(units, INF_SWORD, seed, ticks);
    let mut w = sc.build();
    for _ in 0..ticks {
        w.step();
    }
    (w.state_hash(), w.stats.dead)
}

#[test]
fn same_seed_same_battle() {
    let a = run_hash(4_000, 1200, 42);
    let b = run_hash(4_000, 1200, 42);
    assert_eq!(a.0, b.0, "같은 씨앗인데 상태 해시가 다르다");
    assert_eq!(a.1, b.1, "같은 씨앗인데 전사자 수가 다르다");
    assert!(
        a.1[0] + a.1[1] > 0,
        "1200틱 동안 아무도 죽지 않아 검증이 무의미하다"
    );
}

#[test]
fn different_seed_different_battle() {
    let a = run_hash(4_000, 1200, 1);
    let b = run_hash(4_000, 1200, 2);
    assert_ne!(a.0, b.0, "씨앗이 다른데 전투가 완전히 동일하다");
}

#[test]
fn thread_count_does_not_change_outcome() {
    // 병렬 피해 적용이 청크 순서대로 병합되는지 확인한다.
    let single = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(|| run_hash(4_000, 1200, 7));
    let many = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap()
        .install(|| run_hash(4_000, 1200, 7));
    assert_eq!(
        single.0, many.0,
        "스레드 수에 따라 결과가 달라진다 — 피해 적용에 경합이 있다"
    );
    assert_eq!(single.1, many.1);
}

#[test]
fn hash_tracks_progress() {
    // 해시가 실제로 상태를 반영하는지 (상수를 반환하는 게 아닌지)
    let sc = Scenario::head_on(2_000, INF_SWORD, 3, 200);
    let mut w = sc.build();
    let h0 = w.state_hash();
    for _ in 0..50 {
        w.step();
    }
    assert_ne!(h0, w.state_hash(), "50틱을 돌렸는데 상태 해시가 그대로다");
}
