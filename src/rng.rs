//! 결정론 난수 — 상태 없는 해시 기반.
//!
//! 스레드 실행 순서와 무관하게 같은 (씨앗, 유닛, 틱)이면 같은 값이 나와야 하므로
//! 난수 "스트림"을 두지 않고 매번 좌표를 해싱한다.

#[inline(always)]
pub fn hash3(a: u64, b: u64, c: u64) -> u64 {
    let mut x = a.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ b.wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ c.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// 0.0 ..= 1.0 균등 분포.
#[inline(always)]
pub fn unit_f32(a: u64, b: u64, c: u64) -> f32 {
    (hash3(a, b, c) >> 40) as f32 / 16_777_216.0
}

/// -1.0 ..= 1.0 균등 분포.
#[inline(always)]
pub fn signed_f32(a: u64, b: u64, c: u64) -> f32 {
    unit_f32(a, b, c) * 2.0 - 1.0
}

/// 0..n 균등 정수.
#[inline(always)]
pub fn below(a: u64, b: u64, c: u64, n: u32) -> u32 {
    if n == 0 {
        return 0;
    }
    (hash3(a, b, c) % n as u64) as u32
}
