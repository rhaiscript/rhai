module static;

op minus(int, int) -> int;

op !(bool) -> bool;

/// Return `true` if two arrays are not-equal (i.e. any element not equal or not in the same order).
///
/// The operator `==` is used to compare elements and must be defined,
/// otherwise `false` is assumed.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
/// let y = [1, 2, 3, 4, 5];
/// let z = [1, 2, 3, 4];
///
/// print(x != y);      // prints false
///
/// print(x != z);      // prints true
/// ```
op !=(Array, Array) -> bool;

/// Return `true` if two object maps are not equal (i.e. at least one property value is not equal).
///
/// The operator `==` is used to compare property values and must be defined,
/// otherwise `false` is assumed.
///
/// # Example
///
/// ```rhai
/// let m1 = #{a:1, b:2, c:3};
/// let m2 = #{a:1, b:2, c:3};
/// let m3 = #{a:1, c:3};
///
/// print(m1 != m2);        // prints false
///
/// print(m1 != m3);        // prints true
/// ```
op !=(Map, Map) -> bool;

/// Return `true` if two timestamps are not equal.
op !=(Instant, Instant) -> bool;

op !=(int, f32) -> bool;

op !=(int, float) -> bool;

op !=(f32, int) -> bool;

op !=(f32, f32) -> bool;

op !=(float, int) -> bool;

op !=(i128, i128) -> bool;

op !=(i16, i16) -> bool;

op !=(i32, i32) -> bool;

op !=(i8, i8) -> bool;

op !=(u128, u128) -> bool;

op !=(u16, u16) -> bool;

op !=(u32, u32) -> bool;

op !=(u64, u64) -> bool;

op !=(u8, u8) -> bool;

op %(int, f32) -> f32;

op %(f32, int) -> f32;

op %(f32, f32) -> f32;

op %(i128, i128) -> i128;

op %(i16, i16) -> i16;

op %(i32, i32) -> i32;

op %(i8, i8) -> i8;

op %(u128, u128) -> u128;

op %(u16, u16) -> u16;

op %(u32, u32) -> u32;

op %(u64, u64) -> u64;

op %(u8, u8) -> u8;

op &(i128, i128) -> i128;

op &(i16, i16) -> i16;

op &(i32, i32) -> i32;

op &(i8, i8) -> i8;

op &(u128, u128) -> u128;

op &(u16, u16) -> u16;

op &(u32, u32) -> u32;

op &(u64, u64) -> u64;

op &(u8, u8) -> u8;

op *(int, f32) -> f32;

op *(f32, int) -> f32;

op *(f32, f32) -> f32;

op *(i128, i128) -> i128;

op *(i16, i16) -> i16;

op *(i32, i32) -> i32;

op *(i8, i8) -> i8;

op *(u128, u128) -> u128;

op *(u16, u16) -> u16;

op *(u32, u32) -> u32;

op *(u64, u64) -> u64;

op *(u8, u8) -> u8;

op **(f32, int) -> f32;

op **(f32, f32) -> f32;

op **(i128, int) -> i128;

op **(i16, int) -> i16;

op **(i32, int) -> i32;

op **(i8, int) -> i8;

op **(u128, int) -> u128;

op **(u16, int) -> u16;

op **(u32, int) -> u32;

op **(u64, int) -> u64;

op **(u8, int) -> u8;

op +(Decimal) -> Decimal;

op +(int) -> int;

op +(f32) -> f32;

op +(float) -> float;

op +(i128) -> i128;

op +(i16) -> i16;

op +(i32) -> i32;

op +(i8) -> i8;

op +((), String) -> String;

/// Combine two arrays into a new array and return it.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3];
/// let y = [true, 'x'];
///
/// print(x + y);   // prints "[1, 2, 3, true, 'x']"
///
/// print(x);       // prints "[1, 2, 3"
/// ```
op +(Array, Array) -> Array;

op +(char, String) -> String;

op +(?, String) -> String;

/// Make a copy of the object map, add all property values of another object map
/// (existing property values of the same names are replaced), then returning it.
///
/// # Example
///
/// ```rhai
/// let m = #{a:1, b:2, c:3};
/// let n = #{a: 42, d:0};
///
/// print(m + n);       // prints "#{a:42, b:2, c:3, d:0}"
///
/// print(m);           // prints "#{a:1, b:2, c:3}"
/// ```
op +(Map, Map) -> Map;

op +(String, String) -> String;

op +(String, char) -> String;

op +(String, ?) -> String;

op +(String, Blob) -> String;

op +(String, ()) -> String;

/// Add the specified number of `seconds` to the timestamp and return it as a new timestamp.
op +(Instant, float) -> Instant;

/// Add the specified number of `seconds` to the timestamp and return it as a new timestamp.
op +(Instant, int) -> Instant;

op +(Blob, String) -> String;

op +(int, f32) -> f32;

op +(f32, int) -> f32;

op +(f32, f32) -> f32;

op +(i128, i128) -> i128;

op +(i16, i16) -> i16;

op +(i32, i32) -> i32;

op +(i8, i8) -> i8;

op +(u128, u128) -> u128;

op +(u16, u16) -> u16;

op +(u32, u32) -> u32;

op +(u64, u64) -> u64;

op +(u8, u8) -> u8;

/// Add all property values of another object map into the object map.
/// Existing property values of the same names are replaced.
///
/// # Example
///
/// ```rhai
/// let m = #{a:1, b:2, c:3};
/// let n = #{a: 42, d:0};
///
/// m.mixin(n);
///
/// print(m);       // prints "#{a:42, b:2, c:3, d:0}"
/// ```
op +=(Map, Map) -> ();

op +=(String, String) -> ();

op +=(String, char) -> ();

op +=(String, ()) -> ();

op +=(String, ?) -> ();

op +=(String, Blob) -> ();

/// Add the specified number of `seconds` to the timestamp.
op +=(Instant, float) -> ();

/// Add the specified number of `seconds` to the timestamp.
op +=(Instant, int) -> ();

op -(Decimal) -> Decimal;

op -(int) -> int;

op -(f32) -> f32;

op -(float) -> float;

op -(i128) -> i128;

op -(i16) -> i16;

op -(i32) -> i32;

op -(i8) -> i8;

/// Return the number of seconds between two timestamps.
op -(Instant, Instant) -> RhaiResult;

/// Subtract the specified number of `seconds` from the timestamp and return it as a new timestamp.
op -(Instant, float) -> Instant;

/// Subtract the specified number of `seconds` from the timestamp and return it as a new timestamp.
op -(Instant, int) -> Instant;

op -(int, f32) -> f32;

op -(f32, int) -> f32;

op -(f32, f32) -> f32;

op -(i128, i128) -> i128;

op -(i16, i16) -> i16;

op -(i32, i32) -> i32;

op -(i8, i8) -> i8;

op -(u128, u128) -> u128;

op -(u16, u16) -> u16;

op -(u32, u32) -> u32;

op -(u64, u64) -> u64;

op -(u8, u8) -> u8;

/// Subtract the specified number of `seconds` from the timestamp.
op -=(Instant, float) -> ();

/// Subtract the specified number of `seconds` from the timestamp.
op -=(Instant, int) -> ();

op /(int, f32) -> f32;

op /(f32, int) -> f32;

op /(f32, f32) -> f32;

op /(i128, i128) -> i128;

op /(i16, i16) -> i16;

op /(i32, i32) -> i32;

op /(i8, i8) -> i8;

op /(u128, u128) -> u128;

op /(u16, u16) -> u16;

op /(u32, u32) -> u32;

op /(u64, u64) -> u64;

op /(u8, u8) -> u8;

/// Return `true` if the first timestamp is earlier than the second.
op <(Instant, Instant) -> bool;

op <(int, f32) -> bool;

op <(int, float) -> bool;

op <(f32, int) -> bool;

op <(f32, f32) -> bool;

op <(float, int) -> bool;

op <(i128, i128) -> bool;

op <(i16, i16) -> bool;

op <(i32, i32) -> bool;

op <(i8, i8) -> bool;

op <(u128, u128) -> bool;

op <(u16, u16) -> bool;

op <(u32, u32) -> bool;

op <(u64, u64) -> bool;

op <(u8, u8) -> bool;

op <<(i128, int) -> i128;

op <<(i16, int) -> i16;

op <<(i32, int) -> i32;

op <<(i8, int) -> i8;

op <<(u128, int) -> u128;

op <<(u16, int) -> u16;

op <<(u32, int) -> u32;

op <<(u64, int) -> u64;

op <<(u8, int) -> u8;

/// Return `true` if the first timestamp is earlier than or equals to the second.
op <=(Instant, Instant) -> bool;

op <=(int, f32) -> bool;

op <=(int, float) -> bool;

op <=(f32, int) -> bool;

op <=(f32, f32) -> bool;

op <=(float, int) -> bool;

op <=(i128, i128) -> bool;

op <=(i16, i16) -> bool;

op <=(i32, i32) -> bool;

op <=(i8, i8) -> bool;

op <=(u128, u128) -> bool;

op <=(u16, u16) -> bool;

op <=(u32, u32) -> bool;

op <=(u64, u64) -> bool;

op <=(u8, u8) -> bool;

/// Return `true` if two arrays are equal (i.e. all elements are equal and in the same order).
///
/// The operator `==` is used to compare elements and must be defined,
/// otherwise `false` is assumed.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
/// let y = [1, 2, 3, 4, 5];
/// let z = [1, 2, 3, 4];
///
/// print(x == y);      // prints true
///
/// print(x == z);      // prints false
/// ```
op ==(Array, Array) -> bool;

/// Return `true` if two object maps are equal (i.e. all property values are equal).
///
/// The operator `==` is used to compare property values and must be defined,
/// otherwise `false` is assumed.
///
/// # Example
///
/// ```rhai
/// let m1 = #{a:1, b:2, c:3};
/// let m2 = #{a:1, b:2, c:3};
/// let m3 = #{a:1, c:3};
///
/// print(m1 == m2);        // prints true
///
/// print(m1 == m3);        // prints false
/// ```
op ==(Map, Map) -> bool;

/// Return `true` if two timestamps are equal.
op ==(Instant, Instant) -> bool;

op ==(int, f32) -> bool;

op ==(int, float) -> bool;

op ==(f32, int) -> bool;

op ==(f32, f32) -> bool;

op ==(float, int) -> bool;

op ==(i128, i128) -> bool;

op ==(i16, i16) -> bool;

op ==(i32, i32) -> bool;

op ==(i8, i8) -> bool;

op ==(u128, u128) -> bool;

op ==(u16, u16) -> bool;

op ==(u32, u32) -> bool;

op ==(u64, u64) -> bool;

op ==(u8, u8) -> bool;

/// Return `true` if the first timestamp is later than the second.
op >(Instant, Instant) -> bool;

op >(int, f32) -> bool;

op >(int, float) -> bool;

op >(f32, int) -> bool;

op >(f32, f32) -> bool;

op >(float, int) -> bool;

op >(i128, i128) -> bool;

op >(i16, i16) -> bool;

op >(i32, i32) -> bool;

op >(i8, i8) -> bool;

op >(u128, u128) -> bool;

op >(u16, u16) -> bool;

op >(u32, u32) -> bool;

op >(u64, u64) -> bool;

op >(u8, u8) -> bool;

/// Return `true` if the first timestamp is later than or equals to the second.
op >=(Instant, Instant) -> bool;

op >=(int, f32) -> bool;

op >=(int, float) -> bool;

op >=(f32, int) -> bool;

op >=(f32, f32) -> bool;

op >=(float, int) -> bool;

op >=(i128, i128) -> bool;

op >=(i16, i16) -> bool;

op >=(i32, i32) -> bool;

op >=(i8, i8) -> bool;

op >=(u128, u128) -> bool;

op >=(u16, u16) -> bool;

op >=(u32, u32) -> bool;

op >=(u64, u64) -> bool;

op >=(u8, u8) -> bool;

op >>(i128, int) -> i128;

op >>(i16, int) -> i16;

op >>(i32, int) -> i32;

op >>(i8, int) -> i8;

op >>(u128, int) -> u128;

op >>(u16, int) -> u16;

op >>(u32, int) -> u32;

op >>(u64, int) -> u64;

op >>(u8, int) -> u8;

/// Return the natural number _e_.
fn E() -> float;

/// Return the number π.
fn PI() -> float;

op ^(i128, i128) -> i128;

op ^(i16, i16) -> i16;

op ^(i32, i32) -> i32;

op ^(i8, i8) -> i8;

op ^(u128, u128) -> u128;

op ^(u16, u16) -> u16;

op ^(u32, u32) -> u32;

op ^(u64, u64) -> u64;

op ^(u8, u8) -> u8;

/// Return the absolute value of the decimal number.
fn abs(x: Decimal) -> Decimal;

/// Return the absolute value of the number.
fn abs(x: int) -> int;

/// Return the absolute value of the floating-point number.
fn abs(x: f32) -> f32;

/// Return the absolute value of the floating-point number.
fn abs(x: float) -> float;

/// Return the absolute value of the number.
fn abs(x: i128) -> i128;

/// Return the absolute value of the number.
fn abs(x: i16) -> i16;

/// Return the absolute value of the number.
fn abs(x: i32) -> i32;

/// Return the absolute value of the number.
fn abs(x: i8) -> i8;

/// Return the arc-cosine of the floating-point number, in radians.
fn acos(x: float) -> float;

/// Return the arc-hyperbolic-cosine of the floating-point number, in radians.
fn acosh(x: float) -> float;

/// Return `true` if all elements in the array return `true` when applied a function named by `filter`.
///
/// # Function Parameters
///
/// A function with the same name as the value of `filter` must exist taking these parameters:
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
///
/// print(x.all(|v| v > 3));        // prints false
///
/// print(x.all(|v| v > 1));        // prints true
///
/// print(x.all(|v, i| i > v));     // prints false
/// ```
fn all(array: Array, filter: String) -> bool;

/// Return `true` if all elements in the array return `true` when applied the `filter` function.
///
/// # Function Parameters
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
///
/// print(x.all(|v| v > 3));        // prints false
///
/// print(x.all(|v| v > 1));        // prints true
///
/// print(x.all(|v, i| i > v));     // prints false
/// ```
fn all(array: Array, filter: FnPtr) -> bool;

/// Add all the elements of another array to the end of the array.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3];
/// let y = [true, 'x'];
///
/// x.push(y);
///
/// print(x);       // prints "[1, 2, 3, true, 'x']"
/// ```
fn append(array: Array, new_array: Array) -> ();

/// Add another BLOB to the end of the BLOB.
///
/// # Example
///
/// ```rhai
/// let b1 = blob(5, 0x42);
/// let b2 = blob(3, 0x11);
///
/// b1.push(b2);
///
/// print(b1);      // prints "[4242424242111111]"
/// ```
fn append(blob1: Blob, blob2: Blob) -> ();

/// Add a character (as UTF-8 encoded byte-stream) to the end of the BLOB
///
/// # Example
///
/// ```rhai
/// let b = blob(5, 0x42);
///
/// b.append('!');
///
/// print(b);       // prints "[424242424221]"
/// ```
fn append(blob: Blob, character: char) -> ();

/// Add a string (as UTF-8 encoded byte-stream) to the end of the BLOB
///
/// # Example
///
/// ```rhai
/// let b = blob(5, 0x42);
///
/// b.append("hello");
///
/// print(b);       // prints "[424242424268656c 6c6f]"
/// ```
fn append(blob: Blob, string: String) -> ();

/// Add a new byte `value` to the end of the BLOB.
///
/// Only the lower 8 bits of the `value` are used; all other bits are ignored.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b.push(0x42);
///
/// print(b);       // prints "[42]"
/// ```
fn append(blob: Blob, value: int) -> ();

fn append(string: String, item: ?) -> ();

fn append(string: String, utf8: Blob) -> ();

/// Convert the BLOB into a string.
///
/// The byte stream must be valid UTF-8, otherwise an error is raised.
///
/// # Example
///
/// ```rhai
/// let b = blob(5, 0x42);
///
/// let x = b.as_string();
///
/// print(x);       // prints "FFFFF"
/// ```
fn as_string(blob: Blob) -> String;

/// Return the arc-sine of the floating-point number, in radians.
fn asin(x: float) -> float;

/// Return the arc-hyperbolic-sine of the floating-point number, in radians.
fn asinh(x: float) -> float;

/// Return the arc-tangent of the floating-point number, in radians.
fn atan(x: float) -> float;

