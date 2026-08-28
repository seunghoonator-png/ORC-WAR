//! ORC-WAR 진입점 — 실행파일 하나가 전부를 맡는다.
//!
//! 인자 없이 실행하면(탐색기에서 더블클릭하면) 창이 열리고 설정 화면이 뜬다.
//! 인자를 주면 콘솔 도구가 된다 — 성능 측정, 병종 상성, 공성전, 스냅샷, 자체 점검.
//!
//! 예전에는 exe 가 여섯 개였다. 받는 쪽에서 어느 것을 눌러야 하는지 알 수 없으니
//! 하나로 합쳤다.

use orc_war::config::{BattleConfig, Battlefield, Doctrine, ARMY_SIZES};

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

/// 창 모드에서는 딸려 온 콘솔 창을 떼어 낸다.
///
/// 콘솔 서브시스템으로 빌드해야 `--bench` 결과를 콘솔에서 볼 수 있는데, 그러면
/// 더블클릭했을 때 검은 창이 하나 같이 뜬다. 창 모드로 갈 때만 닫는다.
#[cfg(all(windows, feature = "render"))]
fn detach_console() {
    extern "system" {
        fn FreeConsole() -> i32;
    }
    unsafe {
        FreeConsole();
    }
}

// 화면이 없는 빌드에는 뗄 콘솔도 없다
#[cfg(all(not(windows), feature = "render"))]
fn detach_console() {}

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

const HELP: &str = "\
ORC-WAR — 고대 대규모 전투 시뮬레이터

  orc-war                        설정 화면을 열고 시작한다 (더블클릭과 같다)

설정 화면을 건너뛰고 곧장 전투로 들어가려면
  --field <전장>    plains hills mountain river forest siege  (기본 plains)
  --units <수>      양팀 합계 병력                            (기본 100000)
  --doctrine <편성> balanced infantry cavalry missile         (기본 balanced)
  --seed <수>       같은 씨앗이면 같은 전투                   (기본 1)

  전장·편성은 한글 이름(평지 언덕 산악 도하 삼림 공성 / 균형 보병 기병 사수)
  으로도 됩니다. 다만 명령 프롬프트의 코드페이지에 따라 한글 인자가 깨지는
  일이 있으니, 확실히 하려면 위의 영문 이름을 쓰세요.

콘솔 도구
  --bench [-n 유닛] [-t 틱] [-r 반복] [-q]   성능 측정
  --selftest                                 자체 점검
  --matchup <병종A> <수A> <병종B> <수B> [간격] [씨앗] [지형]
  --siege <공격> <수비> [씨앗] [nomoat] [최대틱]
  --snapshot [유닛] [씨앗] [폴더] [지형]     전장 이미지 굽기
  --help                                     이 도움말

병종 번호: 0 검방보병 1 장창병 2 중갑도끼 3 궁수 4 석궁수
           5 경기병 6 중기병 7 궁기병";

fn parse_field(s: &str) -> Option<Battlefield> {
    Some(match s {
        "평지" | "plains" | "field" => Battlefield::Plains,
        "언덕" | "hills" => Battlefield::Hills,
        "산악" | "mountain" => Battlefield::Mountain,
        "도하" | "river" => Battlefield::River,
        "삼림" | "forest" => Battlefield::Forest,
        "공성" | "siege" => Battlefield::Siege,
        _ => return None,
    })
}

fn parse_doctrine(s: &str) -> Option<Doctrine> {
    Some(match s {
        "균형" | "balanced" => Doctrine::Balanced,
        "보병" | "infantry" => Doctrine::Infantry,
        "기병" | "cavalry" => Doctrine::Cavalry,
        "사수" | "missile" => Doctrine::Missile,
        _ => return None,
    })
}

fn main() {
    setup_console();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let bare = argv.is_empty();

    // --- 콘솔 도구 ---
    let sub = argv.first().map(|s| s.as_str()).unwrap_or("");
    match sub {
        "--help" | "-h" => {
            println!("{HELP}");
            if bare {
                hold_window_open();
            }
            return;
        }
        "--matchup" => return orc_war::tools::matchup::run(&argv[1..]),
        "--siege" => return orc_war::tools::siege::run(&argv[1..]),
        "--snapshot" => {
            if let Err(e) = orc_war::tools::snapshot::run(&argv[1..]) {
                eprintln!("스냅샷 실패: {e}");
            }
            return;
        }
        "--selftest" => {
            // 종료 코드: 0 전부 통과 · 2 성능만 미달 · 1 동작이 어긋남.
            // 코어가 적은 CI 러너에서 성능 미달과 진짜 회귀를 갈라 보기 위한 것이다.
            let verdict = orc_war::tools::selftest::run(&argv[1..]);
            hold_window_open();
            std::process::exit(verdict.exit_code());
        }
        _ => {}
    }
    if argv
        .iter()
        .any(|a| a == "--bench" || a == "--headless" || a == "-r" || a == "--repeat")
    {
        orc_war::tools::bench::run(&argv);
        return;
    }

    // --- 창 모드 ---
    let mut cfg = BattleConfig::default();
    let mut explicit = false;
    let mut i = 0;
    while i < argv.len() {
        let next = || argv.get(i + 1).cloned().unwrap_or_default();
        match argv[i].as_str() {
            "--field" => {
                if let Some(f) = parse_field(&next()) {
                    cfg.field = f;
                    explicit = true;
                }
                i += 1;
            }
            "--doctrine" => {
                if let Some(d) = parse_doctrine(&next()) {
                    cfg.doctrine = d;
                    explicit = true;
                }
                i += 1;
            }
            "--units" | "-n" => {
                if let Ok(n) = next().parse::<u32>() {
                    cfg.total = n.clamp(200, 1_000_000);
                    explicit = true;
                }
                i += 1;
            }
            "--seed" | "-s" => {
                if let Ok(n) = next().parse::<u64>() {
                    cfg.seed = n;
                    explicit = true;
                }
                i += 1;
            }
            other => eprintln!("알 수 없는 인자: {other}"),
        }
        i += 1;
    }
    let _ = ARMY_SIZES;
    run_window(if explicit { Some(cfg) } else { None });
}

#[cfg(feature = "render")]
fn run_window(start: Option<BattleConfig>) {
    detach_console();
    orc_war::app::run::main_loop(start);
}

#[cfg(not(feature = "render"))]
fn run_window(_start: Option<BattleConfig>) {
    eprintln!("이 빌드에는 화면이 들어 있지 않습니다 (--no-default-features).");
    eprintln!("{HELP}");
}
