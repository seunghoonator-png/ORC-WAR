//! ORC-WAR 실행 진입점.
//!
//! 현재는 헤드리스 러너만 있다. 렌더러(M1 작업 1.4)가 붙으면 인자 없이 실행할 때
//! 창이 열리고, --headless 는 지금처럼 콘솔에서 전투만 돌린다.

use std::time::Instant;

use orc_war::scenario::Scenario;
use orc_war::sim::unit_types::{stats, INF_SWORD};
use orc_war::sim::{Outcome, SIM_HZ};

/// Windows 콘솔은 기본 코드페이지가 CP949 라 UTF-8 출력이 깨진다.
#[cfg(windows)]
fn setup_console() {
    extern "system" {
        fn SetConsoleOutputCP(code_page: u32) -> i32;
    }
    const CP_UTF8: u32 = 65001;
    unsafe {
        SetConsoleOutputCP(CP_UTF8);
    }
}

#[cfg(not(windows))]
fn setup_console() {}

/// 탐색기에서 더블클릭해 띄웠다면 결과를 읽기도 전에 창이 닫힌다.
#[cfg(windows)]
fn hold_window_open() {
    use std::io::{Read, Write};
    print!("\n계속하려면 Enter 를 누르세요...");
    let _ = std::io::stdout().flush();
    let _ = std::io::stdin().read(&mut [0u8]);
}

#[cfg(not(windows))]
fn hold_window_open() {}

struct Args {
    units: u32,
    ticks: u64,
    seed: u64,
    bench: bool,
    flip: bool,
    quiet: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            units: 20_000,
            ticks: 1_200,
            seed: 1,
            bench: false,
            flip: false,
            quiet: false,
        }
    }
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let argv: Vec<String> = std::env::args().skip(1).collect();
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
            "--flip" => a.flip = true,
            "--quiet" | "-q" => a.quiet = true,
            "--headless" => {}
            "--help" | "-h" => {
                println!(
                    "ORC-WAR headless runner\n\
                     \n\
                     사용법: orc-war [옵션]\n\
                     --units, -n <N>   총 유닛 수 (기본 20000)\n\
                     --ticks, -t <N>   최대 틱 (기본 1200 = 60초)\n\
                     --seed,  -s <N>   난수 시드 (기본 1)\n\
                     --bench           페이즈별 성능 측정\n\
                     --quiet, -q       진행 로그 생략"
                );
                std::process::exit(0);
            }
            other => eprintln!("알 수 없는 인자: {other}"),
        }
        i += 1;
    }
    a
}

fn main() {
    setup_console();
    // 인자 없이 실행 = 탐색기에서 더블클릭한 경우로 본다
    let launched_bare = std::env::args().len() == 1;
    let args = parse_args();
    let mut sc = Scenario::head_on(args.units, INF_SWORD, args.seed, args.ticks);
    if args.flip {
        // 진단용: 남북 배치를 맞바꾼다. 결과가 따라 뒤집히면 원인은 지형/방향,
        // 팀 번호를 따라가면 원인은 팀 처리 순서다.
        let c0 = sc.formations[0].center;
        sc.formations[0].center = sc.formations[1].center;
        sc.formations[1].center = c0;
    }

    println!(
        "ORC-WAR  |  {} vs {}  ({})  seed={}",
        sc.formations[0].count,
        sc.formations[1].count,
        stats(INF_SWORD).name,
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
    let mut acc = orc_war::sim::PhaseTimes::default();
    let mut worst = 0.0f64;

    let outcome = loop {
        w.step();
        let p = w.phase;
        acc.flow += p.flow;
        acc.movement += p.movement;
        acc.grid += p.grid;
        acc.combat += p.combat;
        acc.shooting += p.shooting;
        acc.morale += p.morale;
        worst = worst.max(p.total());

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

    if args.bench {
        println!("\n=== 성능 ({} 스레드, {} 유닛) ===", threads, w.pool.len());
        println!("  flow      {:>7.2} ms/tick", acc.flow / ticks);
        println!("  movement  {:>7.2} ms/tick", acc.movement / ticks);
        println!("  grid      {:>7.2} ms/tick", acc.grid / ticks);
        println!("  combat    {:>7.2} ms/tick", acc.combat / ticks);
        println!("  shooting  {:>7.2} ms/tick", acc.shooting / ticks);
        println!("  morale    {:>7.2} ms/tick", acc.morale / ticks);
        println!(
            "  ------------------------\n  평균      {:>7.2} ms/tick   (예산 50.00)",
            acc.total() / ticks
        );
        println!("  최악      {:>7.2} ms/tick", worst);
        println!(
            "  실시간 배속 {:.1}x",
            (ticks / SIM_HZ as f64) / wall.max(1e-9)
        );
    }

    if launched_bare {
        println!(
            "\n더 큰 전투를 보려면 명령 프롬프트에서:\n  \
             orc-war.exe -n 300000 --bench\n  \
             orc-war.exe --help"
        );
        hold_window_open();
    }
}
