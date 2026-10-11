use super::*;
use crate::qasm;

#[test]
fn identities_and_inverses_use_gate_periods() {
    let mut ctx = AngleContext::default();
    let two_pi = ctx.pi_fraction(2, 1).unwrap();
    let four_pi = ctx.pi_fraction(4, 1).unwrap();
    for family in [
        GateFamily::P,
        GateFamily::Rx,
        GateFamily::Ry,
        GateFamily::Rz,
        GateFamily::Cp,
        GateFamily::Crx,
        GateFamily::Cry,
        GateFamily::Crz,
    ] {
        for scope in [Scope::Literal, Scope::ProjectiveCircuit] {
            let semantics = GateSemantics { family, scope };
            let two_is_identity = semantics.period() == RotationPeriod::TwoPi;
            assert_eq!(
                semantics.prove_identity(&two_pi),
                if two_is_identity {
                    Proof::Proven
                } else {
                    Proof::Disproven
                }
            );
            assert_eq!(semantics.prove_identity(&four_pi), Proof::Proven);
            let pi = Angle::PI;
            assert_eq!(
                semantics.prove_inverse(&mut ctx, &pi, &pi).unwrap(),
                if two_is_identity {
                    Proof::Proven
                } else {
                    Proof::Disproven
                }
            );
            let unknown = ctx.parse("sin(100)").unwrap();
            assert_eq!(semantics.prove_identity(&unknown), Proof::Unknown);
        }
    }
    assert_ne!(two_pi, Angle::ZERO);
    assert_ne!(four_pi, Angle::ZERO);
}

#[test]
fn legacy_conversion_keeps_literal_coefficients_and_dyadics() {
    let mut ctx = AngleContext::default();
    for n in [-17, -8, -1, 0, 1, 8, 17] {
        for d in [1, 4, 7, i64::MAX] {
            let old = crate::angle::Angle::pi_fraction(n, d).unwrap();
            assert_eq!(
                from_legacy(&mut ctx, &old).unwrap(),
                ctx.pi_fraction(n, d).unwrap()
            );
        }
    }
    for value in [
        0.0,
        -0.0,
        0.1,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::MAX,
    ] {
        let old = crate::angle::Angle::from_f64(value).unwrap();
        assert_eq!(
            from_legacy(&mut ctx, &old).unwrap(),
            Angle::from_f64(value).unwrap()
        );
    }
    for expression in ["2*pi+0.1", "4*pi-0.125", "pi*pi/4"] {
        let circuit = qasm::parse(&format!("qreg q[1]; rz({expression}) q[0];")).unwrap();
        let old = circuit.gates[0].angle().unwrap();
        let (p, r) = old.components().unwrap();
        let pi = ctx.pi_fraction(p.numerator(), p.denominator()).unwrap();
        let residual = Angle::from_f64(r.get()).unwrap();
        let expected = ctx.add(&pi, &residual).unwrap();
        assert_eq!(from_legacy(&mut ctx, old).unwrap(), expected);
    }
    let preserved = crate::angle::Angle::preserved("pi/100000000000000000000".into(), 1);
    assert_eq!(
        from_legacy(&mut ctx, &preserved),
        Err(IntegrationError::OpaqueLegacy)
    );
}

#[test]
fn conversion_and_inverse_respect_tiny_limits() {
    let mut ctx = AngleContext::with_limits(super::super::Limits {
        max_bits: 8,
        ..Default::default()
    });
    let legacy = crate::angle::Angle::from_f64(f64::from_bits(1)).unwrap();
    assert_eq!(
        from_legacy(&mut ctx, &legacy),
        Err(IntegrationError::Scalar(Error::Limit(
            super::super::Resource::Bits
        )))
    );
    let s = GateSemantics {
        family: GateFamily::Rz,
        scope: Scope::Literal,
    };
    let huge = Angle::from_f64(f64::MAX).unwrap();
    assert!(s.prove_inverse(&mut ctx, &huge, &huge).is_err());
}

#[test]
fn invalid_operands_and_unclassified_angles_never_lower_to_identity() {
    let mut ctx = AngleContext::default();
    let s = GateSemantics {
        family: GateFamily::Crz,
        scope: Scope::ProjectiveCircuit,
    };
    assert!(matches!(
        s.lower(&mut ctx, &Angle::ZERO, Operands::Single(0)),
        Err(IntegrationError::InvalidOperands)
    ));
    assert!(matches!(
        s.lower(
            &mut ctx,
            &Angle::ZERO,
            Operands::Controlled {
                control: 0,
                target: 0
            }
        ),
        Err(IntegrationError::InvalidOperands)
    ));
    let unknown = ctx.parse("sin(100)").unwrap();
    assert!(
        s.lower(
            &mut ctx,
            &unknown,
            Operands::Controlled {
                control: 1,
                target: 0
            }
        )
        .unwrap()
        .is_none()
    );
    let pi = ctx.pi_fraction(1, 4).unwrap();
    assert!(
        s.lower(
            &mut ctx,
            &pi,
            Operands::Controlled {
                control: 1,
                target: 0
            }
        )
        .unwrap()
        .is_none()
    );
}