/// Return the arc-tangent of the floating-point numbers `x` and `y`, in radians.
fn atan(x: float, y: float) -> float;

/// Return the arc-hyperbolic-tangent of the floating-point number, in radians.
fn atanh(x: float) -> float;

/// Get an array of object maps containing the function calls stack.
///
/// If there is no debugging interface registered, an empty array is returned.
///
/// An array of strings is returned under `no_object`.
fn back_trace() -> Array;

/// Return an iterator over all the bits in the number.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// for bit in x.bits() {
///     print(bit);
/// }
/// ```
fn bits(value: int) -> Iterator<bool>;

/// Return an iterator over the bits in the number starting from the specified `start` position.
///
/// If `start` < 0, position counts from the MSB (Most Significant Bit)>.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// for bit in x.bits(10) {
///     print(bit);
/// }
/// ```
fn bits(value: int, from: int) -> Iterator<bool>;

/// Return an iterator over an exclusive range of bits in the number.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// for bit in x.bits(10..24) {
///     print(bit);
/// }
/// ```
fn bits(value: int, range: Range<int>) -> Iterator<bool>;

/// Return an iterator over an inclusive range of bits in the number.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// for bit in x.bits(10..=23) {
///     print(bit);
/// }
/// ```
fn bits(value: int, range: RangeInclusive<int>) -> Iterator<bool>;

/// Return an iterator over a portion of bits in the number.
///
/// * If `start` < 0, position counts from the MSB (Most Significant Bit)>.
/// * If `len` ≤ 0, an empty iterator is returned.
/// * If `start` position + `len` ≥ length of string, all bits of the number after the `start` position are iterated.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// for bit in x.bits(10, 8) {
///     print(bit);
/// }
/// ```
fn bits(value: int, from: int, len: int) -> Iterator<bool>;

/// Return a new, empty BLOB.
fn blob() -> Blob;

/// Return a new BLOB of the specified length, filled with zeros.
///
/// If `len` ≤ 0, an empty BLOB is returned.
///
/// # Example
///
/// ```rhai
/// let b = blob(10);
///
/// print(b);       // prints "[0000000000000000 0000]"
/// ```
fn blob(len: int) -> Blob;

/// Return a new BLOB of the specified length, filled with copies of the initial `value`.
///
/// If `len` ≤ 0, an empty BLOB is returned.
///
/// Only the lower 8 bits of the initial `value` are used; all other bits are ignored.
///
/// # Example
///
/// ```rhai
/// let b = blob(10, 0x42);
///
/// print(b);       // prints "[4242424242424242 4242]"
/// ```
fn blob(len: int, value: int) -> Blob;

/// Return the length of the string, in number of bytes used to store it in UTF-8 encoding.
///
/// # Example
///
/// ```rhai
/// let text = "朝には紅顔ありて夕べには白骨となる";
///
/// print(text.bytes);      // prints 51
/// ```
fn bytes(string: String) -> int;

/// Return the smallest whole number larger than or equals to the decimal number.
fn ceiling(x: Decimal) -> Decimal;

/// Return the smallest whole number larger than or equals to the floating-point number.
fn ceiling(x: float) -> float;

/// Return an iterator over the characters in the string.
///
/// # Example
///
/// ```rhai
/// for ch in "hello, world!".chars() {
///     print(ch);
/// }
/// ```
fn chars(string: String) -> Iterator<char>;

/// Return an iterator over the characters in the string starting from the `start` position.
///
/// * If `start` < 0, position counts from the end of the string (`-1` is the last character).
/// * If `start` < -length of string, position counts from the beginning of the string.
/// * If `start` ≥ length of string, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// for ch in "hello, world!".chars(2) {
///     print(ch);
/// }
/// ```
fn chars(string: String, from: int) -> Iterator<char>;

/// Return an iterator over an exclusive range of characters in the string.
///
/// # Example
///
/// ```rhai
/// for ch in "hello, world!".chars(2..5) {
///     print(ch);
/// }
/// ```
fn chars(string: String, range: Range<int>) -> Iterator<char>;

/// Return an iterator over an inclusive range of characters in the string.
///
/// # Example
///
/// ```rhai
/// for ch in "hello, world!".chars(2..=6) {
///     print(ch);
/// }
/// ```
fn chars(string: String, range: RangeInclusive<int>) -> Iterator<char>;

/// Return an iterator over a portion of characters in the string.
///
/// * If `start` < 0, position counts from the end of the string (`-1` is the last character).
/// * If `start` < -length of string, position counts from the beginning of the string.
/// * If `start` ≥ length of string, an empty iterator is returned.
/// * If `len` ≤ 0, an empty iterator is returned.
/// * If `start` position + `len` ≥ length of string, all characters of the string after the `start` position are iterated.
///
/// # Example
///
/// ```rhai
/// for ch in "hello, world!".chars(2, 4) {
///     print(ch);
/// }
/// ```
fn chars(string: String, start: int, len: int) -> Iterator<char>;

/// Cut off the head of the array, leaving a tail of the specified length.
///
/// * If `len` ≤ 0, the array is cleared.
/// * If `len` ≥ length of array, the array is not modified.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// x.chop(3);
///
/// print(x);       // prints "[3, 4, 5]"
///
/// x.chop(10);
///
/// print(x);       // prints "[3, 4, 5]"
/// ```
fn chop(array: Array, len: int) -> ();

/// Cut off the head of the BLOB, leaving a tail of the specified length.
///
/// * If `len` ≤ 0, the BLOB is cleared.
/// * If `len` ≥ length of BLOB, the BLOB is not modified.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// b.chop(3);
///
/// print(b);           // prints "[030405]"
///
/// b.chop(10);
///
/// print(b);           // prints "[030405]"
/// ```
fn chop(blob: Blob, len: int) -> ();

/// Clear the array.
fn clear(array: Array) -> ();

/// Clear the BLOB.
fn clear(blob: Blob) -> ();

/// Clear the object map.
fn clear(map: Map) -> ();

/// Clear the string, making it empty.
fn clear(string: String) -> ();

/// Return `true` if the array contains an element that equals `value`.
///
/// The operator `==` is used to compare elements with `value` and must be defined,
/// otherwise `false` is assumed.
///
/// This function also drives the `in` operator.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// // The 'in' operator calls 'contains' in the background
/// if 4 in x {
///     print("found!");
/// }
/// ```
fn contains(array: Array, value: ?) -> bool;

/// Return the cosine of the decimal number in radians.
fn cos(x: Decimal) -> Decimal;

/// Return the cosine of the floating-point number in radians.
fn cos(x: float) -> float;

/// Return the hyperbolic cosine of the floating-point number in radians.
fn cosh(x: float) -> float;

/// Remove all characters from the string except those within an exclusive `range`.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// text.crop(2..8);
///
/// print(text);        // prints "llo, w"
/// ```
fn crop(string: String, range: Range<int>) -> ();

/// Remove all characters from the string except those within an inclusive `range`.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// text.crop(2..=8);
///
/// print(text);        // prints "llo, wo"
/// ```
fn crop(string: String, range: RangeInclusive<int>) -> ();

/// Remove all characters from the string except until the `start` position.
///
/// * If `start` < 0, position counts from the end of the string (`-1` is the last character).
/// * If `start` < -length of string, the string is not modified.
/// * If `start` ≥ length of string, the entire string is cleared.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// text.crop(5);
///
/// print(text);            // prints ", world!"
///
/// text.crop(-3);
///
/// print(text);            // prints "ld!"
/// ```
fn crop(string: String, start: int) -> ();

/// Remove all characters from the string except those within a range.
///
/// * If `start` < 0, position counts from the end of the string (`-1` is the last character).
/// * If `start` < -length of string, position counts from the beginning of the string.
/// * If `start` ≥ length of string, the entire string is cleared.
/// * If `len` ≤ 0, the entire string is cleared.
/// * If `start` position + `len` ≥ length of string, only the portion of the string after the `start` position is retained.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// text.crop(2, 8);
///
/// print(text);        // prints "llo, wor"
///
/// text.crop(-5, 3);
///
/// print(text);        // prints ", w"
/// ```
fn crop(string: String, start: int, len: int) -> ();

/// Return the empty string.
fn debug() -> String;

/// Convert the array into a string.
fn debug(array: Array) -> String;

/// Convert the string into debug format.
fn debug(character: char) -> String;

/// Convert the function pointer into a string in debug format.
fn debug(f: FnPtr) -> String;

/// Convert the value of the `item` into a string in debug format.
fn debug(item: ?) -> String;

/// Convert the object map into a string.
fn debug(map: Map) -> String;

/// Convert the value of `number` into a string.
fn debug(number: f32) -> String;

/// Convert the value of `number` into a string.
fn debug(number: float) -> String;

/// Convert the string into debug format.
fn debug(string: String) -> String;

/// Convert the unit into a string in debug format.
fn debug(unit: ()) -> String;

/// Convert the boolean value into a string in debug format.
fn debug(value: bool) -> String;

/// Remove duplicated _consecutive_ elements from the array.
///
/// The operator `==` is used to compare elements and must be defined,
/// otherwise `false` is assumed.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 2, 2, 3, 4, 3, 3, 2, 1];
///
/// x.dedup();
///
/// print(x);       // prints "[1, 2, 3, 4, 3, 2, 1]"
/// ```
fn dedup(array: Array) -> ();

/// Remove duplicated _consecutive_ elements from the array that return `true` when applied a
/// function named by `comparer`.
///
/// No element is removed if the correct `comparer` function does not exist.
///
/// # Function Parameters
///
/// * `element1`: copy of the current array element to compare
/// * `element2`: copy of the next array element to compare
///
/// ## Return Value
///
/// `true` if `element1 == element2`, otherwise `false`.
///
/// # Example
///
/// ```rhai
/// fn declining(a, b) { a >= b }
///
/// let x = [1, 2, 2, 2, 3, 1, 2, 3, 4, 3, 3, 2, 1];
///
/// x.dedup("declining");
///
/// print(x);       // prints "[1, 2, 3, 4]"
/// ```
fn dedup(array: Array, comparer: String) -> ();

/// Remove duplicated _consecutive_ elements from the array that return `true` when applied the
/// `comparer` function.
///
/// No element is removed if the correct `comparer` function does not exist.
///
/// # Function Parameters
///
/// * `element1`: copy of the current array element to compare
/// * `element2`: copy of the next array element to compare
///
/// ## Return Value
///
/// `true` if `element1 == element2`, otherwise `false`.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 2, 2, 3, 1, 2, 3, 4, 3, 3, 2, 1];
///
/// x.dedup(|a, b| a >= b);
///
/// print(x);       // prints "[1, 2, 3, 4]"
/// ```
fn dedup(array: Array, comparer: FnPtr) -> ();

/// Remove all elements in the array that returns `true` when applied a function named by `filter`
/// and return them as a new array.
///
/// # Function Parameters
///
/// A function with the same name as the value of `filter` must exist taking these parameters:
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// fn small(x) { x < 3 }
///
/// fn screen(x, i) { x + i > 5 }
///
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.drain("small");
///
/// print(x);       // prints "[3, 4, 5]"
///
/// print(y);       // prints "[1, 2]"
///
/// let z = x.drain("screen");
///
/// print(x);       // prints "[3, 4]"
///
/// print(z);       // prints "[5]"
/// ```
fn drain(array: Array, filter: String) -> Array;

/// Remove all elements in the array that returns `true` when applied the `filter` function and
/// return them as a new array.
///
/// # Function Parameters
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.drain(|v| v < 3);
///
/// print(x);       // prints "[3, 4, 5]"
///
/// print(y);       // prints "[1, 2]"
///
/// let z = x.drain(|v, i| v + i > 5);
///
/// print(x);       // prints "[3, 4]"
///
/// print(z);       // prints "[5]"
/// ```
fn drain(array: Array, filter: FnPtr) -> Array;

/// Remove all elements in the array within an exclusive `range` and return them as a new array.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.drain(1..3);
///
/// print(x);       // prints "[1, 4, 5]"
///
/// print(y);       // prints "[2, 3]"
///
/// let z = x.drain(2..3);
///
/// print(x);       // prints "[1, 4]"
///
/// print(z);       // prints "[5]"
/// ```
fn drain(array: Array, range: Range<int>) -> Array;

/// Remove all elements in the array within an inclusive `range` and return them as a new array.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.drain(1..=2);
///
/// print(x);       // prints "[1, 4, 5]"
///
/// print(y);       // prints "[2, 3]"
///
/// let z = x.drain(2..=2);
///
/// print(x);       // prints "[1, 4]"
///
/// print(z);       // prints "[5]"
/// ```
fn drain(array: Array, range: RangeInclusive<int>) -> Array;

/// Remove all bytes in the BLOB within an exclusive `range` and return them as a new BLOB.
///
/// # Example
///
/// ```rhai
/// let b1 = blob();
///
/// b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;
///
/// let b2 = b1.drain(1..3);
///
/// print(b1);      // prints "[010405]"
///
/// print(b2);      // prints "[0203]"
///
/// let b3 = b1.drain(2..3);
///
/// print(b1);      // prints "[0104]"
///
/// print(b3);      // prints "[05]"
/// ```
fn drain(blob: Blob, range: Range<int>) -> Blob;

/// Remove all bytes in the BLOB within an inclusive `range` and return them as a new BLOB.
///
/// # Example
///
/// ```rhai
/// let b1 = blob();
///
/// b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;
///
/// let b2 = b1.drain(1..=2);
///
/// print(b1);      // prints "[010405]"
///
/// print(b2);      // prints "[0203]"
///
/// let b3 = b1.drain(2..=2);
///
/// print(b1);      // prints "[0104]"
///
/// print(b3);      // prints "[05]"
/// ```
fn drain(blob: Blob, range: RangeInclusive<int>) -> Blob;

/// Remove all elements within a portion of the array and return them as a new array.
///
/// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `start` < -length of array, position counts from the beginning of the array.
/// * If `start` ≥ length of array, no element is removed and an empty array is returned.
/// * If `len` ≤ 0, no element is removed and an empty array is returned.
/// * If `start` position + `len` ≥ length of array, entire portion of the array after the `start` position is removed and returned.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.drain(1, 2);
///
/// print(x);       // prints "[1, 4, 5]"
///
/// print(y);       // prints "[2, 3]"
///
/// let z = x.drain(-1, 1);
///
/// print(x);       // prints "[1, 4]"
///
/// print(z);       // prints "[5]"
/// ```
fn drain(array: Array, start: int, len: int) -> Array;

/// Remove all bytes within a portion of the BLOB and return them as a new BLOB.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, nothing is removed and an empty BLOB is returned.
/// * If `len` ≤ 0, nothing is removed and an empty BLOB is returned.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB after the `start` position is removed and returned.
///
/// # Example
///
/// ```rhai
/// let b1 = blob();
///
/// b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;
///
/// let b2 = b1.drain(1, 2);
///
/// print(b1);      // prints "[010405]"
///
/// print(b2);      // prints "[0203]"
///
/// let b3 = b1.drain(-1, 1);
///
/// print(b3);      // prints "[0104]"
///
/// print(z);       // prints "[5]"
/// ```
fn drain(blob: Blob, start: int, len: int) -> Blob;

/// Return the number of seconds between the current system time and the timestamp.
///
/// # Example
///
/// ```rhai
/// let now = timestamp();
///
/// sleep(10.0);            // sleep for 10 seconds
///
/// print(now.elapsed);     // prints 10.???
/// ```
fn elapsed(timestamp: Instant) -> RhaiResult;

/// Return the end of the exclusive range.
fn end(range: ExclusiveRange) -> int;

/// Return the end of the inclusive range.
fn end(range: InclusiveRange) -> int;

/// Return `true` if the string ends with a specified string.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.ends_with("world!"));    // prints true
///
/// print(text.ends_with("hello"));     // prints false
/// ```
fn ends_with(string: String, match_string: String) -> bool;

/// Return the exponential of the decimal number.
fn exp(x: Decimal) -> Decimal;

/// Return the exponential of the floating-point number.
fn exp(x: float) -> float;

/// Copy an exclusive range of the array and return it as a new array.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// print(x.extract(1..3));     // prints "[2, 3]"
///
/// print(x);                   // prints "[1, 2, 3, 4, 5]"
/// ```
fn extract(array: Array, range: Range<int>) -> Array;

