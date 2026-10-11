//! Preparatory gate adapters. Circuit storage and passes still use the legacy angle.
use super::{Angle, AngleContext, Error, Proof, RotationPeriod};
use crate::circuit::{Gate, Qubit};
use smallvec::SmallVec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateFamily {
    P,
    Rx,
    Ry,
    Rz,
    Cp,
    Crx,
    Cry,
    Crz,
}
impl GateFamily {
    pub fn controlled(self) -> bool {
        matches!(self, Self::Cp | Self::Crx | Self::Cry | Self::Crz)
    }
}

/// Literal operators may subsequently be controlled; projective circuits may not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Literal,
    ProjectiveCircuit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateSemantics {
    pub family: GateFamily,
    pub scope: Scope,
}
impl GateSemantics {
    pub fn period(self) -> RotationPeriod {
        match self.family {
            GateFamily::P | GateFamily::Cp => RotationPeriod::TwoPi,
            GateFamily::Crx | GateFamily::Cry | GateFamily::Crz => RotationPeriod::FourPi,
            _ if self.scope == Scope::Literal => RotationPeriod::FourPi,
            _ => RotationPeriod::TwoPi,
        }
    }
    pub fn prove_identity(self, angle: &Angle) -> Proof {
        angle.prove_identity(self.period())
    }
    pub fn quarter_turns(self, angle: &Angle) -> Option<u8> {
        angle.quarter_turns(self.period())
    }
    pub fn prove_inverse(
        self,
        ctx: &mut AngleContext,
        left: &Angle,
        right: &Angle,
    ) -> Result<Proof, Error> {
        Ok(self.prove_identity(&ctx.add(left, right)?))
    }

    /// Lower the supported quarter-turn fragment to named gates, retaining phase.
    /// `None` means no supported decomposition, never an identity proof.
    /// The original operator equals exp(i * global_phase) times the returned word.
    pub fn lower(
        self,
        ctx: &mut AngleContext,
        angle: &Angle,
        operands: Operands,
    ) -> Result<Option<Lowering>, IntegrationError> {
        let (target, control) = match operands {
            Operands::Single(target) if !self.family.controlled() => (target, None),
            Operands::Controlled { control, target }
                if self.family.controlled() && control != target =>
            {
                (target, Some(control))
            }
            _ => return Err(IntegrationError::InvalidOperands),
        };
        // Even in projective scope, compute the literal phase of the returned word.
        let period = if matches!(self.family, GateFamily::P | GateFamily::Cp) {
            RotationPeriod::TwoPi
        } else {
            RotationPeriod::FourPi
        };
        let Some(k) = angle.quarter_turns(period) else {
            return Ok(None);
        };
        let mut gates = SmallVec::new();
        let mut global_phase = Angle::ZERO;
        if let Some(control) = control {
            if k == 0 {
                return Ok(Some(Lowering {
                    gates,
                    global_phase,
                }));
            }
            if k == 8 {
                gates.push(Gate::z(control));
            } else if k % 2 != 0 {
                return Ok(None);
            } else {
                let half = k / 2;
                if self.family == GateFamily::Cp {
                    phase_word(&mut gates, half, control);
                }
                if self.family == GateFamily::Cry {
                    gates.push(Gate::sdg(target));
                }
                if matches!(self.family, GateFamily::Crx | GateFamily::Cry) {
                    gates.push(Gate::h(target));
                }
                phase_word(&mut gates, half, target);
                gates.push(Gate::cnot { control, target });
                phase_word(&mut gates, (8 - half) % 8, target);
                gates.push(Gate::cnot { control, target });
                if matches!(self.family, GateFamily::Crx | GateFamily::Cry) {
                    gates.push(Gate::h(target));
                }
                if self.family == GateFamily::Cry {
                    gates.push(Gate::s(target));
                }
            }
        } else {
            if self.family != GateFamily::P {
                global_phase = ctx.pi_fraction(-(k as i64), 8)?;
            }
            // An empty phase word needs no basis change, including the -I case.
            if k % 8 != 0 {
                if self.family == GateFamily::Ry {
                    gates.push(Gate::sdg(target));
                }
                if matches!(self.family, GateFamily::Rx | GateFamily::Ry) {
                    gates.push(Gate::h(target));
                }
                phase_word(&mut gates, k % 8, target);
                if matches!(self.family, GateFamily::Rx | GateFamily::Ry) {
                    gates.push(Gate::h(target));
                }
                if self.family == GateFamily::Ry {
                    gates.push(Gate::s(target));
                }
            }
        }
        Ok(Some(Lowering {
            gates,
            global_phase,
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operands {
    Single(Qubit),
    Controlled { control: Qubit, target: Qubit },
}

#[derive(Clone, Debug)]
pub struct Lowering {
    /// At most twelve named gates; the supported fragment needs no heap allocation.
    pub gates: SmallVec<[Gate; 12]>,
    /// Literal scalar phase in radians. Only projective callers may discard it.
    pub global_phase: Angle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntegrationError {
    Scalar(Error),
    OpaqueLegacy,
    InvalidOperands,
}
impl From<Error> for IntegrationError {
    fn from(error: Error) -> Self {
        Self::Scalar(error)
    }
}
impl std::fmt::Display for IntegrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scalar(error) => error.fmt(f),
            Self::OpaqueLegacy => {
                f.write_str("preserved source requires an explicit parsing policy")
            }
            Self::InvalidOperands => f.write_str("operands do not match the rotation family"),
        }
    }
}
impl std::error::Error for IntegrationError {}

/// Preserve the accepted literal pi coefficient and exact binary64 residual.
/// Opaque source expressions are never reparsed under an implicit input policy.
pub fn from_legacy(
    ctx: &mut AngleContext,
    legacy: &crate::angle::Angle,
) -> Result<Angle, IntegrationError> {
    let (pi, residual) = legacy.components().ok_or(IntegrationError::OpaqueLegacy)?;
    let residual = ctx.from_f64(residual.get())?;
    if pi.numerator() == 0 {
        return Ok(residual);
    }
    let pi = ctx.pi_fraction(pi.numerator(), pi.denominator())?;
    if residual.prove_zero() == Proof::Proven {
        return Ok(pi);
    }
    Ok(ctx.add(&pi, &residual)?)
}

fn phase_word(gates: &mut SmallVec<[Gate; 12]>, k: u8, q: Qubit) {
    match k {
        0 => {}
        1 => gates.push(Gate::t(q)),
        2 => gates.push(Gate::s(q)),
        3 => {
            gates.push(Gate::s(q));
            gates.push(Gate::t(q));
        }
        4 => gates.push(Gate::z(q)),
        5 => {
            gates.push(Gate::z(q));
            gates.push(Gate::t(q));
        }
        6 => gates.push(Gate::sdg(q)),
        7 => gates.push(Gate::tdg(q)),
        _ => unreachable!("phase quarters are reduced modulo eight"),
    }
}

#[cfg(test)]
mod baseline_tests;
#[cfg(test)]
mod tests;
