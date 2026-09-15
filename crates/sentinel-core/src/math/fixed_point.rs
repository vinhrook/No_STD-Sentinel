// =============================================================================
//  fixed_point.rs — Q16.16 Fixed-Point Arithmetic (no_std, no floating point)
// =============================================================================
//
// ┌─────────────────────── WHY THIS FILE EXISTS ───────────────────────┐
// │                                                                     │
// │  Sentinel runs on an ESP32-S3 microcontroller.                     │
// │  The ESP32-S3 has NO hardware floating-point unit (FPU).           │
// │  If you use `f32` or `f64`, the compiler emits SOFTWARE emulation  │
// │  — it's 10-50x SLOWER than integer math.                           │
// │                                                                     │
// │  Fixed-point gives us decimal precision using ONLY integers.       │
// │  It's deterministic (same input → same output, always).            │
// │  It's fast (just integer add/subtract/shift operations).           │
// │                                                                     │
// └─────────────────────────────────────────────────────────────────────┘
//
// ┌────────────── WHAT IS Q16.16? ──────────────┐
// │                                              │
// │  A 32-bit integer split into two halves:     │
// │                                              │
// │   [  16 bits integer  |  16 bits fraction ]  │
// │   [  whole number     |  decimal part     ]  │
// │                                              │
// │  The "scale factor" is 2^16 = 65,536         │
// │                                              │
// │  To store 1.5:                               │
// │    1.5 × 65536 = 98304                       │
// │    Binary: 0000_0001_1000_0000_0000_0000      │
// │            ^^^^^^^^ ^^^^^^^^^^^^^^^^          │
// │            int = 1   frac = 0.5               │
// │                                              │
// │  To store 3.75:                              │
// │    3.75 × 65536 = 245760                     │
// │                                              │
// │  Range: -32768.0 to +32767.99998             │
// │  Precision: 1/65536 ≈ 0.0000153              │
// │                                              │
// └──────────────────────────────────────────────┘

// =============================================================================
// CONCEPT: #[derive(...)] — Automatic Trait Implementations
// =============================================================================
//
// In Rust, "traits" are like interfaces/contracts that define behavior.
// `derive` tells the compiler to AUTO-GENERATE implementations for us:
//
//   Clone  — allows `let b = a.clone();` (creates an independent copy)
//   Copy   — allows `let b = a;` WITHOUT moving `a` (a stays valid)
//            This works because our struct is just a single i32 — trivial to copy.
//   Debug  — allows `println!("{:?}", val);` for debugging output
//   PartialEq — allows `a == b` comparisons
//   Eq     — stronger version: guarantees a == a is ALWAYS true
//            (NaN in floats breaks this! That's why f32 is PartialEq but NOT Eq)
//   PartialOrd — allows `a < b`, `a > b` comparisons
//   Ord    — total ordering: any two values can be compared (no NaN ambiguity)
//   Hash   — allows using this type as a HashMap key (we might need it later)

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed {
    // ==========================================================================
    // CONCEPT: Struct Fields & Visibility
    // ==========================================================================
    //
    // `raw` is the internal i32 that holds our scaled integer.
    // It's NOT `pub` — this is called "encapsulation."
    //
    // Why hide it?
    //   If someone writes `Fixed { raw: 42 }` they might think 42 means 42.0,
    //   but it actually means 42/65536 = 0.000641...
    //   By hiding `raw`, we FORCE them to use our constructors:
    //     Fixed::from_int(42)    → represents 42.0
    //     Fixed::from_raw(42)    → represents 0.000641 (expert use only)
    //
    // CONCEPT: i32 vs u32
    //   i32 = signed 32-bit integer → range: -2,147,483,648 to +2,147,483,647
    //   u32 = unsigned 32-bit integer → range: 0 to 4,294,967,295
    //   We use i32 because vibration data oscillates AROUND ZERO (negative values matter).
    raw: i32,
}

// =============================================================================
// CONCEPT: `const` — Compile-Time Constants
// =============================================================================
//
// These are computed at COMPILE TIME, not runtime.
// They cost ZERO CPU cycles on the microcontroller.
// The compiler literally replaces `FRAC_BITS` with `16` everywhere.
//
// Why not just write `16` everywhere? Three reasons:
//   1. Readability — `1 << FRAC_BITS` is self-documenting
//   2. Maintainability — change to Q24.8 by editing ONE line
//   3. No "magic numbers" — professional code never has unexplained literals