/// Copy an inclusive range of the array and return it as a new array.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// print(x.extract(1..=3));    // prints "[2, 3, 4]"
///
/// print(x);                   // prints "[1, 2, 3, 4, 5]"
/// ```
fn extract(array: Array, range: RangeInclusive<int>) -> Array;

/// Copy a portion of the array beginning at the `start` position till the end and return it as
/// a new array.
///
/// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `start` < -length of array, the entire array is copied and returned.
/// * If `start` ≥ length of array, an empty array is returned.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// print(x.extract(2));        // prints "[3, 4, 5]"
///
/// print(x.extract(-3));       // prints "[3, 4, 5]"
///
/// print(x);                   // prints "[1, 2, 3, 4, 5]"
/// ```
fn extract(array: Array, start: int) -> Array;

/// Copy an exclusive `range` of the BLOB and return it as a new BLOB.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// print(b.extract(1..3));     // prints "[0203]"
///
/// print(b);                   // prints "[0102030405]"
/// ```
fn extract(blob: Blob, range: Range<int>) -> Blob;

/// Copy an inclusive `range` of the BLOB and return it as a new BLOB.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// print(b.extract(1..=3));    // prints "[020304]"
///
/// print(b);                   // prints "[0102030405]"
/// ```
fn extract(blob: Blob, range: RangeInclusive<int>) -> Blob;

/// Copy a portion of the BLOB beginning at the `start` position till the end and return it as
/// a new BLOB.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, the entire BLOB is copied and returned.
/// * If `start` ≥ length of BLOB, an empty BLOB is returned.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// print(b.extract(2));        // prints "[030405]"
///
/// print(b.extract(-3));       // prints "[030405]"
///
/// print(b);                   // prints "[0102030405]"
/// ```
fn extract(blob: Blob, start: int) -> Blob;

/// Copy a portion of the array and return it as a new array.
///
/// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `start` < -length of array, position counts from the beginning of the array.
/// * If `start` ≥ length of array, an empty array is returned.
/// * If `len` ≤ 0, an empty array is returned.
/// * If `start` position + `len` ≥ length of array, entire portion of the array after the `start` position is copied and returned.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// print(x.extract(1, 3));     // prints "[2, 3, 4]"
///
/// print(x.extract(-3, 2));    // prints "[3, 4]"
///
/// print(x);                   // prints "[1, 2, 3, 4, 5]"
/// ```
fn extract(array: Array, start: int, len: int) -> Array;

/// Copy a portion of the BLOB and return it as a new BLOB.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, an empty BLOB is returned.
/// * If `len` ≤ 0, an empty BLOB is returned.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB after the `start` position is copied and returned.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// print(b.extract(1, 3));     // prints "[020303]"
///
/// print(b.extract(-3, 2));    // prints "[0304]"
///
/// print(b);                   // prints "[0102030405]"
/// ```
fn extract(blob: Blob, start: int, len: int) -> Blob;

/// Add all property values of another object map into the object map.
/// Only properties that do not originally exist in the object map are added.
///
/// # Example
///
/// ```rhai
/// let m = #{a:1, b:2, c:3};
/// let n = #{a: 42, d:0};
///
/// m.fill_with(n);
///
/// print(m);       // prints "#{a:1, b:2, c:3, d:0}"
/// ```
fn fill_with(map: Map, map2: Map) -> ();

/// Iterate through all the elements in the array, applying a `filter` function to each element
/// in turn, and return a copy of all elements (in order) that return `true` as a new array.
///
/// # Function Parameters
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.filter(|v| v >= 3);
///
/// print(y);       // prints "[3, 4, 5]"
///
/// let y = x.filter(|v, i| v * i >= 10);
///
/// print(y);       // prints "[12, 20]"
/// ```
fn filter(array: Array, filter: FnPtr) -> Array;

/// Iterate through all the elements in the array, applying a function named by `filter` to each
/// element in turn, and return a copy of all elements (in order) that return `true` as a new array.
///
/// # Function Parameters
///
/// A function with the same name as the value of `filter` must exist taking these parameters:
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// fn screen(x, i) { x * i >= 10 }
///
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.filter("is_odd");
///
/// print(y);       // prints "[1, 3, 5]"
///
/// let y = x.filter("screen");
///
/// print(y);       // prints "[12, 20]"
/// ```
fn filter(array: Array, filter_func: String) -> Array;

/// Return the largest whole number less than or equals to the decimal number.
fn floor(x: Decimal) -> Decimal;

/// Return the largest whole number less than or equals to the floating-point number.
fn floor(x: float) -> float;

/// Return the fractional part of the decimal number.
fn fraction(x: Decimal) -> Decimal;

/// Return the fractional part of the floating-point number.
fn fraction(x: float) -> float;

/// Get a copy of the element at the `index` position in the array.
///
/// * If `index` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `index` < -length of array, `()` is returned.
/// * If `index` ≥ length of array, `()` is returned.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3];
///
/// print(x.get(0));        // prints 1
///
/// print(x.get(-1));       // prints 3
///
/// print(x.get(99));       // prints empty (for '()')
/// ```
fn get(array: Array, index: int) -> ?;

/// Get the byte value at the `index` position in the BLOB.
///
/// * If `index` < 0, position counts from the end of the BLOB (`-1` is the last element).
/// * If `index` < -length of BLOB, zero is returned.
/// * If `index` ≥ length of BLOB, zero is returned.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// print(b.get(0));        // prints 1
///
/// print(b.get(-1));       // prints 5
///
/// print(b.get(99));       // prints 0
/// ```
fn get(blob: Blob, index: int) -> int;

/// Get the value of the `property` in the object map and return a copy.
///
/// If `property` does not exist in the object map, `()` is returned.
///
/// # Example
///
/// ```rhai
/// let m = #{a: 1, b: 2, c: 3};
///
/// print(m.get("b"));      // prints 2
///
/// print(m.get("x"));      // prints empty (for '()')
/// ```
fn get(map: Map, property: String) -> ?;

/// Get the character at the `index` position in the string.
///
/// * If `index` < 0, position counts from the end of the string (`-1` is the last character).
/// * If `index` < -length of string, zero is returned.
/// * If `index` ≥ length of string, zero is returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.get(0));     // prints 'h'
///
/// print(text.get(-1));    // prints '!'
///
/// print(text.get(99));    // prints empty (for '()')'
/// ```
fn get(string: String, index: int) -> ?;

/// Return an iterator over all the bits in the number.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// for bit in x.bits {
///     print(bit);
/// }
/// ```
fn get bits(value: int) -> Iterator<bool>;

/// Return the length of the string, in number of bytes used to store it in UTF-8 encoding.
///
/// # Example
///
/// ```rhai
/// let text = "朝には紅顔ありて夕べには白骨となる";
///
/// print(text.bytes);      // prints 51
/// ```
fn get bytes(string: String) -> int;

/// Return the smallest whole number larger than or equals to the decimal number.
fn get ceiling(x: Decimal) -> Decimal;

/// Return the smallest whole number larger than or equals to the floating-point number.
fn get ceiling(x: float) -> float;

/// Return an iterator over all the characters in the string.
///
/// # Example
///
/// ```rhai
/// for ch in "hello, world!".chars {
///     print(ch);
/// }
/// ```
fn get chars(string: String) -> Iterator<char>;

/// Return the number of seconds between the current system time and the timestamp.
///
/// # Example
///
/// ```rhai
/// let now = timestamp();
///
/// sleep(10.0);            // sleep for 10 seconds
///
/// print(now.elapsed);     // prints 10.???
/// ```
fn get elapsed(timestamp: Instant) -> RhaiResult;

/// Return the end of the exclusive range.
fn get end(range: ExclusiveRange) -> int;

/// Return the end of the inclusive range.
fn get end(range: InclusiveRange) -> int;

/// Return the largest whole number less than or equals to the decimal number.
fn get floor(x: Decimal) -> Decimal;

/// Return the largest whole number less than or equals to the floating-point number.
fn get floor(x: float) -> float;

/// Return the fractional part of the decimal number.
fn get fraction(x: Decimal) -> Decimal;

/// Return the fractional part of the floating-point number.
fn get fraction(x: float) -> float;

/// Return the integral part of the decimal number.
fn get int(x: Decimal) -> Decimal;

/// Return the integral part of the floating-point number.
fn get int(x: float) -> float;

/// Return `true` if the function is an anonymous function.
///
/// # Example
///
/// ```rhai
/// let f = |x| x * 2;
///
/// print(f.is_anonymous);      // prints true
/// ```
fn get is_anonymous(fn_ptr: FnPtr) -> bool;

/// Return true if the array is empty.
fn get is_empty(array: Array) -> bool;

/// Return true if the BLOB is empty.
fn get is_empty(blob: Blob) -> bool;

/// Return true if the range contains no items.
fn get is_empty(range: ExclusiveRange) -> bool;

/// Return true if the range contains no items.
fn get is_empty(range: InclusiveRange) -> bool;

/// Return true if the string is empty.
fn get is_empty(string: String) -> bool;

/// Return true if the number is even.
fn get is_even(x: int) -> bool;

/// Return true if the number is even.
fn get is_even(x: i128) -> bool;

/// Return true if the number is even.
fn get is_even(x: i16) -> bool;

/// Return true if the number is even.
fn get is_even(x: i32) -> bool;

/// Return true if the number is even.
fn get is_even(x: i8) -> bool;

/// Return true if the number is even.
fn get is_even(x: u128) -> bool;

/// Return true if the number is even.
fn get is_even(x: u16) -> bool;

/// Return true if the number is even.
fn get is_even(x: u32) -> bool;

/// Return true if the number is even.
fn get is_even(x: u64) -> bool;

/// Return true if the number is even.
fn get is_even(x: u8) -> bool;

/// Return `true` if the range is exclusive.
fn get is_exclusive(range: ExclusiveRange) -> bool;

/// Return `true` if the range is exclusive.
fn get is_exclusive(range: InclusiveRange) -> bool;

/// Return `true` if the floating-point number is finite.
fn get is_finite(x: float) -> bool;

/// Return `true` if the range is inclusive.
fn get is_inclusive(range: ExclusiveRange) -> bool;

/// Return `true` if the range is inclusive.
fn get is_inclusive(range: InclusiveRange) -> bool;

/// Return `true` if the floating-point number is infinite.
fn get is_infinite(x: float) -> bool;

/// Return `true` if the floating-point number is `NaN` (Not A Number).
fn get is_nan(x: float) -> bool;

/// Return true if the number is odd.
fn get is_odd(x: int) -> bool;

/// Return true if the number is odd.
fn get is_odd(x: i128) -> bool;

/// Return true if the number is odd.
fn get is_odd(x: i16) -> bool;

/// Return true if the number is odd.
fn get is_odd(x: i32) -> bool;

/// Return true if the number is odd.
fn get is_odd(x: i8) -> bool;

/// Return true if the number is odd.
fn get is_odd(x: u128) -> bool;

/// Return true if the number is odd.
fn get is_odd(x: u16) -> bool;

/// Return true if the number is odd.
fn get is_odd(x: u32) -> bool;

/// Return true if the number is odd.
fn get is_odd(x: u64) -> bool;

/// Return true if the number is odd.
fn get is_odd(x: u8) -> bool;

/// Return true if the decimal number is zero.
fn get is_zero(x: Decimal) -> bool;

/// Return true if the number is zero.
fn get is_zero(x: int) -> bool;

/// Return true if the floating-point number is zero.
fn get is_zero(x: f32) -> bool;

/// Return true if the floating-point number is zero.
fn get is_zero(x: float) -> bool;

/// Return true if the number is zero.
fn get is_zero(x: i128) -> bool;

/// Return true if the number is zero.
fn get is_zero(x: i16) -> bool;

/// Return true if the number is zero.
fn get is_zero(x: i32) -> bool;

/// Return true if the number is zero.
fn get is_zero(x: i8) -> bool;

/// Return true if the number is zero.
fn get is_zero(x: u128) -> bool;

/// Return true if the number is zero.
fn get is_zero(x: u16) -> bool;

/// Return true if the number is zero.
fn get is_zero(x: u32) -> bool;

/// Return true if the number is zero.
fn get is_zero(x: u64) -> bool;

/// Return true if the number is zero.
fn get is_zero(x: u8) -> bool;

/// Number of elements in the array.
fn get len(array: Array) -> int;

/// Return the length of the BLOB.
///
/// # Example
///
/// ```rhai
/// let b = blob(10, 0x42);
///
/// print(b);           // prints "[4242424242424242 4242]"
///
/// print(b.len());     // prints 10
/// ```
fn get len(blob: Blob) -> int;

/// Return the length of the string, in number of characters.
///
/// # Example
///
/// ```rhai
/// let text = "朝には紅顔ありて夕べには白骨となる";
///
/// print(text.len);        // prints 17
/// ```
fn get len(string: String) -> int;

/// Return the name of the function.
///
/// # Example
///
/// ```rhai
/// fn double(x) { x * 2 }
///
/// let f = Fn("double");
///
/// print(f.name);      // prints "double"
/// ```
fn get name(fn_ptr: FnPtr) -> String;

/// Return the nearest whole number closest to the decimal number.
/// Always round mid-point towards the closest even number.
fn get round(x: Decimal) -> Decimal;

/// Return the nearest whole number closest to the floating-point number.
/// Rounds away from zero.
fn get round(x: float) -> float;

/// Return the start of the exclusive range.
fn get start(range: ExclusiveRange) -> int;

/// Return the start of the inclusive range.
fn get start(range: InclusiveRange) -> int;

/// Return the _tag_ of a `Dynamic` value.
///
/// # Example
///
/// ```rhai
/// let x = "hello, world!";
///
/// x.tag = 42;
///
/// print(x.tag);           // prints 42
/// ```
fn get tag(value: ?) -> int;

/// Return `true` if the specified `bit` in the number is set.
///
/// If `bit` < 0, position counts from the MSB (Most Significant Bit).
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// print(x.get_bit(5));    // prints false
///
/// print(x.get_bit(6));    // prints true
///
/// print(x.get_bit(-48));  // prints true on 64-bit
/// ```
fn get_bit(value: int, bit: int) -> bool;

/// Return an exclusive range of bits in the number as a new number.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// print(x.get_bits(5..10));       // print 18
/// ```
fn get_bits(value: int, range: Range<int>) -> int;

/// Return an inclusive range of bits in the number as a new number.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// print(x.get_bits(5..=9));       // print 18
/// ```
fn get_bits(value: int, range: RangeInclusive<int>) -> int;

/// Return a portion of bits in the number as a new number.
///
/// * If `start` < 0, position counts from the MSB (Most Significant Bit).
/// * If `bits` ≤ 0, zero is returned.
/// * If `start` position + `bits` ≥ total number of bits, the bits after the `start` position are returned.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// print(x.get_bits(5, 8));        // print 18
/// ```
fn get_bits(value: int, start: int, bits: int) -> int;

fn get_fn_metadata_list() -> Array;

fn get_fn_metadata_list(name: String) -> Array;

fn get_fn_metadata_list(name: String, params: int) -> Array;

/// Return the hypotenuse of a triangle with sides `x` and `y`.
fn hypot(x: float, y: float) -> float;

/// Iterate through all the elements in the array, applying a function named by `filter` to each
/// element in turn, and return the index of the first element that returns `true`.
/// If no element returns `true`, `-1` is returned.
///
/// # Function Parameters
///
/// A function with the same name as the value of `filter` must exist taking these parameters:
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// fn is_special(x) { x > 3 }
///
/// fn is_dumb(x) { x > 8 }
///
/// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
///
/// print(x.index_of("is_special"));    // prints 3
///
/// print(x.index_of("is_dumb"));       // prints -1
/// ```
fn index_of(array: Array, filter: String) -> int;

/// Iterate through all the elements in the array, applying a `filter` function to each element
/// in turn, and return the index of the first element that returns `true`.
/// If no element returns `true`, `-1` is returned.
///
/// # Function Parameters
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
///
/// print(x.index_of(|v| v > 3));           // prints 3: 4 > 3
///
/// print(x.index_of(|v| v > 8));           // prints -1: nothing is > 8
///
/// print(x.index_of(|v, i| v * i > 20));   // prints 7: 4 * 7 > 20
/// ```
fn index_of(array: Array, filter: FnPtr) -> int;

