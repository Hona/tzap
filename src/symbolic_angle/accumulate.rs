use super::*;
use rustc_hash::FxHashMap;

/// Bounded addition in an explicitly periodic gate view, not literal scalar addition.
/// Named quarters stay inline. Other terms merge in a sparse map and sort once
/// at `finish`, avoiding repeated reconstruction of a growing canonical sum.
/// Updates are transactional; failed attempts can consume the lifetime work budget.
pub struct PhaseAccumulator {
    period: RotationPeriod,
    quarters: u8,
    terms: FxHashMap<Angle, Rational>,
    nodes: usize,
    budget: Budget,
}
impl PhaseAccumulator {
    pub fn new(period: RotationPeriod, limits: Limits) -> Self {
        Self {
            period,
            quarters: 0,
            terms: FxHashMap::default(),
            nodes: 0,
            budget: Budget::new(limits),
        }
    }
    pub fn period(&self) -> RotationPeriod {
        self.period
    }
    /// Count retained atoms, including a pending quarter-turn contribution.
    pub fn retained_terms(&self) -> usize {
        self.terms.len() + usize::from(self.quarters != 0 && !self.terms.contains_key(&Angle::PI))
    }
    pub fn remaining_work(&self) -> usize {
        self.budget.remaining
    }

    /// Add a named phase in units of pi/4. This path does not allocate.
    pub fn add_quarters(&mut self, quarters: i64) -> Result<(), Error> {
        self.budget.charge(1)?;
        let value = ((self.quarters as i64 + quarters.rem_euclid(self.period.quarters() as i64))
            % self.period.quarters() as i64) as u8;
        let angle = inline_quarters(value);
        self.budget.value(&angle)?;
        self.check_capacity(
            self.terms.len(),
            self.nodes,
            value,
            self.terms.contains_key(&Angle::PI),
        )?;
        self.quarters = value;
        Ok(())
    }
    pub fn add(&mut self, ctx: &mut AngleContext, angle: &Angle) -> Result<(), Error> {
        self.add_signed(ctx, angle, false)
    }
    pub fn sub(&mut self, ctx: &mut AngleContext, angle: &Angle) -> Result<(), Error> {
        self.add_signed(ctx, angle, true)
    }
    fn add_signed(
        &mut self,
        ctx: &mut AngleContext,
        angle: &Angle,
        negative: bool,
    ) -> Result<(), Error> {
        self.budget.value(angle)?;
        if let Some(k) = angle.quarter_turns(self.period) {
            return self.add_quarters(if negative { -(k as i64) } else { k as i64 });
        }
        let mut incoming = Vec::new();
        ctx.terms(
            angle,
            &Rational::integer(if negative { -1 } else { 1 }),
            &mut incoming,
            &mut self.budget,
        )?;
        let mut count = self.terms.len();
        let mut nodes = self.nodes;
        let mut has_pi = self.terms.contains_key(&Angle::PI);
        // A normalized input has distinct atoms. Stage only affected entries,
        // rather than cloning the entire accumulator for each addition.
        for term in &mut incoming {
            self.budget.charge(term.atom.expression_nodes())?;
            if let Some(prior) = self.terms.get(&term.atom) {
                term.coefficient = prior.add(&term.coefficient, &mut self.budget)?;
                if term.coefficient.is_zero() {
                    count -= 1;
                    nodes -= term.atom.expression_nodes();
                    if term.atom == Angle::PI {
                        has_pi = false;
                    }
                }
            } else if !term.coefficient.is_zero() {
                count += 1;
                nodes = nodes
                    .checked_add(term.atom.expression_nodes())
                    .ok_or(Error::Limit(Resource::Nodes))?;
                if term.atom == Angle::PI {
                    has_pi = true;
                }
            }
        }
        self.check_capacity(count, nodes, self.quarters, has_pi)?;
        for term in incoming {
            if term.coefficient.is_zero() {
                self.terms.remove(&term.atom);
            } else {
                self.terms.insert(term.atom, term.coefficient);
            }
        }
        self.nodes = nodes;
        Ok(())
    }
    fn check_capacity(
        &self,
        count: usize,
        nodes: usize,
        quarters: u8,
        has_pi: bool,
    ) -> Result<(), Error> {
        let extra = usize::from(quarters != 0 && !has_pi);
        let count = count
            .checked_add(extra)
            .ok_or(Error::Limit(Resource::Terms))?;
        if count > self.budget.limits.max_terms {
            return Err(Error::Limit(Resource::Terms));
        }
        // Reserve the sum root conservatively; finalization may collapse it.
        let nodes = if nodes == 0 {
            1
        } else {
            nodes
                .checked_add(extra)
                .and_then(|n| n.checked_add(1))
                .ok_or(Error::Limit(Resource::Nodes))?
        };
        if nodes > self.budget.limits.max_nodes {
            return Err(Error::Limit(Resource::Nodes));
        }
        Ok(())
    }
    /// Produce a canonical representative without consuming retained terms.
    /// Composite depth and normalization rules can still fail here. Callers must
    /// finalize successfully before committing any circuit or frame mutation.
    /// Uses this accumulator's captured limits, even with a different context.
    pub fn finish(&mut self, ctx: &mut AngleContext) -> Result<Angle, Error> {
        if self.terms.is_empty() {
            let angle = inline_quarters(self.quarters);
            self.budget.value(&angle)?;
            return Ok(angle);
        }
        self.budget.charge(self.nodes)?;
        let mut terms: Vec<_> = self
            .terms
            .iter()
            .map(|(atom, coefficient)| Term {
                atom: atom.clone(),
                coefficient: coefficient.clone(),
            })
            .collect();
        if self.quarters != 0 {
            terms.push(Term {
                atom: Angle::PI,
                coefficient: Rational::new(self.quarters as i64, 4)?,
            });
        }
        let angle = ctx.sum(terms, &mut self.budget)?;
        // Apply a period only to a proved pure pi result. General expressions
        // retain their exact meaning relative to the pending period contribution.
        if let Some(k) = angle.quarter_turns(self.period) {
            let reduced = inline_quarters(k);
            self.budget.value(&reduced)?;
            Ok(reduced)
        } else {
            Ok(angle)
        }
    }
}

fn inline_quarters(k: u8) -> Angle {
    if k == 0 {
        return Angle::ZERO;
    }
    let shift = k.trailing_zeros().min(2);
    Angle(Value::Pi {
        n: (k >> shift) as i32,
        d: 4 >> shift,
    })
}

#[cfg(test)]
mod tests;
