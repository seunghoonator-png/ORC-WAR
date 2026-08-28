//! 창을 열고 설정 → 전투 → 결과를 돌린다.
//!
//! 이 파일만 minifb 를 안다. 나머지 화면 코드는 화소 버퍼만 만지므로 창 없이도
//! 그대로 구워 확인할 수 있다 — 이 개발 환경에서는 그것이 유일한 검증 수단이다.

use std::time::Instant;

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

use crate::config::BattleConfig;
use crate::render::{draw_hud, draw_units, fit_camera, Decals, Frame, GroundCache};
use crate::sim::{Outcome, World, DT, WORLD_SIZE};

use super::report;
use super::setup::Setup;
use super::{Nav, Screen};

const WIN_W: usize = 1440;
const WIN_H: usize = 810;

/// 한 프레임에 소화할 수 있는 시뮬 틱 상한.
///
/// 이걸 두지 않으면 배속이 기계가 감당하는 속도를 넘어선 순간 따라잡으려다
/// 화면이 아예 멈춘다. 프레임을 지키고 배속을 포기하는 편이 낫다.
const MAX_STEPS_PER_FRAME: u32 = 12;

/// 이만큼 연달아 뒤처지면 배속을 한 단계 내린다.
///
/// 한두 프레임 밀리는 것은 흔한 일이라(지형 캐시가 통째로 다시 그려질 때 등)
/// 그때마다 내리면 배속이 계속 요동친다.
const BEHIND_TO_SLOW: u32 = 45;

const SPEEDS: [f32; 6] = [0.5, 1.0, 2.0, 4.0, 8.0, 16.0];

struct Battle {
    world: World,
    decals: Decals,
    ground: GroundCache,
    cam: crate::render::Camera,
    outcome: Outcome,
    accumulator: f32,
    speed_idx: usize,
    paused: bool,
    behind: u32,
    /// 자동 감속 안내를 몇 프레임 더 띄울지
    note_frames: u32,
}

impl Battle {
    fn new(cfg: &BattleConfig, w: usize, h: usize) -> Self {
        let world = cfg.scenario().build();
        let cam = fit_camera(&world, w, h);
        Self {
            world,
            decals: Decals::new(WORLD_SIZE, 4.0),
            ground: GroundCache::new(),
            cam,
            outcome: Outcome::Ongoing,
            accumulator: 0.0,
            speed_idx: 1,
            paused: false,
            behind: 0,
            note_frames: 0,
        }
    }
}

