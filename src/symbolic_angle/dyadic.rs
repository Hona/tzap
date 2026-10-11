//! Exact integer operations on finite binary64 encodings. These paths never
//! perform rounded floating-point arithmetic; failure requests rational promotion.
use super::{Angle, Value};

#[inline]
fn parts(bits: u64) -> (i128, i32) {
    let exponent = ((bits >> 52) & 2047) as i32;
    let significand = (bits & ((1 << 52) - 1)) | if exponent == 0 { 0 } else { 1 << 52 };
    let n = if bits >> 63 == 0 {
        significand as i128
    } else {
        -(significand as i128)
    };
    (
        n,
        if exponent == 0 {
            -1074
        } else {
            exponent - 1075
        },
    )
}
#[inline]
fn encode(n: i128, mut power: i32) -> Option<Angle> {
    if n == 0 {
        return Some(Angle::ZERO);
    }
    let sign = if n < 0 { 1_u64 << 63 } else { 0 };
    let magnitude = n.unsigned_abs();
    let trailing = magnitude.trailing_zeros();
    let magnitude = magnitude >> trailing;
    power += trailing as i32;
    let digits = (128 - magnitude.leading_zeros()) as i32;
    let highest = power + digits - 1;
    if digits > 53 || highest > 1023 || power < -1074 {
        return None;
    }
    let magnitude = magnitude as u64;
    let bits = if highest >= -1022 {
        let significand = magnitude << (53 - digits);
        sign | (((highest + 1023) as u64) << 52) | (significand & ((1 << 52) - 1))
    } else {
        sign | (magnitude << (power + 1074))
    };
    Some(Angle(Value::Dyadic(bits)))
}
#[inline]
pub(super) fn add(a: u64, b: u64) -> Option<Angle> {
    let (a, ap) = parts(a);
    let (b, bp) = parts(b);
    let power = ap.min(bp);
    let (ashift, bshift) = ((ap - power) as u32, (bp - power) as u32);
    // Each input has at most 53 significand bits, so both shifts and their
    // signed sum fit comfortably in i128. Larger gaps use arbitrary precision.
    if ashift > 64 || bshift > 64 {
        return None;
    }
    encode((a << ashift) + (b << bshift), power)
}
#[inline]
pub(super) fn scale(bits: u64, power: i32, negative: bool) -> Option<Angle> {
    if power == 0 {
        return Some(Angle(Value::Dyadic(
            bits ^ if negative { 1_u64 << 63 } else { 0 },
        )));
    }
    let (n, p) = parts(bits);
    encode(if negative { -n } else { n }, p + power)
}