/// Find the first element in the array that equals a particular `value` and return its index.
/// If no element equals `value`, `-1` is returned.
///
/// The operator `==` is used to compare elements with `value` and must be defined,
/// otherwise `false` is assumed.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
///
/// print(x.index_of(4));       // prints 3 (first index)
///
/// print(x.index_of(9));       // prints -1
///
/// print(x.index_of("foo"));   // prints -1: strings do not equal numbers
/// ```
fn index_of(array: Array, value: ?) -> int;

/// Find the specified `character` in the string and return the first index where it is found.
/// If the `character` is not found, `-1` is returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.index_of('l'));      // prints 2 (first index)
///
/// print(text.index_of('x'));      // prints -1
/// ```
fn index_of(string: String, character: char) -> int;

/// Find the specified `character` in the string and return the first index where it is found.
/// If the `character` is not found, `-1` is returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foobar!";
///
/// print(text.index_of("ll"));     // prints 2 (first index)
///
/// print(text.index_of("xx:));     // prints -1
/// ```
fn index_of(string: String, find_string: String) -> int;

/// Iterate through all the elements in the array, starting from a particular `start` position,
/// applying a function named by `filter` to each element in turn, and return the index of the
/// first element that returns `true`. If no element returns `true`, `-1` is returned.
///
/// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `start` < -length of array, position counts from the beginning of the array.
/// * If `start` ≥ length of array, `-1` is returned.
///
/// # Function Parameters
///
/// A function with the same name as the value of `filter` must exist taking these parameters:
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// fn plural(x) { x > 1 }
///
/// fn singular(x) { x < 2 }
///
/// fn screen(x, i) { x * i > 20 }
///
/// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
///
/// print(x.index_of("plural", 3));     // prints 5: 2 > 1
///
/// print(x.index_of("singular", 9));   // prints -1: nothing < 2 past index 9
///
/// print(x.index_of("plural", 15));    // prints -1: nothing found past end of array
///
/// print(x.index_of("plural", -5));    // prints 9: -5 = start from index 8
///
/// print(x.index_of("plural", -99));   // prints 1: -99 = start from beginning
///
/// print(x.index_of("screen", 8));     // prints 10: 3 * 10 > 20
/// ```
fn index_of(array: Array, filter: String, start: int) -> int;

/// Iterate through all the elements in the array, starting from a particular `start` position,
/// applying a `filter` function to each element in turn, and return the index of the first
/// element that returns `true`. If no element returns `true`, `-1` is returned.
///
/// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `start` < -length of array, position counts from the beginning of the array.
/// * If `start` ≥ length of array, `-1` is returned.
///
/// # Function Parameters
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
///
/// print(x.index_of(|v| v > 1, 3));    // prints 5: 2 > 1
///
/// print(x.index_of(|v| v < 2, 9));    // prints -1: nothing < 2 past index 9
///
/// print(x.index_of(|v| v > 1, 15));   // prints -1: nothing found past end of array
///
/// print(x.index_of(|v| v > 1, -5));   // prints 9: -5 = start from index 8
///
/// print(x.index_of(|v| v > 1, -99));  // prints 1: -99 = start from beginning
///
/// print(x.index_of(|v, i| v * i > 20, 8));    // prints 10: 3 * 10 > 20
/// ```
fn index_of(array: Array, filter: FnPtr, start: int) -> int;

/// Find the first element in the array, starting from a particular `start` position, that
/// equals a particular `value` and return its index. If no element equals `value`, `-1` is returned.
///
/// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `start` < -length of array, position counts from the beginning of the array.
/// * If `start` ≥ length of array, `-1` is returned.
///
/// The operator `==` is used to compare elements with `value` and must be defined,
/// otherwise `false` is assumed.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
///
/// print(x.index_of(4, 2));        // prints 3
///
/// print(x.index_of(4, 5));        // prints 7
///
/// print(x.index_of(4, 15));       // prints -1: nothing found past end of array
///
/// print(x.index_of(4, -5));       // prints 11: -5 = start from index 8
///
/// print(x.index_of(9, 1));        // prints -1: nothing equals 9
///
/// print(x.index_of("foo", 1));    // prints -1: strings do not equal numbers
/// ```
fn index_of(array: Array, value: ?, start: int) -> int;

/// Find the specified `character` in the string, starting from the specified `start` position,
/// and return the first index where it is found.
/// If the `character` is not found, `-1` is returned.
///
/// * If `start` < 0, position counts from the end of the string (`-1` is the last character).
/// * If `start` < -length of string, position counts from the beginning of the string.
/// * If `start` ≥ length of string, `-1` is returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.index_of('l', 5));       // prints 10 (first index after 5)
///
/// print(text.index_of('o', -7));      // prints 8
///
/// print(text.index_of('x', 0));       // prints -1
/// ```
fn index_of(string: String, character: char, start: int) -> int;

/// Find the specified sub-string in the string, starting from the specified `start` position,
/// and return the first index where it is found.
/// If the sub-string is not found, `-1` is returned.
///
/// * If `start` < 0, position counts from the end of the string (`-1` is the last character).
/// * If `start` < -length of string, position counts from the beginning of the string.
/// * If `start` ≥ length of string, `-1` is returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foobar!";
///
/// print(text.index_of("ll", 5));      // prints 16 (first index after 5)
///
/// print(text.index_of("ll", -15));    // prints 16
///
/// print(text.index_of("xx", 0));      // prints -1
/// ```
fn index_of(string: String, find_string: String, start: int) -> int;

/// Add a new element into the array at a particular `index` position.
///
/// * If `index` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `index` < -length of array, the element is added to the beginning of the array.
/// * If `index` ≥ length of array, the element is appended to the end of the array.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3];
///
/// x.insert(0, "hello");
///
/// x.insert(2, true);
///
/// x.insert(-2, 42);
///
/// print(x);       // prints ["hello", 1, true, 2, 42, 3]
/// ```
fn insert(array: Array, index: int, item: ?) -> ();

/// Add a byte `value` to the BLOB at a particular `index` position.
///
/// * If `index` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `index` < -length of BLOB, the byte value is added to the beginning of the BLOB.
/// * If `index` ≥ length of BLOB, the byte value is appended to the end of the BLOB.
///
/// Only the lower 8 bits of the `value` are used; all other bits are ignored.
///
/// # Example
///
/// ```rhai
/// let b = blob(5, 0x42);
///
/// b.insert(2, 0x18);
///
/// print(b);       // prints "[4242184242]"
/// ```
fn insert(blob: Blob, index: int, value: int) -> ();

/// Return the integral part of the decimal number.
fn int(x: Decimal) -> Decimal;

/// Return the integral part of the floating-point number.
fn int(x: float) -> float;

/// Return `true` if the function is an anonymous function.
///
/// # Example
///
/// ```rhai
/// let f = |x| x * 2;
///
/// print(f.is_anonymous);      // prints true
/// ```
fn is_anonymous(fn_ptr: FnPtr) -> bool;

/// Return true if the array is empty.
fn is_empty(array: Array) -> bool;

/// Return true if the BLOB is empty.
fn is_empty(blob: Blob) -> bool;

/// Return true if the map is empty.
fn is_empty(map: Map) -> bool;

/// Return true if the range contains no items.
fn is_empty(range: ExclusiveRange) -> bool;

/// Return true if the range contains no items.
fn is_empty(range: InclusiveRange) -> bool;

/// Return true if the string is empty.
fn is_empty(string: String) -> bool;

/// Return true if the number is even.
fn is_even(x: int) -> bool;

/// Return true if the number is even.
fn is_even(x: i128) -> bool;

/// Return true if the number is even.
fn is_even(x: i16) -> bool;

/// Return true if the number is even.
fn is_even(x: i32) -> bool;

/// Return true if the number is even.
fn is_even(x: i8) -> bool;

/// Return true if the number is even.
fn is_even(x: u128) -> bool;

/// Return true if the number is even.
fn is_even(x: u16) -> bool;

/// Return true if the number is even.
fn is_even(x: u32) -> bool;

/// Return true if the number is even.
fn is_even(x: u64) -> bool;

/// Return true if the number is even.
fn is_even(x: u8) -> bool;

/// Return `true` if the range is exclusive.
fn is_exclusive(range: ExclusiveRange) -> bool;

/// Return `true` if the range is exclusive.
fn is_exclusive(range: InclusiveRange) -> bool;

/// Return `true` if the floating-point number is finite.
fn is_finite(x: float) -> bool;

/// Return `true` if the range is inclusive.
fn is_inclusive(range: ExclusiveRange) -> bool;

/// Return `true` if the range is inclusive.
fn is_inclusive(range: InclusiveRange) -> bool;

/// Return `true` if the floating-point number is infinite.
fn is_infinite(x: float) -> bool;

/// Return `true` if the floating-point number is `NaN` (Not A Number).
fn is_nan(x: float) -> bool;

/// Return true if the number is odd.
fn is_odd(x: int) -> bool;

/// Return true if the number is odd.
fn is_odd(x: i128) -> bool;

/// Return true if the number is odd.
fn is_odd(x: i16) -> bool;

/// Return true if the number is odd.
fn is_odd(x: i32) -> bool;

/// Return true if the number is odd.
fn is_odd(x: i8) -> bool;

/// Return true if the number is odd.
fn is_odd(x: u128) -> bool;

/// Return true if the number is odd.
fn is_odd(x: u16) -> bool;

/// Return true if the number is odd.
fn is_odd(x: u32) -> bool;

/// Return true if the number is odd.
fn is_odd(x: u64) -> bool;

/// Return true if the number is odd.
fn is_odd(x: u8) -> bool;

/// Return true if the decimal number is zero.
fn is_zero(x: Decimal) -> bool;

/// Return true if the number is zero.
fn is_zero(x: int) -> bool;

/// Return true if the floating-point number is zero.
fn is_zero(x: f32) -> bool;

/// Return true if the floating-point number is zero.
fn is_zero(x: float) -> bool;

/// Return true if the number is zero.
fn is_zero(x: i128) -> bool;

/// Return true if the number is zero.
fn is_zero(x: i16) -> bool;

/// Return true if the number is zero.
fn is_zero(x: i32) -> bool;

/// Return true if the number is zero.
fn is_zero(x: i8) -> bool;

/// Return true if the number is zero.
fn is_zero(x: u128) -> bool;

/// Return true if the number is zero.
fn is_zero(x: u16) -> bool;

/// Return true if the number is zero.
fn is_zero(x: u32) -> bool;

/// Return true if the number is zero.
fn is_zero(x: u64) -> bool;

/// Return true if the number is zero.
fn is_zero(x: u8) -> bool;

/// Return an array with all the property names in the object map.
///
/// # Example
///
/// ```rhai
/// let m = #{a:1, b:2, c:3};
///
/// print(m.keys());        // prints ["a", "b", "c"]
/// ```
fn keys(map: Map) -> Array;

/// Number of elements in the array.
fn len(array: Array) -> int;

/// Return the length of the BLOB.
///
/// # Example
///
/// ```rhai
/// let b = blob(10, 0x42);
///
/// print(b);           // prints "[4242424242424242 4242]"
///
/// print(b.len());     // prints 10
/// ```
fn len(blob: Blob) -> int;

/// Return the number of properties in the object map.
fn len(map: Map) -> int;

/// Return the length of the string, in number of characters.
///
/// # Example
///
/// ```rhai
/// let text = "朝には紅顔ありて夕べには白骨となる";
///
/// print(text.len);        // prints 17
/// ```
fn len(string: String) -> int;

/// Return the natural log of the decimal number.
fn ln(x: Decimal) -> Decimal;

/// Return the natural log of the floating-point number.
fn ln(x: float) -> float;

/// Return the log of the decimal number with base 10.
fn log(x: Decimal) -> Decimal;

/// Return the log of the floating-point number with base 10.
fn log(x: float) -> float;

/// Return the log of the floating-point number with `base`.
fn log(x: float, base: float) -> float;

/// Convert the character to lower-case.
///
/// # Example
///
/// ```rhai
/// let ch = 'A';
///
/// ch.make_lower();
///
/// print(ch);          // prints 'a'
/// ```
fn make_lower(character: char) -> ();

/// Convert the string to all lower-case.
///
/// # Example
///
/// ```rhai
/// let text = "HELLO, WORLD!"
///
/// text.make_lower();
///
/// print(text);        // prints "hello, world!";
/// ```
fn make_lower(string: String) -> ();

/// Convert the character to upper-case.
///
/// # Example
///
/// ```rhai
/// let ch = 'a';
///
/// ch.make_upper();
///
/// print(ch);          // prints 'A'
/// ```
fn make_upper(character: char) -> ();

/// Convert the string to all upper-case.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!"
///
/// text.make_upper();
///
/// print(text);        // prints "HELLO, WORLD!";
/// ```
fn make_upper(string: String) -> ();

/// Iterate through all the elements in the array, applying a function named by `mapper` to each
/// element in turn, and return the results as a new array.
///
/// # Function Parameters
///
/// A function with the same name as the value of `mapper` must exist taking these parameters:
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// fn square(x) { x * x }
///
/// fn multiply(x, i) { x * i }
///
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.map("square");
///
/// print(y);       // prints "[1, 4, 9, 16, 25]"
///
/// let y = x.map("multiply");
///
/// print(y);       // prints "[0, 2, 6, 12, 20]"
/// ```
fn map(array: Array, mapper: String) -> Array;

/// Iterate through all the elements in the array, applying a `mapper` function to each element
/// in turn, and return the results as a new array.
///
/// # Function Parameters
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.map(|v| v * v);
///
/// print(y);       // prints "[1, 4, 9, 16, 25]"
///
/// let y = x.map(|v, i| v * i);
///
/// print(y);       // prints "[0, 2, 6, 12, 20]"
/// ```
fn map(array: Array, mapper: FnPtr) -> Array;

/// Add all property values of another object map into the object map.
/// Existing property values of the same names are replaced.
///
/// # Example
///
/// ```rhai
/// let m = #{a:1, b:2, c:3};
/// let n = #{a: 42, d:0};
///
/// m.mixin(n);
///
/// print(m);       // prints "#{a:42, b:2, c:3, d:0}"
/// ```
fn mixin(map: Map, map2: Map) -> ();

/// Return the name of the function.
///
/// # Example
///
/// ```rhai
/// fn double(x) { x * 2 }
///
/// let f = Fn("double");
///
/// print(f.name);      // prints "double"
/// ```
fn name(fn_ptr: FnPtr) -> String;

/// Pad the array to at least the specified length with copies of a specified element.
///
/// If `len` ≤ length of array, no padding is done.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3];
///
/// x.pad(5, 42);
///
/// print(x);       // prints "[1, 2, 3, 42, 42]"
///
/// x.pad(3, 123);
///
/// print(x);       // prints "[1, 2, 3, 42, 42]"
/// ```
fn pad(array: Array, len: int, item: ?) -> ();

/// Pad the BLOB to at least the specified length with copies of a specified byte `value`.
///
/// If `len` ≤ length of BLOB, no padding is done.
///
/// Only the lower 8 bits of the `value` are used; all other bits are ignored.
///
/// # Example
///
/// ```rhai
/// let b = blob(3, 0x42);
///
/// b.pad(5, 0x18)
///
/// print(b);               // prints "[4242421818]"
///
/// b.pad(3, 0xab)
///
/// print(b);               // prints "[4242421818]"
/// ```
fn pad(blob: Blob, len: int, value: int) -> ();

/// Pad the string to at least the specified number of characters with the specified `character`.
///
/// If `len` ≤ length of string, no padding is done.
///
/// # Example
///
/// ```rhai
/// let text = "hello";
///
/// text.pad(8, '!');
///
/// print(text);        // prints "hello!!!"
///
/// text.pad(5, '*');
///
/// print(text);        // prints "hello!!!"
/// ```
fn pad(string: String, len: int, character: char) -> ();

/// Pad the string to at least the specified number of characters with the specified string.
///
/// If `len` ≤ length of string, no padding is done.
///
/// # Example
///
/// ```rhai
/// let text = "hello";
///
/// text.pad(10, "(!)");
///
/// print(text);        // prints "hello(!)(!)"
///
/// text.pad(8, '***');
///
/// print(text);        // prints "hello(!)(!)"
/// ```
fn pad(string: String, len: int, padding: String) -> ();

/// Parse the bytes within an exclusive `range` in the BLOB as a `FLOAT`
/// in big-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `FLOAT`, zeros are padded.
/// * If number of bytes in `range` > number of bytes for `FLOAT`, extra bytes are ignored.
fn parse_be_float(blob: Blob, range: Range<int>) -> float;

