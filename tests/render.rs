//! 렌더 회귀 — 배경 캐시가 제때 다시 그려지고, 제때 재사용되는가.
//!
//! 창이 없어도 검사할 수 있다. 그리는 대상이 순수한 메모리 버퍼이기 때문이다.

use orc_war::render::{draw_units, fit_camera, Decals, Frame, GroundCache};
use orc_war::scenario::Scenario;
use orc_war::sim::unit_types::INF_SWORD;
use orc_war::sim::WORLD_SIZE;

fn world_after(ticks: u64) -> (orc_war::sim::World, Decals) {
    let sc = Scenario::head_on(4_000, INF_SWORD, 1, 20_000);
    let mut w = sc.build();
    let mut d = Decals::new(WORLD_SIZE, 4.0);
    for _ in 0..ticks {
        w.step();
        d.absorb(&w);
    }
    (w, d)
}

#[test]
fn cache_redraws_only_when_the_view_moves() {
    let (world, mut decals) = world_after(1_200);
    let mut frame = Frame::new(640, 360);
    let cam = fit_camera(&world, 640, 360);
    let mut cache = GroundCache::new();

    assert!(
        cache.blit(&mut frame, &world, &decals, &cam),
        "첫 장은 새로 그려야 한다"
    );
    decals.clear_dirty();
    assert!(
        !cache.blit(&mut frame, &world, &decals, &cam),
        "카메라가 그대로인데 배경을 다시 그린다 — 캐시가 듣지 않는다"
    );

    let mut moved = cam;
    moved.center[0] += 40.0;
    assert!(
        cache.blit(&mut frame, &world, &decals, &moved),
        "카메라를 옮겼는데 예전 배경을 그대로 쓴다"
    );

    let mut zoomed = moved;
    zoomed.mpp *= 0.5;
    assert!(
        cache.blit(&mut frame, &world, &decals, &zoomed),
        "확대했는데 예전 배경을 그대로 쓴다"
    );

    decals.clear_dirty();
    cache.blit(&mut frame, &world, &decals, &zoomed);
    cache.invalidate();
    assert!(
        cache.blit(&mut frame, &world, &decals, &zoomed),
        "무효화했는데도 캐시를 쓴다 — 성벽이 무너져도 옛 지형이 남는다"
    );
}

#[test]
fn patched_cache_matches_a_full_redraw() {
    // 시체가 쌓인 자리만 덧칠한 결과가, 통째로 다시 그린 것과 같아야 한다.
    // 다르면 전장에 얼룩이 남거나 지워진 자국이 보인다.
    let (mut world, mut decals) = world_after(1_200);
    let cam = fit_camera(&world, 640, 360);

    let mut cached = Frame::new(640, 360);
    let mut cache = GroundCache::new();
    cache.blit(&mut cached, &world, &decals, &cam);
    decals.clear_dirty();

    // 전투를 더 진행시켜 시체를 쌓는다
    for _ in 0..40 {
        world.step();
        decals.absorb(&world);
        cache.blit(&mut cached, &world, &decals, &cam);
        decals.clear_dirty();
    }

    let mut fresh = Frame::new(640, 360);
    let mut clean = GroundCache::new();
    clean.blit(&mut fresh, &world, &decals, &cam);

    let diff = cached
        .px
        .iter()
        .zip(fresh.px.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(diff, 0, "덧칠한 배경이 통째로 그린 것과 {diff} 화소 다르다");
}

#[test]
fn units_land_inside_the_frame() {
    let (world, decals) = world_after(600);
    let mut frame = Frame::new(320, 180);
    let cam = fit_camera(&world, 320, 180);
    let mut cache = GroundCache::new();
    cache.blit(&mut frame, &world, &decals, &cam);
    let before = frame.px.clone();
    draw_units(&mut frame, &world, &cam);
    assert!(
        frame.px.iter().zip(before.iter()).any(|(a, b)| a != b),
        "병력을 그렸는데 화면이 그대로다"
    );
}
