//! 콘솔 성능 측정 — 창 없이 한 판을 끝까지 돌리며 페이즈별 시간을 잰다.
//!
//! `orc-war --bench -n 300000` 처럼 쓴다. 30만이 이 기계에서 실시간(50ms/틱)
//! 안에 도는지 확인하는 것이 목적이다.

use std::time::Instant;

use crate::config::{BattleConfig, Battlefield, Doctrine};
use crate::scenario::Scenario;
use crate::sim::unit_types::{stats, INF_SWORD};
use crate::sim::{Outcome, SIM_HZ};

struct Args {
    units: u32,
    ticks: u64,
    seed: u64,
    bench: bool,
    repeat: u32,
    flip: bool,
    quiet: bool,
    /// 지정하면 실제 설정(지형·편성)대로 재고, 없으면 검방보병 정면 대결로 잰다.
    ///
    /// 기본을 바꾸지 않는 이유는 지금까지 문서에 적어 온 수치와 비교가 되어야
    /// 하기 때문이다. 출하 점검용 "풀 시나리오"는 --field 로 부른다.
    field: Option<Battlefield>,
    doctrine: Doctrine,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            units: 20_000,
            ticks: 1_200,
            seed: 1,
            bench: false,
            repeat: 1,
            flip: false,
            quiet: false,
            field: None,
            doctrine: Doctrine::Balanced,
        }
    }
}

fn parse_args(argv: &[String]) -> Args {
    let mut a = Args::default();
    let mut i = 0;
    while i < argv.len() {
        let next = |i: usize| -> String { argv.get(i + 1).cloned().unwrap_or_default() };
        match argv[i].as_str() {
            "--units" | "-n" => {
                a.units = next(i).parse().unwrap_or(a.units);
                i += 1;
            }
            "--ticks" | "-t" => {
                a.ticks = next(i).parse().unwrap_or(a.ticks);
                i += 1;
            }
            "--seed" | "-s" => {
                a.seed = next(i).parse().unwrap_or(a.seed);
                i += 1;
            }
            "--bench" => a.bench = true,
            "--repeat" | "-r" => {
                a.repeat = next(i).parse().unwrap_or(a.repeat).max(1);
                a.bench = true;
                i += 1;
            }
            "--field" => {
                a.field = match next(i).as_str() {
                    "평지" | "plains" | "field" => Some(Battlefield::Plains),
                    "언덕" | "hills" => Some(Battlefield::Hills),
                    "산악" | "mountain" => Some(Battlefield::Mountain),
                    "도하" | "river" => Some(Battlefield::River),
                    "삼림" | "forest" => Some(Battlefield::Forest),
                    "공성" | "siege" => Some(Battlefield::Siege),
                    _ => None,
                };
                i += 1;
            }
            "--doctrine" => {
                a.doctrine = match next(i).as_str() {
                    "보병" | "infantry" => Doctrine::Infantry,
                    "기병" | "cavalry" => Doctrine::Cavalry,
                    "사수" | "missile" => Doctrine::Missile,
                    _ => Doctrine::Balanced,
                };
                i += 1;
            }
            "--flip" => a.flip = true,
            "--quiet" | "-q" => a.quiet = true,
            "--headless" => {}
            "--help" | "-h" => {}
            _ => {}
        }
        i += 1;
    }
    a
}

/// 한 판을 끝까지 돌리고 틱당 평균 성능을 돌려준다.
fn measure(sc: &Scenario, max_ticks: u64) -> (crate::sim::PhaseTimes, f64, u64) {
    let mut w = sc.build();
    let mut acc = crate::sim::PhaseTimes::default();
    let mut worst = 0.0f64;
    loop {
        w.step();
        let p = w.phase;
        acc.flow += p.flow;
        acc.movement += p.movement;
        acc.grid += p.grid;
        acc.combat += p.combat;
        acc.shooting += p.shooting;
        acc.siege += p.siege;
        acc.morale += p.morale;
        worst = worst.max(p.total());
        if !matches!(w.outcome(max_ticks), Outcome::Ongoing) {
            break;
        }
    }
    (acc, worst, w.tick)
}