/// Parse the bytes within an inclusive `range` in the BLOB as a `FLOAT`
/// in big-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `FLOAT`, zeros are padded.
/// * If number of bytes in `range` > number of bytes for `FLOAT`, extra bytes are ignored.
fn parse_be_float(blob: Blob, range: RangeInclusive<int>) -> float;

/// Parse the bytes beginning at the `start` position in the BLOB as a `FLOAT`
/// in big-endian byte order.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, zero is returned.
/// * If `len` ≤ 0, zero is returned.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB after the `start` position is parsed.
///
/// * If number of bytes in range < number of bytes for `FLOAT`, zeros are padded.
/// * If number of bytes in range > number of bytes for `FLOAT`, extra bytes are ignored.
fn parse_be_float(blob: Blob, start: int, len: int) -> float;

/// Parse the bytes within an exclusive `range` in the BLOB as an `INT`
/// in big-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `INT`, zeros are padded.
/// * If number of bytes in `range` > number of bytes for `INT`, extra bytes are ignored.
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// let x = b.parse_be_int(1..3);   // parse two bytes
///
/// print(x.to_hex());              // prints "02030000...00"
/// ```
fn parse_be_int(blob: Blob, range: Range<int>) -> int;

/// Parse the bytes within an inclusive `range` in the BLOB as an `INT`
/// in big-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `INT`, zeros are padded.
/// * If number of bytes in `range` > number of bytes for `INT`, extra bytes are ignored.
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// let x = b.parse_be_int(1..=3);  // parse three bytes
///
/// print(x.to_hex());              // prints "0203040000...00"
/// ```
fn parse_be_int(blob: Blob, range: RangeInclusive<int>) -> int;

/// Parse the bytes beginning at the `start` position in the BLOB as an `INT`
/// in big-endian byte order.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, zero is returned.
/// * If `len` ≤ 0, zero is returned.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB after the `start` position is parsed.
///
/// * If number of bytes in range < number of bytes for `INT`, zeros are padded.
/// * If number of bytes in range > number of bytes for `INT`, extra bytes are ignored.
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// let x = b.parse_be_int(1, 2);
///
/// print(x.to_hex());      // prints "02030000...00"
/// ```
fn parse_be_int(blob: Blob, start: int, len: int) -> int;

/// Parse a string into a decimal number.
///
/// # Example
///
/// ```rhai
/// let x = parse_decimal("123.456");
///
/// print(x);       // prints 123.456
/// ```
fn parse_decimal(string: String) -> Decimal;

/// Parse a string into a floating-point number.
///
/// # Example
///
/// ```rhai
/// let x = parse_int("123.456");
///
/// print(x);       // prints 123.456
/// ```
fn parse_float(string: String) -> float;

/// Parse a string into an integer number.
///
/// # Example
///
/// ```rhai
/// let x = parse_int("123");
///
/// print(x);       // prints 123
/// ```
fn parse_int(string: String) -> int;

/// Parse a string into an integer number of the specified `radix`.
///
/// `radix` must be between 2 and 36.
///
/// # Example
///
/// ```rhai
/// let x = parse_int("123");
///
/// print(x);       // prints 123
///
/// let y = parse_int("123abc", 16);
///
/// print(y);       // prints 1194684 (0x123abc)
/// ```
fn parse_int(string: String, radix: int) -> int;

/// Parse the bytes within an exclusive `range` in the BLOB as a `FLOAT`
/// in little-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `FLOAT`, zeros are padded.
/// * If number of bytes in `range` > number of bytes for `FLOAT`, extra bytes are ignored.
fn parse_le_float(blob: Blob, range: Range<int>) -> float;

/// Parse the bytes within an inclusive `range` in the BLOB as a `FLOAT`
/// in little-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `FLOAT`, zeros are padded.
/// * If number of bytes in `range` > number of bytes for `FLOAT`, extra bytes are ignored.
fn parse_le_float(blob: Blob, range: RangeInclusive<int>) -> float;

/// Parse the bytes beginning at the `start` position in the BLOB as a `FLOAT`
/// in little-endian byte order.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, zero is returned.
/// * If `len` ≤ 0, zero is returned.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB after the `start` position is parsed.
///
/// * If number of bytes in range < number of bytes for `FLOAT`, zeros are padded.
/// * If number of bytes in range > number of bytes for `FLOAT`, extra bytes are ignored.
fn parse_le_float(blob: Blob, start: int, len: int) -> float;

/// Parse the bytes within an exclusive `range` in the BLOB as an `INT`
/// in little-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `INT`, zeros are padded.
/// * If number of bytes in `range` > number of bytes for `INT`, extra bytes are ignored.
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// let x = b.parse_le_int(1..3);   // parse two bytes
///
/// print(x.to_hex());              // prints "0302"
/// ```
fn parse_le_int(blob: Blob, range: Range<int>) -> int;

/// Parse the bytes within an inclusive `range` in the BLOB as an `INT`
/// in little-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `INT`, zeros are padded.
/// * If number of bytes in `range` > number of bytes for `INT`, extra bytes are ignored.
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// let x = b.parse_le_int(1..=3);  // parse three bytes
///
/// print(x.to_hex());              // prints "040302"
/// ```
fn parse_le_int(blob: Blob, range: RangeInclusive<int>) -> int;

/// Parse the bytes beginning at the `start` position in the BLOB as an `INT`
/// in little-endian byte order.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, zero is returned.
/// * If `len` ≤ 0, zero is returned.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB after the `start` position is parsed.
///
/// * If number of bytes in range < number of bytes for `INT`, zeros are padded.
/// * If number of bytes in range > number of bytes for `INT`, extra bytes are ignored.
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// let x = b.parse_le_int(1, 2);
///
/// print(x.to_hex());      // prints "0302"
/// ```
fn parse_le_int(blob: Blob, start: int, len: int) -> int;

/// Remove the last element from the array and return it.
///
/// If the array is empty, `()` is returned.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3];
///
/// print(x.pop());     // prints 3
///
/// print(x);           // prints "[1, 2]"
/// ```
fn pop(array: Array) -> ?;

/// Remove the last byte from the BLOB and return it.
///
/// If the BLOB is empty, zero is returned.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// print(b.pop());         // prints 5
///
/// print(b);               // prints "[01020304]"
/// ```
fn pop(blob: Blob) -> int;

/// Remove the last character from the string and return it.
///
/// If the string is empty, `()` is returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.pop());      // prints '!'
///
/// print(text);            // prints "hello, world"
/// ```
fn pop(string: String) -> ?;

/// Remove a specified number of characters from the end of the string and return it as a
/// new string.
///
/// * If `len` ≤ 0, the string is not modified and an empty string is returned.
/// * If `len` ≥ length of string, the string is cleared and the entire string returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.pop(4));     // prints "rld!"
///
/// print(text);            // prints "hello, wo"
/// ```
fn pop(string: String, len: int) -> String;

/// Return the empty string.
fn print() -> String;

/// Convert the array into a string.
fn print(array: Array) -> String;

/// Return the character into a string.
fn print(character: char) -> String;

/// Convert the value of the `item` into a string.
fn print(item: ?) -> String;

/// Convert the object map into a string.
fn print(map: Map) -> String;

/// Convert the value of `number` into a string.
fn print(number: f32) -> String;

/// Convert the value of `number` into a string.
fn print(number: float) -> String;

/// Return the `string`.
fn print(string: String) -> String;

/// Return the empty string.
fn print(unit: ()) -> String;

/// Return the boolean value into a string.
fn print(value: bool) -> String;

/// Add a new element, which is not another array, to the end of the array.
///
/// If `item` is `Array`, then `append` is more specific and will be called instead.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3];
///
/// x.push("hello");
///
/// print(x);       // prints [1, 2, 3, "hello"]
/// ```
fn push(array: Array, item: ?) -> ();

/// Add a new byte `value` to the end of the BLOB.
///
/// Only the lower 8 bits of the `value` are used; all other bits are ignored.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b.push(0x42);
///
/// print(b);       // prints "[42]"
/// ```
fn push(blob: Blob, value: int) -> ();

/// Return an iterator over the exclusive range of `from..to`.
/// The value `to` is never included.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17
/// for n in range(8, 18) {
///     print(n);
/// }
/// ```
fn range(from: i128, to: i128) -> Iterator<i128>;

/// Return an iterator over the exclusive range of `from..to`.
/// The value `to` is never included.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17
/// for n in range(8, 18) {
///     print(n);
/// }
/// ```
fn range(from: i16, to: i16) -> Iterator<i16>;

/// Return an iterator over the exclusive range of `from..to`.
/// The value `to` is never included.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17
/// for n in range(8, 18) {
///     print(n);
/// }
/// ```
fn range(from: i32, to: i32) -> Iterator<i32>;

/// Return an iterator over the exclusive range of `from..to`.
/// The value `to` is never included.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17
/// for n in range(8, 18) {
///     print(n);
/// }
/// ```
fn range(from: int, to: int) -> Iterator<int>;

/// Return an iterator over the exclusive range of `from..to`.
/// The value `to` is never included.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17
/// for n in range(8, 18) {
///     print(n);
/// }
/// ```
fn range(from: i8, to: i8) -> Iterator<i8>;

/// Return an iterator over the exclusive range of `from..to`.
/// The value `to` is never included.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17
/// for n in range(8, 18) {
///     print(n);
/// }
/// ```
fn range(from: u128, to: u128) -> Iterator<u128>;

/// Return an iterator over the exclusive range of `from..to`.
/// The value `to` is never included.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17
/// for n in range(8, 18) {
///     print(n);
/// }
/// ```
fn range(from: u16, to: u16) -> Iterator<u16>;

/// Return an iterator over the exclusive range of `from..to`.
/// The value `to` is never included.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17
/// for n in range(8, 18) {
///     print(n);
/// }
/// ```
fn range(from: u32, to: u32) -> Iterator<u32>;

/// Return an iterator over the exclusive range of `from..to`.
/// The value `to` is never included.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17
/// for n in range(8, 18) {
///     print(n);
/// }
/// ```
fn range(from: u64, to: u64) -> Iterator<u64>;

/// Return an iterator over the exclusive range of `from..to`.
/// The value `to` is never included.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17
/// for n in range(8, 18) {
///     print(n);
/// }
/// ```
fn range(from: u8, to: u8) -> Iterator<u8>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<Decimal>, step: Decimal) -> Iterator<Decimal>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<float>, step: float) -> Iterator<float>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<i128>, step: i128) -> Iterator<i128>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<i16>, step: i16) -> Iterator<i16>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<i32>, step: i32) -> Iterator<i32>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<int>, step: int) -> Iterator<int>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<i8>, step: i8) -> Iterator<i8>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<u128>, step: u128) -> Iterator<u128>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<u16>, step: u16) -> Iterator<u16>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<u32>, step: u32) -> Iterator<u32>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<u64>, step: u64) -> Iterator<u64>;

/// Return an iterator over an exclusive range, each iteration increasing by `step`.
///
/// If `range` is reversed and `step` < 0, iteration goes backwards.
///
/// Otherwise, if `range` is empty, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8..18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18..8, -3) {
///     print(n);
/// }
/// ```
fn range(range: Range<u8>, step: u8) -> Iterator<u8>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: Decimal, to: Decimal, step: Decimal) -> Iterator<Decimal>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: float, to: float, step: float) -> Iterator<float>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: i128, to: i128, step: i128) -> Iterator<i128>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: i16, to: i16, step: i16) -> Iterator<i16>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: i32, to: i32, step: i32) -> Iterator<i32>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: int, to: int, step: int) -> Iterator<int>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: i8, to: i8, step: i8) -> Iterator<i8>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: u128, to: u128, step: u128) -> Iterator<u128>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: u16, to: u16, step: u16) -> Iterator<u16>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: u32, to: u32, step: u32) -> Iterator<u32>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: u64, to: u64, step: u64) -> Iterator<u64>;

/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
/// The value `to` is never included.
///
/// If `from` > `to` and `step` < 0, iteration goes backwards.
///
/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
///
/// # Example
///
/// ```rhai
/// // prints all values from 8 to 17 in steps of 3
/// for n in range(8, 18, 3) {
///     print(n);
/// }
///
/// // prints all values down from 18 to 9 in steps of -3
/// for n in range(18, 8, -3) {
///     print(n);
/// }
/// ```
fn range(from: u8, to: u8, step: u8) -> Iterator<u8>;

/// Reduce an array by iterating through all elements while applying a function named by `reducer`.
///
/// # Function Parameters
///
/// A function with the same name as the value of `reducer` must exist taking these parameters:
///
/// * `result`: accumulated result, initially `()`
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// fn process(r, x) {
///     x + (r ?? 0)
/// }
/// fn process_extra(r, x, i) {
///     x + i + (r ?? 0)
/// }
///
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.reduce("process");
///
/// print(y);       // prints 15
///
/// let y = x.reduce("process_extra");
///
/// print(y);       // prints 25
/// ```
fn reduce(array: Array, reducer: String) -> RhaiResult;

/// Reduce an array by iterating through all elements while applying the `reducer` function.
///
/// # Function Parameters
///
/// * `result`: accumulated result, initially `()`
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.reduce(|r, v| v + (r ?? 0));
///
/// print(y);       // prints 15
///
/// let y = x.reduce(|r, v, i| v + i + (r ?? 0));
///
/// print(y);       // prints 25
/// ```
fn reduce(array: Array, reducer: FnPtr) -> RhaiResult;

/// Reduce an array by iterating through all elements while applying a function named by `reducer`.
///
/// # Function Parameters
///
/// A function with the same name as the value of `reducer` must exist taking these parameters:
///
/// * `result`: accumulated result, starting with the value of `initial`
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// fn process(r, x) { x + r }
///
/// fn process_extra(r, x, i) { x + i + r }
///
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.reduce("process", 5);
///
/// print(y);       // prints 20
///
/// let y = x.reduce("process_extra", 5);
///
/// print(y);       // prints 30
/// ```
fn reduce(array: Array, reducer: String, initial: ?) -> RhaiResult;

/// Reduce an array by iterating through all elements while applying the `reducer` function.
///
/// # Function Parameters
///
/// * `result`: accumulated result, starting with the value of `initial`
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.reduce(|r, v| v + r, 5);
///
/// print(y);       // prints 20
///
/// let y = x.reduce(|r, v, i| v + i + r, 5);
///
/// print(y);       // prints 30
/// ```
fn reduce(array: Array, reducer: FnPtr, initial: ?) -> RhaiResult;

/// Reduce an array by iterating through all elements, in _reverse_ order,
/// while applying a function named by `reducer`.
///
/// # Function Parameters
///
/// A function with the same name as the value of `reducer` must exist taking these parameters:
///
/// * `result`: accumulated result, initially `()`
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// fn process(r, x) {
///     x + (r ?? 0)
/// }
/// fn process_extra(r, x, i) {
///     x + i + (r ?? 0)
/// }
///
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.reduce_rev("process");
///
/// print(y);       // prints 15
///
/// let y = x.reduce_rev("process_extra");
///
/// print(y);       // prints 25
/// ```
fn reduce_rev(array: Array, reducer: String) -> RhaiResult;

/// Reduce an array by iterating through all elements, in _reverse_ order,
/// while applying the `reducer` function.
///
/// # Function Parameters
///
/// * `result`: accumulated result, initially `()`
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.reduce_rev(|r, v| v + (r ?? 0));
///
/// print(y);       // prints 15
///
/// let y = x.reduce_rev(|r, v, i| v + i + (r ?? 0));
///
/// print(y);       // prints 25
/// ```
fn reduce_rev(array: Array, reducer: FnPtr) -> RhaiResult;

/// Reduce an array by iterating through all elements, in _reverse_ order,
/// while applying a function named by `reducer`.
///
/// # Function Parameters
///
/// A function with the same name as the value of `reducer` must exist taking these parameters:
///
/// * `result`: accumulated result, starting with the value of `initial`
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// fn process(r, x) { x + r }
///
/// fn process_extra(r, x, i) { x + i + r }
///
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.reduce_rev("process", 5);
///
/// print(y);       // prints 20
///
/// let y = x.reduce_rev("process_extra", 5);
///
/// print(y);       // prints 30
/// ```
fn reduce_rev(array: Array, reducer: String, initial: ?) -> RhaiResult;

