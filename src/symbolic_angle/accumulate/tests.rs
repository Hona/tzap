use super::*;
use num_rational::BigRational;
use rand::{Rng, SeedableRng, rngs::StdRng};

#[test]
fn quarters_respect_the_selected_period_without_allocated_storage() {
    let mut ctx = AngleContext::default();
    for period in [RotationPeriod::TwoPi, RotationPeriod::FourPi] {
        let mut accumulator = PhaseAccumulator::new(period, ctx.limits());
        for k in [i64::MAX, i64::MIN, 8, -1, 17] {
            accumulator.add_quarters(k).unwrap();
        }
        let expected = [i64::MAX, i64::MIN, 8, -1, 17]
            .into_iter()
            .map(|k| k.rem_euclid(period.quarters() as i64))
            .sum::<i64>()
            .rem_euclid(period.quarters() as i64);
        assert_eq!(
            accumulator.finish(&mut ctx).unwrap(),
            ctx.pi_fraction(expected, 4).unwrap()
        );
        assert!(accumulator.terms.is_empty());
        assert_eq!(accumulator.period(), period);
    }
    let mut accumulator = PhaseAccumulator::new(
        RotationPeriod::FourPi,
        Limits {
            max_nodes: 1,
            max_depth: 1,
            ..Default::default()
        },
    );
    accumulator.add_quarters(8).unwrap();
    assert_eq!(
        accumulator.finish(&mut ctx).unwrap(),
        ctx.pi_fraction(2, 1).unwrap()
    );
}

#[test]
fn mixed_terms_cancel_and_finalize_with_core_identities() {
    let mut ctx = AngleContext::default();
    let mut accumulator = PhaseAccumulator::new(RotationPeriod::FourPi, ctx.limits());
    let a = ctx.parse("sin(pi/7)+2*cos(pi/7)+0.1+pi/7").unwrap();
    accumulator.add(&mut ctx, &a).unwrap();
    accumulator.add_quarters(8).unwrap();
    accumulator.sub(&mut ctx, &a).unwrap();
    assert_eq!(accumulator.retained_terms(), 1);
    assert_eq!(
        accumulator.finish(&mut ctx).unwrap(),
        ctx.pi_fraction(2, 1).unwrap()
    );
    let sine = ctx.parse("sin(pi/7)^2").unwrap();
    let cosine = ctx.parse("cos(pi/7)^2").unwrap();
    let mut identity = PhaseAccumulator::new(RotationPeriod::TwoPi, ctx.limits());
    identity.add(&mut ctx, &sine).unwrap();
    identity.add(&mut ctx, &cosine).unwrap();
    assert_eq!(identity.finish(&mut ctx).unwrap(), Angle::ONE);
}

#[test]
fn randomized_signed_rational_accumulation_matches_independent_oracle() {
    let mut ctx = AngleContext::default();
    let mut accumulator = PhaseAccumulator::new(RotationPeriod::TwoPi, ctx.limits());
    let mut rng = StdRng::seed_from_u64(0x51a9_acc0);
    let mut expected = BigRational::from_integer(0.into());
    for _ in 0..1000 {
        let n = rng.gen_range(-1000..1000);
        let d = rng.gen_range(1..32);
        let value = ctx.rational(n, d).unwrap();
        if rng.gen_bool(0.5) {
            accumulator.add(&mut ctx, &value).unwrap();
            expected += BigRational::new(n.into(), d.into());
        } else {
            accumulator.sub(&mut ctx, &value).unwrap();
            expected -= BigRational::new(n.into(), d.into());
        }
    }
    assert_eq!(
        accumulator
            .finish(&mut ctx)
            .unwrap()
            .as_rational()
            .unwrap()
            .to_big_rational(),
        expected
    );
}

#[test]
fn failure_after_staging_multiple_updates_is_transactional() {
    let mut source = AngleContext::default();
    let a = source.parse("sin(1)").unwrap();
    let b = source.parse("cos(1)").unwrap();
    let incoming = source.add(&a, &b).unwrap();
    let limits = Limits {
        max_terms: 1,
        ..Default::default()
    };
    let mut accumulator = PhaseAccumulator::new(RotationPeriod::TwoPi, limits);
    accumulator.add(&mut source, &a).unwrap();
    let terms_before = accumulator.terms.clone();
    // The input itself exceeds the accumulator's term limit.
    assert_eq!(
        accumulator.add(&mut source, &incoming),
        Err(Error::Limit(Resource::Terms))
    );
    assert_eq!(accumulator.terms, terms_before);
    assert_eq!(accumulator.finish(&mut source).unwrap(), a);

    // Both incoming atoms fit individually; merging would overflow coefficient bits.
    let limits = Limits {
        max_bits: 3,
        ..Default::default()
    };
    let mut accumulator = PhaseAccumulator::new(RotationPeriod::TwoPi, limits);
    let seven_a = source.scale(&a, &Rational::integer(7)).unwrap();
    accumulator.add(&mut source, &seven_a).unwrap();
    let terms_before = accumulator.terms.clone();
    assert_eq!(
        accumulator.add(&mut source, &incoming),
        Err(Error::Limit(Resource::Bits))
    );
    assert_eq!(accumulator.terms, terms_before);
    assert_eq!(accumulator.finish(&mut source).unwrap(), seven_a);
}

