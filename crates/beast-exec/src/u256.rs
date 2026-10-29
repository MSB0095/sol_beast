//! A minimal 256-bit unsigned integer, ported from Orca Whirlpool's
//! `u256_math.rs`.
//!
//! Whirlpool's concentrated-liquidity math divides a 256-bit product by a
//! 256-bit product, so `u128` is not enough and approximating the result would
//! make the local quote disagree with the program on every lamport. This is the
//! program's own implementation, kept bit-for-bit so a quote computed here is
//! the quote the on-chain program will compute.
//!
//! Only the operations the swap math needs are present; the error type is
//! replaced by `Option` so this module carries no program-specific error enum.

use std::cmp::Ordering;

const NUM_WORDS: usize = 4;
const U64_RESOLUTION: u32 = 64;
const U64_MAX: u128 = u64::MAX as u128;

/// A 256-bit integer stored as four little-endian 64-bit words.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct U256(pub [u64; NUM_WORDS]);

impl U256 {
    /// Build from a high and low `u128`.
    pub fn new(high: u128, low: u128) -> Self {
        U256([low.lo(), low.hi(), high.lo(), high.hi()])
    }

    fn copy(&self) -> Self {
        *self
    }

    fn update_word(&mut self, index: usize, value: u64) {
        self.0[index] = value;
    }

    fn num_words(&self) -> usize {
        for index in (0..self.0.len()).rev() {
            if self.0[index] != 0 {
                return index + 1;
            }
        }
        0
    }

    pub fn get_word(&self, index: usize) -> u64 {
        self.0[index]
    }

    pub fn get_word_u128(&self, index: usize) -> u128 {
        self.0[index] as u128
    }

    /// Logical left shift by one 64-bit word; overflow is discarded.
    pub fn shift_word_left(&self) -> Self {
        let mut result = U256::new(0, 0);
        for index in (0..NUM_WORDS - 1).rev() {
            result.0[index + 1] = self.0[index];
        }
        result
    }

    /// Like [`U256::shift_word_left`], but returns `None` if a word would be
    /// lost.
    pub fn checked_shift_word_left(&self) -> Option<Self> {
        match self.0.last() {
            None => Some(self.shift_word_left()),
            Some(&element) if element > 0 => None,
            Some(_) => Some(self.shift_word_left()),
        }
    }

    /// Logical left shift by an arbitrary bit count; bits shifted past 256 are
    /// discarded, matching the program's semantics.
    pub fn shift_left(&self, mut shift_amount: u32) -> Self {
        if shift_amount >= U64_RESOLUTION * NUM_WORDS as u32 {
            return U256::new(0, 0);
        }

        let mut result = self.copy();

        while shift_amount >= U64_RESOLUTION {
            result = result.shift_word_left();
            shift_amount -= U64_RESOLUTION;
        }

        if shift_amount == 0 {
            return result;
        }

        for index in (1..NUM_WORDS).rev() {
            result.0[index] = (result.0[index] << shift_amount)
                | (result.0[index - 1] >> (U64_RESOLUTION - shift_amount));
        }
        result.0[0] <<= shift_amount;
        result
    }

    /// Logical right shift by one 64-bit word.
    pub fn shift_word_right(&self) -> Self {
        let mut result = U256::new(0, 0);
        for index in 0..NUM_WORDS - 1 {
            result.0[index] = self.0[index + 1];
        }
        result
    }

    /// Logical right shift by an arbitrary bit count.
    pub fn shift_right(&self, mut shift_amount: u32) -> Self {
        if shift_amount >= U64_RESOLUTION * NUM_WORDS as u32 {
            return U256::new(0, 0);
        }

        let mut result = self.copy();

        while shift_amount >= U64_RESOLUTION {
            result = result.shift_word_right();
            shift_amount -= U64_RESOLUTION;
        }

        if shift_amount == 0 {
            return result;
        }

        for index in 0..NUM_WORDS - 1 {
            result.0[index] = (result.0[index] >> shift_amount)
                | (result.0[index + 1] << (U64_RESOLUTION - shift_amount));
        }
        result.0[3] >>= shift_amount;
        result
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }

    /// Number of significant bits, i.e. the index of the highest set bit plus
    /// one. Zero for zero.
    pub fn bit_length(&self) -> u32 {
        for index in (0..NUM_WORDS).rev() {
            if self.0[index] != 0 {
                return index as u32 * U64_RESOLUTION
                    + (U64_RESOLUTION - self.0[index].leading_zeros());
            }
        }
        0
    }

    /// Narrow to `u128`, or `None` if any high word is set.
    pub fn try_into_u128(&self) -> Option<u128> {
        if self.num_words() > 2 {
            return None;
        }
        Some(((self.0[1] as u128) << U64_RESOLUTION) | (self.0[0] as u128))
    }

    /// Wrapping-style addition; carry out of the top word is discarded.
    pub fn add(&self, other: U256) -> Self {
        let mut result = U256::new(0, 0);
        let mut carry = 0u128;
        for index in 0..NUM_WORDS {
            let x = self.get_word_u128(index);
            let y = other.get_word_u128(index);
            let sum = x + y + carry;
            result.update_word(index, sum.lo());
            carry = sum.hi_u128();
        }
        result
    }

    /// Subtraction modulo 2^256.
    pub fn sub(&self, other: U256) -> Self {
        let mut result = U256::new(0, 0);
        let mut borrow = 0u64;
        for index in 0..NUM_WORDS {
            let x = self.get_word(index);
            let y = other.get_word(index);
            let (t0, overflow0) = x.overflowing_sub(y);
            let (t1, overflow1) = t0.overflowing_sub(borrow);
            result.update_word(index, t1);
            borrow = u64::from(overflow0 || overflow1);
        }
        result
    }

    /// Multiplication modulo 2^256.
    pub fn mul(&self, other: U256) -> Self {
        let mut result = U256::new(0, 0);
        let m = self.num_words();
        let n = other.num_words();

        for j in 0..n {
            let mut carry = 0u128;
            for i in 0..m {
                let x = self.get_word_u128(i);
                let y = other.get_word_u128(j);
                if i + j < NUM_WORDS {
                    let z = result.get_word_u128(i + j);
                    let t = x.wrapping_mul(y).wrapping_add(z).wrapping_add(carry);
                    result.update_word(i + j, t.lo());
                    carry = t.hi_u128();
                }
            }
            if j + m < NUM_WORDS {
                result.update_word(j + m, carry as u64);
            }
        }

        result
    }