/// Reduce an array by iterating through all elements, in _reverse_ order,
/// while applying the `reducer` function.
///
/// # Function Parameters
///
/// * `result`: accumulated result, starting with the value of `initial`
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.reduce_rev(|r, v| v + r, 5);
///
/// print(y);       // prints 20
///
/// let y = x.reduce_rev(|r, v, i| v + i + r, 5);
///
/// print(y);       // prints 30
/// ```
fn reduce_rev(array: Array, reducer: FnPtr, initial: ?) -> RhaiResult;

/// Remove the element at the specified `index` from the array and return it.
///
/// * If `index` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `index` < -length of array, `()` is returned.
/// * If `index` ≥ length of array, `()` is returned.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3];
///
/// print(x.remove(1));     // prints 2
///
/// print(x);               // prints "[1, 3]"
///
/// print(x.remove(-2));    // prints 1
///
/// print(x);               // prints "[3]"
/// ```
fn remove(array: Array, index: int) -> ?;

/// Remove the byte at the specified `index` from the BLOB and return it.
///
/// * If `index` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `index` < -length of BLOB, zero is returned.
/// * If `index` ≥ length of BLOB, zero is returned.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// print(x.remove(1));     // prints 2
///
/// print(x);               // prints "[01030405]"
///
/// print(x.remove(-2));    // prints 4
///
/// print(x);               // prints "[010305]"
/// ```
fn remove(blob: Blob, index: int) -> int;

/// Remove any property of the specified `name` from the object map, returning its value.
///
/// If the property does not exist, `()` is returned.
///
/// # Example
///
/// ```rhai
/// let m = #{a:1, b:2, c:3};
///
/// let x = m.remove("b");
///
/// print(x);       // prints 2
///
/// print(m);       // prints "#{a:1, c:3}"
/// ```
fn remove(map: Map, property: String) -> ?;

/// Remove all occurrences of a character from the string.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foobar!";
///
/// text.remove("o");
///
/// print(text);        // prints "hell, wrld! hell, fbar!"
/// ```
fn remove(string: String, character: char) -> ();

/// Remove all occurrences of a sub-string from the string.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foobar!";
///
/// text.remove("hello");
///
/// print(text);        // prints ", world! , foobar!"
/// ```
fn remove(string: String, sub_string: String) -> ();

/// Replace all occurrences of the specified character in the string with another character.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foobar!";
///
/// text.replace("l", '*');
///
/// print(text);        // prints "he**o, wor*d! he**o, foobar!"
/// ```
fn replace(string: String, find_character: char, substitute_character: char) -> ();

/// Replace all occurrences of the specified character in the string with another string.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foobar!";
///
/// text.replace('l', "(^)");
///
/// print(text);        // prints "he(^)(^)o, wor(^)d! he(^)(^)o, foobar!"
/// ```
fn replace(string: String, find_character: char, substitute_string: String) -> ();

/// Replace all occurrences of the specified sub-string in the string with the specified character.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foobar!";
///
/// text.replace("hello", '*');
///
/// print(text);        // prints "*, world! *, foobar!"
/// ```
fn replace(string: String, find_string: String, substitute_character: char) -> ();

/// Replace all occurrences of the specified sub-string in the string with another string.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foobar!";
///
/// text.replace("hello", "hey");
///
/// print(text);        // prints "hey, world! hey, foobar!"
/// ```
fn replace(string: String, find_string: String, substitute_string: String) -> ();

/// Remove all elements in the array that do not return `true` when applied a function named by
/// `filter` and return them as a new array.
///
/// # Function Parameters
///
/// A function with the same name as the value of `filter` must exist taking these parameters:
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// fn large(x) { x >= 3 }
///
/// fn screen(x, i) { x + i <= 5 }
///
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.retain("large");
///
/// print(x);       // prints "[3, 4, 5]"
///
/// print(y);       // prints "[1, 2]"
///
/// let z = x.retain("screen");
///
/// print(x);       // prints "[3, 4]"
///
/// print(z);       // prints "[5]"
/// ```
fn retain(array: Array, filter: String) -> Array;

/// Remove all elements in the array that do not return `true` when applied the `filter`
/// function and return them as a new array.
///
/// # Function Parameters
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.retain(|v| v >= 3);
///
/// print(x);       // prints "[3, 4, 5]"
///
/// print(y);       // prints "[1, 2]"
///
/// let z = x.retain(|v, i| v + i <= 5);
///
/// print(x);       // prints "[3, 4]"
///
/// print(z);       // prints "[5]"
/// ```
fn retain(array: Array, filter: FnPtr) -> Array;

/// Remove all elements in the array not within an exclusive `range` and return them as a new array.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.retain(1..4);
///
/// print(x);       // prints "[2, 3, 4]"
///
/// print(y);       // prints "[1, 5]"
///
/// let z = x.retain(1..3);
///
/// print(x);       // prints "[3, 4]"
///
/// print(z);       // prints "[1]"
/// ```
fn retain(array: Array, range: Range<int>) -> Array;

/// Remove all elements in the array not within an inclusive `range` and return them as a new array.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.retain(1..=3);
///
/// print(x);       // prints "[2, 3, 4]"
///
/// print(y);       // prints "[1, 5]"
///
/// let z = x.retain(1..=2);
///
/// print(x);       // prints "[3, 4]"
///
/// print(z);       // prints "[1]"
/// ```
fn retain(array: Array, range: RangeInclusive<int>) -> Array;

/// Remove all bytes in the BLOB not within an exclusive `range` and return them as a new BLOB.
///
/// # Example
///
/// ```rhai
/// let b1 = blob();
///
/// b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;
///
/// let b2 = b1.retain(1..4);
///
/// print(b1);      // prints "[020304]"
///
/// print(b2);      // prints "[0105]"
///
/// let b3 = b1.retain(1..3);
///
/// print(b1);      // prints "[0304]"
///
/// print(b2);      // prints "[01]"
/// ```
fn retain(blob: Blob, range: Range<int>) -> Blob;

/// Remove all bytes in the BLOB not within an inclusive `range` and return them as a new BLOB.
///
/// # Example
///
/// ```rhai
/// let b1 = blob();
///
/// b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;
///
/// let b2 = b1.retain(1..=3);
///
/// print(b1);      // prints "[020304]"
///
/// print(b2);      // prints "[0105]"
///
/// let b3 = b1.retain(1..=2);
///
/// print(b1);      // prints "[0304]"
///
/// print(b2);      // prints "[01]"
/// ```
fn retain(blob: Blob, range: RangeInclusive<int>) -> Blob;

/// Remove all elements not within a portion of the array and return them as a new array.
///
/// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `start` < -length of array, position counts from the beginning of the array.
/// * If `start` ≥ length of array, all elements are removed returned.
/// * If `len` ≤ 0, all elements are removed and returned.
/// * If `start` position + `len` ≥ length of array, entire portion of the array before the `start` position is removed and returned.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.retain(1, 2);
///
/// print(x);       // prints "[2, 3]"
///
/// print(y);       // prints "[1, 4, 5]"
///
/// let z = x.retain(-1, 1);
///
/// print(x);       // prints "[3]"
///
/// print(z);       // prints "[2]"
/// ```
fn retain(array: Array, start: int, len: int) -> Array;

/// Remove all bytes not within a portion of the BLOB and return them as a new BLOB.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, all elements are removed returned.
/// * If `len` ≤ 0, all elements are removed and returned.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB before the `start` position is removed and returned.
///
/// # Example
///
/// ```rhai
/// let b1 = blob();
///
/// b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;
///
/// let b2 = b1.retain(1, 2);
///
/// print(b1);      // prints "[0203]"
///
/// print(b2);      // prints "[010405]"
///
/// let b3 = b1.retain(-1, 1);
///
/// print(b1);      // prints "[03]"
///
/// print(b3);      // prints "[02]"
/// ```
fn retain(blob: Blob, start: int, len: int) -> Blob;

/// Reverse all the elements in the array.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// x.reverse();
///
/// print(x);       // prints "[5, 4, 3, 2, 1]"
/// ```
fn reverse(array: Array) -> ();

/// Reverse the BLOB.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// print(b);           // prints "[0102030405]"
///
/// b.reverse();
///
/// print(b);           // prints "[0504030201]"
/// ```
fn reverse(blob: Blob) -> ();

/// Return the nearest whole number closest to the decimal number.
/// Always round mid-point towards the closest even number.
fn round(x: Decimal) -> Decimal;

/// Return the nearest whole number closest to the floating-point number.
/// Rounds away from zero.
fn round(x: float) -> float;

/// Round the decimal number to the specified number of `digits` after the decimal point and return it.
/// Always round mid-point towards the closest even number.
fn round(x: Decimal, digits: int) -> Decimal;

/// Round the decimal number to the specified number of `digits` after the decimal point and return it.
/// Always round towards zero.
fn round_down(x: Decimal, digits: int) -> Decimal;

/// Round the decimal number to the specified number of `digits` after the decimal point and return it.
/// Always round mid-points towards zero.
fn round_half_down(x: Decimal, digits: int) -> Decimal;

/// Round the decimal number to the specified number of `digits` after the decimal point and return it.
/// Always round mid-points away from zero.
fn round_half_up(x: Decimal, digits: int) -> Decimal;

/// Round the decimal number to the specified number of `digits` after the decimal point and return it.
/// Always round away from zero.
fn round_up(x: Decimal, digits: int) -> Decimal;

/// Set the element at the `index` position in the array to a new `value`.
///
/// * If `index` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `index` < -length of array, the array is not modified.
/// * If `index` ≥ length of array, the array is not modified.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3];
///
/// x.set(0, 42);
///
/// print(x);           // prints "[42, 2, 3]"
///
/// x.set(-3, 0);
///
/// print(x);           // prints "[0, 2, 3]"
///
/// x.set(99, 123);
///
/// print(x);           // prints "[0, 2, 3]"
/// ```
fn set(array: Array, index: int, value: ?) -> ();

/// Set the particular `index` position in the BLOB to a new byte `value`.
///
/// * If `index` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `index` < -length of BLOB, the BLOB is not modified.
/// * If `index` ≥ length of BLOB, the BLOB is not modified.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// b.set(0, 0x42);
///
/// print(b);           // prints "[4202030405]"
///
/// b.set(-3, 0);
///
/// print(b);           // prints "[4202000405]"
///
/// b.set(99, 123);
///
/// print(b);           // prints "[4202000405]"
/// ```
fn set(blob: Blob, index: int, value: int) -> ();

/// Set the value of the `property` in the object map to a new `value`.
///
/// If `property` does not exist in the object map, it is added.
///
/// # Example
///
/// ```rhai
/// let m = #{a: 1, b: 2, c: 3};
///
/// m.set("b", 42)'
///
/// print(m);           // prints "#{a: 1, b: 42, c: 3}"
///
/// x.set("x", 0);
///
/// print(m);           // prints "#{a: 1, b: 42, c: 3, x: 0}"
/// ```
fn set(map: Map, property: String, value: ?) -> ();

/// Set the `index` position in the string to a new `character`.
///
/// * If `index` < 0, position counts from the end of the string (`-1` is the last character).
/// * If `index` < -length of string, the string is not modified.
/// * If `index` ≥ length of string, the string is not modified.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// text.set(3, 'x');
///
/// print(text);     // prints "helxo, world!"
///
/// text.set(-3, 'x');
///
/// print(text);    // prints "hello, worxd!"
///
/// text.set(99, 'x');
///
/// print(text);    // prints "hello, worxd!"
/// ```
fn set(string: String, index: int, character: char) -> ();

/// Set the _tag_ of a `Dynamic` value.
///
/// # Example
///
/// ```rhai
/// let x = "hello, world!";
///
/// x.tag = 42;
///
/// print(x.tag);           // prints 42
/// ```
fn set tag(value: ?, tag: int) -> ();

/// Set the specified `bit` in the number if the new value is `true`.
/// Clear the `bit` if the new value is `false`.
///
/// If `bit` < 0, position counts from the MSB (Most Significant Bit).
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// x.set_bit(5, true);
///
/// print(x);               // prints 123488
///
/// x.set_bit(6, false);
///
/// print(x);               // prints 123424
///
/// x.set_bit(-48, false);
///
/// print(x);               // prints 57888 on 64-bit
/// ```
fn set_bit(value: int, bit: int, new_value: bool) -> ();

/// Replace an exclusive range of bits in the number with a new value.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// x.set_bits(5..10, 42);
///
/// print(x);           // print 123200
/// ```
fn set_bits(value: int, range: Range<int>, new_value: int) -> ();

/// Replace an inclusive range of bits in the number with a new value.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// x.set_bits(5..=9, 42);
///
/// print(x);           // print 123200
/// ```
fn set_bits(value: int, range: RangeInclusive<int>, new_value: int) -> ();

/// Replace a portion of bits in the number with a new value.
///
/// * If `start` < 0, position counts from the MSB (Most Significant Bit).
/// * If `bits` ≤ 0, the number is not modified.
/// * If `start` position + `bits` ≥ total number of bits, the bits after the `start` position are replaced.
///
/// # Example
///
/// ```rhai
/// let x = 123456;
///
/// x.set_bits(5, 8, 42);
///
/// print(x);           // prints 124224
///
/// x.set_bits(-16, 10, 42);
///
/// print(x);           // prints 11821949021971776 on 64-bit
/// ```
fn set_bits(value: int, bit: int, bits: int, new_value: int) -> ();

/// Set the _tag_ of a `Dynamic` value.
///
/// # Example
///
/// ```rhai
/// let x = "hello, world!";
///
/// x.tag = 42;
///
/// print(x.tag);           // prints 42
/// ```
fn set_tag(value: ?, tag: int) -> ();

/// Remove the first element from the array and return it.
///
/// If the array is empty, `()` is returned.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3];
///
/// print(x.shift());   // prints 1
///
/// print(x);           // prints "[2, 3]"
/// ```
fn shift(array: Array) -> ?;

/// Remove the first byte from the BLOB and return it.
///
/// If the BLOB is empty, zero is returned.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// print(b.shift());       // prints 1
///
/// print(b);               // prints "[02030405]"
/// ```
fn shift(blob: Blob) -> int;

/// Return the sign (as an integer) of the decimal number according to the following:
///
/// * `0` if the number is zero
/// * `1` if the number is positive
/// * `-1` if the number is negative
fn sign(x: Decimal) -> int;

/// Return the sign (as an integer) of the number according to the following:
///
/// * `0` if the number is zero
/// * `1` if the number is positive
/// * `-1` if the number is negative
fn sign(x: int) -> int;

/// Return the sign (as an integer) of the floating-point number according to the following:
///
/// * `0` if the number is zero
/// * `1` if the number is positive
/// * `-1` if the number is negative
fn sign(x: f32) -> int;

/// Return the sign (as an integer) of the floating-point number according to the following:
///
/// * `0` if the number is zero
/// * `1` if the number is positive
/// * `-1` if the number is negative
fn sign(x: float) -> int;

/// Return the sign (as an integer) of the number according to the following:
///
/// * `0` if the number is zero
/// * `1` if the number is positive
/// * `-1` if the number is negative
fn sign(x: i128) -> int;

/// Return the sign (as an integer) of the number according to the following:
///
/// * `0` if the number is zero
/// * `1` if the number is positive
/// * `-1` if the number is negative
fn sign(x: i16) -> int;

/// Return the sign (as an integer) of the number according to the following:
///
/// * `0` if the number is zero
/// * `1` if the number is positive
/// * `-1` if the number is negative
fn sign(x: i32) -> int;

/// Return the sign (as an integer) of the number according to the following:
///
/// * `0` if the number is zero
/// * `1` if the number is positive
/// * `-1` if the number is negative
fn sign(x: i8) -> int;

/// Return the sine of the decimal number in radians.
fn sin(x: Decimal) -> Decimal;

/// Return the sine of the floating-point number in radians.
fn sin(x: float) -> float;

/// Return the hyperbolic sine of the floating-point number in radians.
fn sinh(x: float) -> float;

/// Block the current thread for a particular number of `seconds`.
fn sleep(seconds: int) -> ();

/// Block the current thread for a particular number of `seconds`.
fn sleep(seconds: float) -> ();

/// Return `true` if any element in the array that returns `true` when applied a function named
/// by `filter`.
///
/// # Function Parameters
///
/// A function with the same name as the value of `filter` must exist taking these parameters:
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// fn large(x) { x > 3 }
///
/// fn huge(x) { x > 10 }
///
/// fn screen(x, i) { i > x }
///
/// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
///
/// print(x.some("large"));     // prints true
///
/// print(x.some("huge"));      // prints false
///
/// print(x.some("screen"));    // prints true
/// ```
fn some(array: Array, filter: String) -> bool;