pub fn run(argv: &[String]) {
    let args = parse_args(argv);
    let mut sc = match args.field {
        Some(field) => BattleConfig {
            field,
            total: args.units,
            doctrine: args.doctrine,
            seed: args.seed,
        }
        .scenario(),
        None => Scenario::head_on(args.units, INF_SWORD, args.seed, args.ticks),
    };
    sc.max_ticks = args.ticks;
    if args.flip {
        // 진단용: 남북 배치를 맞바꾼다. 결과가 따라 뒤집히면 원인은 지형/방향,
        // 팀 번호를 따라가면 원인은 팀 처리 순서다.
        let c0 = sc.formations[0].center;
        sc.formations[0].center = sc.formations[1].center;
        sc.formations[1].center = c0;
    }

    let per_team = |t: u8| -> u32 {
        sc.formations
            .iter()
            .filter(|f| f.team == t)
            .map(|f| f.count)
            .sum()
    };
    println!(
        "ORC-WAR  |  {} vs {}  ({})  seed={}",
        per_team(0),
        per_team(1),
        match args.field {
            Some(f) => format!("{} · {} 편성", f.name(), args.doctrine.name()),
            None => stats(INF_SWORD).name.to_string(),
        },
        args.seed
    );

    let t_build = Instant::now();
    let mut w = sc.build();
    println!(
        "스폰 {} 유닛 / {:.1} MB / {:.0} ms",
        w.pool.len(),
        w.pool.memory_bytes() as f64 / 1e6,
        t_build.elapsed().as_secs_f64() * 1e3
    );

    let threads = rayon::current_num_threads();
    let t0 = Instant::now();
    let mut acc = crate::sim::PhaseTimes::default();
    let mut worst = 0.0f64;
    let mut worst_tick = 0u64;
    let mut worst_phase = crate::sim::PhaseTimes::default();
    // 최악 한 틱은 이 기계에서 다른 프로세스가 잠깐 끼어든 것일 수도 있다.
    // 분포를 함께 봐야 "설계가 넘긴 것"과 "운영체제가 끼어든 것"이 갈린다.
    let mut per_tick: Vec<f64> = Vec::new();

    let outcome = loop {
        w.step();
        let p = w.phase;
        acc.flow += p.flow;
        acc.movement += p.movement;
        acc.grid += p.grid;
        acc.combat += p.combat;
        acc.shooting += p.shooting;
        acc.siege += p.siege;
        acc.morale += p.morale;
        per_tick.push(p.total());
        if p.total() > worst {
            worst = p.total();
            worst_tick = w.tick;
            worst_phase = p;
        }

        if !args.quiet && w.tick.is_multiple_of(100) {
            println!(
                "  t={:>5} ({:>4.0}s)  생존 {:>7} / {:>7}   전사 {:>7} / {:>7}   패주 {:>6} / {:>6}   {:>5.1} ms/tick",
                w.tick,
                w.tick as f64 / SIM_HZ as f64,
                w.stats.alive[0],
                w.stats.alive[1],
                w.stats.dead[0],
                w.stats.dead[1],
                w.stats.routed[0],
                w.stats.routed[1],
                p.total()
            );
        }
        match w.outcome(args.ticks) {
            Outcome::Ongoing => {}
            done => break done,
        }
    };

    let wall = t0.elapsed().as_secs_f64();
    let ticks = w.tick as f64;

    println!("\n=== 결과 ===");
    match outcome {
        Outcome::Victory(t) => println!(
            "{} 승리 ({}틱 / {:.0}초)",
            if t == 0 { "공격측" } else { "방어측" },
            w.tick,
            ticks / SIM_HZ as f64
        ),
        Outcome::Timeout => println!("무승부 — 제한 틱 도달"),
        Outcome::Ongoing => unreachable!(),
    }
    println!(
        "생존  공격측 {:>7}   방어측 {:>7}",
        w.stats.alive[0], w.stats.alive[1]
    );
    println!(
        "전사  공격측 {:>7}   방어측 {:>7}",
        w.stats.dead[0], w.stats.dead[1]
    );
    println!(
        "패주  공격측 {:>7}   방어측 {:>7}",
        w.stats.routed[0], w.stats.routed[1]
    );
    println!(
        "이탈  공격측 {:>7}   방어측 {:>7}",
        w.stats.fled[0], w.stats.fled[1]
    );

    if args.bench && args.repeat > 1 {
        // 한 번만 재면 다른 작업이 CPU 를 나눠 쓰고 있어도 알 수가 없다.
        // 여러 번 재서 흔들림을 함께 보여 준다.
        let mut totals = Vec::new();
        for _ in 0..args.repeat {
            let (acc, _, ticks) = measure(&sc, args.ticks);
            totals.push(acc.total() / ticks as f64);
        }
        totals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let lo = totals[0];
        let hi = totals[totals.len() - 1];
        let mid = totals[totals.len() / 2];
        println!(
            "\n=== 반복 측정 ({}회, {} 스레드) ===",
            args.repeat, threads
        );
        for (k, t) in totals.iter().enumerate() {
            println!("  {}회차  {:>7.2} ms/tick", k + 1, t);
        }
        println!("  중앙값  {mid:>7.2} ms/tick   최소 {lo:.2}  최대 {hi:.2}");
        let spread = (hi - lo) / mid;
        if spread > 0.10 {
            println!(
                "  ⚠ 회차별 편차가 {:.0}% 다. 측정이 흔들리고 있다.",
                spread * 100.0
            );
        }
        println!(
            "  * 다른 작업(빌드·테스트 포함)이 함께 돌면 세 회차가 나란히 느려져\n  \
             편차로는 드러나지 않는다. 반드시 놀고 있는 기계에서 잴 것."
        );
        return;
    }

    if args.bench {
        println!("\n=== 성능 ({} 스레드, {} 유닛) ===", threads, w.pool.len());
        println!("  flow      {:>7.2} ms/tick", acc.flow / ticks);
        println!("  movement  {:>7.2} ms/tick", acc.movement / ticks);
        println!("  grid      {:>7.2} ms/tick", acc.grid / ticks);
        println!("  combat    {:>7.2} ms/tick", acc.combat / ticks);
        println!("  shooting  {:>7.2} ms/tick", acc.shooting / ticks);
        println!("  siege     {:>7.2} ms/tick", acc.siege / ticks);
        println!("  morale    {:>7.2} ms/tick", acc.morale / ticks);
        println!(
            "  ------------------------\n  평균      {:>7.2} ms/tick   (예산 50.00)",
            acc.total() / ticks
        );
        let mut sorted = per_tick.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let at = |q: f64| sorted[((sorted.len() as f64 - 1.0) * q) as usize];
        println!(
            "  중앙값    {:>7.2}   상위 1%   {:>7.2}   상위 0.1%  {:>7.2} ms/tick",
            at(0.5),
            at(0.99),
            at(0.999)
        );
        let over = per_tick.iter().filter(|t| **t > 50.0).count();
        println!(
            "  예산(50ms) 초과 {} / {} 틱  ({:.2}%)",
            over,
            per_tick.len(),
            over as f64 / per_tick.len() as f64 * 100.0
        );
        println!("  최악      {:>7.2} ms/tick  (t={})", worst, worst_tick);
        println!(
            "    그때: 이동 {:.1} 격자 {:.1} 전투 {:.1} 사격 {:.1} 공성 {:.1} 사기 {:.1} 경로 {:.1}",
            worst_phase.movement,
            worst_phase.grid,
            worst_phase.combat,
            worst_phase.shooting,
            worst_phase.siege,
            worst_phase.morale,
            worst_phase.flow
        );
        println!(
            "  실시간 배속 {:.1}x",
            (ticks / SIM_HZ as f64) / wall.max(1e-9)
        );
    }
}