/// Number of fractional bits in our Q16.16 format.
const FRAC_BITS: u32 = 16;

/// The scale factor: 2^16 = 65536.
/// Multiplying a real number by this converts it to fixed-point.
const SCALE: i32 = 1 << FRAC_BITS;
// CONCEPT: `<<` is the LEFT SHIFT operator.
// `1 << 16` means: take the number 1 (binary: 0000_0001)
//                   and shift it left by 16 positions.
//                   Result: 1_0000_0000_0000_0000 = 65536 in decimal.
// This is the same as 2^16 but MUCH faster than calling pow().

// =============================================================================
// CONCEPT: `impl` Block — Methods on a Struct
// =============================================================================
//
// This is where we define functions that "belong to" the Fixed type.
// Two kinds:
//   - Associated functions: `Fixed::from_int(5)` — called on the TYPE, not an instance
//     (like static methods in Java/C++)
//   - Methods: `a.to_int()` — called on an INSTANCE, take `self`/`&self` as first param

impl Fixed {
    // =========================================================================
    // CONCEPT: `pub const fn` — Public, Compile-Time Evaluable Function
    // =========================================================================
    //
    // `pub`      → visible outside this module (users can call it)
    // `const fn` → can be evaluated at COMPILE TIME
    //              e.g., `const GRAVITY: Fixed = Fixed::from_int(9);` works!
    //              The compiler computes 9 * 65536 = 589824 during compilation.
    //
    // CONCEPT: `Self` vs `self`
    //   `Self` (capital S) = the TYPE itself (here, `Fixed`)
    //   `self` (lowercase) = an INSTANCE of the type
    //   `&self` = a REFERENCE to an instance (borrowed, read-only)
    //   `&mut self` = a MUTABLE reference (borrowed, read-write)

    /// Create a Fixed from a raw pre-scaled i32 value.
    ///
    /// # When to use this
    /// Only when you already have a value in Q16.16 format
    /// (e.g., reading from a sensor register that outputs fixed-point).
    ///
    /// # Example
    /// ```
    /// use sentinel_core::math::fixed_point::Fixed;
    /// let raw_from_sensor: i32 = 98304; // This is 1.5 in Q16.16
    /// let val = Fixed::from_raw(raw_from_sensor);
    /// assert_eq!(val.to_int(), 1); // Integer part is 1
    /// ```
    #[inline] // CONCEPT: #[inline] — suggests the compiler paste this function's body
              // directly into the caller instead of making a function call.
              // For tiny functions like this (just returning a field), it eliminates
              // the overhead of: push stack frame → jump → return → pop stack frame.
              // On a microcontroller with limited stack, this MATTERS.
    pub const fn from_raw(raw: i32) -> Self {
        // CONCEPT: Implicit Return
        // In Rust, the LAST expression WITHOUT a semicolon is the return value.
        // `Self { raw }` is equivalent to `return Self { raw: raw };`
        // The shorthand `{ raw }` works because the parameter name matches the field name.
        Self { raw }
    }

    /// Create a Fixed from a whole integer.
    ///
    /// `Fixed::from_int(3)` represents exactly 3.0
    ///
    /// # Panics
    /// In debug mode, panics if `n` overflows the Q16.16 range.
    /// The valid range is -32768 to +32767.
    ///
    /// # Example
    /// ```
    /// use sentinel_core::math::fixed_point::Fixed;
    /// let three = Fixed::from_int(3);
    /// assert_eq!(three.raw_value(), 3 * 65536);
    /// ```
    #[inline]
    pub const fn from_int(n: i32) -> Self {
        // CONCEPT: Overflow
        // If n = 40000, then 40000 * 65536 = 2,621,440,000 which EXCEEDS i32::MAX (2,147,483,647).
        // In debug builds: Rust PANICS (crashes with error message) — this is GOOD, we catch bugs!
        // In release builds: it wraps around silently (undefined-ish behavior).
        // Our Cargo.toml has `overflow-checks = true` for firmware release profile,
        // so we're protected even in production. That's defense-in-depth.
        Self { raw: n * SCALE }
        // Equivalent to: n << 16, but multiplication reads more clearly for learning.
        // The compiler optimizes `* 65536` into `<< 16` anyway — it's free.
    }

