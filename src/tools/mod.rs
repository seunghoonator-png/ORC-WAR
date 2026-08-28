//! 콘솔 도구들.
//!
//! 하나의 실행파일이 창도 띄우고 콘솔 측정도 하도록, 예전에 따로 있던 exe 들의
//! 알맹이를 여기로 옮겼다. 유저가 받는 것은 `orc-war.exe` 하나다.

pub mod bench;
pub mod matchup;
pub mod selftest;
pub mod siege;
pub mod snapshot;
