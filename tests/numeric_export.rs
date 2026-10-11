use tzap::{
    angle::Angle,
    circuit::{Circuit, Gate},
    qasm,
};

fn gates(angle: &Angle) -> Vec<Gate> {
    vec![
        Gate::p(angle.clone(), 1),
        Gate::rx(angle.clone(), 1),
        Gate::ry(angle.clone(), 1),
        Gate::rz(angle.clone(), 1),
        Gate::cp {
            lambda: angle.clone(),
            control: 0,
            target: 1,
        },
        Gate::crx {
            theta: angle.clone(),
            control: 0,
            target: 1,
        },
        Gate::cry {
            theta: angle.clone(),
            control: 0,
            target: 1,
        },
        Gate::crz {
            theta: angle.clone(),
            control: 0,
            target: 1,
        },
    ]
}

#[test]
fn every_angle_gate_is_numeric_with_accurate_conversion_counts() {
    for source in ["pi/7", "(pi/7)+(1e17)", "-(pi/7)"] {
        let angle = qasm::parse(&format!("qreg q[1]; rz({source}) q[0];"))
            .unwrap()
            .gates[0]
            .angle()
            .unwrap()
            .clone();
        for gate in gates(&angle) {
            let mut input = Circuit::with_cbits(2, 3);
            input.gates = vec![gate.clone(), Gate::measure { qubit: 1, cbit: 2 }];
            let output = qasm::serialize_numeric_lossy(&input).unwrap();
            assert!(!output.qasm.contains("pi"), "{gate:?}");
            assert_eq!(output.uncertified_conversions, 1, "{gate:?}");
            let parsed = qasm::parse(&output.qasm).unwrap();
            assert_eq!((parsed.num_qubits, parsed.num_cbits), (2, 3));
            assert_eq!(parsed.gates.last(), input.gates.last());
            for g in parsed.gates {
                if let Some(a) = g.angle() {
                    assert_eq!(a.components().unwrap().0.numerator(), 0);
                }
            }
            assert!(qasm::serialize_sdk(&input).unwrap_err().contains("gate 0"));
        }
    }
}

#[test]
fn named_expansions_are_not_uncertified_conversions() {
    for gate in gates(&Angle::pi_fraction(2, 1).unwrap()) {
        let input = Circuit {
            num_qubits: 2,
            num_cbits: 0,
            gates: vec![gate],
        };
        let export = qasm::serialize_numeric_lossy(&input).unwrap();
        assert_eq!(export.uncertified_conversions, 0);
        assert!(!export.qasm.contains("pi"));
        let exact = qasm::serialize_sdk(&input).unwrap();
        assert!(!exact.contains("pi"));
    }
}

#[test]
fn floats_remain_literal_and_preserved_expressions_are_rejected() {
    for value in [0.0, f64::from_bits(1), -f64::from_bits(1), 1e17, f64::MAX] {
        let input = Circuit {
            num_qubits: 2,
            num_cbits: 0,
            gates: gates(&Angle::from_f64(value).unwrap()),
        };
        let output = qasm::serialize_numeric_lossy(&input).unwrap();
        assert_eq!(output.uncertified_conversions, 0);
        let parsed = qasm::parse(&output.qasm).unwrap();
        for gate in parsed.gates {
            if let Some(a) = gate.angle() {
                assert_eq!(a.components().unwrap().1.get().to_bits(), value.to_bits());
            }
        }
        assert_eq!(qasm::serialize_sdk(&input).unwrap(), input.to_qasm());
    }
    let preserved = qasm::parse("qreg q[1]; rz(pi/9223372036854775808) q[0];")
        .unwrap()
        .gates[0]
        .angle()
        .unwrap()
        .clone();
    for gate in gates(&preserved) {
        let input = Circuit {
            num_qubits: 2,
            num_cbits: 0,
            gates: vec![Gate::h(0), gate],
        };
        for error in [
            qasm::serialize_sdk(&input).unwrap_err(),
            qasm::serialize_numeric_lossy(&input).unwrap_err(),
        ] {
            assert!(error.contains("gate 1") && error.contains("preserved"));
        }
    }
}

#[test]
fn sdk_export_splits_large_mixed_angles_without_rounding() {
    for numeric in [1e17, -1e17, f64::MAX] {
        let angle = Angle::pi_fraction(1, 4)
            .unwrap()
            .checked_add_signed(1, &Angle::from_f64(numeric).unwrap())
            .unwrap();
        for gate in gates(&angle).into_iter().take(4) {
            let input = Circuit {
                num_qubits: 2,
                num_cbits: 0,
                gates: vec![gate],
            };
            let output = qasm::serialize_sdk(&input).unwrap();
            assert!(output.contains("t q[1]"));
            assert!(!output.contains("pi"));
            let parsed = qasm::parse(&output).unwrap();
            let angles: Vec<_> = parsed.gates.iter().filter_map(Gate::angle).collect();
            assert_eq!(angles.len(), 1);
            assert_eq!(
                angles[0].components().unwrap().1.get().to_bits(),
                numeric.to_bits()
            );
            assert_eq!(
                qasm::serialize_numeric_lossy(&input)
                    .unwrap()
                    .uncertified_conversions,
                0
            );
        }
    }
}