#[test]
fn pending_pi_capacity_nodes_depth_and_work_are_bounded() {
    let mut ctx = AngleContext::default();
    let a = ctx.parse("sin(1)").unwrap();
    let mut accumulator = PhaseAccumulator::new(
        RotationPeriod::TwoPi,
        Limits {
            max_terms: 1,
            ..Default::default()
        },
    );
    accumulator.add(&mut ctx, &a).unwrap();
    assert_eq!(
        accumulator.add_quarters(1),
        Err(Error::Limit(Resource::Terms))
    );
    assert_eq!(accumulator.quarters, 0);
    assert_eq!(accumulator.finish(&mut ctx).unwrap(), a);

    let mut accumulator = PhaseAccumulator::new(
        RotationPeriod::TwoPi,
        Limits {
            max_nodes: 2,
            ..Default::default()
        },
    );
    assert_eq!(
        accumulator.add(&mut ctx, &a),
        Err(Error::Limit(Resource::Nodes))
    );
    assert_eq!(accumulator.retained_terms(), 0);

    let b = ctx.parse("cos(1)").unwrap();
    let mut accumulator = PhaseAccumulator::new(
        RotationPeriod::TwoPi,
        Limits {
            max_depth: 2,
            ..Default::default()
        },
    );
    accumulator.add(&mut ctx, &a).unwrap();
    accumulator.add(&mut ctx, &b).unwrap();
    assert_eq!(
        accumulator.finish(&mut ctx),
        Err(Error::Limit(Resource::Depth))
    );
    assert_eq!(accumulator.retained_terms(), 2);

    let mut accumulator = PhaseAccumulator::new(
        RotationPeriod::TwoPi,
        Limits {
            max_work: 3,
            ..Default::default()
        },
    );
    accumulator.add_quarters(1).unwrap();
    assert_eq!(
        accumulator.add_quarters(1),
        Err(Error::Limit(Resource::Work))
    );
    assert_eq!(accumulator.quarters, 1);
    assert_eq!(accumulator.remaining_work(), 0);
}

#[test]
fn cross_context_owned_atoms_and_bounded_unique_stream() {
    let mut ctx = AngleContext::default();
    let mut accumulator = PhaseAccumulator::new(RotationPeriod::TwoPi, ctx.limits());
    for n in 1..=256 {
        let atom = {
            let mut local = AngleContext::default();
            let n = local.rational(n, 1).unwrap();
            local.sin(&n).unwrap()
        };
        accumulator.add(&mut ctx, &atom).unwrap();
    }
    assert_eq!(accumulator.retained_terms(), 256);
    let extra = ctx.parse("sin(257)").unwrap();
    assert_eq!(
        accumulator.add(&mut ctx, &extra),
        Err(Error::Limit(Resource::Terms))
    );
    let before = accumulator.finish(&mut ctx).unwrap();
    assert_eq!(before.expression_nodes(), 513);
    let mut other = AngleContext::default();
    assert_eq!(other.parse(&before.to_string()).unwrap(), before);
}

#[test]
fn colliding_atom_hashes_do_not_merge_distinct_values() {
    let mut ctx = AngleContext::default();
    let a = ctx.parse("sin(1)").unwrap();
    let b = ctx.parse("cos(1)").unwrap();
    let (Value::Shared(na), Value::Shared(nb)) = (&a.0, &b.0) else {
        panic!()
    };
    let collision = Angle(Value::Shared(Arc::new(Node {
        kind: nb.kind.clone(),
        hash: na.hash,
        meta: nb.meta,
    })));
    let mut accumulator = PhaseAccumulator::new(RotationPeriod::TwoPi, ctx.limits());
    accumulator.add(&mut ctx, &a).unwrap();
    accumulator.add(&mut ctx, &collision).unwrap();
    assert_eq!(accumulator.retained_terms(), 2);
    accumulator.sub(&mut ctx, &a).unwrap();
    assert_eq!(accumulator.retained_terms(), 1);
    assert_eq!(accumulator.finish(&mut ctx).unwrap(), collision);
}
