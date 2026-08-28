//! Windows 실행파일에 아이콘을 박는다.
//!
//! 리소스 크레이트를 끌어오는 대신 mingw 의 `windres` 를 직접 부른다. 크로스
//! 빌드에서만 필요한 일이라 의존성을 하나 더 늘릴 만한 값어치가 없다.
//! `windres` 가 없으면 아이콘 없이 그냥 넘어간다 — 빌드가 막히면 안 된다.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/orc-war.rc");
    println!("cargo:rerun-if-changed=assets/orc-war.ico");

    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let obj = out.join("orc-war-icon.o");

    // 툴체인마다 이름이 다르다. 있는 것을 쓴다
    let candidates = [
        "x86_64-w64-mingw32-windres",
        "x86_64-w64-mingw32ucrt-windres",
        "llvm-windres",
        "windres",
    ];
    for exe in candidates {
        let ok = Command::new(exe)
            .args(["assets/orc-war.rc", "-O", "coff", "-o"])
            .arg(&obj)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            println!("cargo:rustc-link-arg-bins={}", obj.display());
            return;
        }
    }
    println!("cargo:warning=windres 를 찾지 못해 아이콘 없이 빌드합니다");
}