    /// Create a Fixed from integer and fractional parts.
    ///
    /// # Arguments
    /// * `integer` — the whole number part (e.g., 3 in 3.75)
    /// * `frac_num` — numerator of the fraction
    /// * `frac_den` — denominator of the fraction
    ///
    /// # Example
    /// ```
    /// use sentinel_core::math::fixed_point::Fixed;
    /// // Represent 3.75 = 3 + 3/4
    /// let val = Fixed::from_parts(3, 3, 4);
    /// assert_eq!(val.to_int(), 3);
    /// ```
    #[inline]
    pub const fn from_parts(integer: i32, frac_num: i32, frac_den: i32) -> Self {
        // Integer part: shift left to put it in the upper 16 bits.
        // Fractional part: (frac_num * SCALE) / frac_den
        //   e.g., 3/4 → (3 * 65536) / 4 = 196608 / 4 = 49152
        //   49152 / 65536 = 0.75 ✓
        Self {
            raw: integer * SCALE + (frac_num * SCALE) / frac_den,
        }
    }

    /// Extract the integer part, discarding the fractional bits.
    ///
    /// This is a RIGHT SHIFT by 16 — it throws away the lower 16 bits.
    ///
    /// # Example
    /// ```
    /// use sentinel_core::math::fixed_point::Fixed;
    /// let val = Fixed::from_parts(7, 1, 2); // 7.5
    /// assert_eq!(val.to_int(), 7); // Fractional part is discarded
    /// ```
    #[inline]
    pub const fn to_int(self) -> i32 {
        // CONCEPT: `>>` Right Shift
        // Shifts all bits to the right, discarding the lower bits.
        // For signed integers (i32), this is an ARITHMETIC shift:
        //   - positive numbers get zero-filled from the left
        //   - negative numbers get one-filled (preserving the sign)
        // This is equivalent to integer division by 2^16 = 65536.
        self.raw >> FRAC_BITS
    }

    /// Get the raw underlying i32 value (for serialization, debugging, etc.)
    #[inline]
    pub const fn raw_value(self) -> i32 {
        self.raw
    }

    // =========================================================================
    // ARITHMETIC OPERATIONS
    // =========================================================================

    /// Add two fixed-point numbers.
    ///
    /// # Why is this so simple?
    /// Fixed-point addition is JUST integer addition!
    /// Because both values are scaled by the same factor:
    ///   (a × 65536) + (b × 65536) = (a + b) × 65536
    ///
    /// # Example
    /// ```
    /// use sentinel_core::math::fixed_point::Fixed;
    /// let a = Fixed::from_int(3);
    /// let b = Fixed::from_int(4);
    /// let c = a.add(b);
    /// assert_eq!(c.to_int(), 7);
    /// ```
    #[inline]
    pub const fn add(self, rhs: Self) -> Self {
        // CONCEPT: `rhs` stands for "right-hand side" — a convention in math/programming.
        // `self` is the left-hand side (the value you call the method on).
        Self {
            raw: self.raw + rhs.raw,
        }
    }

    /// Subtract one fixed-point number from another.
    ///
    /// Same logic as add — just integer subtraction.
    #[inline]
    pub const fn sub(self, rhs: Self) -> Self {
        Self {
            raw: self.raw - rhs.raw,
        }
    }

    /// Multiply two fixed-point numbers.
    ///
    /// # ⚠️ This is the TRICKY one — read carefully!
    ///
    /// If we just did `self.raw * rhs.raw`, the scale factor would be DOUBLED:
    ///   (a × 65536) × (b × 65536) = a × b × 65536² = a × b × 4,294,967,296
    ///
    /// That's wrong AND overflows i32! We need to divide by one SCALE to fix it.
    ///
    /// But there's another problem: `self.raw * rhs.raw` might overflow i32
    /// BEFORE we get a chance to divide. Solution: use i64 for the intermediate.
    ///
    /// # Example
    /// ```
    /// use sentinel_core::math::fixed_point::Fixed;
    /// let a = Fixed::from_parts(2, 1, 2); // 2.5
    /// let b = Fixed::from_int(4);          // 4.0
    /// let c = a.mul(b);
    /// assert_eq!(c.to_int(), 10);          // 2.5 × 4 = 10.0 ✓
    /// ```
    #[inline]
    pub const fn mul(self, rhs: Self) -> Self {
        // Step 1: Widen to 64 bits to prevent overflow.
        // CONCEPT: `as i64` — Type Casting
        //   Converts i32 → i64. This is a LOSSLESS widening cast
        //   (every i32 value fits perfectly in i64).
        //   The `as` keyword does raw bit-level conversion.
        let wide = self.raw as i64 * rhs.raw as i64;

        // Step 2: Divide by SCALE (shift right by 16) to restore the correct scale.
        // Then narrow back to i32.
        // NOTE: If the result exceeds i32 range, this will truncate silently!
        //       For Sentinel's vibration data (small sensor values), this is fine.
        Self {
            raw: (wide >> FRAC_BITS) as i32,
        }
    }

