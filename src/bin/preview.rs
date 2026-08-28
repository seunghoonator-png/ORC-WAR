//! 관전 화면 미리보기.
//!
//! 창을 띄울 수 없는 환경에서 실제 렌더 경로를 그대로 태워 한 장을 굽는다.
//! 화면에 나올 그림을 창 없이 확인하기 위한 것이다.

use std::fs::File;
use std::io::{BufWriter, Write};

use orc_war::map::gen::{MapKind, MapOptions};
use orc_war::render::{draw_ground, draw_hud, draw_units, fit_camera, Decals, Frame, GroundCache};
use orc_war::scenario::Scenario;
use orc_war::sim::{Outcome, WORLD_SIZE};

const W: usize = 1440;
const H: usize = 810;

fn write_ppm(frame: &Frame, path: &str) -> std::io::Result<()> {
    let f = File::create(path)?;
    let mut out = BufWriter::new(f);
    write!(out, "P6\n{} {}\n255\n", frame.w, frame.h)?;
    let mut buf = Vec::with_capacity(frame.w * frame.h * 3);
    for c in &frame.px {
        buf.push((c >> 16) as u8);
        buf.push((c >> 8) as u8);
        buf.push(*c as u8);
    }
    out.write_all(&buf)
}

fn main() -> std::io::Result<()> {
    let mut a = std::env::args().skip(1);
    let kind = a.next().unwrap_or_else(|| "field".into());
    let units: u32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(60_000);
    let ticks: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(900);
    let out = a.next().unwrap_or_else(|| "preview.ppm".into());
    let zoom: f32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);

    let sc = match kind.as_str() {
        "siege" => Scenario::siege(units * 3 / 4, units / 4, 1, 40_000, true),
        "hills" => Scenario::combined_arms(units, 5, 40_000).on_map(MapOptions {
            kind: MapKind::Hills,
            ..Default::default()
        }),
        "river" => Scenario::combined_arms(units, 5, 40_000).on_map(MapOptions {
            river: true,
            forest: true,
            ..Default::default()
        }),
        "mountain" => Scenario::combined_arms(units, 5, 40_000).on_map(MapOptions {
            kind: MapKind::Mountain,
            ..Default::default()
        }),
        _ => Scenario::combined_arms(units, 5, 40_000),
    };

    let mut world = sc.build();
    let mut decals = Decals::new(WORLD_SIZE, 4.0);
    let mut outcome = Outcome::Ongoing;
    for _ in 0..ticks {
        world.step();
        decals.absorb(&world);
        outcome = world.outcome(sc.max_ticks);
        if !matches!(outcome, Outcome::Ongoing) {
            break;
        }
    }

    let mut cam = fit_camera(&world, W, H);
    if zoom > 0.0 {
        // 전장 한복판을 이 폭(m)으로 잘라 본다
        cam.mpp = zoom / W as f32;
    }

    let mut frame = Frame::new(W, H);
    // 렌더 경로를 여러 번 태워 프레임 예산(60fps = 16.7ms) 안에 드는지 본다.
    // 캐시가 있을 때와 없을 때를 나눠 잰다.
    let mut t_ground = 0.0f64;
    let mut t_units = 0.0f64;
    const REPS: u32 = 10;
    for _ in 0..REPS {
        let a = std::time::Instant::now();
        draw_ground(&mut frame, &world, &decals, &cam);
        let b = std::time::Instant::now();
        draw_units(&mut frame, &world, &cam);
        let c = std::time::Instant::now();
        t_ground += (b - a).as_secs_f64() * 1e3;
        t_units += (c - b).as_secs_f64() * 1e3;
    }

    // 실전 조건으로 캐시를 잰다. 전투가 계속 돌아가므로 매 프레임 시체가
    // 새로 쌓이고, 그만큼 배경을 덧칠해야 한다. 멈춘 화면을 재면 의미가 없다.
    let mut cache = GroundCache::new();
    cache.blit(&mut frame, &world, &decals, &cam);
    decals.clear_dirty();
    let mut t_cached = 0.0f64;
    let mut t_naive = 0.0f64;
    let mut deaths = 0usize;
    const LIVE: u32 = 60;
    for _ in 0..LIVE {
        // 60fps 화면이라면 한 프레임에 시뮬 한 틱쯤 돈다
        world.step();
        decals.absorb(&world);
        deaths += world.death_events.len();

        let a = std::time::Instant::now();
        cache.blit(&mut frame, &world, &decals, &cam);
        t_cached += a.elapsed().as_secs_f64() * 1e3;
        decals.clear_dirty();

        let b = std::time::Instant::now();
        draw_ground(&mut frame, &world, &decals, &cam);
        t_naive += b.elapsed().as_secs_f64() * 1e3;
    }
    println!(
        "  살아 있는 전장에서: 캐시 {:.2} ms  통째로 {:.2} ms  (프레임당 사망 {:.0})",
        t_cached / LIVE as f64,
        t_naive / LIVE as f64,
        deaths as f64 / LIVE as f64
    );
    draw_hud(&mut frame, &world, 2.0, false, 60.0, outcome);
    println!(
        "  렌더 {:.2} ms/frame  (지형 {:.2} + 병력 {:.2})  {}x{}",
        (t_ground + t_units) / REPS as f64,
        t_ground / REPS as f64,
        t_units / REPS as f64,
        W,
        H
    );
    write_ppm(&frame, &out)?;
    println!(
        "{out}  t={}  생존 {} / {}  (화소당 {:.2}m)",
        world.tick, world.stats.alive[0], world.stats.alive[1], cam.mpp
    );
    Ok(())
}
