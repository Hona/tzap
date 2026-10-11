//! Independent exact matrix oracle in Q[zeta_16], with zeta_16^8 = -1.
//! This checks literal global phases, not numerical or projective equivalence.
use num_rational::BigRational;
use num_traits::{One, Zero};
use tzap::{
    circuit::Gate,
    symbolic_angle::{
        Angle, AngleContext, Proof,
        integration::{GateFamily, GateSemantics, Operands, Scope},
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Scalar([BigRational; 8]);
impl Scalar {
    fn zero() -> Self {
        Self(std::array::from_fn(|_| BigRational::zero()))
    }
    fn root(power: i64) -> Self {
        let mut result = Self::zero();
        let power = power.rem_euclid(16) as usize;
        result.0[power % 8] =
            BigRational::from_integer(if power < 8 { 1.into() } else { (-1).into() });
        result
    }
    fn add(&self, rhs: &Self) -> Self {
        Self(std::array::from_fn(|i| &self.0[i] + &rhs.0[i]))
    }
    fn neg(&self) -> Self {
        Self(std::array::from_fn(|i| -&self.0[i]))
    }
    fn half(&self) -> Self {
        Self(std::array::from_fn(|i| {
            &self.0[i] / BigRational::from_integer(2.into())
        }))
    }
    fn mul(&self, rhs: &Self) -> Self {
        let mut result = Self::zero();
        for i in 0..8 {
            if self.0[i].is_zero() {
                continue;
            }
            for j in 0..8 {
                if rhs.0[j].is_zero() {
                    continue;
                }
                let product = &self.0[i] * &rhs.0[j];
                if i + j < 8 {
                    result.0[i + j] += product;
                } else {
                    result.0[i + j - 8] -= product;
                }
            }
        }
        result
    }
}
type Matrix = Vec<Vec<Scalar>>;
fn identity(n: usize) -> Matrix {
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    if i == j {
                        Scalar::root(0)
                    } else {
                        Scalar::zero()
                    }
                })
                .collect()
        })
        .collect()
}
fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    (0..a.len())
        .map(|i| {
            (0..a.len())
                .map(|j| {
                    (0..a.len()).fold(Scalar::zero(), |sum, k| sum.add(&a[i][k].mul(&b[k][j])))
                })
                .collect()
        })
        .collect()
}
fn rotation(family: GateFamily, quarters: i64) -> [[Scalar; 2]; 2] {
    let positive = Scalar::root(quarters);
    let negative = Scalar::root(-quarters);
    let cosine = positive.add(&negative).half();
    let sine = positive.add(&negative.neg()).mul(&Scalar::root(-4)).half();
    match family {
        GateFamily::P | GateFamily::Cp => [
            [Scalar::root(0), Scalar::zero()],
            [Scalar::zero(), Scalar::root(2 * quarters)],
        ],
        GateFamily::Rz | GateFamily::Crz => {
            [[negative, Scalar::zero()], [Scalar::zero(), positive]]
        }
        GateFamily::Rx | GateFamily::Crx => {
            let off = sine.mul(&Scalar::root(-4));
            [[cosine.clone(), off.clone()], [off, cosine]]
        }
        GateFamily::Ry | GateFamily::Cry => [[cosine.clone(), sine.neg()], [sine, cosine]],
    }
}
fn embed(local: [[Scalar; 2]; 2], target: u32, control: Option<u32>, n: usize) -> Matrix {
    let mut out = identity(n);
    for col in (0..n).filter(|col| control.is_none_or(|c| *col & (1 << c) != 0)) {
        let bit = usize::from(col & (1 << target) != 0);
        for (row_bit, values) in local.iter().enumerate() {
            let row = (col & !(1 << target)) | (row_bit << target);
            out[row][col] = values[bit].clone();
        }
    }
    out
}
fn named(gate: &Gate, n: usize) -> Matrix {
    let one = Scalar::root(0);
    let zero = Scalar::zero();
    match gate {
        Gate::h(q) => {
            let r = Scalar::root(2).add(&Scalar::root(6).neg()).half();
            embed([[r.clone(), r.clone()], [r.clone(), r.neg()]], *q, None, n)
        }
        Gate::t(q) => embed([[one, zero.clone()], [zero, Scalar::root(2)]], *q, None, n),
        Gate::tdg(q) => embed([[one, zero.clone()], [zero, Scalar::root(-2)]], *q, None, n),
        Gate::s(q) => embed([[one, zero.clone()], [zero, Scalar::root(4)]], *q, None, n),
        Gate::sdg(q) => embed([[one, zero.clone()], [zero, Scalar::root(-4)]], *q, None, n),
        Gate::z(q) => embed([[one, zero.clone()], [zero, Scalar::root(8)]], *q, None, n),
        Gate::cnot { control, target } => embed(
            [[zero.clone(), one.clone()], [one, zero]],
            *target,
            Some(*control),
            n,
        ),
        _ => panic!("adapter emitted non-named gate: {gate:?}"),
    }
}

