//! Enclosing fixed-point arithmetic. Endpoints are integers in units of 2^-bits.
//! All divisions round outwards; no binary64 arithmetic enters the oracle.
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{Signed, Zero};

#[derive(Clone, Debug)]
pub(crate) struct Interval {
    pub lo: BigInt,
    pub hi: BigInt,
}

impl Interval {
    pub fn point(value: BigInt) -> Self {
        Self {
            lo: value.clone(),
            hi: value,
        }
    }
    pub fn add(&self, rhs: &Self) -> Self {
        Self {
            lo: &self.lo + &rhs.lo,
            hi: &self.hi + &rhs.hi,
        }
    }
    pub fn neg(&self) -> Self {
        Self {
            lo: -&self.hi,
            hi: -&self.lo,
        }
    }
    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }
    pub fn scale(&self, n: &BigInt) -> Self {
        if n.is_negative() {
            self.neg().scale(&-n)
        } else {
            Self {
                lo: &self.lo * n,
                hi: &self.hi * n,
            }
        }
    }
    pub fn div_int(&self, n: u64) -> Self {
        let n = BigInt::from(n);
        Self {
            lo: self.lo.div_floor(&n),
            hi: self.hi.div_ceil(&n),
        }
    }
    pub fn abs_upper(&self) -> BigInt {
        self.lo.abs().max(self.hi.abs())
    }
    pub fn middle(&self) -> BigInt {
        (&self.lo + &self.hi).div_floor(&2.into())
    }
    pub fn widen(&self, radius: &BigInt) -> Self {
        Self {
            lo: &self.lo - radius,
            hi: &self.hi + radius,
        }
    }
}

pub(crate) struct Context {
    pub bits: usize,
    pub unit: BigInt,
}