    /// Divide one fixed-point number by another.
    ///
    /// # The reverse of multiplication's problem
    ///
    /// If we just did `self.raw / rhs.raw`, the scale factors would CANCEL:
    ///   (a × 65536) / (b × 65536) = a / b  ← no scale factor left!
    ///
    /// We fix this by pre-multiplying the numerator by SCALE before dividing.
    ///
    /// # Panics
    /// Panics if `rhs` is zero (just like integer division).
    ///
    /// # Example
    /// ```
    /// use sentinel_core::math::fixed_point::Fixed;
    /// let a = Fixed::from_int(10);
    /// let b = Fixed::from_int(4);
    /// let c = a.div(b);
    /// assert_eq!(c, Fixed::from_parts(2, 1, 2)); // 10 / 4 = 2.5
    /// ```
    #[inline]
    pub const fn div(self, rhs: Self) -> Self {
        // Widen numerator and pre-shift to maintain scale.
        let wide = (self.raw as i64) << FRAC_BITS;
        Self {
            raw: (wide / rhs.raw as i64) as i32,
        }
    }

    /// Scale by an integer (multiply without the double-scaling problem).
    ///
    /// This is more efficient than `mul()` when one operand is a whole number,
    /// because we don't need the shift correction.
    ///
    /// # Example
    /// ```
    /// use sentinel_core::math::fixed_point::Fixed;
    /// let val = Fixed::from_parts(1, 1, 2); // 1.5
    /// let scaled = val.scale(3);              // 1.5 × 3 = 4.5
    /// assert_eq!(scaled.to_int(), 4);         // Integer part is 4
    /// ```
    #[inline]
    pub const fn scale(self, factor: i32) -> Self {
        // No need for i64 widening IF factor is small (which it usually is for our use case).
        // But to be safe with larger factors, we widen.
        Self {
            raw: ((self.raw as i64 * factor as i64) as i32),
        }
    }

    /// Absolute value — important for magnitude calculations.
    ///
    /// # Why we need this
    /// Vibration signals oscillate around zero. To measure amplitude (how "hard"
    /// something is vibrating), we often need |value| — the distance from zero.
    ///
    /// # Panics
    /// Panics on i32::MIN because its absolute value (2,147,483,648) doesn't fit in i32!
    /// This is a fundamental limitation of two's complement representation.
    #[inline]
    pub const fn abs(self) -> Self {
        // CONCEPT: Conditional Expression
        // Rust's `if` is an EXPRESSION — it returns a value, not just controls flow.
        // This is equivalent to the ternary operator `? :` in C/Java.
        Self {
            raw: if self.raw < 0 { -self.raw } else { self.raw },
        }
    }

    /// Check if this value is negative.
    #[inline]
    pub const fn is_negative(self) -> bool {
        self.raw < 0
    }

    /// The fixed-point representation of zero.
    ///
    /// CONCEPT: Associated Constants
    /// These are constants that live on the TYPE, not on instances.
    /// Access with `Fixed::ZERO`, not `my_fixed.ZERO`.
    pub const ZERO: Self = Self { raw: 0 };

    /// The fixed-point representation of 1.0.
    pub const ONE: Self = Self { raw: SCALE };

    /// Maximum representable value ≈ 32767.99998
    pub const MAX: Self = Self { raw: i32::MAX };

    /// Minimum representable value = -32768.0
    pub const MIN: Self = Self { raw: i32::MIN };
}

// =============================================================================
// CONCEPT: Operator Overloading via core::ops Traits
// =============================================================================
//
// In Rust, `a + b` is syntactic sugar for `a.add(b)`.
// To make `+` work with our type, we implement the `core::ops::Add` trait.
//
// WHY `core::ops` instead of `std::ops`?
// Because this crate is `#![no_std]` — we don't have the standard library!
// `core` is the subset of `std` that works EVERYWHERE, even bare metal.
// `core::ops` contains the same traits as `std::ops`.

use core::ops;