#[test]
fn all_gate_families_lower_with_exact_literal_phase_and_operand_order() {
    let mut ctx = AngleContext::default();
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
            for (target, control) in [(0, 1), (1, 0)] {
                // Two active wires and one untouched spectator.
                for k in -17..=17 {
                    let angle = ctx.pi_fraction(k, 4).unwrap();
                    let operands = if family.controlled() {
                        Operands::Controlled { control, target }
                    } else {
                        Operands::Single(target)
                    };
                    let lowered = semantics.lower(&mut ctx, &angle, operands).unwrap();
                    if family.controlled() && k % 2 != 0 {
                        assert!(lowered.is_none());
                        continue;
                    }
                    let lowered = lowered.unwrap();
                    assert!(!lowered.gates.spilled());
                    let mut word = identity(8);
                    for gate in &lowered.gates {
                        word = multiply(&named(gate, 8), &word);
                    }
                    let phase = if lowered.global_phase == Angle::ZERO {
                        Scalar::root(0)
                    } else {
                        let r = lowered
                            .global_phase
                            .pi_coefficient()
                            .unwrap()
                            .to_big_rational()
                            * BigRational::from_integer(8.into());
                        assert!(r.denom().is_one());
                        let n: i64 = r.numer().try_into().unwrap();
                        Scalar::root(n)
                    };
                    for row in &mut word {
                        for entry in row {
                            *entry = entry.mul(&phase);
                        }
                    }
                    assert_eq!(
                        word,
                        embed(
                            rotation(family, k),
                            target,
                            family.controlled().then_some(control),
                            8
                        ),
                        "{family:?}, {scope:?}, {k}, {target}"
                    );
                }
            }
        }
    }
}

#[test]
fn controlled_two_pi_remains_a_control_phase_and_four_pi_is_identity() {
    let mut ctx = AngleContext::default();
    for family in [GateFamily::Crx, GateFamily::Cry, GateFamily::Crz] {
        let s = GateSemantics {
            family,
            scope: Scope::ProjectiveCircuit,
        };
        let two = ctx.pi_fraction(2, 1).unwrap();
        assert_eq!(s.prove_identity(&two), Proof::Disproven);
        let out = s
            .lower(
                &mut ctx,
                &two,
                Operands::Controlled {
                    control: 1,
                    target: 0,
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(out.gates.as_slice(), &[Gate::z(1)]);
        assert_eq!(out.global_phase, Angle::ZERO);
        let four = ctx.pi_fraction(4, 1).unwrap();
        assert!(
            s.lower(
                &mut ctx,
                &four,
                Operands::Controlled {
                    control: 1,
                    target: 0
                }
            )
            .unwrap()
            .unwrap()
            .gates
            .is_empty()
        );
    }
}
