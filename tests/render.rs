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

/// 띠로 갈라 그린 병력이, 배열을 처음부터 끝까지 훑어 그린 것과 같아야 한다.
///
/// 화면을 가로 띠로 나눠 코어에 던지는 구조라 띠 경계가 늘 위험하다. 경계에
/// 걸친 유닛을 한 띠도 맡지 않으면 전선에 가로줄이 그어지고, 눈으로는 좀처럼
/// 잡히지 않는다. 느리지만 단순한 기준 구현과 화소 단위로 맞춰 본다.
#[test]
fn banded_unit_pass_matches_a_plain_scan() {
    use orc_war::render::{rgb, Camera};
    use orc_war::sim::pool::UnitState;
    use orc_war::sim::unit_types::{is_engine, stats};

    /// 옛 방식: 유닛 배열을 순서대로 훑으며 한 점씩 찍는다.
    fn plain(frame: &mut Frame, world: &orc_war::sim::World, cam: &Camera) {
        let (w, h) = (frame.w, frame.h);
        let mpp = cam.mpp;
        let size = if mpp > 1.2 {
            1
        } else if mpp > 0.55 {
            2
        } else if mpp > 0.28 {
            3
        } else {
            4
        };
        let bounds = cam.view_bounds(w, h);
        let pool = &world.pool;
        for i in 0..pool.len() {
            let p = pool.pos[i];
            if p[0] < bounds[0] || p[0] > bounds[2] || p[1] < bounds[1] || p[1] > bounds[3] {
                continue;
            }
            let st = pool.state[i];
            if matches!(st, UnitState::Dead | UnitState::Fled) {
                continue;
            }
            let (sx, sy) = cam.to_screen(p, w, h);
            let ty = pool.type_id[i];
            let s = stats(ty);
            let team = pool.team[i];
            let c = if is_engine(ty) {
                rgb(196, 168, 96)
            } else if st == UnitState::Rout {
                if team == 0 {
                    rgb(120, 62, 58)
                } else {
                    rgb(58, 78, 120)
                }
            } else if s.is_cavalry {
                if team == 0 {
                    rgb(255, 150, 90)
                } else {
                    rgb(120, 210, 255)
                }
            } else if s.range > 0.0 {
                if team == 0 {
                    rgb(226, 96, 74)
                } else {
                    rgb(96, 150, 236)
                }
            } else if team == 0 {
                rgb(214, 52, 44)
            } else {
                rgb(58, 106, 214)
            };
            let es = if is_engine(ty) { size + 2 } else { size };
            frame.blot(sx as i32, sy as i32, es, c);
        }
        let tick = world.tick;
        let mut shots: Vec<[f32; 2]> = Vec::new();
        world
            .projectiles
            .for_each_in_flight(tick, |p, _, _| shots.push(p));
        for p in shots {
            if p[0] < bounds[0] || p[0] > bounds[2] || p[1] < bounds[1] || p[1] > bounds[3] {
                continue;
            }
            let (sx, sy) = cam.to_screen(p, w, h);
            frame.put(sx as i32, sy as i32, rgb(236, 230, 196));
        }
    }

    let (world, _) = world_after(900);
    let (w, h) = (480, 270);
    let fit = fit_camera(&world, w, h);

    // 확대 배율마다 점 크기가 달라지고, 그만큼 띠 경계를 넘는 폭도 달라진다.
    // 네 단계를 모두 밟아 본다
    for mpp in [fit.mpp, 1.0, 0.4, 0.2] {
        let cam = Camera {
            center: fit.center,
            mpp,
        };
        let size = if mpp > 1.2 {
            1
        } else if mpp > 0.55 {
            2
        } else if mpp > 0.28 {
            3
        } else {
            4
        };
        let mut a = Frame::new(w, h);
        let mut b = Frame::new(w, h);
        draw_units(&mut a, &world, &cam);
        plain(&mut b, &world, &cam);

        // 옛 방식이 찍은 자리를 새 방식이 빠뜨리면 안 된다.
        // 띠 경계에서 유닛을 흘리면 전선에 가로줄이 그어지고, 여기서 걸린다
        let lost =
            a.px.iter()
                .zip(b.px.iter())
                .filter(|(x, y)| **x == 0 && **y != 0)
                .count();
        assert_eq!(lost, 0, "화소당 {mpp}m 에서 {lost} 화소를 빠뜨렸다");

        // 반대로 새 방식만 찍는 자리는 있다. 옛 방식은 화면 밖 유닛을 통째로
        // 걸러내서, 화면 가장자리에 반쯤 걸친 사람을 지워 버렸다.
        // 그러니 새로 생긴 자리는 테두리에만 있어야 한다
        let stray =
            a.px.iter()
                .zip(b.px.iter())
                .enumerate()
                .filter(|(_, (x, y))| **x != 0 && **y == 0)
                .map(|(i, _)| (i % w, i / w))
                .find(|(x, y)| *x >= size && *x + size < w && *y >= size && *y + size < h);
        assert!(
            stray.is_none(),
            "화소당 {mpp}m 에서 테두리도 아닌 {stray:?} 에 없던 점이 생겼다"
        );

        // 색이 다른 자리는 겹쳐 선 두 사람 중 누가 위냐의 차이뿐이다.
        // 훑는 순서가 다르니 접촉면에서 소수가 어긋난다
        let recolored = a.px.iter().zip(b.px.iter()).filter(|(x, y)| x != y).count();
        assert!(
            recolored * 500 < w * h,
            "화소당 {mpp}m 에서 색이 다른 화소가 {recolored} 개다 — 겹침 순서로 설명되는 양이 아니다"
        );
        assert!(
            a.px.iter().any(|p| *p != 0),
            "화소당 {mpp}m 에서 아무것도 그리지 않았다"
        );
    }
}
