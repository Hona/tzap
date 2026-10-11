//! Exact Clifford+T words in Z[omega]/sqrt(2)^k. Unbounded temporary
//! coefficients avoid interval wrapping over long words. Conversion to enclosing
//! intervals occurs only once, after the entire word has been evaluated.
use super::interval::{Complex, Context, Interval};
use num_bigint::BigInt;
use num_integer::Integer;

#[derive(Clone)]
struct Ring([BigInt; 4]);
impl Ring {
    fn integer(value: i64) -> Self {
        Self([value.into(), 0.into(), 0.into(), 0.into()])
    }
    fn add(&self, other: &Self) -> Self {
        Self(std::array::from_fn(|i| &self.0[i] + &other.0[i]))
    }
    fn sub(&self, other: &Self) -> Self {
        Self(std::array::from_fn(|i| &self.0[i] - &other.0[i]))
    }
    fn omega(&self, power: u8) -> Self {
        let mut result = self.clone();
        for _ in 0..power % 8 {
            let [a, b, c, d] = result.0;
            result.0 = [-d, a, b, c];
        }
        result
    }
    fn interval(&self, denominator: usize, ctx: &Context) -> Complex {
        let [a, b, c, d] = &self.0;
        let root = ctx.inv_sqrt_two();
        let mut re = Interval::point(&ctx.unit * a).add(&root.scale(&(b - d)));
        let mut im = Interval::point(&ctx.unit * c).add(&root.scale(&(b + d)));
        if denominator & 1 != 0 {
            re = ctx.mul(&re, &root);
            im = ctx.mul(&im, &root);
        }
        let power = BigInt::from(1) << (denominator / 2);
        let divide = |value: Interval| Interval {
            lo: value.lo.div_floor(&power),
            hi: value.hi.div_ceil(&power),
        };
        Complex {
            re: divide(re),
            im: divide(im),
        }
    }
}
pub(super) struct ExactWord<const N: usize> {
    entries: [[Ring; N]; N],
    denominator: usize,
}
impl<const N: usize> ExactWord<N> {
    pub fn identity() -> Self {
        Self {
            entries: std::array::from_fn(|i| {
                std::array::from_fn(|j| Ring::integer(i64::from(i == j)))
            }),
            denominator: 0,
        }
    }
    pub fn apply(&mut self, instruction: char, qubit: usize) -> Result<(), &'static str> {
        let mask = 1 << qubit;
        match instruction {
            'H' => {
                for i in 0..N {
                    if i & mask == 0 {
                        let first = self.entries[i].clone();
                        let second = self.entries[i | mask].clone();
                        self.entries[i] = std::array::from_fn(|j| first[j].add(&second[j]));
                        self.entries[i | mask] = std::array::from_fn(|j| first[j].sub(&second[j]));
                    }
                }
                self.denominator += 1;
            }
            'X' => {
                for i in 0..N {
                    if i & mask == 0 {
                        self.entries.swap(i, i | mask);
                    }
                }
            }
            'T' | 'S' | 'W' => {
                let power = if instruction == 'S' { 2 } else { 1 };
                for i in 0..N {
                    if instruction == 'W' || i & mask != 0 {
                        for entry in &mut self.entries[i] {
                            *entry = entry.omega(power);
                        }
                    }
                }
            }
            'I' => {}
            _ => return Err("unknown gridsynth instruction"),
        }
        Ok(())
    }
    pub fn cnot(&mut self, control: usize, target: usize) {
        for i in 0..N {
            if i & (1 << control) != 0 && i & (1 << target) == 0 {
                self.entries.swap(i, i | (1 << target));
            }
        }
    }
    pub fn intervals(&self, ctx: &Context) -> [[Complex; N]; N] {
        std::array::from_fn(|i| {
            std::array::from_fn(|j| self.entries[i][j].interval(self.denominator, ctx))
        })
    }
}