impl ops::Add for Fixed {
    // CONCEPT: Associated Types
    // The `Add` trait has an "associated type" called `Output`.
    // This tells Rust what type `a + b` returns.
    // For us, Fixed + Fixed = Fixed.
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        // Delegate to our `const fn add` method above.
        Self::add(self, rhs)
    }
}

impl ops::Sub for Fixed {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self::sub(self, rhs)
    }
}

impl ops::Mul for Fixed {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        Self::mul(self, rhs)
    }
}

impl ops::Div for Fixed {
    type Output = Self;

    #[inline]
    fn div(self, rhs: Self) -> Self::Output {
        Self::div(self, rhs)
    }
}

/// Implement negation so `-val` works (unary minus).
impl ops::Neg for Fixed {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self::Output {
        Self { raw: -self.raw }
    }
}

// =============================================================================
// CONCEPT: Display Trait — Human-Readable Output
// =============================================================================
//
// `core::fmt::Display` controls what happens when you write:
//   println!("{}", my_fixed);  ← calls Display
// vs:
//   println!("{:?}", my_fixed); ← calls Debug (auto-derived above)
//
// Display gives PRETTY output: "3.750000"
// Debug gives STRUCTURAL output: "Fixed { raw: 245760 }"

impl core::fmt::Display for Fixed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Convert to integer and fractional parts for display.
        let int_part = self.raw >> FRAC_BITS;
        let frac_part = self.raw & (SCALE - 1); // Mask lower 16 bits

        // CONCEPT: Bitwise AND Masking
        // `SCALE - 1` = 65535 = 0xFFFF = 1111_1111_1111_1111 in binary
        // `raw & 0xFFFF` extracts ONLY the lower 16 bits (the fractional part).
        // This is a standard bit-manipulation technique.

        if self.raw < 0 && frac_part != 0 {
            // Handle negative numbers with fractional parts carefully
            let abs_raw = if self.raw == i32::MIN {
                // Can't negate i32::MIN, just display as min
                return write!(f, "-32768.00000");
            } else {
                (-self.raw) as u32
            };
            let abs_int = abs_raw >> FRAC_BITS;
            let abs_frac = abs_raw & (SCALE as u32 - 1);
            let decimal = (abs_frac as u64 * 100_000) >> FRAC_BITS;
            write!(f, "-{abs_int}.{decimal:05}")
        } else {
            let abs_frac = if frac_part >= 0 {
                frac_part as u32
            } else {
                (-frac_part) as u32
            };
            let decimal = (abs_frac as u64 * 100_000) >> FRAC_BITS;
            write!(f, "{int_part}.{decimal:05}")
        }
    }
}

// =============================================================================
// TESTS — The Bottom of the File
// =============================================================================
//
// CONCEPT: #[cfg(test)] — Conditional Compilation
//
// `cfg` = "configuration". This tells the compiler:
//   "Only compile this module when running `cargo test`."
//   In a normal build or release, this code DOES NOT EXIST in the binary.
//   Zero cost, zero flash space on the MCU.
//
// CONCEPT: `mod tests` — A Nested Module
//   Tests live in their own module. The `use super::*;` imports everything
//   from the parent module (our Fixed type and its methods).

#[cfg(test)]
mod tests {
    use super::*;

    // CONCEPT: #[test] — Test Function Attribute
    // The `#[test]` attribute marks a function as a test case.
    // `cargo test` finds and runs all functions with this attribute.

    #[test]
    fn test_scaling() {
        // =====================================================================
        // Test 1: Verify that from_int correctly scales integers
        // =====================================================================
        // When we say Fixed::from_int(1), internally it stores 1 × 65536 = 65536.
        // This is the fundamental invariant of our fixed-point system.
        let one = Fixed::from_int(1);
        assert_eq!(one.raw_value(), SCALE); // raw should be exactly 65536

        // And converting back should give us 1
        assert_eq!(one.to_int(), 1);

        // Test with zero
        let zero = Fixed::from_int(0);
        assert_eq!(zero.raw_value(), 0);
        assert_eq!(zero.to_int(), 0);

        // Test with negative
        let neg = Fixed::from_int(-5);
        assert_eq!(neg.to_int(), -5);
        assert_eq!(neg.raw_value(), -5 * SCALE);
    }

    #[test]
    fn test_addition() {
        // =====================================================================
        // Test 2: a + b should work like regular addition
        // =====================================================================
        let a = Fixed::from_int(3);
        let b = Fixed::from_int(4);
        let c = a + b; // This calls our ops::Add implementation!
        assert_eq!(c.to_int(), 7);

        // Test with fractional parts: 1.5 + 2.25 = 3.75
        let x = Fixed::from_parts(1, 1, 2); // 1.5
        let y = Fixed::from_parts(2, 1, 4); // 2.25
        let z = x + y;
        assert_eq!(z, Fixed::from_parts(3, 3, 4)); // 3.75
    }

