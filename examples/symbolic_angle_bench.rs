//! Run with `cargo run --release --example symbolic_angle_bench`.
//! Microbenchmarks exercise the standalone scalar API, with no circuit passes.
use std::{
    hash::{Hash, Hasher},
    hint::black_box,
    time::Instant,
};
use tzap::{
    angle::Angle as Legacy,
    symbolic_angle::{Angle, AngleContext, RotationPeriod},
};

fn bench(mut name: &str, iterations: usize, mut operation: impl FnMut()) {
    for _ in 0..1000 {
        operation();
    }
    let mut samples = [0.0; 7];
    for sample in &mut samples {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        *sample = start.elapsed().as_nanos() as f64 / iterations as f64;
    }
    samples.sort_by(f64::total_cmp);
    if name.len() > 36 {
        name = &name[..36];
    }
    println!(
        "{name:36} {:9.2} ns/op [{:.2}, {:.2}]",
        samples[3], samples[0], samples[6]
    );
}
fn main() {
    let count = 100_000;
    println!("Median of 7 samples; range in brackets; {count} iterations/sample");
    println!("Angle size: {} bytes", std::mem::size_of::<Angle>());
    let mut ctx = AngleContext::default();
    bench("core construct pi/7", count, || {
        black_box(ctx.pi_fraction(black_box(1), black_box(7)).unwrap());
    });
    bench("legacy construct pi/7", count, || {
        black_box(Legacy::pi_fraction(black_box(1), black_box(7)).unwrap());
    });
    for (d, label) in [(7, "pi/7 + 2pi/7"), (4, "pi/4 + 2pi/4")] {
        let a = ctx.pi_fraction(1, d).unwrap();
        let b = ctx.pi_fraction(2, d).unwrap();
        let x = Legacy::pi_fraction(1, d).unwrap();
        let y = Legacy::pi_fraction(2, d).unwrap();
        bench(&format!("core {label}"), count, || {
            black_box(ctx.add(black_box(&a), black_box(&b)).unwrap());
        });
        bench(&format!("legacy {label}"), count, || {
            black_box(x.checked_add_signed(1, black_box(&y)).unwrap());
        });
    }
    let quarter = ctx.pi_fraction(1, 4).unwrap();
    let old_quarter = Legacy::pi_fraction(1, 4).unwrap();
    bench("core quarter-turn classification", count, || {
        black_box(black_box(&quarter).quarter_turns(RotationPeriod::TwoPi));
    });
    bench("legacy quarter-turn classification", count, || {
        black_box(black_box(&old_quarter).quarter_turns());
    });
    let a = Angle::from_f64(0.25).unwrap();
    let b = Angle::from_f64(0.5).unwrap();
    let x = Legacy::from_f64(0.25).unwrap();
    let y = Legacy::from_f64(0.5).unwrap();
    bench("core exact binary64 add", count, || {
        black_box(ctx.add(black_box(&a), black_box(&b)).unwrap());
    });
    bench("legacy exact binary64 add", count, || {
        black_box(x.checked_add_signed(1, black_box(&y)).unwrap());
    });
    let arg = ctx.pi_fraction(1, 7).unwrap();
    let live = ctx.sin(&arg).unwrap();
    bench("core interned sin(pi/7) hit", count, || {
        black_box(ctx.sin(black_box(&arg)).unwrap());
    });
    let negative = ctx.neg(&live).unwrap();
    bench("core symbolic cancellation", count, || {
        black_box(ctx.add(black_box(&live), black_box(&negative)).unwrap());
    });
    let arg2 = ctx.rational(1, 3).unwrap();
    bench("core create/drop sin(1/3)", count, || {
        black_box(ctx.sin(black_box(&arg2)).unwrap());
    });
    let big = ctx.parse("1e100+1/7").unwrap();
    let one = Angle::ONE;
    bench(
        &format!("core {}-bit rational add", big.coefficient_bits()),
        10_000,
        || {
            black_box(ctx.add(black_box(&big), black_box(&one)).unwrap());
        },
    );
    let dyadic = Angle::from_f64(0.1).unwrap();
    bench("core negate binary64 0.1", count, || {
        black_box(ctx.neg(black_box(&dyadic)).unwrap());
    });
    bench("core halve binary64 0.1", count, || {
        black_box(ctx.half(black_box(&dyadic)).unwrap());
    });
    let subnormal = Angle::from_f64(f64::from_bits(1)).unwrap();
    bench("core add minimum subnormals", count, || {
        black_box(
            ctx.add(black_box(&subnormal), black_box(&subnormal))
                .unwrap(),
        );
    });
    bench("core parse decimal 0.125", count, || {
        black_box(ctx.decimal(black_box("0.125")).unwrap());
    });
    bench("core parse sin(pi/7) cache hit", count, || {
        black_box(ctx.parse(black_box("sin(pi/7)")).unwrap());
    });
    let big_coefficient = big.as_rational().unwrap();
    bench("core hash 336-bit coefficient", count, || {
        let mut hasher = rustc_hash::FxHasher::default();
        black_box(&big_coefficient).hash(&mut hasher);
        black_box(hasher.finish());
    });
    println!("Interner: {:?}", ctx.stats());
}