    /// Long division. Returns `(quotient, remainder)`. Panics on a zero
    /// divisor, which the swap math already excludes.
    ///
    /// This is a binary long division: one quotient bit per step. The program
    /// uses Knuth's algorithm, which is faster but has several normalization
    /// corner cases; correctness matters more here than the constant factor.
    /// Only the significant bits of the dividend are walked, so a 209-bit value
    /// costs 209 steps rather than 256 — and, more importantly, the loop makes
    /// no assumption about how the quotient and remainder relate beyond the
    /// bit being processed.
    pub fn div(&self, divisor: U256, return_remainder: bool) -> (Self, Self) {
        assert!(!divisor.is_zero(), "U256 division by zero");

        // The swap math divides wide numerators by wide denominators, but a
        // large share of calls still fit in 128 bits. A native division is
        // orders of magnitude cheaper than walking bits.
        if let (Some(dividend), Some(divisor)) =
            (self.try_into_u128(), divisor.try_into_u128())
        {
            let quotient = dividend / divisor;
            let remainder = if return_remainder {
                U256::new(0, dividend % divisor)
            } else {
                U256::new(0, 0)
            };
            return (U256::new(0, quotient), remainder);
        }

        let mut quotient = U256::new(0, 0);
        let mut remainder = U256::new(0, 0);

        for bit in (0..self.bit_length()).rev() {
            let carry = remainder.0[NUM_WORDS - 1] >> 63;
            remainder = remainder.shift_left(1);
            let word = (bit / U64_RESOLUTION) as usize;
            let offset = bit % U64_RESOLUTION;
            if (self.0[word] >> offset) & 1 == 1 {
                remainder.0[0] |= 1;
            }
            // `carry` means the shifted remainder set bit 256. The true value
            // is then at least divisor (which is < 2^256), so the subtraction
            // below always applies; wrapping subtraction yields the correct
            // low 256 bits.
            if carry == 1 || remainder >= divisor {
                remainder = remainder.sub(divisor);
                quotient.0[word] |= 1u64 << offset;
            }
        }

        if return_remainder {
            (quotient, remainder)
        } else {
            (quotient, U256::new(0, 0))
        }
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> Ordering {
        for index in (0..NUM_WORDS).rev() {
            match self.0[index].cmp(&other.0[index]) {
                Ordering::Equal => {}
                ordering => return ordering,
            }
        }
        Ordering::Equal
    }
}

/// Split a `u128` into its low and high `u64` halves.
pub trait LoHi {
    fn lo(self) -> u64;
    fn hi(self) -> u64;
    fn lo_u128(self) -> u128;
    fn hi_u128(self) -> u128;
}

impl LoHi for u128 {
    fn lo(self) -> u64 {
        (self & U64_MAX) as u64
    }
    fn lo_u128(self) -> u128 {
        self & U64_MAX
    }
    fn hi(self) -> u64 {
        (self >> U64_RESOLUTION) as u64
    }
    fn hi_u128(self) -> u128 {
        self >> U64_RESOLUTION
    }
}

/// Join two `u64` halves into a `u128`.
pub fn hi_lo(hi: u64, lo: u64) -> u128 {
    ((hi as u128) << U64_RESOLUTION) | (lo as u128)
}

/// Full-width `u128 * u128 -> u256`.
pub fn mul_u256(v: u128, n: u128) -> U256 {
    let mut c0 = v.lo_u128() * n.lo_u128();
    let a1 = v.lo_u128() * n.hi_u128();
    let b0 = v.hi_u128() * n.lo_u128();

    let mut c1 = c0.hi_u128() + a1.lo_u128() + b0.lo_u128();
    c0 = hi_lo(c1.lo(), c0.lo());

    c1 = v.hi_u128() * n.hi_u128() + c1.hi_u128() + a1.hi_u128() + b0.hi_u128();
    U256::new(c1, c0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_u256_matches_u128_when_it_fits() {
        assert_eq!(
            mul_u256(123_456_789, 987_654_321)
                .try_into_u128()
                .unwrap(),
            123_456_789u128 * 987_654_321u128
        );
    }

    #[test]
    fn mul_u256_keeps_the_high_half() {
        // (2^64)^2 = 2^128, i.e. low word zero, second word one.
        let product = mul_u256(1u128 << 64, 1u128 << 64);
        assert_eq!(product.get_word(0), 0);
        assert_eq!(product.get_word(1), 0);
        assert_eq!(product.get_word(2), 1);
        assert_eq!(product.try_into_u128(), None);
    }

    #[test]
    fn shift_left_and_right_round_trip() {
        let value = U256::new(0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210);
        assert_eq!(value.shift_left(64).shift_right(64), value);
        assert_eq!(value.shift_left(256), U256::new(0, 0));
    }

    #[test]
    fn division_is_exact_for_large_values() {
        // (2^200) / (2^100) = 2^100.
        let numerator = U256::new(1u128 << 72, 0); // 2^(72+128) = 2^200
        let divisor = U256::new(0, 1u128 << 100);
        let (quotient, remainder) = numerator.div(divisor, true);
        assert_eq!(quotient.try_into_u128(), Some(1u128 << 100));
        assert!(remainder.is_zero());
    }

    #[test]
    fn division_remainder_round_trips() {
        let numerator = U256::new(0, 1_000_000_007);
        let divisor = U256::new(0, 9_973);
        let (quotient, remainder) = numerator.div(divisor, true);
        let q = quotient.try_into_u128().unwrap();
        let r = remainder.try_into_u128().unwrap();
        assert_eq!(q * 9_973 + r, 1_000_000_007);
        assert!(r < 9_973);
    }
}