/// Return `true` if any element in the array that returns `true` when applied the `filter` function.
///
/// # Function Parameters
///
/// * `element`: copy of array element
/// * `index` _(optional)_: current index in the array
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
///
/// print(x.some(|v| v > 3));       // prints true
///
/// print(x.some(|v| v > 10));      // prints false
///
/// print(x.some(|v, i| i > v));    // prints true
/// ```
fn some(array: Array, filter: FnPtr) -> bool;

/// Sort the array.
///
/// All elements in the array must be of the same data type.
///
/// # Supported Data Types
///
/// * integer numbers
/// * floating-point numbers
/// * decimal numbers
/// * characters
/// * strings
/// * booleans
/// * `()`
///
/// # Example
///
/// ```rhai
/// let x = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];
///
/// x.sort();
///
/// print(x);       // prints "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]"
/// ```
fn sort(array: Array) -> ();

/// Sort the array based on applying a function named by `comparer`.
///
/// # Function Parameters
///
/// A function with the same name as the value of `comparer` must exist taking these parameters:
///
/// * `element1`: copy of the current array element to compare
/// * `element2`: copy of the next array element to compare
///
/// ## Return Value
///
/// * Any integer > 0 if `element1 > element2`
/// * Zero if `element1 == element2`
/// * Any integer < 0 if `element1 < element2`
///
/// # Example
///
/// ```rhai
/// fn reverse(a, b) {
///     if a > b {
///         -1
///     } else if a < b {
///         1
///     } else {
///         0
///     }
/// }
/// let x = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];
///
/// x.sort("reverse");
///
/// print(x);       // prints "[10, 9, 8, 7, 6, 5, 4, 3, 2, 1]"
/// ```
fn sort(array: Array, comparer: String) -> ();

/// Sort the array based on applying the `comparer` function.
///
/// # Function Parameters
///
/// * `element1`: copy of the current array element to compare
/// * `element2`: copy of the next array element to compare
///
/// ## Return Value
///
/// * Any integer > 0 if `element1 > element2`
/// * Zero if `element1 == element2`
/// * Any integer < 0 if `element1 < element2`
///
/// # Example
///
/// ```rhai
/// let x = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];
///
/// // Do comparisons in reverse
/// x.sort(|a, b| if a > b { -1 } else if a < b { 1 } else { 0 });
///
/// print(x);       // prints "[10, 9, 8, 7, 6, 5, 4, 3, 2, 1]"
/// ```
fn sort(array: Array, comparer: FnPtr) -> ();

/// Replace an exclusive range of the array with another array.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
/// let y = [7, 8, 9, 10];
///
/// x.splice(1..3, y);
///
/// print(x);       // prints "[1, 7, 8, 9, 10, 4, 5]"
/// ```
fn splice(array: Array, range: Range<int>, replace: Array) -> ();

/// Replace an inclusive range of the array with another array.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
/// let y = [7, 8, 9, 10];
///
/// x.splice(1..=3, y);
///
/// print(x);       // prints "[1, 7, 8, 9, 10, 5]"
/// ```
fn splice(array: Array, range: RangeInclusive<int>, replace: Array) -> ();

/// Replace an exclusive `range` of the BLOB with another BLOB.
///
/// # Example
///
/// ```rhai
/// let b1 = blob(10, 0x42);
/// let b2 = blob(5, 0x18);
///
/// b1.splice(1..4, b2);
///
/// print(b1);      // prints "[4218181818184242 42424242]"
/// ```
fn splice(blob: Blob, range: Range<int>, replace: Blob) -> ();

/// Replace an inclusive `range` of the BLOB with another BLOB.
///
/// # Example
///
/// ```rhai
/// let b1 = blob(10, 0x42);
/// let b2 = blob(5, 0x18);
///
/// b1.splice(1..=4, b2);
///
/// print(b1);      // prints "[4218181818184242 424242]"
/// ```
fn splice(blob: Blob, range: RangeInclusive<int>, replace: Blob) -> ();

/// Replace a portion of the array with another array.
///
/// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `start` < -length of array, position counts from the beginning of the array.
/// * If `start` ≥ length of array, the other array is appended to the end of the array.
/// * If `len` ≤ 0, the other array is inserted into the array at the `start` position without replacing any element.
/// * If `start` position + `len` ≥ length of array, entire portion of the array after the `start` position is replaced.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
/// let y = [7, 8, 9, 10];
///
/// x.splice(1, 2, y);
///
/// print(x);       // prints "[1, 7, 8, 9, 10, 4, 5]"
///
/// x.splice(-5, 4, y);
///
/// print(x);       // prints "[1, 7, 7, 8, 9, 10, 5]"
/// ```
fn splice(array: Array, start: int, len: int, replace: Array) -> ();

/// Replace a portion of the BLOB with another BLOB.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, the other BLOB is appended to the end of the BLOB.
/// * If `len` ≤ 0, the other BLOB is inserted into the BLOB at the `start` position without replacing anything.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB after the `start` position is replaced.
///
/// # Example
///
/// ```rhai
/// let b1 = blob(10, 0x42);
/// let b2 = blob(5, 0x18);
///
/// b1.splice(1, 3, b2);
///
/// print(b1);      // prints "[4218181818184242 42424242]"
///
/// b1.splice(-5, 4, b2);
///
/// print(b1);      // prints "[4218181818184218 1818181842]"
/// ```
fn splice(blob: Blob, start: int, len: int, replace: Blob) -> ();

/// Split the string into segments based on whitespaces, returning an array of the segments.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foo!";
///
/// print(text.split());        // prints ["hello,", "world!", "hello,", "foo!"]
/// ```
fn split(string: String) -> Array;

/// Cut off the array at `index` and return it as a new array.
///
/// * If `index` < 0, position counts from the end of the array (`-1` is the last element).
/// * If `index` is zero, the entire array is cut and returned.
/// * If `index` < -length of array, the entire array is cut and returned.
/// * If `index` ≥ length of array, nothing is cut from the array and an empty array is returned.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// let y = x.split(2);
///
/// print(y);           // prints "[3, 4, 5]"
///
/// print(x);           // prints "[1, 2]"
/// ```
fn split(array: Array, index: int) -> Array;

/// Cut off the BLOB at `index` and return it as a new BLOB.
///
/// * If `index` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `index` is zero, the entire BLOB is cut and returned.
/// * If `index` < -length of BLOB, the entire BLOB is cut and returned.
/// * If `index` ≥ length of BLOB, nothing is cut from the BLOB and an empty BLOB is returned.
///
/// # Example
///
/// ```rhai
/// let b1 = blob();
///
/// b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;
///
/// let b2 = b1.split(2);
///
/// print(b2);          // prints "[030405]"
///
/// print(b1);          // prints "[0102]"
/// ```
fn split(blob: Blob, index: int) -> Blob;

/// Split the string into two at the specified `index` position and return it both strings
/// as an array.
///
/// The character at the `index` position (if any) is returned in the _second_ string.
///
/// * If `index` < 0, position counts from the end of the string (`-1` is the last character).
/// * If `index` < -length of string, it is equivalent to cutting at position 0.
/// * If `index` ≥ length of string, it is equivalent to cutting at the end of the string.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.split(6));       // prints ["hello,", " world!"]
///
/// print(text.split(13));      // prints ["hello, world!", ""]
///
/// print(text.split(-6));      // prints ["hello, ", "world!"]
///
/// print(text.split(-99));     // prints ["", "hello, world!"]
/// ```
fn split(string: String, index: int) -> Array;

/// Split the string into segments based on a `delimiter` string, returning an array of the segments.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foo!";
///
/// print(text.split("ll"));    // prints ["he", "o, world! he", "o, foo!"]
/// ```
fn split(string: String, delimiter: String) -> Array;

/// Split the string into segments based on a `delimiter` character, returning an array of the segments.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foo!";
///
/// print(text.split('l'));     // prints ["he", "", "o, wor", "d! he", "", "o, foo!"]
/// ```
fn split(string: String, delimiter: char) -> Array;

/// Split the string into at most the specified number of `segments` based on a `delimiter` string,
/// returning an array of the segments.
///
/// If `segments` < 1, only one segment is returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foo!";
///
/// print(text.split("ll", 2));     // prints ["he", "o, world! hello, foo!"]
/// ```
fn split(string: String, delimiter: String, segments: int) -> Array;

/// Split the string into at most the specified number of `segments` based on a `delimiter` character,
/// returning an array of the segments.
///
/// If `segments` < 1, only one segment is returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foo!";
///
/// print(text.split('l', 3));      // prints ["he", "", "o, world! hello, foo!"]
/// ```
fn split(string: String, delimiter: char, segments: int) -> Array;

/// Split the string into segments based on a `delimiter` string, returning an array of the
/// segments in _reverse_ order.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foo!";
///
/// print(text.split_rev("ll"));    // prints ["o, foo!", "o, world! he", "he"]
/// ```
fn split_rev(string: String, delimiter: String) -> Array;

/// Split the string into segments based on a `delimiter` character, returning an array of
/// the segments in _reverse_ order.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foo!";
///
/// print(text.split_rev('l'));     // prints ["o, foo!", "", "d! he", "o, wor", "", "he"]
/// ```
fn split_rev(string: String, delimiter: char) -> Array;

/// Split the string into at most a specified number of `segments` based on a `delimiter` string,
/// returning an array of the segments in _reverse_ order.
///
/// If `segments` < 1, only one segment is returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foo!";
///
/// print(text.split_rev("ll", 2));     // prints ["o, foo!", "hello, world! he"]
/// ```
fn split_rev(string: String, delimiter: String, segments: int) -> Array;

/// Split the string into at most the specified number of `segments` based on a `delimiter` character,
/// returning an array of the segments.
///
/// If `segments` < 1, only one segment is returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foo!";
///
/// print(text.split('l', 3));      // prints ["o, foo!", "", "hello, world! he"
/// ```
fn split_rev(string: String, delimiter: char, segments: int) -> Array;

/// Return the square root of the decimal number.
fn sqrt(x: Decimal) -> Decimal;

/// Return the square root of the floating-point number.
fn sqrt(x: float) -> float;

/// Return the start of the exclusive range.
fn start(range: ExclusiveRange) -> int;

/// Return the start of the inclusive range.
fn start(range: InclusiveRange) -> int;

/// Return `true` if the string starts with a specified string.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.starts_with("hello"));   // prints true
///
/// print(text.starts_with("world"));   // prints false
/// ```
fn starts_with(string: String, match_string: String) -> bool;

/// Copy an exclusive range of characters from the string and return it as a new string.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.sub_string(3..7));   // prints "lo, "
/// ```
fn sub_string(string: String, range: Range<int>) -> String;

/// Copy an inclusive range of characters from the string and return it as a new string.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.sub_string(3..=7));  // prints "lo, w"
/// ```
fn sub_string(string: String, range: RangeInclusive<int>) -> String;

/// Copy a portion of the string beginning at the `start` position till the end and return it as
/// a new string.
///
/// * If `start` < 0, position counts from the end of the string (`-1` is the last character).
/// * If `start` < -length of string, the entire string is copied and returned.
/// * If `start` ≥ length of string, an empty string is returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.sub_string(5));      // prints ", world!"
///
/// print(text.sub_string(-5));      // prints "orld!"
/// ```
fn sub_string(string: String, start: int) -> String;

/// Copy a portion of the string and return it as a new string.
///
/// * If `start` < 0, position counts from the end of the string (`-1` is the last character).
/// * If `start` < -length of string, position counts from the beginning of the string.
/// * If `start` ≥ length of string, an empty string is returned.
/// * If `len` ≤ 0, an empty string is returned.
/// * If `start` position + `len` ≥ length of string, entire portion of the string after the `start` position is copied and returned.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!";
///
/// print(text.sub_string(3, 4));   // prints "lo, "
///
/// print(text.sub_string(-8, 3));  // prints ", w"
/// ```
fn sub_string(string: String, start: int, len: int) -> String;

/// Return the _tag_ of a `Dynamic` value.
///
/// # Example
///
/// ```rhai
/// let x = "hello, world!";
///
/// x.tag = 42;
///
/// print(x.tag);           // prints 42
/// ```
fn tag(value: ?) -> int;

/// Return the tangent of the decimal number in radians.
fn tan(x: Decimal) -> Decimal;

/// Return the tangent of the floating-point number in radians.
fn tan(x: float) -> float;

/// Return the hyperbolic tangent of the floating-point number in radians.
fn tanh(x: float) -> float;

/// Create a timestamp containing the current system time.
///
/// # Example
///
/// ```rhai
/// let now = timestamp();
///
/// sleep(10.0);            // sleep for 10 seconds
///
/// print(now.elapsed);     // prints 10.???
/// ```
fn timestamp() -> Instant;

/// Convert the BLOB into an array of integers.
///
/// # Example
///
/// ```rhai
/// let b = blob(5, 0x42);
///
/// let x = b.to_array();
///
/// print(x);       // prints "[66, 66, 66, 66, 66]"
/// ```
fn to_array(blob: Blob) -> Array;

/// Convert the `value` into a string in binary format.
fn to_binary(value: i128) -> String;

/// Convert the `value` into a string in binary format.
fn to_binary(value: i16) -> String;

/// Convert the `value` into a string in binary format.
fn to_binary(value: i32) -> String;

/// Convert the `value` into a string in binary format.
fn to_binary(value: int) -> String;

/// Convert the `value` into a string in binary format.
fn to_binary(value: i8) -> String;

/// Convert the `value` into a string in binary format.
fn to_binary(value: u128) -> String;

/// Convert the `value` into a string in binary format.
fn to_binary(value: u16) -> String;

/// Convert the `value` into a string in binary format.
fn to_binary(value: u32) -> String;

/// Convert the `value` into a string in binary format.
fn to_binary(value: u64) -> String;

/// Convert the `value` into a string in binary format.
fn to_binary(value: u8) -> String;

/// Convert the string into an UTF-8 encoded byte-stream as a BLOB.
///
/// # Example
///
/// ```rhai
/// let text = "朝には紅顔ありて夕べには白骨となる";
///
/// let bytes = text.to_blob();
///
/// print(bytes.len());     // prints 51
/// ```
fn to_blob(string: String) -> Blob;

/// Return an array containing all the characters of the string.
///
/// # Example
///
/// ```rhai
/// let text = "hello";
///
/// print(text.to_chars());     // prints "['h', 'e', 'l', 'l', 'o']"
/// ```
fn to_chars(string: String) -> Array;

/// Convert the array into a string.
fn to_debug(array: Array) -> String;

/// Convert the string into debug format.
fn to_debug(character: char) -> String;

/// Convert the function pointer into a string in debug format.
fn to_debug(f: FnPtr) -> String;

/// Convert the value of the `item` into a string in debug format.
fn to_debug(item: ?) -> String;

/// Convert the object map into a string.
fn to_debug(map: Map) -> String;

/// Convert the value of `number` into a string.
fn to_debug(number: f32) -> String;

/// Convert the value of `number` into a string.
fn to_debug(number: float) -> String;

/// Convert the string into debug format.
fn to_debug(string: String) -> String;

/// Convert the unit into a string in debug format.
fn to_debug(unit: ()) -> String;

/// Convert the boolean value into a string in debug format.
fn to_debug(value: bool) -> String;

/// Convert the floating-point number to decimal.
fn to_decimal(x: f32) -> Decimal;

/// Convert the floating-point number to decimal.
fn to_decimal(x: float) -> Decimal;

fn to_decimal(x: i16) -> Decimal;

fn to_decimal(x: i32) -> Decimal;

fn to_decimal(x: int) -> Decimal;

fn to_decimal(x: i8) -> Decimal;

fn to_decimal(x: u16) -> Decimal;

fn to_decimal(x: u32) -> Decimal;

fn to_decimal(x: u64) -> Decimal;

fn to_decimal(x: u8) -> Decimal;

/// Convert radians to degrees.
fn to_degrees(x: float) -> float;

/// Convert the decimal number to floating-point.
fn to_float(x: Decimal) -> float;

/// Convert the 32-bit floating-point number to 64-bit.
fn to_float(x: f32) -> float;

fn to_float(x: i128) -> float;

fn to_float(x: i16) -> float;

fn to_float(x: i32) -> float;

fn to_float(x: int) -> float;

