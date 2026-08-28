//! 설정·결과 화면 회귀.
//!
//! 창을 띄울 수 없는 환경에서도 검사할 수 있다 — 화면은 순수한 화소 버퍼다.

use orc_war::app::setup::{breakdown, Setup};
use orc_war::app::{report, Nav};
use orc_war::config::{BattleConfig, Battlefield, Doctrine, ARMY_SIZES, BATTLEFIELDS, DOCTRINES};
use orc_war::render::{uifont, Frame};
use orc_war::sim::Outcome;

/// 화면에 쓰는 글자가 전부 글꼴 표에 있는가.
///
/// 없으면 빈 네모로 나오는데, 창을 못 띄우는 환경에서는 알아채기 어렵다.
/// 문구를 고치고 `tools/genfont.py` 다시 돌리는 것을 잊으면 여기서 걸린다.
#[test]
fn every_ui_string_has_glyphs() {
    let mut texts: Vec<String> = vec![
        "고대 대규모 전투 시뮬레이터".into(),
        "ENTER  전투 시작".into(),
        "위아래 항목 고르기 · 좌우 값 바꾸기 · ESC 종료".into(),
        "병력을 세우는 중".into(),
        "공격측 승리".into(),
        "방어측 승리".into(),
        "결판 없음".into(),
        "무엇에 죽었나".into(),
        "그중 패주".into(),
        "ENTER  설정으로 돌아가기      R  같은 설정으로 다시      ESC  종료".into(),
        "스페이스 멈춤 · [ ] 배속 · WASD 이동 · 휠 확대 · F 전체 · R 다시 · ESC 나가기".into(),
        "이 기계가 따라오지 못해 배속을 0.5배로 낮췄습니다".into(),
    ];
    for f in BATTLEFIELDS {
        texts.push(f.name().into());
        texts.push(f.blurb().into());
    }
    for d in DOCTRINES {
        texts.push(d.name().into());
        texts.push(d.blurb().into());
    }
    for ty in 0..orc_war::sim::unit_types::N_TYPES {
        texts.push(orc_war::sim::unit_types::stats(ty as u8).name.into());
    }
    for n in orc_war::sim::CAUSE_NAMES {
        texts.push(n.into());
    }
    texts.push(BattleConfig::default().title());

    let mut missing: Vec<char> = Vec::new();
    for t in &texts {
        for c in t.chars() {
            if c != ' ' && !uifont::has_glyph(c) && !missing.contains(&c) {
                missing.push(c);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "글꼴 표에 없는 글자 {missing:?} — tools/genfont.py 를 다시 돌려야 한다"
    );
}

/// 어떤 값 조합이든 설정 화면이 그려지고, 항목이 순환한다.
#[test]
fn setup_cycles_every_value() {
    let mut s = Setup::new();
    let mut frame = Frame::new(900, 560);
    // 네 항목을 각각 한 바퀴 돌린다
    for row in 0..4 {
        s.row = row;
        for _ in 0..8 {
            s.input(Nav::Right);
            s.draw(&mut frame);
        }
        for _ in 0..8 {
            s.input(Nav::Left);
        }
    }
    // 한 바퀴 돌면 처음 값으로 돌아와야 한다
    assert_eq!(s.cfg.field, BattleConfig::default().field);
    assert_eq!(s.cfg.doctrine, BattleConfig::default().doctrine);
    assert_eq!(s.cfg.total, BattleConfig::default().total);
    assert!(s.input(Nav::Enter), "ENTER 는 전투를 시작해야 한다");
}

/// 창이 작아도 화면이 밖으로 새지 않는다.
#[test]
fn screens_survive_a_small_window() {
    let mut s = Setup::new();
    for (w, h) in [(480, 360), (800, 600), (1920, 1080)] {
        let mut frame = Frame::new(w, h);
        s.draw(&mut frame);
        assert!(
            frame.px.iter().any(|p| *p != 0),
            "{w}x{h} 에서 아무것도 안 그렸다"
        );
    }
}

/// 씨앗은 아래로 새지 않는다.
#[test]
fn seed_stays_in_range() {
    let mut s = Setup::new();
    s.row = 3;
    for _ in 0..40 {
        s.input(Nav::LeftFast);
    }
    assert_eq!(s.cfg.seed, 1);
    for _ in 0..2000 {
        s.input(Nav::RightFast);
    }
    assert_eq!(s.cfg.seed, 9999);
}

/// 설정이 실제로 그만큼의 병력을 세우는가.
#[test]
fn every_setting_spawns_what_it_says() {
    for field in BATTLEFIELDS {
        for doctrine in DOCTRINES {
            let cfg = BattleConfig {
                field,
                total: 20_000,
                doctrine,
                seed: 2,
            };
            let sc = cfg.scenario();
            let n = sc.total_units();
            // 비율을 정수로 자르므로 몇 명은 새어 나간다. 5% 안이면 된다
            assert!(
                n as f32 > cfg.total as f32 * 0.95 && n <= cfg.total + 40,
                "{:?}/{:?} 가 {} 명을 세웠다 (요청 {})",
                field,
                doctrine,
                n,
                cfg.total
            );
            assert!(!breakdown(&cfg).is_empty());
        }
    }
}

/// 병력 규모를 키워도 설정이 성립한다.
#[test]
fn every_army_size_builds() {
    for total in ARMY_SIZES {
        let cfg = BattleConfig {
            field: Battlefield::Plains,
            total,
            doctrine: Doctrine::Balanced,
            seed: 1,
        };
        let sc = cfg.scenario();
        assert!(sc.total_units() > 0);
    }
}

/// 결과 화면이 실제 전투 결과로 그려진다.
#[test]
fn report_draws_a_finished_battle() {
    let cfg = BattleConfig {
        field: Battlefield::Plains,
        total: 6_000,
        doctrine: Doctrine::Balanced,
        seed: 3,
    };
    let sc = cfg.scenario();
    let mut w = sc.build();
    let mut outcome = Outcome::Ongoing;
    for _ in 0..4_000 {
        w.step();
        outcome = w.outcome(sc.max_ticks);
        if !matches!(outcome, Outcome::Ongoing) {
            break;
        }
    }
    assert!(!matches!(outcome, Outcome::Ongoing), "전투가 끝나지 않았다");

    // 병종별 전사자 합이 전체 전사자와 맞아야 한다
    for t in 0..2 {
        let by_type: u32 = w.stats.dead_by_type[t].iter().sum();
        let by_cause: u32 = w.stats.dead_by_cause[t].iter().sum();
        assert_eq!(by_type, w.stats.dead[t], "팀 {t} 병종별 합이 어긋난다");
        assert_eq!(by_cause, w.stats.dead[t], "팀 {t} 사인별 합이 어긋난다");
    }

    let mut frame = Frame::new(1280, 720);
    report::draw(&mut frame, &w, &cfg, outcome);
    assert!(frame.px.iter().any(|p| *p != 0));
}
