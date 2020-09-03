Numeric Functions
================

{{#include ../links.md}}

Integer Functions
----------------

The following standard functions (defined in the [`BasicMathPackage`][packages] but excluded if using a [raw `Engine`])
operate on `i8`, `i16`, `i32`, `i64`, `f32` and `f64` only:

| Function     | Description                                                             |
| ------------ | ----------------------------------------------------------------------- |
| `abs`        | absolute value                                                          |
| `sign`       | returns -1 (`INT`) if the number is negative, +1 if positive, 0 if zero |
| [`to_float`] | converts an integer type to `FLOAT`                                     |

Floating-Point Functions
-----------------------

The following standard functions (defined in the [`BasicMathPackage`][packages] but excluded if using a [raw `Engine`])
operate on `f64` only:

| Category         | Functions                                                             |
| ---------------- | --------------------------------------------------------------------- |
| Trigonometry     | `sin`, `cos`, `tan`, `sinh`, `cosh`, `tanh` in degrees                |
| Arc-trigonometry | `asin`, `acos`, `atan`, `asinh`, `acosh`, `atanh` in degrees          |
| Square root      | `sqrt`                                                                |
| Exponential      | `exp` (base _e_)                                                      |
| Logarithmic      | `ln` (base _e_), `log10` (base 10), `log` (any base)                  |
| Rounding         | `floor`, `ceiling`, `round`, `int`, `fraction` methods and properties |
| Conversion       | [`to_int`]                                                            |
| Testing          | `is_nan`, `is_finite`, `is_infinite` methods and properties           |
