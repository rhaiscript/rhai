//! Helper module that allows registration of the _core library_ and
//! _standard library_ of utility functions.

use crate::any::{Any, Dynamic};
use crate::engine::{Engine, FUNC_TO_STRING, KEYWORD_DEBUG, KEYWORD_PRINT};
use crate::fn_register::{RegisterDynamicFn, RegisterFn, RegisterResultFn};
use crate::parser::{Position, INT};
use crate::result::EvalAltResult;

#[cfg(not(feature = "no_index"))]
use crate::engine::Array;

#[cfg(not(feature = "no_object"))]
use crate::engine::Map;

#[cfg(not(feature = "no_float"))]
use crate::parser::FLOAT;

use num_traits::{
    identities::Zero, CheckedAdd, CheckedDiv, CheckedMul, CheckedNeg, CheckedRem, CheckedShl,
    CheckedShr, CheckedSub,
};

use crate::stdlib::{
    boxed::Box,
    fmt::{Debug, Display},
    format,
    ops::{Add, BitAnd, BitOr, BitXor, Div, Mul, Neg, Range, Rem, Shl, Shr, Sub},
    string::{String, ToString},
    time::Instant,
    vec::Vec,
    {i32, i64, u32},
};

macro_rules! reg_op {
    ($self:expr, $x:expr, $op:expr, $( $y:ty ),*) => (
        $(
            $self.register_fn($x, $op as fn(x: $y, y: $y)->$y);
        )*
    )
}

#[cfg(not(feature = "unchecked"))]
macro_rules! reg_op_result {
    ($self:expr, $x:expr, $op:expr, $( $y:ty ),*) => (
        $(
            $self.register_result_fn($x, $op as fn(x: $y, y: $y)->Result<$y,EvalAltResult>);
        )*
    )
}

#[cfg(not(feature = "unchecked"))]
macro_rules! reg_op_result1 {
    ($self:expr, $x:expr, $op:expr, $v:ty, $( $y:ty ),*) => (
        $(
            $self.register_result_fn($x, $op as fn(x: $y, y: $v)->Result<$y,EvalAltResult>);
        )*
    )
}

macro_rules! reg_cmp {
    ($self:expr, $x:expr, $op:expr, $( $y:ty ),*) => (
        $(
            $self.register_fn($x, $op as fn(x: $y, y: $y)->bool);
        )*
    )
}

// Comparison operators
fn lt<T: PartialOrd>(x: T, y: T) -> bool {
    x < y
}
fn lte<T: PartialOrd>(x: T, y: T) -> bool {
    x <= y
}
fn gt<T: PartialOrd>(x: T, y: T) -> bool {
    x > y
}
fn gte<T: PartialOrd>(x: T, y: T) -> bool {
    x >= y
}
fn eq<T: PartialEq>(x: T, y: T) -> bool {
    x == y
}
fn ne<T: PartialEq>(x: T, y: T) -> bool {
    x != y
}

