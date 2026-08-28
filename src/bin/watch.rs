//! 전장 관전 창.
//!
//! 유저가 하는 일은 보는 것뿐이다. 카메라를 옮기고, 배속을 바꾸고, 멈춘다.
//! 전투 자체에는 개입하지 않는다 — 그것이 이 시뮬레이터의 전제다.

use std::time::Instant;

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

use orc_war::render::{draw_ground, draw_hud, draw_units, fit_camera, Decals, Frame};
use orc_war::scenario::Scenario;
use orc_war::sim::{Outcome, DT, WORLD_SIZE};

const WIN_W: usize = 1440;
const WIN_H: usize = 810;
/// 한 프레임에 소화할 수 있는 시뮬 틱 상한.
///
/// 이걸 두지 않으면 배속이 기계가 감당하는 속도를 넘어선 순간 따라잡으려다
/// 화면이 아예 멈춘다. 프레임을 지키고 배속을 포기하는 편이 낫다.
const MAX_STEPS_PER_FRAME: u32 = 12;

const SPEEDS: [f32; 6] = [0.5, 1.0, 2.0, 4.0, 8.0, 16.0];

struct Setup {
    name: String,
    scenario: Scenario,
}

fn build_scenario(args: &[String]) -> Setup {
    let kind = args.first().map(|s| s.as_str()).unwrap_or("field");
    let units: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(60_000);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let max_ticks = 40_000;

    use orc_war::map::gen::{MapKind, MapOptions};
    let (name, scenario) = match kind {
        "siege" => (
            format!("공성전 {}", units),
            Scenario::siege(units * 3 / 4, units / 4, seed, max_ticks, true),
        ),
        "hills" => (
            format!("언덕 {}", units),
            Scenario::combined_arms(units, seed, max_ticks).on_map(MapOptions {
                kind: MapKind::Hills,
                ..Default::default()
            }),
        ),
        "mountain" => (
            format!("산악 {}", units),
            Scenario::combined_arms(units, seed, max_ticks).on_map(MapOptions {
                kind: MapKind::Mountain,
                ..Default::default()
            }),
        ),
        "river" => (
            format!("도하전 {}", units),
            Scenario::combined_arms(units, seed, max_ticks).on_map(MapOptions {
                river: true,
                forest: true,
                ..Default::default()
            }),
        ),
        "forest" => (
            format!("삼림전 {}", units),
            Scenario::combined_arms(units, seed, max_ticks).on_map(MapOptions {
                forest: true,
                rocks: true,
                ..Default::default()
            }),
        ),
        _ => (
            format!("야전 {}", units),
            Scenario::combined_arms(units, seed, max_ticks),
        ),
    };
    Setup { name, scenario }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s == "--help" || s == "-h") == Some(true) {
        println!(
            "ORC-WAR 관전\n\n\
             사용법: watch [지형] [유닛수] [시드]\n\
             지형: field(기본) / hills / mountain / river / forest / siege\n\n\
             예)  watch field 80000\n     watch siege 40000 3\n\n\
             조작\n\
             \x20 스페이스   일시정지\n\
             \x20 [ ]        배속 내리기 / 올리기\n\
             \x20 WASD 화살표 카메라 이동   (드래그도 됨)\n\
             \x20 휠 또는 QE 확대 / 축소\n\
             \x20 F          전장 전체 보기\n\
             \x20 R          같은 설정으로 다시\n\
             \x20 Esc        종료"
        );
        return;
    }

    let setup = build_scenario(&args);
    let mut world = setup.scenario.build();
    let mut decals = Decals::new(WORLD_SIZE, 4.0);

    let mut window = Window::new(
        "ORC-WAR",
        WIN_W,
        WIN_H,
        WindowOptions {
            resize: true,
            ..WindowOptions::default()
        },
    )
    .expect("창을 열 수 없습니다");
    window.set_target_fps(60);

    let mut frame = Frame::new(WIN_W, WIN_H);
    let mut cam = fit_camera(&world, WIN_W, WIN_H);
    let mut speed_idx = 1usize;
    let mut paused = false;
    let mut accumulator = 0.0f32;
    let mut last = Instant::now();
    let mut fps_timer = Instant::now();
    let mut fps_frames = 0u32;
    let mut fps = 0.0f32;
    let mut drag_from: Option<(f32, f32)> = None;
    let mut outcome = Outcome::Ongoing;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let (w, h) = window.get_size();
        let (w, h) = (w.max(320), h.max(240));
        frame.resize(w, h);

        // --- 입력 ---
        for k in window.get_keys_pressed(KeyRepeat::No) {
            match k {
                Key::Space => paused = !paused,
                Key::LeftBracket => speed_idx = speed_idx.saturating_sub(1),
                Key::RightBracket => speed_idx = (speed_idx + 1).min(SPEEDS.len() - 1),
                Key::F => cam = fit_camera(&world, w, h),
                Key::R => {
                    world = setup.scenario.build();
                    decals = Decals::new(WORLD_SIZE, 4.0);
                    cam = fit_camera(&world, w, h);
                    accumulator = 0.0;
                    outcome = Outcome::Ongoing;
                }
                _ => {}
            }
        }
        let pan = 520.0 * cam.mpp * (1.0 / 60.0);
        if window.is_key_down(Key::A) || window.is_key_down(Key::Left) {
            cam.center[0] -= pan;
        }
        if window.is_key_down(Key::D) || window.is_key_down(Key::Right) {
            cam.center[0] += pan;
        }
        if window.is_key_down(Key::W) || window.is_key_down(Key::Up) {
            cam.center[1] += pan;
        }
        if window.is_key_down(Key::S) || window.is_key_down(Key::Down) {
            cam.center[1] -= pan;
        }
        if window.is_key_down(Key::Q) {
            cam.mpp = (cam.mpp * 1.03).min(8.0);
        }
        if window.is_key_down(Key::E) {
            cam.mpp = (cam.mpp / 1.03).max(0.06);
        }
        if let Some((_, sy)) = window.get_scroll_wheel() {
            if sy.abs() > 0.01 {
                // 커서가 가리키는 곳을 붙들고 확대한다
                let (mx, my) = window
                    .get_mouse_pos(MouseMode::Clamp)
                    .unwrap_or((w as f32 * 0.5, h as f32 * 0.5));
                let before = cam.to_world(mx, my, w, h);
                cam.mpp = (cam.mpp * if sy > 0.0 { 0.86 } else { 1.16 }).clamp(0.06, 8.0);
                let after = cam.to_world(mx, my, w, h);
                cam.center[0] += before[0] - after[0];
                cam.center[1] += before[1] - after[1];
            }
        }
        if window.get_mouse_down(MouseButton::Left) {
            if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Pass) {
                if let Some((px, py)) = drag_from {
                    cam.center[0] -= (mx - px) * cam.mpp;
                    cam.center[1] += (my - py) * cam.mpp;
                }
                drag_from = Some((mx, my));
            }
        } else {
            drag_from = None;
        }
        let half = WORLD_SIZE * 0.5;
        cam.center[0] = cam.center[0].clamp(-half, WORLD_SIZE + half);
        cam.center[1] = cam.center[1].clamp(-half, WORLD_SIZE + half);

        // --- 시뮬레이션 ---
        let now = Instant::now();
        let real_dt = (now - last).as_secs_f32().min(0.1);
        last = now;
        let speed = SPEEDS[speed_idx];
        if !paused && matches!(outcome, Outcome::Ongoing) {
            accumulator += real_dt * speed;
            let mut steps = 0;
            while accumulator >= DT && steps < MAX_STEPS_PER_FRAME {
                world.step();
                decals.absorb(&world);
                accumulator -= DT;
                steps += 1;
                outcome = world.outcome(setup.scenario.max_ticks);
                if !matches!(outcome, Outcome::Ongoing) {
                    break;
                }
            }
            if steps == MAX_STEPS_PER_FRAME {
                // 따라잡기를 포기한다. 화면이 멈추는 것보다 낫다
                accumulator = 0.0;
            }
        }

        // --- 그리기 ---
        draw_ground(&mut frame, &world, &decals, &cam);
        draw_units(&mut frame, &world, &cam);
        draw_hud(&mut frame, &world, speed, paused, fps, outcome);

        fps_frames += 1;
        if fps_timer.elapsed().as_secs_f32() >= 0.5 {
            fps = fps_frames as f32 / fps_timer.elapsed().as_secs_f32();
            fps_frames = 0;
            fps_timer = Instant::now();
            window.set_title(&format!(
                "ORC-WAR — {}  |  공격 {} vs 방어 {}  |  {:.0}초  |  {}배속{}",
                setup.name,
                world.stats.alive[0],
                world.stats.alive[1],
                world.tick as f32 * DT,
                speed,
                if paused { " (멈춤)" } else { "" }
            ));
        }

        window
            .update_with_buffer(&frame.px, frame.w, frame.h)
            .expect("화면 갱신 실패");
    }
}