fn to_float(x: i8) -> float;

fn to_float(x: u128) -> float;

fn to_float(x: u16) -> float;

fn to_float(x: u32) -> float;

fn to_float(x: u8) -> float;

/// Convert the `value` into a string in hex format.
fn to_hex(value: i128) -> String;

/// Convert the `value` into a string in hex format.
fn to_hex(value: i16) -> String;

/// Convert the `value` into a string in hex format.
fn to_hex(value: i32) -> String;

/// Convert the `value` into a string in hex format.
fn to_hex(value: int) -> String;

/// Convert the `value` into a string in hex format.
fn to_hex(value: i8) -> String;

/// Convert the `value` into a string in hex format.
fn to_hex(value: u128) -> String;

/// Convert the `value` into a string in hex format.
fn to_hex(value: u16) -> String;

/// Convert the `value` into a string in hex format.
fn to_hex(value: u32) -> String;

/// Convert the `value` into a string in hex format.
fn to_hex(value: u64) -> String;

/// Convert the `value` into a string in hex format.
fn to_hex(value: u8) -> String;

/// Convert the decimal number into an integer.
fn to_int(x: Decimal) -> int;

fn to_int(x: char) -> int;

/// Convert the floating-point number into an integer.
fn to_int(x: f32) -> int;

/// Convert the floating-point number into an integer.
fn to_int(x: float) -> int;

fn to_int(x: i128) -> int;

fn to_int(x: i16) -> int;

fn to_int(x: i32) -> int;

fn to_int(x: int) -> int;

fn to_int(x: i8) -> int;

fn to_int(x: u128) -> int;

fn to_int(x: u16) -> int;

fn to_int(x: u32) -> int;

fn to_int(x: u64) -> int;

fn to_int(x: u8) -> int;

/// Return the JSON representation of the object map.
///
/// # Data types
///
/// Only the following data types should be kept inside the object map:
/// `INT`, `FLOAT`, `ImmutableString`, `char`, `bool`, `()`, `Array`, `Map`.
///
/// # Errors
///
/// Data types not supported by JSON serialize into formats that may
/// invalidate the result.
///
/// # Example
///
/// ```rhai
/// let m = #{a:1, b:2, c:3};
///
/// print(m.to_json());     // prints {"a":1, "b":2, "c":3}
/// ```
fn to_json(map: Map) -> String;

/// Convert the character to lower-case and return it as a new character.
///
/// # Example
///
/// ```rhai
/// let ch = 'A';
///
/// print(ch.to_lower());       // prints 'a'
///
/// print(ch);                  // prints 'A'
/// ```
fn to_lower(character: char) -> char;

/// Convert the string to all lower-case and return it as a new string.
///
/// # Example
///
/// ```rhai
/// let text = "HELLO, WORLD!"
///
/// print(text.to_lower());     // prints "hello, world!"
///
/// print(text);                // prints "HELLO, WORLD!"
/// ```
fn to_lower(string: String) -> String;

/// Convert the `value` into a string in octal format.
fn to_octal(value: i128) -> String;

/// Convert the `value` into a string in octal format.
fn to_octal(value: i16) -> String;

/// Convert the `value` into a string in octal format.
fn to_octal(value: i32) -> String;

/// Convert the `value` into a string in octal format.
fn to_octal(value: int) -> String;

/// Convert the `value` into a string in octal format.
fn to_octal(value: i8) -> String;

/// Convert the `value` into a string in octal format.
fn to_octal(value: u128) -> String;

/// Convert the `value` into a string in octal format.
fn to_octal(value: u16) -> String;

/// Convert the `value` into a string in octal format.
fn to_octal(value: u32) -> String;

/// Convert the `value` into a string in octal format.
fn to_octal(value: u64) -> String;

/// Convert the `value` into a string in octal format.
fn to_octal(value: u8) -> String;

/// Convert degrees to radians.
fn to_radians(x: float) -> float;

/// Convert the array into a string.
fn to_string(array: Array) -> String;

/// Return the character into a string.
fn to_string(character: char) -> String;

/// Convert the value of the `item` into a string.
fn to_string(item: ?) -> String;

/// Convert the object map into a string.
fn to_string(map: Map) -> String;

/// Convert the value of `number` into a string.
fn to_string(number: f32) -> String;

/// Convert the value of `number` into a string.
fn to_string(number: float) -> String;

/// Return the `string`.
fn to_string(string: String) -> String;

/// Return the empty string.
fn to_string(unit: ()) -> String;

/// Return the boolean value into a string.
fn to_string(value: bool) -> String;

/// Convert the character to upper-case and return it as a new character.
///
/// # Example
///
/// ```rhai
/// let ch = 'a';
///
/// print(ch.to_upper());       // prints 'A'
///
/// print(ch);                  // prints 'a'
/// ```
fn to_upper(character: char) -> char;

/// Convert the string to all upper-case and return it as a new string.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world!"
///
/// print(text.to_upper());     // prints "HELLO, WORLD!"
///
/// print(text);                // prints "hello, world!"
/// ```
fn to_upper(string: String) -> String;

/// Remove whitespace characters from both ends of the string.
///
/// # Example
///
/// ```rhai
/// let text = "   hello     ";
///
/// text.trim();
///
/// print(text);    // prints "hello"
/// ```
fn trim(string: String) -> ();

/// Cut off the array at the specified length.
///
/// * If `len` ≤ 0, the array is cleared.
/// * If `len` ≥ length of array, the array is not truncated.
///
/// # Example
///
/// ```rhai
/// let x = [1, 2, 3, 4, 5];
///
/// x.truncate(3);
///
/// print(x);       // prints "[1, 2, 3]"
///
/// x.truncate(10);
///
/// print(x);       // prints "[1, 2, 3]"
/// ```
fn truncate(array: Array, len: int) -> ();

/// Cut off the BLOB at the specified length.
///
/// * If `len` ≤ 0, the BLOB is cleared.
/// * If `len` ≥ length of BLOB, the BLOB is not truncated.
///
/// # Example
///
/// ```rhai
/// let b = blob();
///
/// b += 1; b += 2; b += 3; b += 4; b += 5;
///
/// b.truncate(3);
///
/// print(b);           // prints "[010203]"
///
/// b.truncate(10);
///
/// print(b);           // prints "[010203]"
/// ```
fn truncate(blob: Blob, len: int) -> ();

/// Cut off the string at the specified number of characters.
///
/// * If `len` ≤ 0, the string is cleared.
/// * If `len` ≥ length of string, the string is not truncated.
///
/// # Example
///
/// ```rhai
/// let text = "hello, world! hello, foobar!";
///
/// text.truncate(13);
///
/// print(text);    // prints "hello, world!"
///
/// x.truncate(10);
///
/// print(text);    // prints "hello, world!"
/// ```
fn truncate(string: String, len: int) -> ();

/// Return an array with all the property values in the object map.
///
/// # Example
///
/// ```rhai
/// let m = #{a:1, b:2, c:3};
///
/// print(m.values());      // prints "[1, 2, 3]""
/// ```
fn values(map: Map) -> Array;

/// Write an ASCII string to the bytes within an exclusive `range` in the BLOB.
///
/// Each ASCII character encodes to one single byte in the BLOB.
/// Non-ASCII characters are ignored.
///
/// * If number of bytes in `range` < length of `string`, extra bytes in `string` are not written.
/// * If number of bytes in `range` > length of `string`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8);
///
/// b.write_ascii(1..5, "hello, world!");
///
/// print(b);       // prints "[0068656c6c000000]"
/// ```
fn write_ascii(blob: Blob, range: Range<int>, string: String) -> ();

/// Write an ASCII string to the bytes within an inclusive `range` in the BLOB.
///
/// Each ASCII character encodes to one single byte in the BLOB.
/// Non-ASCII characters are ignored.
///
/// * If number of bytes in `range` < length of `string`, extra bytes in `string` are not written.
/// * If number of bytes in `range` > length of `string`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8);
///
/// b.write_ascii(1..=5, "hello, world!");
///
/// print(b);       // prints "[0068656c6c6f0000]"
/// ```
fn write_ascii(blob: Blob, range: RangeInclusive<int>, string: String) -> ();

/// Write an ASCII string to the bytes within an exclusive `range` in the BLOB.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, the BLOB is not modified.
/// * If `len` ≤ 0, the BLOB is not modified.
/// * If `start` position + `len` ≥ length of BLOB, only the portion of the BLOB after the `start` position is modified.
///
/// * If number of bytes in `range` < length of `string`, extra bytes in `string` are not written.
/// * If number of bytes in `range` > length of `string`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8);
///
/// b.write_ascii(1, 5, "hello, world!");
///
/// print(b);       // prints "[0068656c6c6f0000]"
/// ```
fn write_ascii(blob: Blob, start: int, len: int, string: String) -> ();

/// Write a `FLOAT` value to the bytes within an exclusive `range` in the BLOB
/// in big-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `FLOAT`, extra bytes in `FLOAT` are not written.
/// * If number of bytes in `range` > number of bytes for `FLOAT`, extra bytes in `range` are not modified.
fn write_be(blob: Blob, range: Range<int>, value: float) -> ();

/// Write an `INT` value to the bytes within an exclusive `range` in the BLOB
/// in big-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `INT`, extra bytes in `INT` are not written.
/// * If number of bytes in `range` > number of bytes for `INT`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8, 0x42);
///
/// b.write_be_int(1..3, 0x99);
///
/// print(b);       // prints "[4200004242424242]"
/// ```
fn write_be(blob: Blob, range: Range<int>, value: int) -> ();

/// Write a `FLOAT` value to the bytes within an inclusive `range` in the BLOB
/// in big-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `FLOAT`, extra bytes in `FLOAT` are not written.
/// * If number of bytes in `range` > number of bytes for `FLOAT`, extra bytes in `range` are not modified.
fn write_be(blob: Blob, range: RangeInclusive<int>, value: float) -> ();

/// Write an `INT` value to the bytes within an inclusive `range` in the BLOB
/// in big-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `INT`, extra bytes in `INT` are not written.
/// * If number of bytes in `range` > number of bytes for `INT`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8, 0x42);
///
/// b.write_be_int(1..=3, 0x99);
///
/// print(b);       // prints "[4200000042424242]"
/// ```
fn write_be(blob: Blob, range: RangeInclusive<int>, value: int) -> ();

/// Write a `FLOAT` value to the bytes beginning at the `start` position in the BLOB
/// in big-endian byte order.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, zero is returned.
/// * If `len` ≤ 0, zero is returned.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB after the `start` position is parsed.
///
/// * If number of bytes in `range` < number of bytes for `FLOAT`, extra bytes in `FLOAT` are not written.
/// * If number of bytes in `range` > number of bytes for `FLOAT`, extra bytes in `range` are not modified.
fn write_be(blob: Blob, start: int, len: int, value: float) -> ();

/// Write an `INT` value to the bytes beginning at the `start` position in the BLOB
/// in big-endian byte order.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, zero is returned.
/// * If `len` ≤ 0, zero is returned.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB after the `start` position is parsed.
///
/// * If number of bytes in `range` < number of bytes for `INT`, extra bytes in `INT` are not written.
/// * If number of bytes in `range` > number of bytes for `INT`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8, 0x42);
///
/// b.write_be_int(1, 3, 0x99);
///
/// print(b);       // prints "[4200000042424242]"
/// ```
fn write_be(blob: Blob, start: int, len: int, value: int) -> ();

/// Write a `FLOAT` value to the bytes within an exclusive `range` in the BLOB
/// in little-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `FLOAT`, extra bytes in `FLOAT` are not written.
/// * If number of bytes in `range` > number of bytes for `FLOAT`, extra bytes in `range` are not modified.
fn write_le(blob: Blob, range: Range<int>, value: float) -> ();

/// Write an `INT` value to the bytes within an exclusive `range` in the BLOB
/// in little-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `INT`, extra bytes in `INT` are not written.
/// * If number of bytes in `range` > number of bytes for `INT`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8);
///
/// b.write_le_int(1..3, 0x12345678);
///
/// print(b);       // prints "[0078560000000000]"
/// ```
fn write_le(blob: Blob, range: Range<int>, value: int) -> ();

/// Write a `FLOAT` value to the bytes within an inclusive `range` in the BLOB
/// in little-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `FLOAT`, extra bytes in `FLOAT` are not written.
/// * If number of bytes in `range` > number of bytes for `FLOAT`, extra bytes in `range` are not modified.
fn write_le(blob: Blob, range: RangeInclusive<int>, value: float) -> ();

/// Write an `INT` value to the bytes within an inclusive `range` in the BLOB
/// in little-endian byte order.
///
/// * If number of bytes in `range` < number of bytes for `INT`, extra bytes in `INT` are not written.
/// * If number of bytes in `range` > number of bytes for `INT`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8);
///
/// b.write_le_int(1..=3, 0x12345678);
///
/// print(b);       // prints "[0078563400000000]"
/// ```
fn write_le(blob: Blob, range: RangeInclusive<int>, value: int) -> ();

/// Write a `FLOAT` value to the bytes beginning at the `start` position in the BLOB
/// in little-endian byte order.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, zero is returned.
/// * If `len` ≤ 0, zero is returned.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB after the `start` position is parsed.
///
/// * If number of bytes in `range` < number of bytes for `FLOAT`, extra bytes in `FLOAT` are not written.
/// * If number of bytes in `range` > number of bytes for `FLOAT`, extra bytes in `range` are not modified.
fn write_le(blob: Blob, start: int, len: int, value: float) -> ();

/// Write an `INT` value to the bytes beginning at the `start` position in the BLOB
/// in little-endian byte order.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, zero is returned.
/// * If `len` ≤ 0, zero is returned.
/// * If `start` position + `len` ≥ length of BLOB, entire portion of the BLOB after the `start` position is parsed.
///
/// * If number of bytes in `range` < number of bytes for `INT`, extra bytes in `INT` are not written.
/// * If number of bytes in `range` > number of bytes for `INT`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8);
///
/// b.write_le_int(1, 3, 0x12345678);
///
/// print(b);       // prints "[0078563400000000]"
/// ```
fn write_le(blob: Blob, start: int, len: int, value: int) -> ();

/// Write a string to the bytes within an exclusive `range` in the BLOB in UTF-8 encoding.
///
/// * If number of bytes in `range` < length of `string`, extra bytes in `string` are not written.
/// * If number of bytes in `range` > length of `string`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8);
///
/// b.write_utf8(1..5, "朝には紅顔ありて夕べには白骨となる");
///
/// print(b);       // prints "[00e69c9de3000000]"
/// ```
fn write_utf8(blob: Blob, range: Range<int>, string: String) -> ();

/// Write a string to the bytes within an inclusive `range` in the BLOB in UTF-8 encoding.
///
/// * If number of bytes in `range` < length of `string`, extra bytes in `string` are not written.
/// * If number of bytes in `range` > length of `string`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8);
///
/// b.write_utf8(1..=5, "朝には紅顔ありて夕べには白骨となる");
///
/// print(b);       // prints "[00e69c9de3810000]"
/// ```
fn write_utf8(blob: Blob, range: RangeInclusive<int>, string: String) -> ();

/// Write a string to the bytes within an inclusive `range` in the BLOB in UTF-8 encoding.
///
/// * If `start` < 0, position counts from the end of the BLOB (`-1` is the last byte).
/// * If `start` < -length of BLOB, position counts from the beginning of the BLOB.
/// * If `start` ≥ length of BLOB, the BLOB is not modified.
/// * If `len` ≤ 0, the BLOB is not modified.
/// * If `start` position + `len` ≥ length of BLOB, only the portion of the BLOB after the `start` position is modified.
///
/// * If number of bytes in `range` < length of `string`, extra bytes in `string` are not written.
/// * If number of bytes in `range` > length of `string`, extra bytes in `range` are not modified.
///
/// ```rhai
/// let b = blob(8);
///
/// b.write_utf8(1, 5, "朝には紅顔ありて夕べには白骨となる");
///
/// print(b);       // prints "[00e69c9de3810000]"
/// ```
fn write_utf8(blob: Blob, start: int, len: int, string: String) -> ();

op |(i128, i128) -> i128;

op |(i16, i16) -> i16;

op |(i32, i32) -> i32;

op |(i8, i8) -> i8;

op |(u128, u128) -> u128;

op |(u16, u16) -> u16;

op |(u32, u32) -> u32;

op |(u64, u64) -> u64;

op |(u8, u8) -> u8;