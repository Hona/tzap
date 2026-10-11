//! Separate numeric-only SDK export from explicitly approximate export.
use crate::{
    angle::Angle,
    circuit::{Circuit, Gate},
    qasm::NumericExport,
};

fn replace_angle(gate: &Gate, angle: Angle) -> Gate {
    match gate {
        Gate::p(_, q) => Gate::p(angle, *q),
        Gate::rx(_, q) => Gate::rx(angle, *q),
        Gate::ry(_, q) => Gate::ry(angle, *q),
        Gate::rz(_, q) => Gate::rz(angle, *q),
        Gate::cp {
            control, target, ..
        } => Gate::cp {
            lambda: angle,
            control: *control,
            target: *target,
        },
        Gate::crx {
            control, target, ..
        } => Gate::crx {
            theta: angle,
            control: *control,
            target: *target,
        },
        Gate::cry {
            control, target, ..
        } => Gate::cry {
            theta: angle,
            control: *control,
            target: *target,
        },
        Gate::crz {
            control, target, ..
        } => Gate::crz {
            theta: angle,
            control: *control,
            target: *target,
        },
        _ => unreachable!("caller checks gate.angle()"),
    }
}

fn exact_gate(gate: &Gate, out: &mut Circuit) -> Result<(), String> {
    let Some(angle) = gate.angle() else {
        out.apply(gate.clone());
        return Ok(());
    };
    let (pi, _) = angle
        .components()
        .ok_or("preserved angle expression cannot be exported to a numeric SDK")?;
    if pi.numerator() == 0 {
        out.apply(gate.clone());
        return Ok(());
    }
    match gate {
        Gate::p(_, q) | Gate::rx(_, q) | Gate::ry(_, q) | Gate::rz(_, q) => {
            if pi.quarter_turns().is_none() {
                return Err("symbolic angle cannot be exported exactly to a numeric SDK; request certified rotation synthesis explicitly".into());
            }
            if matches!(gate, Gate::ry(..)) {
                out.apply(Gate::sdg(*q));
            }
            if matches!(gate, Gate::rx(..) | Gate::ry(..)) {
                out.apply(Gate::h(*q));
            }
            // Splits the mathematical-pi contribution from the literal float.
            // Only standalone rotations are normalized up to global phase.
            angle.emit(out, *q);
            if matches!(gate, Gate::rx(..) | Gate::ry(..)) {
                out.apply(Gate::h(*q));
            }
            if matches!(gate, Gate::ry(..)) {
                out.apply(Gate::s(*q));
            }
        }
        _ => {
            // Expand before projective normalization: CR*(2*pi) is not identity.
            let terms = crate::decompose::expand_controlled_rotation(gate)
                .map_err(|e| format!("controlled angle cannot be exported exactly: {e}"))?
                .ok_or("unsupported angle-bearing gate")?;
            for term in &terms {
                exact_gate(term, out)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn sdk(circuit: &Circuit) -> Result<String, String> {
    let mut out = circuit.empty_like();
    for (index, gate) in circuit.gates.iter().enumerate() {
        exact_gate(gate, &mut out).map_err(|e| format!("gate {index}: {e}"))?;
    }
    Ok(crate::qasm::serialize(&out))
}

pub(crate) fn lossy(circuit: &Circuit) -> Result<NumericExport, String> {
    let mut out = circuit.empty_like();
    let mut conversions = 0;
    for (index, gate) in circuit.gates.iter().enumerate() {
        let Some(angle) = gate.angle() else {
            out.apply(gate.clone());
            continue;
        };
        let (pi, _) = angle.components().ok_or_else(|| {
            format!("gate {index}: preserved angle expression cannot be exported numerically")
        })?;
        if pi.numerator() == 0 {
            out.apply(gate.clone());
        } else {
            // Prefer an exact named-gate expansion if one is available.
            let mut exact = circuit.empty_like();
            if exact_gate(gate, &mut exact).is_ok() {
                out.gates.extend(exact.gates);
            } else {
                let value = angle
                    .to_f64_lossy()
                    .map_err(|e| format!("gate {index}: {e}"))?;
                out.apply(replace_angle(
                    gate,
                    Angle::from_f64(value).map_err(|e| format!("gate {index}: {e}"))?,
                ));
                conversions += 1;
            }
        }
    }
    Ok(NumericExport {
        qasm: crate::qasm::serialize(&out),
        uncertified_conversions: conversions,
    })
}
