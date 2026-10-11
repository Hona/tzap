//! Replayable stress tests using independent formal basis-state phases.
//! SYMBOLIC_STRESS_SEED=1 SYMBOLIC_STRESS_START=17 SYMBOLIC_STRESS_CASES=1 \
//! cargo test --release symbolic_pipeline_stress -- --ignored --nocapture
use super::{
    AngleContext, Limits,
    pass_tests::{passes, signatures},
};
use crate::{
    circuit::{Circuit, Gate},
    optimize::{Options, PassName, optimize},
    pass::PassState,
};

fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut n = *state;
    n = (n ^ (n >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    n = (n ^ (n >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    n ^ (n >> 31)
}

fn stream(seed: u64, gates: usize) -> Circuit {
    let mut random = seed;
    let width = (next(&mut random) % 4 + 1) as u32;
    let mut ctx = AngleContext::default();
    let mut atoms = Vec::new();
    // Vary the functions' inputs in every case rather than recycling a tiny
    // interned set. Products, quotients and powers remain formal atoms too.
    for _ in 0..24 {
        let n = next(&mut random) % 97 + 1;
        let d = next(&mut random) % 23 + 1;
        let source = match next(&mut random) % 10 {
            0 => format!("sin({n}/{d})"),
            1 => format!("cos({n}/{d})"),
            // Stay inside a domain the constructor can prove excludes poles.
            2 => format!("tan({n}/101)/17"),
            3 => format!("sqrt({n}/{d})"),
            4 => format!("exp({n}/{d})/11"),
            5 => format!("ln({n}/{d})"),
            6 => format!("sin({n})*cos({d})"),
            7 => format!("sin({n})/exp({d})"),
            8 => format!("cos(sin({n}/{d}))^3"),
            _ => format!("pi/{d}+{n}/101"),
        };
        atoms.push(
            ctx.parse(&source)
                .unwrap_or_else(|e| panic!("{source}: {e}")),
        );
    }
    let mut input = Circuit::new(width as usize);
    for _ in 0..gates {
        let r = next(&mut random);
        let q = ((r >> 32) % u64::from(width)) as u32;
        let target = (q + 1) % width;
        let gate = match r % 9 {
            0 => Gate::x(q),
            1 if width > 1 => Gate::cnot { control: q, target },
            2 if width > 1 => Gate::cz { control: q, target },
            3 => Gate::t(q),
            4 => Gate::tdg(q),
            _ => {
                let a = &atoms[((r >> 16) % atoms.len() as u64) as usize];
                let a = if r & 256 != 0 {
                    ctx.neg(a).unwrap()
                } else {
                    a.clone()
                };
                if r & 512 != 0 {
                    Gate::p(a, q)
                } else {
                    Gate::rz(a, q)
                }
            }
        };
        input.apply(gate);
    }
    // Input angles must survive destruction of the construction context.
    input
}

fn run_cases(seed: u64, start: u64, cases: u64, gates: usize) {
    for case in start..start.checked_add(cases).unwrap() {
        let input = stream(seed.wrapping_add(case), gates);
        let reference = signatures(&input);
        let check = |output: &Circuit, profile: &str| {
            assert_eq!(
                signatures(output),
                reference,
                "seed={seed} case={case} {profile}"
            );
            let reparsed = crate::qasm::parse(&output.to_qasm()).unwrap();
            assert_eq!(
                signatures(&reparsed),
                reference,
                "seed={seed} case={case} {profile} QASM"
            );
        };
        check(&input, "input");
        for p in passes() {
            check(&p.run(&input), p.name());
        }
        // Tight and zero limits exercise rollback after earlier successful
        // rewrites, including reusing the depleted budget across passes.
        for limits in [
            Limits {
                max_terms: 2,
                ..Limits::default()
            },
            Limits {
                max_work: 200,
                ..Limits::default()
            },
            Limits {
                max_work: 0,
                ..Limits::default()
            },
        ] {
            let mut state = PassState::with_limits(limits);
            let mut output = input.clone();
            for _ in 0..2 {
                for p in passes().into_iter().rev() {
                    output = p.run_with_state(&output, &mut state);
                    check(&output, &format!("{} limits={limits:?}", p.name()));
                }
            }
        }
        for parallel in [false, true] {
            let options = Options {
                passes: Some(vec![
                    PassName::PhaseFoldRand,
                    PassName::CnotMin,
                    PassName::PhaseFoldPauli,
                    PassName::CancelGates,
                ]),
                fixpoint: true,
                parallel,
                ..Options::default()
            };
            let (output, _) = optimize(&input, &options).unwrap();
            check(
                &output,
                if parallel {
                    "parallel fixpoint"
                } else {
                    "serial fixpoint"
                },
            );
        }
    }
    eprintln!("symbolic stress seed={seed} start={start} cases={cases} gates={gates}: passed");
}

#[test]
fn symbolic_streams_preserve_formal_phases_under_depleted_budgets() {
    run_cases(0x5359_4d42_4f4c_4943, 0, 8, 64);
}

fn setting(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .map(|s| s.parse().expect("unsigned decimal setting"))
        .unwrap_or(default)
}

#[test]
#[ignore = "extended symbolic pipeline fuzzing; environment settings support replay"]
fn symbolic_pipeline_stress() {
    let cases = setting("SYMBOLIC_STRESS_CASES", 1024);
    let gates = usize::try_from(setting("SYMBOLIC_STRESS_GATES", 256)).unwrap();
    assert!(cases > 0 && gates > 0);
    run_cases(
        setting("SYMBOLIC_STRESS_SEED", 1),
        setting("SYMBOLIC_STRESS_START", 0),
        cases,
        gates,
    );
}

#[test]
fn unique_symbolic_terms_cross_the_default_limit_without_changing_phases() {
    let mut ctx = AngleContext::default();
    let mut input = Circuit::new(1);
    for n in 1..=600 {
        let a = ctx.parse(&format!("sin({n})")).unwrap();
        input.apply(Gate::rz(a.clone(), 0));
    }
    // Cancel only after every accumulator has had an opportunity to overflow.
    for n in (1..=600).rev() {
        let a = ctx.parse(&format!("-sin({n})")).unwrap();
        input.apply(Gate::rz(a, 0));
    }
    drop(ctx);
    let reference = signatures(&input);
    for p in passes() {
        let output = p.run(&input);
        assert_eq!(signatures(&output), reference, "{}", p.name());
        assert_eq!(
            signatures(&crate::qasm::parse(&output.to_qasm()).unwrap()),
            reference
        );
    }
}

#[test]
#[ignore = "extended cross-thread weak-interner churn and ownership stress"]
fn symbolic_interner_stress() {
    let count = setting("SYMBOLIC_INTERNER_CASES", 25_000);
    let mut threads = Vec::new();
    for thread in 0..8 {
        threads.push(std::thread::spawn(move || {
            let mut ctx = AngleContext::with_limits(Limits {
                max_nodes: 128,
                ..Limits::default()
            });
            let anchor = ctx.parse("sin(1)+cos(2)").unwrap();
            for i in 0..count {
                let source = format!("sin({})+cos({})", i * 8 + thread + 3, i * 8 + thread + 4);
                let value = ctx.parse(&source).unwrap();
                let text = value.to_string();
                assert_eq!(ctx.parse(&text).unwrap(), value);
                assert!(ctx.stats().retained_entries <= 128);
                if i % 97 == 0 {
                    ctx.clear_interner();
                    assert_eq!(ctx.parse("sin(1)+cos(2)").unwrap(), anchor);
                }
            }
            drop(ctx);
            (anchor, count)
        }));
    }
    for thread in threads {
        let (anchor, count) = thread.join().unwrap();
        assert_eq!(
            AngleContext::default().parse(&anchor.to_string()).unwrap(),
            anchor
        );
        eprintln!("interner thread: {count} unique expressions passed");
    }
}
