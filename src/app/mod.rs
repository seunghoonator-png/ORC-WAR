//! 화면 세 개와 그 사이의 흐름.
//!
//! 설정 → 전투 → 결과 → 설정. 유저가 개입하는 곳은 처음과 끝뿐이고, 전투가
//! 시작되면 보는 일만 남는다 — 그것이 이 시뮬레이터의 전제다.
//!
//! 창과 입력은 `bin/orc-war` 가 맡는다. 여기 있는 것은 **화소 버퍼에 그리는 일과
//! 그리기에 필요한 상태**뿐이라, 창이 없는 환경에서도 화면을 그대로 구워
//! 확인할 수 있다.

pub mod report;
#[cfg(feature = "render")]
pub mod run;
pub mod setup;

/// 화면이 받는 입력. minifb 의 키를 여기까지 끌고 오지 않는다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    /// 왼쪽/오른쪽을 크게 — Shift 를 누른 채
    LeftFast,
    RightFast,
    Enter,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Setup,
    Battle,
    Report,
}
