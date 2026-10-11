//! Compare sequential canonical merging, balanced batching, and the sparse map.
use std::{hint::black_box, time::Instant};
use tzap::symbolic_angle::{Angle, AngleContext, Limits, PhaseAccumulator, RotationPeriod};

fn balanced(ctx: &mut AngleContext, terms: &[Angle]) -> Angle {
    if terms.len() == 1 {
        return terms[0].clone();
    }
    let half = terms.len() / 2;
    let left = balanced(ctx, &terms[..half]);
    let right = balanced(ctx, &terms[half..]);
    ctx.add(&left, &right).unwrap()
}
fn measure(mut operation: impl FnMut(), count: usize) -> f64 {
    operation();
    let mut samples = [0.0; 7];
    for sample in &mut samples {
        let start = Instant::now();
        for _ in 0..count {
            operation();
        }
        *sample = start.elapsed().as_secs_f64() * 1e6 / count as f64;
    }
    samples.sort_by(f64::total_cmp);
    samples[3]
}
fn main() {
    let limits = Limits {
        max_terms: 4096,
        max_nodes: 32768,
        max_work: 100_000_000,
        ..Default::default()
    };
    println!("Median of seven samples, microseconds per complete fold including finalization");
    for repeated in [false, true] {
        for n in [16, 64, 256, 1024] {
            let mut ctx = AngleContext::with_limits(limits);
            let terms: Vec<_> = (1..=n)
                .map(|i| {
                    let arg = ctx.rational(if repeated { 1 } else { i }, 1).unwrap();
                    ctx.sin(&arg).unwrap()
                })
                .collect();
            let mut sequential_ctx = AngleContext::with_limits(limits);
            let sequential = measure(
                || {
                    let mut sum = Angle::ZERO;
                    for term in &terms {
                        sum = sequential_ctx.add(&sum, term).unwrap();
                    }
                    black_box(sum);
                },
                10,
            );
            let mut balanced_ctx = AngleContext::with_limits(limits);
            let batch = measure(
                || {
                    black_box(balanced(&mut balanced_ctx, &terms));
                },
                10,
            );
            let mut sparse_ctx = AngleContext::with_limits(limits);
            let sparse = measure(
                || {
                    let mut sum = PhaseAccumulator::new(RotationPeriod::TwoPi, limits);
                    for term in &terms {
                        sum.add(&mut sparse_ctx, term).unwrap();
                    }
                    black_box(sum.finish(&mut sparse_ctx).unwrap());
                },
                10,
            );
            // All inputs are sin of integer radians: no period reductions in these folds.
            let expected = balanced(&mut ctx, &terms);
            let mut actual = PhaseAccumulator::new(RotationPeriod::TwoPi, limits);
            for term in &terms {
                actual.add(&mut ctx, term).unwrap();
            }
            assert_eq!(actual.finish(&mut ctx).unwrap(), expected);
            println!(
                "{} n={n:4}: sequential={sequential:10.2} balanced={batch:10.2} sparse={sparse:10.2}",
                if repeated { "repeated" } else { "unique  " }
            );
        }
    }
}