impl Engine<'_> {
    /// Register the core built-in library.
    pub(crate) fn register_core_lib(&mut self) {
        // Checked add
        #[cfg(not(feature = "unchecked"))]
        fn add<T: Display + CheckedAdd>(x: T, y: T) -> Result<T, EvalAltResult> {
            x.checked_add(&y).ok_or_else(|| {
                EvalAltResult::ErrorArithmetic(
                    format!("Addition overflow: {} + {}", x, y),
                    Position::none(),
                )
            })
        }
        // Checked subtract
        #[cfg(not(feature = "unchecked"))]
        fn sub<T: Display + CheckedSub>(x: T, y: T) -> Result<T, EvalAltResult> {
            x.checked_sub(&y).ok_or_else(|| {
                EvalAltResult::ErrorArithmetic(
                    format!("Subtraction underflow: {} - {}", x, y),
                    Position::none(),
                )
            })
        }
        // Checked multiply
        #[cfg(not(feature = "unchecked"))]
        fn mul<T: Display + CheckedMul>(x: T, y: T) -> Result<T, EvalAltResult> {
            x.checked_mul(&y).ok_or_else(|| {
                EvalAltResult::ErrorArithmetic(
                    format!("Multiplication overflow: {} * {}", x, y),
                    Position::none(),
                )
            })
        }
        // Checked divide
        #[cfg(not(feature = "unchecked"))]
        fn div<T>(x: T, y: T) -> Result<T, EvalAltResult>
        where
            T: Display + CheckedDiv + PartialEq + Zero,
        {
            // Detect division by zero
            if y == T::zero() {
                return Err(EvalAltResult::ErrorArithmetic(
                    format!("Division by zero: {} / {}", x, y),
                    Position::none(),
                ));
            }

            x.checked_div(&y).ok_or_else(|| {
                EvalAltResult::ErrorArithmetic(
                    format!("Division overflow: {} / {}", x, y),
                    Position::none(),
                )
            })
        }
        // Checked negative - e.g. -(i32::MIN) will overflow i32::MAX
        #[cfg(not(feature = "unchecked"))]
        fn neg<T: Display + CheckedNeg>(x: T) -> Result<T, EvalAltResult> {
            x.checked_neg().ok_or_else(|| {
                EvalAltResult::ErrorArithmetic(
                    format!("Negation overflow: -{}", x),
                    Position::none(),
                )
            })
        }
        // Checked absolute
        #[cfg(not(feature = "unchecked"))]
        fn abs<T: Display + CheckedNeg + PartialOrd + Zero>(x: T) -> Result<T, EvalAltResult> {
            // FIX - We don't use Signed::abs() here because, contrary to documentation, it panics
            //       when the number is ::MIN instead of returning ::MIN itself.
            if x >= <T as Zero>::zero() {
                Ok(x)
            } else {
                x.checked_neg().ok_or_else(|| {
                    EvalAltResult::ErrorArithmetic(
                        format!("Negation overflow: -{}", x),
                        Position::none(),
                    )
                })
            }
        }
        // Unchecked add - may panic on overflow
        #[cfg(any(feature = "unchecked", not(feature = "no_float")))]
        fn add_u<T: Add>(x: T, y: T) -> <T as Add>::Output {
            x + y
        }
        // Unchecked subtract - may panic on underflow
        #[cfg(any(feature = "unchecked", not(feature = "no_float")))]
        fn sub_u<T: Sub>(x: T, y: T) -> <T as Sub>::Output {
            x - y
        }
        // Unchecked multiply - may panic on overflow
        #[cfg(any(feature = "unchecked", not(feature = "no_float")))]
        fn mul_u<T: Mul>(x: T, y: T) -> <T as Mul>::Output {
            x * y
        }
        // Unchecked divide - may panic when dividing by zero
        #[cfg(any(feature = "unchecked", not(feature = "no_float")))]
        fn div_u<T: Div>(x: T, y: T) -> <T as Div>::Output {
            x / y
        }
        // Unchecked negative - may panic on overflow
        #[cfg(any(feature = "unchecked", not(feature = "no_float")))]
        fn neg_u<T: Neg>(x: T) -> <T as Neg>::Output {
            -x
        }
        // Unchecked absolute - may panic on overflow
        #[cfg(any(feature = "unchecked", not(feature = "no_float")))]
        fn abs_u<T>(x: T) -> <T as Neg>::Output
        where
            T: Neg + PartialOrd + Default + Into<<T as Neg>::Output>,
        {
            // Numbers should default to zero
            if x < Default::default() {
                -x
            } else {
                x.into()
            }
        }

        // Logic operators
        fn and(x: bool, y: bool) -> bool {
            x && y
        }
        fn or(x: bool, y: bool) -> bool {
            x || y
        }
        fn not(x: bool) -> bool {
            !x
        }

        // Bit operators
        fn binary_and<T: BitAnd>(x: T, y: T) -> <T as BitAnd>::Output {
            x & y
        }
        fn binary_or<T: BitOr>(x: T, y: T) -> <T as BitOr>::Output {
            x | y
        }
        fn binary_xor<T: BitXor>(x: T, y: T) -> <T as BitXor>::Output {
            x ^ y
        }

        // Checked left-shift
        #[cfg(not(feature = "unchecked"))]
        fn shl<T: Display + CheckedShl>(x: T, y: INT) -> Result<T, EvalAltResult> {
            // Cannot shift by a negative number of bits
            if y < 0 {
                return Err(EvalAltResult::ErrorArithmetic(
                    format!("Left-shift by a negative number: {} << {}", x, y),
                    Position::none(),
                ));
            }

            CheckedShl::checked_shl(&x, y as u32).ok_or_else(|| {
                EvalAltResult::ErrorArithmetic(
                    format!("Left-shift by too many bits: {} << {}", x, y),
                    Position::none(),
                )
            })
        }
        // Checked right-shift
        #[cfg(not(feature = "unchecked"))]
        fn shr<T: Display + CheckedShr>(x: T, y: INT) -> Result<T, EvalAltResult> {
            // Cannot shift by a negative number of bits
            if y < 0 {
                return Err(EvalAltResult::ErrorArithmetic(
                    format!("Right-shift by a negative number: {} >> {}", x, y),
                    Position::none(),
                ));
            }

            CheckedShr::checked_shr(&x, y as u32).ok_or_else(|| {
                EvalAltResult::ErrorArithmetic(
                    format!("Right-shift by too many bits: {} % {}", x, y),
                    Position::none(),
                )
            })
        }
        // Unchecked left-shift - may panic if shifting by a negative number of bits
        #[cfg(feature = "unchecked")]
        fn shl_u<T: Shl<T>>(x: T, y: T) -> <T as Shl<T>>::Output {
            x.shl(y)
        }
        // Unchecked right-shift - may panic if shifting by a negative number of bits
        #[cfg(feature = "unchecked")]
        fn shr_u<T: Shr<T>>(x: T, y: T) -> <T as Shr<T>>::Output {
            x.shr(y)
        }
        // Checked modulo
        #[cfg(not(feature = "unchecked"))]
        fn modulo<T: Display + CheckedRem>(x: T, y: T) -> Result<T, EvalAltResult> {
            x.checked_rem(&y).ok_or_else(|| {
                EvalAltResult::ErrorArithmetic(
                    format!("Modulo division by zero or overflow: {} % {}", x, y),
                    Position::none(),
                )
            })
        }
        // Unchecked modulo - may panic if dividing by zero
        #[cfg(any(feature = "unchecked", not(feature = "no_float")))]
        fn modulo_u<T: Rem>(x: T, y: T) -> <T as Rem>::Output {
            x % y
        }
        // Checked power
        #[cfg(not(feature = "unchecked"))]
        fn pow_i_i(x: INT, y: INT) -> Result<INT, EvalAltResult> {
            #[cfg(not(feature = "only_i32"))]
            {
                if y > (u32::MAX as INT) {
                    Err(EvalAltResult::ErrorArithmetic(
                        format!("Integer raised to too large an index: {} ~ {}", x, y),
                        Position::none(),
                    ))
                } else if y < 0 {
                    Err(EvalAltResult::ErrorArithmetic(
                        format!("Integer raised to a negative index: {} ~ {}", x, y),
                        Position::none(),
                    ))
                } else {
                    x.checked_pow(y as u32).ok_or_else(|| {
                        EvalAltResult::ErrorArithmetic(
                            format!("Power overflow: {} ~ {}", x, y),
                            Position::none(),
                        )
                    })
                }
            }

            #[cfg(feature = "only_i32")]
            {
                if y < 0 {
                    Err(EvalAltResult::ErrorArithmetic(
                        format!("Integer raised to a negative index: {} ~ {}", x, y),
                        Position::none(),
                    ))
                } else {
                    x.checked_pow(y as u32).ok_or_else(|| {
                        EvalAltResult::ErrorArithmetic(
                            format!("Power overflow: {} ~ {}", x, y),
                            Position::none(),
                        )
                    })
                }
            }
        }
        // Unchecked integer power - may panic on overflow or if the power index is too high (> u32::MAX)
        #[cfg(feature = "unchecked")]
        fn pow_i_i_u(x: INT, y: INT) -> INT {
            x.pow(y as u32)
        }
        // Floating-point power - always well-defined
        #[cfg(not(feature = "no_float"))]
        fn pow_f_f(x: FLOAT, y: FLOAT) -> FLOAT {
            x.powf(y)
        }
        // Checked power
        #[cfg(not(feature = "unchecked"))]
        #[cfg(not(feature = "no_float"))]
        fn pow_f_i(x: FLOAT, y: INT) -> Result<FLOAT, EvalAltResult> {
            // Raise to power that is larger than an i32
            if y > (i32::MAX as INT) {
                return Err(EvalAltResult::ErrorArithmetic(
                    format!("Number raised to too large an index: {} ~ {}", x, y),
                    Position::none(),
                ));
            }

            Ok(x.powi(y as i32))
        }
        // Unchecked power - may be incorrect if the power index is too high (> i32::MAX)
        #[cfg(feature = "unchecked")]
        #[cfg(not(feature = "no_float"))]
        fn pow_f_i_u(x: FLOAT, y: INT) -> FLOAT {
            x.powi(y as i32)
        }

        #[cfg(not(feature = "unchecked"))]
        {
            reg_op_result!(self, "+", add, INT);
            reg_op_result!(self, "-", sub, INT);
            reg_op_result!(self, "*", mul, INT);
            reg_op_result!(self, "/", div, INT);

            #[cfg(not(feature = "only_i32"))]
            #[cfg(not(feature = "only_i64"))]
            {
                reg_op_result!(self, "+", add, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_op_result!(self, "-", sub, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_op_result!(self, "*", mul, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_op_result!(self, "/", div, i8, u8, i16, u16, i32, i64, u32, u64);
            }
        }

        #[cfg(feature = "unchecked")]
        {
            reg_op!(self, "+", add_u, INT);
            reg_op!(self, "-", sub_u, INT);
            reg_op!(self, "*", mul_u, INT);
            reg_op!(self, "/", div_u, INT);

            #[cfg(not(feature = "only_i32"))]
            #[cfg(not(feature = "only_i64"))]
            {
                reg_op!(self, "+", add_u, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_op!(self, "-", sub_u, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_op!(self, "*", mul_u, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_op!(self, "/", div_u, i8, u8, i16, u16, i32, i64, u32, u64);
            }
        }

        #[cfg(not(feature = "no_float"))]
        {
            reg_op!(self, "+", add_u, f32, f64);
            reg_op!(self, "-", sub_u, f32, f64);
            reg_op!(self, "*", mul_u, f32, f64);
            reg_op!(self, "/", div_u, f32, f64);
        }

        {
            reg_cmp!(self, "<", lt, INT, String, char);
            reg_cmp!(self, "<=", lte, INT, String, char);
            reg_cmp!(self, ">", gt, INT, String, char);
            reg_cmp!(self, ">=", gte, INT, String, char);
            reg_cmp!(self, "==", eq, INT, String, char, bool);
            reg_cmp!(self, "!=", ne, INT, String, char, bool);

            #[cfg(not(feature = "only_i32"))]
            #[cfg(not(feature = "only_i64"))]
            {
                reg_cmp!(self, "<", lt, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_cmp!(self, "<=", lte, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_cmp!(self, ">", gt, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_cmp!(self, ">=", gte, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_cmp!(self, "==", eq, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_cmp!(self, "!=", ne, i8, u8, i16, u16, i32, i64, u32, u64);
            }

            #[cfg(not(feature = "no_float"))]
            {
                reg_cmp!(self, "<", lt, f32, f64);
                reg_cmp!(self, "<=", lte, f32, f64);
                reg_cmp!(self, ">", gt, f32, f64);
                reg_cmp!(self, ">=", gte, f32, f64);
                reg_cmp!(self, "==", eq, f32, f64);
                reg_cmp!(self, "!=", ne, f32, f64);
            }
        }

        // `&&` and `||` are treated specially as they short-circuit.
        // They are implemented as special `Expr` Instants, not function calls.
        //reg_op!(self, "||", or, bool);
        //reg_op!(self, "&&", and, bool);

        reg_op!(self, "|", or, bool);
        reg_op!(self, "&", and, bool);

        reg_op!(self, "|", binary_or, INT);
        reg_op!(self, "&", binary_and, INT);
        reg_op!(self, "^", binary_xor, INT);

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            reg_op!(self, "|", binary_or, i8, u8, i16, u16, i32, i64, u32, u64);
            reg_op!(self, "&", binary_and, i8, u8, i16, u16, i32, i64, u32, u64);
            reg_op!(self, "^", binary_xor, i8, u8, i16, u16, i32, i64, u32, u64);
        }

        #[cfg(not(feature = "unchecked"))]
        {
            reg_op_result1!(self, "<<", shl, INT, INT);
            reg_op_result1!(self, ">>", shr, INT, INT);
            reg_op_result!(self, "%", modulo, INT);

            #[cfg(not(feature = "only_i32"))]
            #[cfg(not(feature = "only_i64"))]
            {
                reg_op_result1!(self, "<<", shl, i64, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_op_result1!(self, ">>", shr, i64, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_op_result!(self, "%", modulo, i8, u8, i16, u16, i32, i64, u32, u64);
            }
        }

        #[cfg(feature = "unchecked")]
        {
            reg_op!(self, "<<", shl_u, INT, INT);
            reg_op!(self, ">>", shr_u, INT, INT);
            reg_op!(self, "%", modulo_u, INT);

            #[cfg(not(feature = "only_i32"))]
            #[cfg(not(feature = "only_i64"))]
            {
                reg_op!(self, "<<", shl_u, i64, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_op!(self, ">>", shr_u, i64, i8, u8, i16, u16, i32, i64, u32, u64);
                reg_op!(self, "%", modulo_u, i8, u8, i16, u16, i32, i64, u32, u64);
            }
        }

        #[cfg(not(feature = "no_float"))]
        {
            reg_op!(self, "%", modulo_u, f32, f64);
            self.register_fn("~", pow_f_f);
        }

        #[cfg(not(feature = "unchecked"))]
        {
            self.register_result_fn("~", pow_i_i);

            #[cfg(not(feature = "no_float"))]
            self.register_result_fn("~", pow_f_i);
        }

        #[cfg(feature = "unchecked")]
        {
            self.register_fn("~", pow_i_i_u);

            #[cfg(not(feature = "no_float"))]
            self.register_fn("~", pow_f_i_u);
        }

        {
            macro_rules! reg_un {
                ($self:expr, $x:expr, $op:expr, $( $y:ty ),*) => (
                    $(
                        $self.register_fn($x, $op as fn(x: $y)->$y);
                    )*
                )
            }

            #[cfg(not(feature = "unchecked"))]
            macro_rules! reg_un_result {
                ($self:expr, $x:expr, $op:expr, $( $y:ty ),*) => (
                    $(
                        $self.register_result_fn($x, $op as fn(x: $y)->Result<$y,EvalAltResult>);
                    )*
                )
            }

            #[cfg(not(feature = "unchecked"))]
            {
                reg_un_result!(self, "-", neg, INT);
                reg_un_result!(self, "abs", abs, INT);

                #[cfg(not(feature = "only_i32"))]
                #[cfg(not(feature = "only_i64"))]
                {
                    reg_un_result!(self, "-", neg, i8, i16, i32, i64);
                    reg_un_result!(self, "abs", abs, i8, i16, i32, i64);
                }
            }

            #[cfg(feature = "unchecked")]
            {
                reg_un!(self, "-", neg_u, INT);
                reg_un!(self, "abs", abs_u, INT);

                #[cfg(not(feature = "only_i32"))]
                #[cfg(not(feature = "only_i64"))]
                {
                    reg_un!(self, "-", neg_u, i8, i16, i32, i64);
                    reg_un!(self, "abs", abs_u, i8, i16, i32, i64);
                }
            }

            #[cfg(not(feature = "no_float"))]
            {
                reg_un!(self, "-", neg_u, f32, f64);
                reg_un!(self, "abs", abs_u, f32, f64);
            }

            reg_un!(self, "!", not, bool);
        }

        self.register_fn("+", |x: String, y: String| x + &y); // String + String
        self.register_fn("==", |_: (), _: ()| true); // () == ()

        // Register print and debug
        fn to_debug<T: Debug>(x: T) -> String {
            format!("{:?}", x)
        }
        fn to_string<T: Display>(x: T) -> String {
            format!("{}", x)
        }

        {
            macro_rules! reg_fn1 {
                ($self:expr, $x:expr, $op:expr, $r:ty, $( $y:ty ),*) => (
                    $(
                        $self.register_fn($x, $op as fn(x: $y)->$r);
                    )*
                )
            }

            reg_fn1!(self, KEYWORD_PRINT, to_string, String, INT, bool);
            reg_fn1!(self, FUNC_TO_STRING, to_string, String, INT, bool);
            reg_fn1!(self, KEYWORD_PRINT, to_string, String, char, String);
            reg_fn1!(self, FUNC_TO_STRING, to_string, String, char, String);
            self.register_fn(KEYWORD_PRINT, || "".to_string());
            self.register_fn(KEYWORD_PRINT, |_: ()| "".to_string());
            self.register_fn(FUNC_TO_STRING, |_: ()| "".to_string());
            reg_fn1!(self, KEYWORD_DEBUG, to_debug, String, INT, bool, ());
            reg_fn1!(self, KEYWORD_DEBUG, to_debug, String, char, String);

            #[cfg(not(feature = "only_i32"))]
            #[cfg(not(feature = "only_i64"))]
            {
                reg_fn1!(self, KEYWORD_PRINT, to_string, String, i8, u8, i16, u16);
                reg_fn1!(self, FUNC_TO_STRING, to_string, String, i8, u8, i16, u16);
                reg_fn1!(self, KEYWORD_PRINT, to_string, String, i32, i64, u32, u64);
                reg_fn1!(self, FUNC_TO_STRING, to_string, String, i32, i64, u32, u64);
                reg_fn1!(self, KEYWORD_DEBUG, to_debug, String, i8, u8, i16, u16);
                reg_fn1!(self, KEYWORD_DEBUG, to_debug, String, i32, i64, u32, u64);
            }

            #[cfg(not(feature = "no_float"))]
            {
                reg_fn1!(self, KEYWORD_PRINT, to_string, String, f32, f64);
                reg_fn1!(self, FUNC_TO_STRING, to_string, String, f32, f64);
                reg_fn1!(self, KEYWORD_DEBUG, to_debug, String, f32, f64);
            }

            #[cfg(not(feature = "no_index"))]
            {
                reg_fn1!(self, KEYWORD_PRINT, to_debug, String, Array);
                reg_fn1!(self, FUNC_TO_STRING, to_debug, String, Array);
                reg_fn1!(self, KEYWORD_DEBUG, to_debug, String, Array);

                // Register array iterator
                self.register_iterator::<Array, _>(|a: &Dynamic| {
                    Box::new(a.downcast_ref::<Array>().unwrap().clone().into_iter())
                        as Box<dyn Iterator<Item = Dynamic>>
                });
            }

            #[cfg(not(feature = "no_object"))]
            {
                self.register_fn(KEYWORD_PRINT, |x: &mut Map| format!("#{:?}", x));
                self.register_fn(FUNC_TO_STRING, |x: &mut Map| format!("#{:?}", x));
                self.register_fn(KEYWORD_DEBUG, |x: &mut Map| format!("#{:?}", x));

                // Register map access functions
                #[cfg(not(feature = "no_index"))]
                self.register_fn("keys", |map: Map| {
                    map.into_iter()
                        .map(|(k, _)| k.into_dynamic())
                        .collect::<Vec<_>>()
                });

                #[cfg(not(feature = "no_index"))]
                self.register_fn("values", |map: Map| {
                    map.into_iter().map(|(_, v)| v).collect::<Vec<_>>()
                });
            }
        }

        // Register range function
        fn reg_range<T: Any + Clone>(engine: &mut Engine)
        where
            Range<T>: Iterator<Item = T>,
        {
            engine.register_iterator::<Range<T>, _>(|a: &Dynamic| {
                Box::new(
                    a.downcast_ref::<Range<T>>()
                        .unwrap()
                        .clone()
                        .map(|x| x.into_dynamic()),
                ) as Box<dyn Iterator<Item = Dynamic>>
            });
        }

        reg_range::<INT>(self);
        self.register_fn("range", |i1: INT, i2: INT| (i1..i2));

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            macro_rules! reg_range {
                ($self:expr, $x:expr, $( $y:ty ),*) => (
                    $(
                        reg_range::<$y>(self);
                        $self.register_fn($x, (|x: $y, y: $y| x..y) as fn(x: $y, y: $y)->Range<$y>);
                    )*
                )
            }

            reg_range!(self, "range", i8, u8, i16, u16, i32, i64, u32, u64);
        }

        // Register range function with step
        #[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
        struct StepRange<T>(T, T, T)
        where
            for<'a> &'a T: Add<&'a T, Output = T>,
            T: Any + Clone + PartialOrd;

        impl<T> Iterator for StepRange<T>
        where
            for<'a> &'a T: Add<&'a T, Output = T>,
            T: Any + Clone + PartialOrd,
        {
            type Item = T;

            fn next(&mut self) -> Option<T> {
                if self.0 < self.1 {
                    let v = self.0.clone();
                    self.0 = &v + &self.2;
                    Some(v)
                } else {
                    None
                }
            }
        }

        fn reg_step<T>(engine: &mut Engine)
        where
            for<'a> &'a T: Add<&'a T, Output = T>,
            T: Any + Clone + PartialOrd,
            StepRange<T>: Iterator<Item = T>,
        {
            engine.register_iterator::<StepRange<T>, _>(|a: &Dynamic| {
                Box::new(
                    a.downcast_ref::<StepRange<T>>()
                        .unwrap()
                        .clone()
                        .map(|x| x.into_dynamic()),
                ) as Box<dyn Iterator<Item = Dynamic>>
            });
        }

        reg_step::<INT>(self);
        self.register_fn("range", |i1: INT, i2: INT, step: INT| {
            StepRange(i1, i2, step)
        });

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            macro_rules! reg_step {
                ($self:expr, $x:expr, $( $y:ty ),*) => (
                    $(
                        reg_step::<$y>(self);
                        $self.register_fn($x, (|x: $y, y: $y, step: $y| StepRange(x,y,step)) as fn(x: $y, y: $y, step: $y)->StepRange<$y>);
                    )*
                )
            }

            reg_step!(self, "range", i8, u8, i16, u16, i32, i64, u32, u64);
        }
    }
}

macro_rules! reg_fn2x {
    ($self:expr, $x:expr, $op:expr, $v:ty, $r:ty, $( $y:ty ),*) => (
        $(
            $self.register_fn($x, $op as fn(x: $v, y: $y)->$r);
        )*
    )
}

macro_rules! reg_fn2y {
    ($self:expr, $x:expr, $op:expr, $v:ty, $r:ty, $( $y:ty ),*) => (
        $(
            $self.register_fn($x, $op as fn(y: $y, x: $v)->$r);
        )*
    )
}

/// Register the built-in library.
impl Engine<'_> {
    pub fn register_stdlib(&mut self) {
        #[cfg(not(feature = "no_float"))]
        {
            // Advanced math functions
            self.register_fn("sin", |x: FLOAT| x.to_radians().sin());
            self.register_fn("cos", |x: FLOAT| x.to_radians().cos());
            self.register_fn("tan", |x: FLOAT| x.to_radians().tan());
            self.register_fn("sinh", |x: FLOAT| x.to_radians().sinh());
            self.register_fn("cosh", |x: FLOAT| x.to_radians().cosh());
            self.register_fn("tanh", |x: FLOAT| x.to_radians().tanh());
            self.register_fn("asin", |x: FLOAT| x.asin().to_degrees());
            self.register_fn("acos", |x: FLOAT| x.acos().to_degrees());
            self.register_fn("atan", |x: FLOAT| x.atan().to_degrees());
            self.register_fn("asinh", |x: FLOAT| x.asinh().to_degrees());
            self.register_fn("acosh", |x: FLOAT| x.acosh().to_degrees());
            self.register_fn("atanh", |x: FLOAT| x.atanh().to_degrees());
            self.register_fn("sqrt", |x: FLOAT| x.sqrt());
            self.register_fn("exp", |x: FLOAT| x.exp());
            self.register_fn("ln", |x: FLOAT| x.ln());
            self.register_fn("log", |x: FLOAT, base: FLOAT| x.log(base));
            self.register_fn("log10", |x: FLOAT| x.log10());
            self.register_fn("floor", |x: FLOAT| x.floor());
            self.register_fn("ceiling", |x: FLOAT| x.ceil());
            self.register_fn("round", |x: FLOAT| x.ceil());
            self.register_fn("int", |x: FLOAT| x.trunc());
            self.register_fn("fraction", |x: FLOAT| x.fract());
            self.register_fn("is_nan", |x: FLOAT| x.is_nan());
            self.register_fn("is_finite", |x: FLOAT| x.is_finite());
            self.register_fn("is_infinite", |x: FLOAT| x.is_infinite());

            // Register conversion functions
            self.register_fn("to_float", |x: INT| x as FLOAT);
            self.register_fn("to_float", |x: f32| x as FLOAT);

            #[cfg(not(feature = "only_i32"))]
            #[cfg(not(feature = "only_i64"))]
            {
                self.register_fn("to_float", |x: i8| x as FLOAT);
                self.register_fn("to_float", |x: u8| x as FLOAT);
                self.register_fn("to_float", |x: i16| x as FLOAT);
                self.register_fn("to_float", |x: u16| x as FLOAT);
                self.register_fn("to_float", |x: i32| x as FLOAT);
                self.register_fn("to_float", |x: u32| x as FLOAT);
                self.register_fn("to_float", |x: i64| x as FLOAT);
                self.register_fn("to_float", |x: u64| x as FLOAT);
            }
        }

        self.register_fn("to_int", |ch: char| ch as INT);

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            self.register_fn("to_int", |x: i8| x as INT);
            self.register_fn("to_int", |x: u8| x as INT);
            self.register_fn("to_int", |x: i16| x as INT);
            self.register_fn("to_int", |x: u16| x as INT);
        }

        #[cfg(not(feature = "only_i32"))]
        {
            self.register_fn("to_int", |x: i32| x as INT);
            self.register_fn("to_int", |x: u64| x as INT);

            #[cfg(feature = "only_i64")]
            self.register_fn("to_int", |x: u32| x as INT);
        }

        #[cfg(not(feature = "no_float"))]
        {
            #[cfg(not(feature = "unchecked"))]
            {
                self.register_result_fn("to_int", |x: f32| {
                    if x > (i64::MAX as f32) {
                        return Err(EvalAltResult::ErrorArithmetic(
                            format!("Integer overflow: to_int({})", x),
                            Position::none(),
                        ));
                    }

                    Ok(x.trunc() as INT)
                });
                self.register_result_fn("to_int", |x: FLOAT| {
                    if x > (i64::MAX as FLOAT) {
                        return Err(EvalAltResult::ErrorArithmetic(
                            format!("Integer overflow: to_int({})", x),
                            Position::none(),
                        ));
                    }

                    Ok(x.trunc() as INT)
                });
            }

            #[cfg(feature = "unchecked")]
            {
                self.register_fn("to_int", |x: f32| x as INT);
                self.register_fn("to_int", |x: f64| x as INT);
            }
        }

        #[cfg(not(feature = "no_index"))]
        {
            macro_rules! reg_fn3 {
                ($self:expr, $x:expr, $op:expr, $v:ty, $w:ty, $r:ty, $( $y:ty ),*) => (
                    $(
                        $self.register_fn($x, $op as fn(x: $v, y: $w, z: $y)->$r);
                    )*
                )
            }

            // Register array utility functions
            fn push<T: Any>(list: &mut Array, item: T) {
                list.push(Box::new(item));
            }
            fn ins<T: Any>(list: &mut Array, position: INT, item: T) {
                if position <= 0 {
                    list.insert(0, Box::new(item));
                } else if (position as usize) >= list.len() - 1 {
                    push(list, item);
                } else {
                    list.insert(position as usize, Box::new(item));
                }
            }
            fn pad<T: Any + Clone>(list: &mut Array, len: INT, item: T) {
                if len >= 0 {
                    while list.len() < len as usize {
                        push(list, item.clone());
                    }
                }
            }

            reg_fn2x!(self, "push", push, &mut Array, (), INT, bool, char);
            reg_fn2x!(self, "push", push, &mut Array, (), String, Array, ());
            reg_fn3!(self, "pad", pad, &mut Array, INT, (), INT, bool, char);
            reg_fn3!(self, "pad", pad, &mut Array, INT, (), String, Array, ());
            reg_fn3!(self, "insert", ins, &mut Array, INT, (), String, Array, ());

            self.register_fn("append", |list: &mut Array, array: Array| {
                list.extend(array)
            });
            self.register_fn("+", |mut list: Array, array: Array| {
                list.extend(array);
                list
            });

            #[cfg(not(feature = "only_i32"))]
            #[cfg(not(feature = "only_i64"))]
            {
                reg_fn2x!(self, "push", push, &mut Array, (), i8, u8, i16, u16);
                reg_fn2x!(self, "push", push, &mut Array, (), i32, i64, u32, u64);
                reg_fn3!(self, "pad", pad, &mut Array, INT, (), i8, u8, i16, u16);
                reg_fn3!(self, "pad", pad, &mut Array, INT, (), i32, u32, i64, u64);
                reg_fn3!(self, "insert", ins, &mut Array, INT, (), i8, u8, i16, u16);
                reg_fn3!(self, "insert", ins, &mut Array, INT, (), i32, i64, u32, u64);
            }

            #[cfg(not(feature = "no_float"))]
            {
                reg_fn2x!(self, "push", push, &mut Array, (), f32, f64);
                reg_fn3!(self, "pad", pad, &mut Array, INT, (), f32, f64);
                reg_fn3!(self, "insert", ins, &mut Array, INT, (), f32, f64);
            }

            self.register_dynamic_fn("pop", |list: &mut Array| {
                list.pop().unwrap_or_else(|| ().into_dynamic())
            });
            self.register_dynamic_fn("shift", |list: &mut Array| {
                if !list.is_empty() {
                    ().into_dynamic()
                } else {
                    list.remove(0)
                }
            });
            self.register_dynamic_fn("remove", |list: &mut Array, len: INT| {
                if len < 0 || (len as usize) >= list.len() {
                    ().into_dynamic()
                } else {
                    list.remove(len as usize)
                }
            });
            self.register_fn("len", |list: &mut Array| list.len() as INT);
            self.register_fn("clear", |list: &mut Array| list.clear());
            self.register_fn("truncate", |list: &mut Array, len: INT| {
                if len >= 0 {
                    list.truncate(len as usize);
                }
            });
        }

        // Register map functions
        #[cfg(not(feature = "no_object"))]
        {
            self.register_fn("has", |map: &mut Map, prop: String| map.contains_key(&prop));
            self.register_fn("len", |map: &mut Map| map.len() as INT);
            self.register_fn("clear", |map: &mut Map| map.clear());
            self.register_dynamic_fn("remove", |x: &mut Map, name: String| {
                x.remove(&name).unwrap_or(().into_dynamic())
            });
            self.register_fn("mixin", |map1: &mut Map, map2: Map| {
                map2.into_iter().for_each(|(key, value)| {
                    map1.insert(key, value);
                });
            });
            self.register_fn("+", |mut map1: Map, map2: Map| {
                map2.into_iter().for_each(|(key, value)| {
                    map1.insert(key, value);
                });
                map1
            });
        }

        // Register string concatenate functions
        fn prepend<T: Display>(x: T, y: String) -> String {
            format!("{}{}", x, y)
        }
        fn append<T: Display>(x: String, y: T) -> String {
            format!("{}{}", x, y)
        }

        reg_fn2x!(self, "+", append, String, String, INT, bool, char);
        self.register_fn("+", |x: String, _: ()| x);

        reg_fn2y!(self, "+", prepend, String, String, INT, bool, char);
        self.register_fn("+", |_: (), y: String| y);

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            reg_fn2x!(self, "+", append, String, String, i8, u8, i16, u16, i32, i64, u32, u64);
            reg_fn2y!(self, "+", prepend, String, String, i8, u8, i16, u16, i32, i64, u32, u64);
        }

        #[cfg(not(feature = "no_float"))]
        {
            reg_fn2x!(self, "+", append, String, String, f32, f64);
            reg_fn2y!(self, "+", prepend, String, String, f32, f64);
        }

        #[cfg(not(feature = "no_index"))]
        {
            self.register_fn("+", |x: String, y: Array| format!("{}{:?}", x, y));
            self.register_fn("+", |x: Array, y: String| format!("{:?}{}", x, y));
        }

        // Register string utility functions
        self.register_fn("len", |s: &mut String| s.chars().count() as INT);
        self.register_fn("contains", |s: &mut String, ch: char| s.contains(ch));
        self.register_fn("contains", |s: &mut String, find: String| s.contains(&find));
        self.register_fn("clear", |s: &mut String| s.clear());
        self.register_fn("append", |s: &mut String, ch: char| s.push(ch));
        self.register_fn("append", |s: &mut String, add: String| s.push_str(&add));
        self.register_fn("truncate", |s: &mut String, len: INT| {
            if len >= 0 {
                let chars: Vec<_> = s.chars().take(len as usize).collect();
                s.clear();
                chars.iter().for_each(|&ch| s.push(ch));
            } else {
                s.clear();
            }
        });
        self.register_fn("pad", |s: &mut String, len: INT, ch: char| {
            for _ in 0..s.chars().count() - len as usize {
                s.push(ch);
            }
        });
        self.register_fn("replace", |s: &mut String, find: String, sub: String| {
            let new_str = s.replace(&find, &sub);
            s.clear();
            s.push_str(&new_str);
        });
        self.register_fn("reverse", |s: &mut String| {
            s.chars().rev().collect::<String>()
        });
        self.register_fn("trim", |s: &mut String| {
            let trimmed = s.trim();

            if trimmed.len() < s.len() {
                *s = trimmed.to_string();
            }
        });

        // Register date/time functions
        self.register_fn("timestamp", || Instant::now());

        self.register_fn("-", |ts1: Instant, ts2: Instant| {
            if ts2 > ts1 {
                #[cfg(not(feature = "no_float"))]
                return -(ts2 - ts1).as_secs_f64();

                #[cfg(feature = "no_float")]
                return -((ts2 - ts1).as_secs() as INT);
            } else {
                #[cfg(not(feature = "no_float"))]
                return (ts1 - ts2).as_secs_f64();

                #[cfg(feature = "no_float")]
                return (ts1 - ts2).as_secs() as INT;
            }
        });

        reg_cmp!(self, "<", lt, Instant);
        reg_cmp!(self, "<=", lte, Instant);
        reg_cmp!(self, ">", gt, Instant);
        reg_cmp!(self, ">=", gte, Instant);
        reg_cmp!(self, "==", eq, Instant);
        reg_cmp!(self, "!=", ne, Instant);

        self.register_fn("elapsed", |timestamp: Instant| {
            #[cfg(not(feature = "no_float"))]
            return timestamp.elapsed().as_secs_f64();

            #[cfg(feature = "no_float")]
            return timestamp.elapsed().as_secs() as INT;
        });
    }
}