impl Context {
    pub fn new(bits: usize) -> Self {
        Self {
            bits,
            unit: BigInt::from(1) << bits,
        }
    }
    pub fn integer(&self, n: i64) -> Interval {
        Interval::point(&self.unit * n)
    }
    pub fn float(&self, value: f64) -> Interval {
        let bits = value.to_bits();
        let exp = ((bits >> 52) & 2047) as i32;
        let mut mantissa = bits & ((1u64 << 52) - 1);
        assert!(exp != 2047, "finite input required");
        if exp != 0 {
            mantissa |= 1 << 52;
        }
        let power = if exp == 0 { -1074 } else { exp - 1075 };
        let signed: BigInt = BigInt::from(mantissa) * if bits >> 63 == 0 { 1 } else { -1 };
        let shift = power + self.bits as i32;
        if shift >= 0 {
            Interval::point(signed << shift as usize)
        } else {
            let divisor = BigInt::from(1) << (-shift) as usize;
            Interval {
                lo: signed.div_floor(&divisor),
                hi: signed.div_ceil(&divisor),
            }
        }
    }
    pub fn mul(&self, a: &Interval, b: &Interval) -> Interval {
        let products = [&a.lo * &b.lo, &a.lo * &b.hi, &a.hi * &b.lo, &a.hi * &b.hi];
        Interval {
            lo: products.iter().min().unwrap().div_floor(&self.unit),
            hi: products.iter().max().unwrap().div_ceil(&self.unit),
        }
    }
    // Machin: pi = 16 atan(1/5) - 4 atan(1/239). Alternating series
    // remainder is no larger than the first omitted term (< one unit).
    fn atan_reciprocal(&self, q: u64) -> Interval {
        let mut power = BigInt::from(q);
        let mut sum = self.integer(0);
        for n in 0..self.bits + 1 {
            let denominator = &power * (2 * n + 1);
            if denominator > self.unit {
                return sum.widen(&1.into());
            }
            let term = Interval {
                lo: self.unit.div_floor(&denominator),
                hi: self.unit.div_ceil(&denominator),
            };
            sum = if n % 2 == 0 {
                sum.add(&term)
            } else {
                sum.sub(&term)
            };
            power *= q * q;
        }
        unreachable!("geometric series must reach the truncation threshold")
    }
    pub fn pi(&self) -> Interval {
        self.atan_reciprocal(5)
            .scale(&16.into())
            .sub(&self.atan_reciprocal(239).scale(&4.into()))
    }
    // Taylor polynomial with a Lagrange remainder. Inputs are bounded by 4.
    // The first omitted monomial encloses |x|^n/n!, since derivatives of
    // sin and cos have magnitude at most one. The interval recurrence also
    // encloses all rounding and dependence errors in the retained polynomial.
    fn trig(&self, x: &Interval, sine: bool) -> Result<Interval, &'static str> {
        if x.abs_upper() > &self.unit * 4 {
            return Err("trigonometric argument is not bounded");
        }
        let square = self.mul(x, x);
        let mut term = if sine { x.clone() } else { self.integer(1) };
        let mut sum = term.clone();
        for n in 0..self.bits + 32 {
            let k = if sine { 2 * n + 2 } else { 2 * n + 1 };
            let next = self.mul(&term, &square).neg().div_int((k * (k + 1)) as u64);
            let remainder = next.abs_upper();
            if n >= 8 && remainder <= 2.into() {
                return Ok(sum.widen(&remainder));
            }
            sum = sum.add(&next);
            term = next;
        }
        Err("trigonometric precision limit reached")
    }
    pub fn sin_cos(&self, x: &Interval) -> Result<(Interval, Interval), &'static str> {
        Ok((self.trig(x, true)?, self.trig(x, false)?))
    }
    pub fn inv_sqrt_two(&self) -> Interval {
        let square: BigInt = (&self.unit * &self.unit) / 2;
        let root: BigInt = square.sqrt();
        Interval {
            lo: root.clone(),
            hi: root + 1,
        }
    }
    pub fn reciprocal_sqrt(&self, value: &Interval) -> Result<Interval, &'static str> {
        if value.lo <= BigInt::zero() {
            return Err("global phase cannot be enclosed");
        }
        let lower = (&value.lo * &self.unit).sqrt();
        let upper = (&value.hi * &self.unit).sqrt() + 1;
        let square = &self.unit * &self.unit;
        Ok(Interval {
            lo: square.div_floor(&upper),
            hi: square.div_ceil(&lower),
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct Complex {
    pub re: Interval,
    pub im: Interval,
}
impl Complex {
    pub fn real(ctx: &Context, n: i64) -> Self {
        Self {
            re: ctx.integer(n),
            im: ctx.integer(0),
        }
    }
    pub fn neg(&self) -> Self {
        Self {
            re: self.re.neg(),
            im: self.im.neg(),
        }
    }
    pub fn mul(&self, other: &Self, ctx: &Context) -> Self {
        Self {
            re: ctx
                .mul(&self.re, &other.re)
                .sub(&ctx.mul(&self.im, &other.im)),
            im: ctx
                .mul(&self.re, &other.im)
                .add(&ctx.mul(&self.im, &other.re)),
        }
    }
    pub fn difference_square_upper(&self, other: &Self) -> BigInt {
        let re = self.re.sub(&other.re).abs_upper();
        let im = self.im.sub(&other.im).abs_upper();
        &re * &re + &im * &im
    }
    pub fn difference_square_bounds(&self, other: &Self) -> (BigInt, BigInt) {
        let re = self.re.sub(&other.re);
        let im = self.im.sub(&other.im);
        let minimum = |x: &Interval| {
            if x.lo > BigInt::zero() {
                x.lo.clone()
            } else if x.hi < BigInt::zero() {
                -&x.hi
            } else {
                BigInt::zero()
            }
        };
        let (re_lo, im_lo) = (minimum(&re), minimum(&im));
        let (re_hi, im_hi) = (re.abs_upper(), im.abs_upper());
        (
            &re_lo * &re_lo + &im_lo * &im_lo,
            &re_hi * &re_hi + &im_hi * &im_hi,
        )
    }
}

pub(super) type Matrix = [[Complex; 2]; 2];
pub(super) fn identity(ctx: &Context) -> Matrix {
    [
        [Complex::real(ctx, 1), Complex::real(ctx, 0)],
        [Complex::real(ctx, 0), Complex::real(ctx, 1)],
    ]
}
pub(super) fn word_matrix(word: &[char], ctx: &Context) -> Result<(Matrix, u8), &'static str> {
    let mut exact = super::exact_word::ExactWord::<2>::identity();
    let mut determinant = 0u8;
    for ch in word {
        determinant = (determinant
            + match ch {
                'I' => 0,
                'W' => 2,
                'T' => 1,
                'S' => 2,
                'H' | 'X' => 4,
                _ => return Err("unknown gridsynth instruction"),
            })
            % 8;
        exact.apply(*ch, 0)?;
    }
    let matrix = exact.intervals(ctx);
    Ok((matrix, determinant))
}

pub(super) fn verifies(
    target: &Matrix,
    word: &[char],
    pi: &Interval,
    budget: &Interval,
    ctx: &Context,
) -> Result<bool, &'static str> {
    let (candidate, det) = word_matrix(word, ctx)?;
    let phase_angle = pi.scale(&BigInt::from(-(det as i32))).div_int(8);
    let (sin, cos) = ctx.sin_cos(&phase_angle)?;
    let phase = Complex { re: cos, im: sin };
    for sign in [1, -1] {
        let mut squared = BigInt::zero();
        for i in 0..2 {
            for j in 0..2 {
                let entry = candidate[i][j].mul(&phase, ctx);
                let entry = if sign < 0 { entry.neg() } else { entry };
                squared += entry.difference_square_upper(&target[i][j]);
            }
        }
        // Frobenius norm upper bound implies the spectral norm upper bound.
        if budget.lo > BigInt::zero() && squared <= &budget.lo * &budget.lo {
            return Ok(true);
        }
    }
    Ok(false)
}