    #[test]
    fn test_subtraction() {
        let a = Fixed::from_int(10);
        let b = Fixed::from_int(3);
        let c = a - b;
        assert_eq!(c.to_int(), 7);

        // Test negative result
        let d = b - a;
        assert_eq!(d.to_int(), -7);
    }

    #[test]
    fn test_multiplication() {
        // =====================================================================
        // Test 3: Multiplication with fractional values
        // =====================================================================
        // 2.5 × 4.0 = 10.0
        let a = Fixed::from_parts(2, 1, 2); // 2.5
        let b = Fixed::from_int(4);
        let c = a * b;
        assert_eq!(c.to_int(), 10);

        // 3.0 × 3.0 = 9.0
        let x = Fixed::from_int(3);
        let y = Fixed::from_int(3);
        assert_eq!((x * y).to_int(), 9);
    }

    #[test]
    fn test_division() {
        // =====================================================================
        // Test 4: Division
        // =====================================================================
        // 10 / 4 = 2.5
        let a = Fixed::from_int(10);
        let b = Fixed::from_int(4);
        let c = a / b;
        assert_eq!(c, Fixed::from_parts(2, 1, 2));

        // 7 / 2 = 3.5
        let x = Fixed::from_int(7);
        let y = Fixed::from_int(2);
        let z = x / y;
        assert_eq!(z, Fixed::from_parts(3, 1, 2));
    }

    #[test]
    fn test_scale_by_integer() {
        // =====================================================================
        // Test 5: Scaling — the operation from your images!
        // =====================================================================
        // "Verify your math works on your laptop CPU right now"
        let val = Fixed::from_parts(1, 1, 2); // 1.5
        let scaled = val.scale(3);              // 1.5 × 3 = 4.5
        assert_eq!(scaled.to_int(), 4);         // Integer part is 4

        // Verify the raw value: 4.5 × 65536 = 294912
        assert_eq!(scaled.raw_value(), 294912);
    }

    #[test]
    fn test_negation() {
        let a = Fixed::from_int(5);
        let b = -a;
        assert_eq!(b.to_int(), -5);
        assert_eq!((-b).to_int(), 5); // Double negation
    }

    #[test]
    fn test_abs() {
        let pos = Fixed::from_int(5);
        let neg = Fixed::from_int(-5);
        assert_eq!(pos.abs(), pos);
        assert_eq!(neg.abs(), pos);
        assert_eq!(Fixed::ZERO.abs(), Fixed::ZERO);
    }

    #[test]
    fn test_comparison() {
        // =====================================================================
        // Test 6: Comparisons (Ord trait)
        // =====================================================================
        let small = Fixed::from_int(1);
        let big = Fixed::from_int(100);
        assert!(small < big);
        assert!(big > small);
        assert_eq!(small, Fixed::from_int(1));
    }

    #[test]
    fn test_constants() {
        assert_eq!(Fixed::ZERO.to_int(), 0);
        assert_eq!(Fixed::ONE.to_int(), 1);
        assert!(Fixed::MAX > Fixed::ZERO);
        assert!(Fixed::MIN < Fixed::ZERO);
    }

    #[test]
    fn test_from_raw_roundtrip() {
        // =====================================================================
        // Test 7: from_raw → raw_value roundtrip
        // =====================================================================
        // Whatever raw value goes in should come back out unchanged.
        let raw = 123456;
        let fixed = Fixed::from_raw(raw);
        assert_eq!(fixed.raw_value(), raw);
    }

    #[test]
    fn test_operator_overloading() {
        // =====================================================================
        // Test 8: Verify operators work (not just methods)
        // =====================================================================
        let a = Fixed::from_int(10);
        let b = Fixed::from_int(3);

        // These use the operator overloads (ops::Add, ops::Sub, etc.)
        let sum = a + b;
        let diff = a - b;
        let prod = a * b;
        let quot = a / b;

        assert_eq!(sum.to_int(), 13);
        assert_eq!(diff.to_int(), 7);
        assert_eq!(prod.to_int(), 30);
        assert_eq!(quot.to_int(), 3); // 10/3 = 3.333..., integer part = 3
    }
}
