//! The harness also builds against main: it uses only the existing circuit API.
use std::{hint::black_box, time::Instant};
use tzap::{decompose::DecomposeRotations, qasm};

fn measure(name: &str, mut operation: impl FnMut(), count: usize) {
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
    println!("{name}: {:.2} us/op", samples[3]);
}
fn main() {
    let source = include_str!("../tests/fixtures/symbolic-angle-baseline.qasm");
    let circuit = qasm::parse(source).unwrap();
    measure(
        "parse native fixture",
        || {
            black_box(qasm::parse(source).unwrap());
        },
        1000,
    );
    measure(
        "SDK export native fixture",
        || {
            black_box(qasm::serialize_sdk(&circuit).unwrap());
        },
        1000,
    );
    let pass = DecomposeRotations { epsilon: 1e-10 };
    measure(
        "certified synthesis warm cache",
        || {
            black_box(pass.try_run(&circuit).unwrap());
        },
        10,
    );
}
