//! Per-run arithmetic diagnostics, propagated explicitly into rayon workers.
use crate::{
    angle::AngleError,
    circuit::Circuit,
    optimize::{Options, PassName},
};
use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NumericalReport {
    pub preserved_expressions: usize,
    pub numerical_fallbacks: usize,
    pub skipped_rounded_folds: usize,
    pub skipped_nonfinite_folds: usize,
    pub skipped_coefficient_limit_folds: usize,
    pub uncertified_syntheses: usize,
    /// Original input gates whose approximation was verified against accepted angles.
    pub certified_syntheses: usize,
    pub failed_certifications: usize,
    /// Outward-rounded sum of synthesized input-gate bounds, or None on overflow.
    pub certified_error_bound: Option<f64>,
    pub randomized_matching: bool,
}
#[derive(Default)]
pub(crate) struct Counters {
    rounded: AtomicUsize,
    nonfinite: AtomicUsize,
    coefficient: AtomicUsize,
    certified: AtomicUsize,
    failed: AtomicUsize,
    bound: Mutex<Bound>,
}
struct Bound(Option<f64>);
impl Default for Bound {
    fn default() -> Self {
        Self(Some(0.0))
    }
}
thread_local! {static ACTIVE:RefCell<Option<Arc<Counters>>>=const{RefCell::new(None)};}
pub(crate) fn current() -> Option<Arc<Counters>> {
    ACTIVE.with(|a| a.borrow().clone())
}
pub(crate) struct Scope(Option<Arc<Counters>>);
impl Scope {
    pub(crate) fn install(counters: Option<Arc<Counters>>) -> Self {
        Self(ACTIVE.with(|a| a.replace(counters)))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.with(|a| a.replace(self.0.take()));
    }
}
pub(crate) fn failure(error: AngleError) {
    ACTIVE.with(|a| {
        if let Some(c) = a.borrow().as_ref() {
            let target = match error {
                AngleError::RoundedAddition => &c.rounded,
                AngleError::NonFinite => &c.nonfinite,
                AngleError::RepresentationLimit => &c.coefficient,
                _ => return,
            };
            target.fetch_add(1, Ordering::Relaxed);
        }
    });
}
pub(crate) fn certified_synthesis(bound: f64) {
    ACTIVE.with(|a| {
        if let Some(c) = a.borrow().as_ref() {
            c.certified.fetch_add(1, Ordering::Relaxed);
            let mut sum = c.bound.lock().unwrap_or_else(|e| e.into_inner());
            sum.0 = sum.0.and_then(|previous| {
                let next = (previous + bound).next_up();
                next.is_finite().then_some(next)
            });
        }
    });
}
pub(crate) fn certification_failure() {
    if let Some(c) = current() {
        c.failed.fetch_add(1, Ordering::Relaxed);
    }
}
impl NumericalReport {
    pub fn input(circuit: &Circuit, options: &Options) -> Self {
        let mut result = Self {
            certified_error_bound: Some(0.0),
            randomized_matching: options
                .passes
                .as_ref()
                .is_none_or(|passes| passes.contains(&PassName::PhaseFoldRand)),
            ..Self::default()
        };
        for gate in &circuit.gates {
            if let Some(a) = gate.angle() {
                result.preserved_expressions += usize::from(a.is_preserved());
                result.numerical_fallbacks += usize::from(a.used_numeric_fallback());
            }
        }
        result
    }
    pub(crate) fn collected(circuit: &Circuit, options: &Options, c: &Counters) -> Self {
        Self {
            skipped_rounded_folds: c.rounded.load(Ordering::Relaxed),
            skipped_nonfinite_folds: c.nonfinite.load(Ordering::Relaxed),
            skipped_coefficient_limit_folds: c.coefficient.load(Ordering::Relaxed),
            uncertified_syntheses: 0,
            certified_syntheses: c.certified.load(Ordering::Relaxed),
            failed_certifications: c.failed.load(Ordering::Relaxed),
            certified_error_bound: c.bound.lock().unwrap_or_else(|e| e.into_inner()).0,
            ..Self::input(circuit, options)
        }
    }
}
