//! One isolated test binary measures allocations in the hot scalar paths.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hash::{Hash, Hasher},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tzap::symbolic_angle::{
    Angle, AngleContext, PhaseAccumulator, RotationPeriod,
    integration::{GateFamily, GateSemantics, Operands, Scope, from_legacy},
};
struct CountingAllocator;
static MEASURING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if MEASURING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if MEASURING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
fn allocations(operation: impl FnOnce()) -> usize {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    MEASURING.store(true, Ordering::Relaxed);
    operation();
    MEASURING.store(false, Ordering::Relaxed);
    ALLOCATIONS.load(Ordering::Relaxed)
}
#[test]
fn hot_scalar_paths_and_live_interned_functions_allocate_nothing() {
    let mut ctx = AngleContext::default();
    let quarter = ctx.pi_fraction(1, 4).unwrap();
    let a = ctx.pi_fraction(1, 7).unwrap();
    let b = ctx.pi_fraction(2, 7).unwrap();
    let float_a = Angle::from_f64(0.25).unwrap();
    let float_b = Angle::from_f64(0.5).unwrap();
    assert_eq!(
        allocations(|| {
            black_box(ctx.pi_fraction(1, 7).unwrap());
            black_box(ctx.rational(3, 10).unwrap());
            black_box(ctx.add(&a, &b).unwrap());
            black_box(ctx.neg(&a).unwrap());
            black_box(ctx.half(&a).unwrap());
            black_box(ctx.add(&float_a, &float_b).unwrap());
            black_box(quarter.quarter_turns(RotationPeriod::TwoPi));
            black_box(a.clone());
            black_box(Angle::from_f64(f64::from_bits(1)).unwrap());
        }),
        0
    );
    let live_sin = ctx.sin(&a).unwrap();
    let live_cos = ctx.cos(&a).unwrap();
    assert_eq!(
        allocations(|| {
            for _ in 0..100 {
                black_box(ctx.sin(&a).unwrap());
                black_box(ctx.cos(&a).unwrap());
            }
        }),
        0
    );
    let dyadic = Angle::from_f64(0.1).unwrap();
    let tiny = Angle::from_f64(f64::from_bits(1)).unwrap();
    let negative = ctx.neg(&live_sin).unwrap();
    let big = ctx.parse("1e100+1/7").unwrap().as_rational().unwrap();
    assert_eq!(
        allocations(|| {
            black_box(ctx.neg(&dyadic).unwrap());
            black_box(ctx.half(&dyadic).unwrap());
            black_box(ctx.add(&tiny, &tiny).unwrap());
            black_box(ctx.add(&live_sin, &negative).unwrap());
            black_box(ctx.decimal("0.125").unwrap());
            black_box(ctx.parse("0.1+0.2").unwrap());
            black_box(ctx.parse("sin(pi/7)").unwrap());
            let mut hasher = rustc_hash::FxHasher::default();
            big.hash(&mut hasher);
            black_box(hasher.finish());
        }),
        0
    );
    assert!(ctx.sin(&a).unwrap().shares_storage(&live_sin));
    assert!(ctx.cos(&a).unwrap().shares_storage(&live_cos));
    let mut accumulator = PhaseAccumulator::new(RotationPeriod::TwoPi, ctx.limits());
    let legacy = tzap::angle::Angle::from_f64(0.125).unwrap();
    let literal = GateSemantics {
        family: GateFamily::Ry,
        scope: Scope::Literal,
    };
    let controlled = GateSemantics {
        family: GateFamily::Crx,
        scope: Scope::ProjectiveCircuit,
    };
    let two_pi = ctx.pi_fraction(2, 1).unwrap();
    assert_eq!(
        allocations(|| {
            for _ in 0..100 {
                accumulator.add_quarters(1).unwrap();
                accumulator.add_quarters(-1).unwrap();
                black_box(accumulator.finish(&mut ctx).unwrap());
                black_box(
                    literal
                        .lower(&mut ctx, &quarter, Operands::Single(0))
                        .unwrap(),
                );
                black_box(
                    controlled
                        .lower(
                            &mut ctx,
                            &two_pi,
                            Operands::Controlled {
                                control: 1,
                                target: 0,
                            },
                        )
                        .unwrap(),
                );
                black_box(from_legacy(&mut ctx, &legacy).unwrap());
            }
        }),
        0
    );
}
