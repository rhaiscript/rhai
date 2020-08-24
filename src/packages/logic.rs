use crate::def_package;
use crate::module::FuncReturn;

// Comparison operators
pub fn lt<T: PartialOrd>(x: T, y: T) -> FuncReturn<bool> {
    Ok(x < y)
}
pub fn lte<T: PartialOrd>(x: T, y: T) -> FuncReturn<bool> {
    Ok(x <= y)
}
pub fn gt<T: PartialOrd>(x: T, y: T) -> FuncReturn<bool> {
    Ok(x > y)
}
pub fn gte<T: PartialOrd>(x: T, y: T) -> FuncReturn<bool> {
    Ok(x >= y)
}
pub fn eq<T: PartialEq>(x: T, y: T) -> FuncReturn<bool> {
    Ok(x == y)
}
pub fn ne<T: PartialEq>(x: T, y: T) -> FuncReturn<bool> {
    Ok(x != y)
}

// Logic operators
fn not(x: bool) -> FuncReturn<bool> {
    Ok(!x)
}

macro_rules! reg_op {
    ($lib:expr, $op:expr, $func:ident, $($par:ty),*) => {
        $( $lib.set_fn_2($op, $func::<$par>); )*
    };
}

def_package!(crate:LogicPackage:"Logical operators.", lib, {
    if cfg!(not(feature = "only_i32")) && cfg!(not(feature = "only_i64")) {
        reg_op!(lib, "<", lt, i8, u8, i16, u16, i32, u32, u64);
        reg_op!(lib, "<=", lte, i8, u8, i16, u16, i32, u32, u64);
        reg_op!(lib, ">", gt, i8, u8, i16, u16, i32, u32, u64);
        reg_op!(lib, ">=", gte, i8, u8, i16, u16, i32, u32, u64);
        reg_op!(lib, "==", eq, i8, u8, i16, u16, i32, u32, u64);
        reg_op!(lib, "!=", ne, i8, u8, i16, u16, i32, u32, u64);

        if cfg!(not(target_arch = "wasm32")) {
            reg_op!(lib, "<", lt, i128, u128);
            reg_op!(lib, "<=", lte, i128, u128);
            reg_op!(lib, ">", gt, i128, u128);
            reg_op!(lib, ">=", gte, i128, u128);
            reg_op!(lib, "==", eq, i128, u128);
            reg_op!(lib, "!=", ne, i128, u128);
        }
    }

    #[cfg(not(feature = "no_float"))]
    {
        reg_op!(lib, "<", lt, f32);
        reg_op!(lib, "<=", lte, f32);
        reg_op!(lib, ">", gt, f32);
        reg_op!(lib, ">=", gte, f32);
        reg_op!(lib, "==", eq, f32);
        reg_op!(lib, "!=", ne, f32);
    }

    lib.set_fn_1("!", not);
});
