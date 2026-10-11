//! One isolated test binary measures allocations in the hot scalar paths.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tzap::symbolic_angle::{Angle, AngleContext, RotationPeriod};
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
    assert!(ctx.sin(&a).unwrap().shares_storage(&live_sin));
    assert!(ctx.cos(&a).unwrap().shares_storage(&live_cos));
}
