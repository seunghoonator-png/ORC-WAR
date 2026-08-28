//! 창 없이 설정·결과 화면을 구워 본다. 눈으로 확인할 유일한 방법이다.
use orc_war::app::setup::Setup;
use orc_war::app::{report, Nav};
use orc_war::config::{BattleConfig, Battlefield, Doctrine};
use orc_war::render::Frame;
use orc_war::sim::{Outcome, WORLD_SIZE};

fn save(f: &Frame, path: &str) {
    let mut o = Vec::new();
    o.extend_from_slice(format!("P6\n{} {}\n255\n", f.w, f.h).as_bytes());
    for c in &f.px {
        o.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, *c as u8]);
    }
    std::fs::write(path, o).unwrap();
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let (w, h) = (1440usize, 810usize);

    let mut s = Setup::new();
    let mut f = Frame::new(w, h);
    s.draw(&mut f);
    save(&f, &format!("{dir}/setup_plains.ppm"));

    // 공성 + 기병 편성 + 30만
    s.cfg = BattleConfig {
        field: Battlefield::Siege,
        total: 300_000,
        doctrine: Doctrine::Cavalry,
        seed: 4,
    };
    s.row = 2;
    s.draw(&mut f);
    save(&f, &format!("{dir}/setup_siege.ppm"));

    s.cfg = BattleConfig {
        field: Battlefield::Mountain,
        total: 50_000,
        doctrine: Doctrine::Missile,
        seed: 7,
    };
    s.row = 0;
    s.input(Nav::Right);
    s.draw(&mut f);
    save(&f, &format!("{dir}/setup_river.ppm"));

    // 결과 화면 — 실제로 한 판 돌린다
    let cfg = BattleConfig {
        field: Battlefield::Plains,
        total: 40_000,
        doctrine: Doctrine::Balanced,
        seed: 3,
    };
    let sc = cfg.scenario();
    let mut world = sc.build();
    let mut outcome = Outcome::Ongoing;
    let mut decals = orc_war::render::Decals::new(WORLD_SIZE, 4.0);
    for _ in 0..6_000 {
        world.step();
        decals.absorb(&world);
        outcome = world.outcome(sc.max_ticks);
        if !matches!(outcome, Outcome::Ongoing) {
            break;
        }
    }
    report::draw(&mut f, &world, &cfg, outcome);
    save(&f, &format!("{dir}/report.ppm"));
    println!("t={} {:?}", world.tick, outcome);
}
