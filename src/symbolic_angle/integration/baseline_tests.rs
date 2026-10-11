use crate::{
    circuit::Gate,
    decompose::DecomposeRotations,
    optimize::{Level, Options, optimize},
    qasm,
    symbolic_angle::{AngleContext, integration::from_legacy},
    unitary::circuits_equiv,
};

const SOURCE: &str = include_str!("../../../tests/fixtures/symbolic-angle-baseline.qasm");

#[test]
fn native_fixture_converts_each_accepted_literal_without_period_reduction() {
    let circuit = qasm::parse(SOURCE).unwrap();
    let mut ctx = AngleContext::default();
    let mut families = [0; 8];
    for gate in &circuit.gates {
        let index = match gate {
            Gate::p(..) => 0,
            Gate::rx(..) => 1,
            Gate::ry(..) => 2,
            Gate::rz(..) => 3,
            Gate::cp { .. } => 4,
            Gate::crx { .. } => 5,
            Gate::cry { .. } => 6,
            Gate::crz { .. } => 7,
            _ => continue,
        };
        families[index] += 1;
        let old = gate.angle().unwrap();
        let (pi, residual) = old.components().unwrap();
        let angle = from_legacy(&mut ctx, old).unwrap();
        let coefficient = ctx.pi_fraction(pi.numerator(), pi.denominator()).unwrap();
        let numeric = ctx.sub(&angle, &coefficient).unwrap();
        assert_eq!(
            numeric.as_rational().unwrap().to_big_rational(),
            binary(residual.get())
        );
    }
    assert_eq!(families, [1, 2, 1, 1, 2, 1, 1, 2]);
}

#[test]
fn native_fixture_keeps_current_optimization_synthesis_and_sdk_guarantees() {
    let input = qasm::parse(SOURCE).unwrap();
    for parallel in [false, true] {
        let (output, report) = optimize(
            &input,
            &Options {
                level: Level::O1,
                parallel,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(report.numerical.certified_error_bound, Some(0.0));
        assert!(circuits_equiv(&input, &output, 1e-12));
        let round_trip = qasm::parse(&output.to_qasm()).unwrap();
        assert!(circuits_equiv(&input, &round_trip, 1e-12));
        let sdk = qasm::parse(&qasm::serialize_sdk(&output).unwrap()).unwrap();
        assert!(circuits_equiv(&input, &sdk, 1e-12));
    }
    let output = DecomposeRotations { epsilon: 1e-10 }
        .try_run(&input)
        .unwrap();
    assert!(output.gates.iter().all(|gate| gate.angle().is_none()));
    // Numerical comparison supplements the independent certificate checks in synthesis tests.
    assert!(circuits_equiv(&input, &output, 2e-9));
}

// Independent binary64 decoding, without arithmetic on the input float.
fn binary(value: f64) -> num_rational::BigRational {
    use num_bigint::BigInt;
    use num_rational::BigRational;
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 2047) as i32;
    let mantissa = bits & ((1 << 52) - 1);
    let significand = if exponent == 0 {
        mantissa
    } else {
        mantissa | (1 << 52)
    };
    let power = if exponent == 0 {
        -1074
    } else {
        exponent - 1075
    };
    let signed = BigInt::from(significand) * if bits >> 63 == 0 { 1 } else { -1 };
    if power >= 0 {
        BigRational::from_integer(signed << power as usize)
    } else {
        BigRational::new(signed, BigInt::from(1) << (-power) as usize)
    }
}
