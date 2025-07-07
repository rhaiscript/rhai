use crate::def_package;
use crate::plugin::*;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(any(
    not(feature = "no_float"),
    all(not(feature = "only_i32"), not(feature = "only_i64"))
))]
macro_rules! gen_cmp_functions {
    ($mod_name:ident => $($arg_type:ident),+) => {
        $({
            #[export_module]
            #[allow(clippy::missing_const_for_fn)]
            pub mod cmp_functions {
                #[rhai_fn(name = "<")] pub fn lt(x: $arg_type, y: $arg_type) -> bool { x < y }
                #[rhai_fn(name = "<=")] pub fn lte(x: $arg_type, y: $arg_type) -> bool { x <= y }
                #[rhai_fn(name = ">")] pub fn gt(x: $arg_type, y: $arg_type) -> bool { x > y }
                #[rhai_fn(name = ">=")] pub fn gte(x: $arg_type, y: $arg_type) -> bool { x >= y }
                #[rhai_fn(name = "==")] pub fn eq(x: $arg_type, y: $arg_type) -> bool { x == y }
                #[rhai_fn(name = "!=")] pub fn ne(x: $arg_type, y: $arg_type) -> bool { x != y }
                pub fn max(x: $arg_type, y: $arg_type) -> $arg_type { if x >= y { x } else { y } }
                pub fn min(x: $arg_type, y: $arg_type) -> $arg_type { if x <= y { x } else { y } }
            }

            combine_with_exported_module!($mod_name, concat!("logic_", stringify($arg_type)), cmp_functions);
        })*
    };
}

def_package! {
    /// Package of basic logic operators.
    pub LogicPackage(lib) {
        lib.set_standard_lib(true);

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            gen_cmp_functions!(lib => i8, u8, i16, u16, i32, u32, u64);

            #[cfg(not(target_family = "wasm"))]
            gen_cmp_functions!(lib => i128, u128);
        }

        #[cfg(not(feature = "no_float"))]
        {
            combine_with_exported_module!(lib, "float", float_functions);

            #[cfg(not(feature = "f32_float"))]
            {
                gen_cmp_functions!(lib => f32);
                combine_with_exported_module!(lib, "f32", f32_functions);
            }
            #[cfg(feature = "f32_float")]
            {
                gen_cmp_functions!(lib => f64);
                combine_with_exported_module!(lib, "f64", f64_functions);
            }
        }

        #[cfg(feature = "decimal")]
        combine_with_exported_module!(lib, "decimal", decimal_functions);

        combine_with_exported_module!(lib, "logic", logic_functions);

        combine_with_exported_module!(lib, "min_max", min_max_functions);
    }
}

#[export_module]
mod logic_functions {
    #[rhai_fn(name = "!")]
    pub const fn not(x: bool) -> bool {
        !x
    }
}

#[export_module]
mod min_max_functions {
    use crate::INT;

    /// Return the number that is larger than the other number.
    ///
    /// # Example
    ///
    /// ```rhai
    /// max(42, 123);   // returns 132
    /// ```
    pub const fn max(x: INT, y: INT) -> INT {
        if x >= y {
            x
        } else {
            y
        }
    }
    /// Return the number that is smaller than the other number.
    ///
    /// # Example
    ///
    /// ```rhai
    /// min(42, 123);   // returns 42
    /// ```
    pub const fn min(x: INT, y: INT) -> INT {
        if x <= y {
            x
        } else {
            y
        }
    }
}

#[cfg(not(feature = "no_float"))]
#[allow(clippy::cast_precision_loss)]
#[export_module]
mod float_functions {
    use crate::INT;