/// 설정 화면부터 시작한다. `start` 가 있으면 그 설정으로 곧장 전투에 들어간다.
pub fn main_loop(start: Option<BattleConfig>) {
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
    let mut setup = Setup::new();
    let mut screen = Screen::Setup;
    let mut battle: Option<Battle> = None;
    if let Some(cfg) = start {
        setup.cfg = cfg;
        battle = Some(Battle::new(&cfg, WIN_W, WIN_H));
        screen = Screen::Battle;
    }

    let mut last = Instant::now();
    let mut fps_timer = Instant::now();
    let mut fps_frames = 0u32;
    let mut fps = 0.0f32;
    let mut drag_from: Option<(f32, f32)> = None;
    let mut title_timer = Instant::now();

    while window.is_open() {
        let (w, h) = window.get_size();
        let (w, h) = (w.max(480), h.max(360));
        frame.resize(w, h);
        let keys = window.get_keys_pressed(KeyRepeat::Yes);
        let esc = window.is_key_pressed(Key::Escape, KeyRepeat::No);

        match screen {
            // ---------------------------------------------------------- 설정
            Screen::Setup => {
                if esc {
                    return;
                }
                let shift =
                    window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
                for k in &keys {
                    let nav = match k {
                        Key::Up | Key::W => Some(Nav::Up),
                        Key::Down | Key::S => Some(Nav::Down),
                        Key::Left | Key::A => Some(if shift { Nav::LeftFast } else { Nav::Left }),
                        Key::Right | Key::D => {
                            Some(if shift { Nav::RightFast } else { Nav::Right })
                        }
                        Key::Enter | Key::NumPadEnter | Key::Space => Some(Nav::Enter),
                        _ => None,
                    };
                    if let Some(nav) = nav {
                        if setup.input(nav) {
                            // 30만이면 만드는 데 몇 초 걸린다. 만드는 중임을 알린다
                            splash(&mut frame, &mut window, &setup.cfg);
                            battle = Some(Battle::new(&setup.cfg, w, h));
                            screen = Screen::Battle;
                            last = Instant::now();
                        }
                    }
                }
                if matches!(screen, Screen::Setup) {
                    setup.draw(&mut frame);
                    window.set_title("ORC-WAR — 설정");
                }
            }

            // ---------------------------------------------------------- 전투
            Screen::Battle => {
                let b = battle.as_mut().expect("전투 상태가 없다");
                if esc {
                    screen = Screen::Report;
                    continue;
                }
                for k in &keys {
                    match k {
                        Key::Space => b.paused = !b.paused,
                        Key::LeftBracket => b.speed_idx = b.speed_idx.saturating_sub(1),
                        Key::RightBracket => b.speed_idx = (b.speed_idx + 1).min(SPEEDS.len() - 1),
                        Key::F => b.cam = fit_camera(&b.world, w, h),
                        Key::R => {
                            *b = Battle::new(&setup.cfg, w, h);
                        }
                        Key::Enter | Key::NumPadEnter => {
                            if !matches!(b.outcome, Outcome::Ongoing) {
                                screen = Screen::Report;
                            }
                        }
                        _ => {}
                    }
                }
                if !matches!(screen, Screen::Battle) {
                    continue;
                }
                camera_input(&mut window, &mut b.cam, w, h, &mut drag_from);

                // --- 시뮬레이션 ---
                let now = Instant::now();
                let real_dt = (now - last).as_secs_f32().min(0.1);
                last = now;
                let speed = SPEEDS[b.speed_idx];
                if !b.paused && matches!(b.outcome, Outcome::Ongoing) {
                    b.accumulator += real_dt * speed;
                    let mut steps = 0;
                    while b.accumulator >= DT && steps < MAX_STEPS_PER_FRAME {
                        b.world.step();
                        b.decals.absorb(&b.world);
                        if !b.world.breach_events.is_empty() {
                            // 성벽이 무너졌으면 지형 자체가 달라졌다
                            b.ground.invalidate();
                        }
                        b.accumulator -= DT;
                        steps += 1;
                        b.outcome = b.world.outcome(BattleConfig::MAX_TICKS);
                        if !matches!(b.outcome, Outcome::Ongoing) {
                            break;
                        }
                    }
                    // 상한까지 돌고도 빚이 남았으면 이 기계가 못 따라오고 있다
                    if steps == MAX_STEPS_PER_FRAME && b.accumulator >= DT {
                        b.behind += 1;
                        b.accumulator = 0.0;
                    } else {
                        b.behind = b.behind.saturating_sub(1);
                    }
                    if b.behind >= BEHIND_TO_SLOW && b.speed_idx > 0 {
                        b.speed_idx -= 1;
                        b.behind = 0;
                        b.note_frames = 180;
                    }
                }

                // --- 그리기 ---
                b.ground.blit(&mut frame, &b.world, &b.decals, &b.cam);
                b.decals.clear_dirty();
                draw_units(&mut frame, &b.world, &b.cam);
                let note = if b.note_frames > 0 {
                    b.note_frames -= 1;
                    Some(format!(
                        "이 기계가 따라오지 못해 배속을 {}배로 낮췄습니다",
                        SPEEDS[b.speed_idx]
                    ))
                } else {
                    None
                };
                draw_hud(
                    &mut frame,
                    &b.world,
                    SPEEDS[b.speed_idx],
                    b.paused,
                    fps,
                    b.outcome,
                    note.as_deref(),
                );

                if title_timer.elapsed().as_secs_f32() >= 0.5 {
                    title_timer = Instant::now();
                    window.set_title(&format!(
                        "ORC-WAR — {}  |  공격 {} vs 방어 {}  |  {:.0}초  |  {}배속{}",
                        setup.cfg.title(),
                        b.world.stats.alive[0],
                        b.world.stats.alive[1],
                        b.world.tick as f32 * DT,
                        SPEEDS[b.speed_idx],
                        if b.paused { " (멈춤)" } else { "" }
                    ));
                }
            }

            // ---------------------------------------------------------- 결과
            Screen::Report => {
                if esc {
                    return;
                }
                let b = battle.as_mut().expect("전투 상태가 없다");
                for k in &keys {
                    match k {
                        Key::Enter | Key::NumPadEnter | Key::Space => {
                            screen = Screen::Setup;
                        }
                        Key::R => {
                            splash(&mut frame, &mut window, &setup.cfg);
                            *b = Battle::new(&setup.cfg, w, h);
                            screen = Screen::Battle;
                            last = Instant::now();
                        }
                        _ => {}
                    }
                }
                if matches!(screen, Screen::Report) {
                    report::draw(&mut frame, &b.world, &setup.cfg, b.outcome);
                    window.set_title("ORC-WAR — 결과");
                }
            }
        }

        fps_frames += 1;
        if fps_timer.elapsed().as_secs_f32() >= 0.5 {
            fps = fps_frames as f32 / fps_timer.elapsed().as_secs_f32();
            fps_frames = 0;
            fps_timer = Instant::now();
        }
        window
            .update_with_buffer(&frame.px, frame.w, frame.h)
            .expect("화면 갱신 실패");
    }
}

/// 30만을 세우는 데는 몇 초가 걸린다. 그동안 창이 죽은 것처럼 보이지 않게 한다.
fn splash(frame: &mut Frame, window: &mut Window, cfg: &BattleConfig) {
    use crate::render::{rgb, uifont};
    for p in frame.px.iter_mut() {
        *p = rgb(16, 18, 21);
    }
    let cx = frame.w as i32 / 2;
    let cy = frame.h as i32 / 2;
    uifont::text_center(
        frame,
        cx,
        cy - 30,
        "병력을 세우는 중",
        2,
        rgb(232, 232, 220),
    );
    uifont::text_center(frame, cx, cy + 10, &cfg.title(), 1, rgb(126, 128, 124));
    let _ = window.update_with_buffer(&frame.px, frame.w, frame.h);
}

fn camera_input(
    window: &mut Window,
    cam: &mut crate::render::Camera,
    w: usize,
    h: usize,
    drag_from: &mut Option<(f32, f32)>,
) {
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
            if let Some((px, py)) = *drag_from {
                cam.center[0] -= (mx - px) * cam.mpp;
                cam.center[1] += (my - py) * cam.mpp;
            }
            *drag_from = Some((mx, my));
        }
    } else {
        *drag_from = None;
    }
    let half = WORLD_SIZE * 0.5;
    cam.center[0] = cam.center[0].clamp(-half, WORLD_SIZE + half);
    cam.center[1] = cam.center[1].clamp(-half, WORLD_SIZE + half);
}
