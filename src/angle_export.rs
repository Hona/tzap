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
    if angle.to_f64_exact().is_some() {
        out.apply(gate.clone());
        return Ok(());
    }
    // A folded SDK input can exceed 53 significant bits. Emit an exact sum
    // of binary64 parameters, rather than rounding the result back to one.
    if let Some(rational) = angle.as_rational() {
        for piece in binary64_parts(&rational)? {
            out.apply(replace_angle(gate, piece));
        }
        return Ok(());
    }
    let (pi, residual) = angle.affine_coefficients().ok_or("symbolic angle cannot be exported exactly to a numeric SDK; request certified rotation synthesis explicitly")?;
    let mut ctx = crate::angle::AngleContext::default();
    let pi = ctx
        .big_pi_fraction(
            pi.to_big_rational().numer().clone(),
            pi.to_big_rational().denom().clone(),
        )
        .map_err(|e| e.to_string())?;
    match gate {
        Gate::p(_, q) | Gate::rx(_, q) | Gate::ry(_, q) | Gate::rz(_, q) => {
            if pi.projective_quarters().is_none() {
                return Err("symbolic angle cannot be exported exactly to a numeric SDK; request certified rotation synthesis explicitly".into());
            }
            let pieces = binary64_parts(&residual)?;
            if matches!(gate, Gate::ry(..)) {
                out.apply(Gate::sdg(*q));
            }
            if matches!(gate, Gate::rx(..) | Gate::ry(..)) {
                out.apply(Gate::h(*q));
            }
            pi.emit(out, *q);
            for piece in pieces {
                out.apply(Gate::rz(piece, *q));
            }
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

fn binary64_parts(r: &crate::symbolic_angle::Rational) -> Result<Vec<Angle>, String> {
    use num_traits::{Signed, ToPrimitive, Zero};
    let mut r = r.to_big_rational();
    if r.denom().trailing_zeros() != Some(r.denom().bits() - 1) {
        return Err("angle is not an exact sum of binary64 values; request certified rotation synthesis explicitly".into());
    }
    let minimum = crate::symbolic_angle::Rational::from_bits(1).to_big_rational();
    let mut pieces = Vec::new();
    for _ in 0..64 {
        if r.is_zero() {
            return Ok(pieces);
        }
        if r.abs() < minimum {
            break;
        }
        let value = r.to_f64().ok_or("numeric SDK magnitude limit")?;
        let value = if value.is_infinite() {
            if r.is_negative() { -f64::MAX } else { f64::MAX }
        } else {
            value
        };
        if value == 0.0 {
            break;
        }
        r -= crate::symbolic_angle::Rational::from_bits(value.to_bits()).to_big_rational();
        pieces.push(Angle::from_f64(value).map_err(|e| e.to_string())?);
    }
    Err("exact numeric SDK expansion exceeds the resource or subnormal limit".into())
}

pub(crate) fn sdk(circuit: &Circuit) -> Result<String, String> {
    let mut out = circuit.empty_like();
    for (index, gate) in circuit.gates.iter().enumerate() {
        exact_gate(gate, &mut out).map_err(|e| format!("gate {index}: {e}"))?;
    }
    Ok(crate::qasm::serialize_numeric(&out))
}

pub(crate) fn lossy(circuit: &Circuit) -> Result<NumericExport, String> {
    let mut out = circuit.empty_like();
    let mut conversions = 0;
    for (index, gate) in circuit.gates.iter().enumerate() {
        let Some(angle) = gate.angle() else {
            out.apply(gate.clone());
            continue;
        };
        let mut exact = circuit.empty_like();
        if exact_gate(gate, &mut exact).is_ok() {
            out.gates.extend(exact.gates);
        } else {
            let value = angle
                .to_f64_lossy()
                .map_err(|e| format!("gate {index}: {e}"))?;
            out.apply(replace_angle(
                gate,
                Angle::from_f64(value).map_err(|e| e.to_string())?,
            ));
            conversions += 1;
        }
    }
    Ok(NumericExport {
        qasm: crate::qasm::serialize_numeric(&out),
        uncertified_conversions: conversions,
    })
}