    #[rhai_fn(name = "max")]
    pub fn max_ff_32(x: f32, y: f32) -> f32 {
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "max")]
    pub fn max_if_32(x: INT, y: f32) -> f32 {
        let (x, y) = (x as f32, y);
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "max")]
    pub fn max_fi_32(x: f32, y: INT) -> f32 {
        let (x, y) = (x, y as f32);
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_ff_32(x: f32, y: f32) -> f32 {
        if x <= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_if_32(x: INT, y: f32) -> f32 {
        let (x, y) = (x as f32, y);
        if x <= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_fi_32(x: f32, y: INT) -> f32 {
        let (x, y) = (x, y as f32);
        if x <= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "max")]
    pub fn max_ff_64(x: f64, y: f64) -> f64 {
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "max")]
    pub fn max_if_64(x: INT, y: f64) -> f64 {
        let (x, y) = (x as f64, y);
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "max")]
    pub fn max_fi_64(x: f64, y: INT) -> f64 {
        let (x, y) = (x, y as f64);
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_ff_64(x: f64, y: f64) -> f64 {
        if x <= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_if_64(x: INT, y: f64) -> f64 {
        let (x, y) = (x as f64, y);
        if x <= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_fi_64(x: f64, y: INT) -> f64 {
        let (x, y) = (x, y as f64);
        if x <= y {
            x
        } else {
            y
        }
    }
}

#[cfg(not(feature = "no_float"))]
#[cfg(not(feature = "f32_float"))]
#[allow(clippy::cast_precision_loss)]
#[export_module]
mod f32_functions {
    use crate::{FLOAT, INT};
    #[cfg(feature = "no_std")]
    use num_traits::Float;

    #[rhai_fn(name = "==")]
    pub fn eq_if(x: INT, y: f32) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f32) == y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            (x - y).abs() / max <= f32::EPSILON
        }
    }
    #[rhai_fn(name = "==")]
    pub fn eq_fi(x: f32, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x == (y as f32);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            (x - y).abs() / max <= f32::EPSILON
        }
    }
    #[rhai_fn(name = "!=")]
    pub fn neq_if(x: INT, y: f32) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f32) != y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            (x - y).abs() / max > f32::EPSILON
        }
    }
    #[rhai_fn(name = "!=")]
    pub fn neq_fi(x: f32, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x != (y as f32);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            (x - y).abs() / max > f32::EPSILON
        }
    }
    #[rhai_fn(name = ">")]
    pub fn gt_if(x: INT, y: f32) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f32) > y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            (x - y) / max > f32::EPSILON
        }
    }
    #[rhai_fn(name = ">")]
    pub fn gt_fi(x: f32, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x > (y as f32);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            (x - y) / max > f32::EPSILON
        }
    }
    #[rhai_fn(name = ">=")]
    pub fn gte_if(x: INT, y: f32) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f32) >= y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            (x - y) / max > -f32::EPSILON
        }
    }
    #[rhai_fn(name = ">=")]
    pub fn gte_fi(x: f32, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x >= (y as f32);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            (x - y) / max > -f32::EPSILON
        }
    }
    #[rhai_fn(name = "<")]
    pub fn lt_if(x: INT, y: f32) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f32) < y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            (y - x) / max > f32::EPSILON
        }
    }
    #[rhai_fn(name = "<")]
    pub fn lt_fi(x: f32, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x < (y as f32);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            (y - x) / max > f32::EPSILON
        }
    }
    #[rhai_fn(name = "<=")]
    pub fn lte_if(x: INT, y: f32) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f32) <= y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            (y - x) / max > -f32::EPSILON
        }
    }
    #[rhai_fn(name = "<=")]
    pub fn lte_fi(x: f32, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x <= (y as f32);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f32;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            (y - x) / max > -f32::EPSILON
        }
    }

    #[rhai_fn(name = "max")]
    pub fn max_64_32(x: FLOAT, y: f32) -> FLOAT {
        let (x, y) = (x, y as FLOAT);
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "max")]
    pub fn max_32_64(x: f32, y: FLOAT) -> FLOAT {
        let (x, y) = (x as FLOAT, y);
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_64_32(x: FLOAT, y: f32) -> FLOAT {
        let (x, y) = (x, y as FLOAT);
        if x <= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_32_64(x: f32, y: FLOAT) -> FLOAT {
        let (x, y) = (x as FLOAT, y);
        if x <= y {
            x
        } else {
            y
        }
    }
}

#[cfg(not(feature = "no_float"))]
#[cfg(feature = "f32_float")]
#[allow(clippy::cast_precision_loss)]
#[export_module]
mod f64_functions {
    use crate::{FLOAT, INT};
    #[cfg(feature = "no_std")]
    use num_traits::Float;

    #[rhai_fn(name = "==")]
    pub fn eq_if(x: INT, y: f64) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f64) == y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            return (x - y).abs() / max <= f64::EPSILON;
        }
    }
    #[rhai_fn(name = "==")]
    pub fn eq_fi(x: f64, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x == (y as f64);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            return (x - y).abs() / max <= f64::EPSILON;
        }
    }
    #[rhai_fn(name = "!=")]
    pub fn neq_if(x: INT, y: f64) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f64) != y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            return (x - y).abs() / max > f64::EPSILON;
        }
    }
    #[rhai_fn(name = "!=")]
    pub fn neq_fi(x: f64, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x != (y as f64);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            return (x - y).abs() / max > f64::EPSILON;
        }
    }
    #[rhai_fn(name = ">")]
    pub fn gt_if(x: INT, y: f64) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f64) > y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            return (x - y) / max > f64::EPSILON;
        }
    }
    #[rhai_fn(name = ">")]
    pub fn gt_fi(x: f64, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x > (y as f64);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            return (x - y) / max > f64::EPSILON;
        }
    }
    #[rhai_fn(name = ">=")]
    pub fn gte_if(x: INT, y: f64) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f64) >= y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            return (x - y) / max > -f64::EPSILON;
        }
    }
    #[rhai_fn(name = ">=")]
    pub fn gte_fi(x: f64, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x >= (y as f64);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            return (x - y) / max > -f64::EPSILON;
        }
    }
    #[rhai_fn(name = "<")]
    pub fn lt_if(x: INT, y: f64) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f64) < y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            return (y - x) / max > f64::EPSILON;
        }
    }
    #[rhai_fn(name = "<")]
    pub fn lt_fi(x: f64, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x < (y as f64);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return false;
            }
            return (y - x) / max > f64::EPSILON;
        }
    }
    #[rhai_fn(name = "<=")]
    pub fn lte_if(x: INT, y: f64) -> bool {
        #[cfg(feature = "unchecked")]
        return (x as f64) <= y;

        #[cfg(not(feature = "unchecked"))]
        {
            let x = x as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            return (y - x) / max > -f64::EPSILON;
        }
    }
    #[rhai_fn(name = "<=")]
    pub fn lte_fi(x: f64, y: INT) -> bool {
        #[cfg(feature = "unchecked")]
        return x <= (y as f64);

        #[cfg(not(feature = "unchecked"))]
        {
            let y = y as f64;
            let max = if x * y == 0.0 {
                1.0
            } else {
                x.abs().max(y.abs())
            };
            if max == 0.0 {
                return true;
            }
            return (y - x) / max > -f64::EPSILON;
        }
    }

    #[rhai_fn(name = "max")]
    pub fn max_32_64(x: FLOAT, y: f64) -> FLOAT {
        let (x, y) = (x, y as FLOAT);
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "max")]
    pub fn max_64_32(x: f64, y: FLOAT) -> FLOAT {
        let (x, y) = (x as FLOAT, y);
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_32_64(x: FLOAT, y: f64) -> FLOAT {
        let (x, y) = (x, y as FLOAT);
        if x <= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_64_32(x: f64, y: FLOAT) -> FLOAT {
        let (x, y) = (x as FLOAT, y);
        if x <= y {
            x
        } else {
            y
        }
    }
}

#[cfg(feature = "decimal")]
#[export_module]
mod decimal_functions {
    use crate::INT;
    use rust_decimal::Decimal;

    #[rhai_fn(name = "max")]
    pub fn max_dd(x: Decimal, y: Decimal) -> Decimal {
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "max")]
    pub fn max_id(x: INT, y: Decimal) -> Decimal {
        let x = x.into();
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "max")]
    pub fn max_di(x: Decimal, y: INT) -> Decimal {
        let y = y.into();
        if x >= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_dd(x: Decimal, y: Decimal) -> Decimal {
        if x <= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_id(x: INT, y: Decimal) -> Decimal {
        let x = x.into();
        if x <= y {
            x
        } else {
            y
        }
    }
    #[rhai_fn(name = "min")]
    pub fn min_di(x: Decimal, y: INT) -> Decimal {
        let y = y.into();
        if x <= y {
            x
        } else {
            y
        }
    }
}
