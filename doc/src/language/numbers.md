Numbers
=======

{{#include ../links.md}}

Integer numbers follow C-style format with support for decimal, binary ('`0b`'), octal ('`0o`') and hex ('`0x`') notations.

The default system integer type (also aliased to `INT`) is `i64`. It can be turned into `i32` via the [`only_i32`] feature.

Floating-point numbers are also supported if not disabled with [`no_float`]. The default system floating-point type is `i64`
(also aliased to `FLOAT`).

Decimal numbers are also supported if explicitly enabled with ['decimal'].
Be aware that Decimal support replaces float support, meaning that ['decimla'] implies ['no_float']. 
The two features cannot be enabled at the same time.

'`_`' separators can be added freely and are ignored within a number.

| Format           | Type             |
| ---------------- | ---------------- |
| `123_345`, `-42` | `i64` in decimal |
| `0o07_76`        | `i64` in octal   |
| `0xabcd_ef`      | `i64` in hex     |
| `0b0101_1001`    | `i64` in binary  |
| `123_456.789`    | `f64`            |
