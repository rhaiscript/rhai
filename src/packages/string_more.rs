use crate::plugin::*;
use crate::{def_package, Dynamic, ExclusiveRange, InclusiveRange, RhaiResultOf, StaticVec, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{any::TypeId, mem};

use super::string_basic::{print_with_func, FUNC_TO_STRING};

def_package! {
    /// Package of additional string utilities over [`BasicStringPackage`][super::BasicStringPackage]
    pub MoreStringPackage(lib) {
        lib.standard = true;

        combine_with_exported_module!(lib, "string", string_functions);
    }
}

#[export_module]
mod string_functions {
    use crate::{ImmutableString, SmartString};

    #[rhai_fn(name = "+", pure)]
    pub fn add_append(
        ctx: NativeCallContext,
        string: &mut ImmutableString,
        mut item: Dynamic,
    ) -> ImmutableString {
        let s = print_with_func(FUNC_TO_STRING, &ctx, &mut item);

        if s.is_empty() {
            string.clone()
        } else {
            format!("{string}{s}").into()
        }
    }
    #[rhai_fn(name = "+=", name = "append")]
    pub fn add(ctx: NativeCallContext, string: &mut ImmutableString, mut item: Dynamic) {
        let s = print_with_func(FUNC_TO_STRING, &ctx, &mut item);

        if !s.is_empty() {
            *string = format!("{string}{s}").into();
        }
    }
    #[rhai_fn(name = "+", pure)]
    pub fn add_prepend(
        ctx: NativeCallContext,
        item: &mut Dynamic,
        string: &str,
    ) -> ImmutableString {
        let mut s = print_with_func(FUNC_TO_STRING, &ctx, item);

        if !string.is_empty() {
            s.make_mut().push_str(string);
        }

        s
    }

    // The following are needed in order to override the generic versions with `Dynamic` parameters.

    #[rhai_fn(name = "+", pure)]
    pub fn add_append_str(
        string1: &mut ImmutableString,
        string2: ImmutableString,
    ) -> ImmutableString {
        &*string1 + string2
    }
    #[rhai_fn(name = "+", pure)]
    pub fn add_append_char(string: &mut ImmutableString, character: char) -> ImmutableString {
        &*string + character
    }
    #[rhai_fn(name = "+")]
    pub fn add_prepend_char(character: char, string: &str) -> ImmutableString {
        format!("{character}{string}").into()
    }

    #[rhai_fn(name = "+")]
    pub fn add_append_unit(string: ImmutableString, item: ()) -> ImmutableString {
        let _ = item;
        string
    }
    #[rhai_fn(name = "+")]
    pub fn add_prepend_unit(_item: (), string: ImmutableString) -> ImmutableString {
        string
    }

    #[rhai_fn(name = "+=")]
    pub fn add_assign_append_str(string1: &mut ImmutableString, string2: ImmutableString) {
        *string1 += string2
    }
    #[rhai_fn(name = "+=", pure)]
    pub fn add_assign_append_char(string: &mut ImmutableString, character: char) {
        *string += character
    }
    #[rhai_fn(name = "+=")]
    pub fn add_assign_append_unit(string: &mut ImmutableString, item: ()) {
        let _ = string;
        let _ = item;
    }

    #[cfg(not(feature = "no_index"))]
    pub mod blob_functions {
        use crate::Blob;

        #[rhai_fn(name = "+", pure)]
        pub fn add_append(string: &mut ImmutableString, utf8: Blob) -> ImmutableString {
            if utf8.is_empty() {
                return string.clone();
            }

            let s = String::from_utf8_lossy(&utf8);

            if string.is_empty() {
                match s {
                    std::borrow::Cow::Borrowed(_) => String::from_utf8(utf8).unwrap(),
                    std::borrow::Cow::Owned(_) => s.into_owned(),
                }
                .into()
            } else {
                let mut x = SmartString::from(string.as_str());
                x.push_str(s.as_ref());
                x.into()
            }
        }
        #[rhai_fn(name = "+=", name = "append")]
        pub fn add(string: &mut ImmutableString, utf8: Blob) {
            let mut s = crate::SmartString::from(string.as_str());
            if !utf8.is_empty() {
                s.push_str(&String::from_utf8_lossy(&utf8));
                *string = s.into();
            }
        }
        #[rhai_fn(name = "+")]
        pub fn add_prepend(utf8: Blob, string: &str) -> ImmutableString {
            let s = String::from_utf8_lossy(&utf8);
            let mut s = match s {
                std::borrow::Cow::Borrowed(_) => String::from_utf8(utf8).unwrap(),
                std::borrow::Cow::Owned(_) => s.into_owned(),
            };

            if !string.is_empty() {
                s.push_str(string);
            }

            s.into()
        }
    }

    /// Return the length of the string, in number of characters.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let text = "朝には紅顔ありて夕べには白骨となる";
    ///
    /// print(text.len);        // prints 17
    /// ```
    #[rhai_fn(name = "len", get = "len")]
    pub fn len(string: &str) -> INT {
        if string.is_empty() {
            0
        } else {
            string.chars().count() as INT
        }
    }
    /// Return true if the string is empty.
    #[rhai_fn(name = "is_empty", get = "is_empty")]
    pub fn is_empty(string: &str) -> bool {
        string.len() == 0
    }
    /// Return the length of the string, in number of bytes used to store it in UTF-8 encoding.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let text = "朝には紅顔ありて夕べには白骨となる";
    ///
    /// print(text.bytes);      // prints 51
    /// ```
    #[rhai_fn(name = "bytes", get = "bytes")]
    pub fn bytes(string: &str) -> INT {
        if string.is_empty() {
            0
        } else {
            string.len() as INT
        }
    }
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
    #[cfg(not(feature = "no_index"))]
    pub fn to_blob(string: &str) -> crate::Blob {
        if string.is_empty() {
            crate::Blob::new()
        } else {
            string.as_bytes().into()
        }
    }
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
    pub fn remove(string: &mut ImmutableString, sub_string: &str) {
        *string -= sub_string;
    }
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
    #[rhai_fn(name = "remove")]
    pub fn remove_char(string: &mut ImmutableString, character: char) {
        *string -= character;
    }
    /// Clear the string, making it empty.
    pub fn clear(string: &mut ImmutableString) {
        if !string.is_empty() {
            if let Some(s) = string.get_mut() {
                s.clear();
            } else {
                *string = ImmutableString::new();
            }
        }
    }
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
    pub fn truncate(string: &mut ImmutableString, len: INT) {
        if len > 0 {
            let chars: StaticVec<_> = string.chars().collect();
            let copy = string.make_mut();
            copy.clear();
            copy.extend(chars.into_iter().take(len as usize));
        } else {
            clear(string);
        }
    }
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
    pub fn trim(string: &mut ImmutableString) {
        if let Some(s) = string.get_mut() {
            let trimmed = s.trim();

            if trimmed != s {
                *s = trimmed.into();
            }
        } else {
            let trimmed = string.trim();

            if trimmed != string {
                *string = trimmed.into();
            }
        }
    }
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
    pub fn pop(string: &mut ImmutableString) -> Dynamic {
        if string.is_empty() {
            Dynamic::UNIT
        } else {
            match string.make_mut().pop() {
                Some(c) => c.into(),
                None => Dynamic::UNIT,
            }
        }
    }
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
    #[rhai_fn(name = "pop")]
    pub fn pop_string(
        ctx: NativeCallContext,
        string: &mut ImmutableString,
        len: INT,
    ) -> ImmutableString {
        if string.is_empty() || len <= 0 {
            return ctx.engine().get_interned_string("");
        }

        let mut chars = StaticVec::<char>::with_capacity(len as usize);

        for _ in 0..len {
            match string.make_mut().pop() {
                Some(c) => chars.push(c),
                None => break,
            }
        }

        chars.into_iter().rev().collect::<SmartString>().into()
    }

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
    #[rhai_fn(pure)]
    pub fn to_upper(string: &mut ImmutableString) -> ImmutableString {
        if string.chars().all(char::is_uppercase) {
            string.clone()
        } else {
            string.to_uppercase().into()
        }
    }
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
    pub fn make_upper(string: &mut ImmutableString) {
        if !string.is_empty() && string.chars().any(|ch| !ch.is_uppercase()) {
            *string = string.to_uppercase().into();
        }
    }
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
    #[rhai_fn(pure)]
    pub fn to_lower(string: &mut ImmutableString) -> ImmutableString {
        if string.is_empty() || string.chars().all(char::is_lowercase) {
            string.clone()
        } else {
            string.to_lowercase().into()
        }
    }
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
    pub fn make_lower(string: &mut ImmutableString) {
        if string.chars().any(|ch| !ch.is_lowercase()) {
            *string = string.to_lowercase().into();
        }
    }

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
    #[rhai_fn(name = "to_upper")]
    pub fn to_upper_char(character: char) -> char {
        let mut stream = character.to_uppercase();
        let ch = stream.next().unwrap();
        if stream.next().is_some() {
            character
        } else {
            ch
        }
    }
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
    #[rhai_fn(name = "make_upper")]
    pub fn make_upper_char(character: &mut char) {
        *character = to_upper_char(*character);
    }
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
    #[rhai_fn(name = "to_lower")]
    pub fn to_lower_char(character: char) -> char {
        let mut stream = character.to_lowercase();
        let ch = stream.next().unwrap();
        if stream.next().is_some() {
            character
        } else {
            ch
        }
    }
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
    #[rhai_fn(name = "make_lower")]
    pub fn make_lower_char(character: &mut char) {
        *character = to_lower_char(*character);
    }

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
    pub fn starts_with(string: &str, match_string: &str) -> bool {
        string.starts_with(match_string)
    }
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
    pub fn ends_with(string: &str, match_string: &str) -> bool {
        string.ends_with(match_string)
    }

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
    #[rhai_fn(name = "index_of")]
    pub fn index_of_char_starting_from(string: &str, character: char, start: INT) -> INT {
        if string.is_empty() {
            return -1;
        }

        let start = if start < 0 {
            let abs_start = start.unsigned_abs() as usize;
            let chars: Vec<_> = string.chars().collect();
            let num_chars = chars.len();
            if abs_start > num_chars {
                0
            } else {
                chars
                    .into_iter()
                    .take(num_chars - abs_start)
                    .collect::<String>()
                    .len()
            }
        } else if start == 0 {
            0
        } else if start as usize >= string.chars().count() {
            return -1 as INT;
        } else {
            string
                .chars()
                .take(start as usize)
                .collect::<String>()
                .len()
        };

        string[start..].find(character).map_or(-1 as INT, |index| {
            string[0..start + index].chars().count() as INT
        })
    }
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
    #[rhai_fn(name = "index_of")]
    pub fn index_of_char(string: &str, character: char) -> INT {
        if string.is_empty() {
            -1
        } else {
            string
                .find(character)
                .map_or(-1 as INT, |index| string[0..index].chars().count() as INT)
        }
    }
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
    #[rhai_fn(name = "index_of")]
    pub fn index_of_string_starting_from(string: &str, find_string: &str, start: INT) -> INT {
        if string.is_empty() {
            return -1;
        }

        let start = if start < 0 {
            let abs_start = start.unsigned_abs() as usize;
            let chars = string.chars().collect::<Vec<_>>();
            let num_chars = chars.len();
            if abs_start > num_chars {
                0
            } else {
                chars
                    .into_iter()
                    .take(num_chars - abs_start)
                    .collect::<String>()
                    .len()
            }
        } else if start == 0 {
            0
        } else if start as usize >= string.chars().count() {
            return -1 as INT;
        } else {
            string
                .chars()
                .take(start as usize)
                .collect::<String>()
                .len()
        };

        string[start..]
            .find(find_string)
            .map_or(-1 as INT, |index| {
                string[0..start + index].chars().count() as INT
            })
    }
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
    #[rhai_fn(name = "index_of")]
    pub fn index_of(string: &str, find_string: &str) -> INT {
        if string.is_empty() {
            -1
        } else {
            string
                .find(find_string)
                .map_or(-1 as INT, |index| string[0..index].chars().count() as INT)
        }
    }

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
    pub fn get(string: &str, index: INT) -> Dynamic {
        if index >= 0 {
            string
                .chars()
                .nth(index as usize)
                .map_or_else(|| Dynamic::UNIT, Into::into)
        } else {
            // Count from end if negative
            string
                .chars()
                .rev()
                .nth((index.unsigned_abs() as usize) - 1)
                .map_or_else(|| Dynamic::UNIT, Into::into)
        }
    }
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
    pub fn set(string: &mut ImmutableString, index: INT, character: char) {
        if index >= 0 {
            let index = index as usize;
            *string = string
                .chars()
                .enumerate()
                .map(|(i, ch)| if i == index { character } else { ch })
                .collect();
        } else {
            let abs_index = index.unsigned_abs() as usize;
            let string_len = string.chars().count();

            if abs_index <= string_len {
                let index = string_len - abs_index;
                *string = string
                    .chars()
                    .enumerate()
                    .map(|(i, ch)| if i == index { character } else { ch })
                    .collect();
            }
        }
    }

    /// Copy an exclusive range of characters from the string and return it as a new string.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let text = "hello, world!";
    ///
    /// print(text.sub_string(3..7));   // prints "lo, "
    /// ```
    #[rhai_fn(name = "sub_string")]
    pub fn sub_string_range(
        ctx: NativeCallContext,
        string: &str,
        range: ExclusiveRange,
    ) -> ImmutableString {
        let start = INT::max(range.start, 0);
        let end = INT::max(range.end, start);
        sub_string(ctx, string, start, end - start)
    }
    /// Copy an inclusive range of characters from the string and return it as a new string.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let text = "hello, world!";
    ///
    /// print(text.sub_string(3..=7));  // prints "lo, w"
    /// ```
    #[rhai_fn(name = "sub_string")]
    pub fn sub_string_inclusive_range(
        ctx: NativeCallContext,
        string: &str,
        range: InclusiveRange,
    ) -> ImmutableString {
        let start = INT::max(*range.start(), 0);
        let end = INT::max(*range.end(), start);
        sub_string(ctx, string, start, end - start + 1)
    }
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
    pub fn sub_string(
        ctx: NativeCallContext,
        string: &str,
        start: INT,
        len: INT,
    ) -> ImmutableString {
        if string.is_empty() {
            return ctx.engine().get_interned_string("");
        }

        let mut chars = StaticVec::with_capacity(string.len());

        let offset = if string.is_empty() || len <= 0 {
            return ctx.engine().get_interned_string("");
        } else if start < 0 {
            let abs_start = start.unsigned_abs() as usize;
            chars.extend(string.chars());
            if abs_start > chars.len() {
                0
            } else {
                chars.len() - abs_start
            }
        } else if start as usize >= string.chars().count() {
            return ctx.engine().get_interned_string("");
        } else {
            start as usize
        };

        if chars.is_empty() {
            chars.extend(string.chars());
        }

        let len = if offset + len as usize > chars.len() {
            chars.len() - offset
        } else {
            len as usize
        };

        chars
            .iter()
            .skip(offset)
            .take(len)
            .copied()
            .collect::<String>()
            .into()
    }
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
    #[rhai_fn(name = "sub_string")]
    pub fn sub_string_starting_from(
        ctx: NativeCallContext,
        string: &str,
        start: INT,
    ) -> ImmutableString {
        if string.is_empty() {
            ctx.engine().get_interned_string("")
        } else {
            let len = string.len() as INT;
            sub_string(ctx, string, start, len)
        }
    }

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
    #[rhai_fn(name = "crop")]
    pub fn crop_range(string: &mut ImmutableString, range: ExclusiveRange) {
        let start = INT::max(range.start, 0);
        let end = INT::max(range.end, start);
        crop(string, start, end - start);
    }
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
    #[rhai_fn(name = "crop")]
    pub fn crop_inclusive_range(string: &mut ImmutableString, range: InclusiveRange) {
        let start = INT::max(*range.start(), 0);
        let end = INT::max(*range.end(), start);
        crop(string, start, end - start + 1);
    }

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
    #[rhai_fn(name = "crop")]
    pub fn crop(string: &mut ImmutableString, start: INT, len: INT) {
        if string.is_empty() {
            return;
        }

        let mut chars = StaticVec::with_capacity(string.len());

        let offset = if string.is_empty() || len <= 0 {
            string.make_mut().clear();
            return;
        } else if start < 0 {
            let abs_start = start.unsigned_abs() as usize;
            chars.extend(string.chars());
            if abs_start > chars.len() {
                0
            } else {
                chars.len() - abs_start
            }
        } else if start as usize >= string.chars().count() {
            string.make_mut().clear();
            return;
        } else {
            start as usize
        };

        if chars.is_empty() {
            chars.extend(string.chars());
        }

        let len = if offset + len as usize > chars.len() {
            chars.len() - offset
        } else {
            len as usize
        };

        let copy = string.make_mut();
        copy.clear();
        copy.extend(chars.iter().skip(offset).take(len));
    }
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
    #[rhai_fn(name = "crop")]
    pub fn crop_string_starting_from(string: &mut ImmutableString, start: INT) {
        crop(string, start, string.len() as INT);
    }

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
    #[rhai_fn(name = "replace")]
    pub fn replace(string: &mut ImmutableString, find_string: &str, substitute_string: &str) {
        if !string.is_empty() {
            *string = string.replace(find_string, substitute_string).into();
        }
    }
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
    #[rhai_fn(name = "replace")]
    pub fn replace_string_with_char(
        string: &mut ImmutableString,
        find_string: &str,
        substitute_character: char,
    ) {
        if !string.is_empty() {
            *string = string
                .replace(find_string, &substitute_character.to_string())
                .into();
        }
    }
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
    #[rhai_fn(name = "replace")]
    pub fn replace_char_with_string(
        string: &mut ImmutableString,
        find_character: char,
        substitute_string: &str,
    ) {
        if !string.is_empty() {
            *string = string
                .replace(&find_character.to_string(), substitute_string)
                .into();
        }
    }
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
    #[rhai_fn(name = "replace")]
    pub fn replace_char(
        string: &mut ImmutableString,
        find_character: char,
        substitute_character: char,
    ) {
        if !string.is_empty() {
            *string = string
                .replace(
                    &find_character.to_string(),
                    &substitute_character.to_string(),
                )
                .into();
        }
    }

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
    #[rhai_fn(return_raw)]
    pub fn pad(
        ctx: NativeCallContext,
        string: &mut ImmutableString,
        len: INT,
        character: char,
    ) -> RhaiResultOf<()> {
        if len <= 0 {
            return Ok(());
        }
        let _ctx = ctx;

        // Check if string will be over max size limit
        #[cfg(not(feature = "unchecked"))]
        if _ctx.engine().max_string_size() > 0 && len as usize > _ctx.engine().max_string_size() {
            return Err(crate::ERR::ErrorDataTooLarge(
                "Length of string".to_string(),
                crate::Position::NONE,
            )
            .into());
        }

        let orig_len = string.chars().count();

        if len as usize > orig_len {
            let p = string.make_mut();

            for _ in 0..(len as usize - orig_len) {
                p.push(character);
            }

            #[cfg(not(feature = "unchecked"))]
            if _ctx.engine().max_string_size() > 0 && string.len() > _ctx.engine().max_string_size()
            {
                return Err(crate::ERR::ErrorDataTooLarge(
                    "Length of string".to_string(),
                    crate::Position::NONE,
                )
                .into());
            }
        }

        Ok(())
    }
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
    #[rhai_fn(name = "pad", return_raw)]
    pub fn pad_with_string(
        ctx: NativeCallContext,
        string: &mut ImmutableString,
        len: INT,
        padding: &str,
    ) -> RhaiResultOf<()> {
        if len <= 0 {
            return Ok(());
        }
        let _ctx = ctx;

        // Check if string will be over max size limit
        #[cfg(not(feature = "unchecked"))]
        if _ctx.engine().max_string_size() > 0 && len as usize > _ctx.engine().max_string_size() {
            return Err(crate::ERR::ErrorDataTooLarge(
                "Length of string".to_string(),
                crate::Position::NONE,
            )
            .into());
        }

        let mut str_len = string.chars().count();
        let padding_len = padding.chars().count();

        if len as usize > str_len {
            let p = string.make_mut();

            while str_len < len as usize {
                if str_len + padding_len <= len as usize {
                    p.push_str(padding);
                    str_len += padding_len;
                } else {
                    p.extend(padding.chars().take(len as usize - str_len));
                    str_len = len as usize;
                }
            }

            #[cfg(not(feature = "unchecked"))]
            if _ctx.engine().max_string_size() > 0 && string.len() > _ctx.engine().max_string_size()
            {
                return Err(crate::ERR::ErrorDataTooLarge(
                    "Length of string".to_string(),
                    crate::Position::NONE,
                )
                .into());
            }
        }

        Ok(())
    }

    #[cfg(not(feature = "no_index"))]
    pub mod arrays {
        use crate::{Array, ImmutableString};

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
        #[rhai_fn(name = "split")]
        pub fn split_at(ctx: NativeCallContext, string: &mut ImmutableString, index: INT) -> Array {
            if index <= 0 {
                let abs_index = index.unsigned_abs() as usize;
                let num_chars = string.chars().count();
                if abs_index > num_chars {
                    vec![
                        ctx.engine().get_interned_string("").into(),
                        string.as_str().into(),
                    ]
                } else {
                    let prefix: String = string.chars().take(num_chars - abs_index).collect();
                    let prefix_len = prefix.len();
                    vec![prefix.into(), string[prefix_len..].into()]
                }
            } else {
                let prefix: String = string.chars().take(index as usize).collect();
                let prefix_len = prefix.len();
                vec![prefix.into(), string[prefix_len..].into()]
            }
        }
        /// Return an array containing all the characters of the string.
        ///
        /// # Example
        ///
        /// ```rhai
        /// let text = "hello";
        ///
        /// print(text.to_chars());     // prints "['h', 'e', 'l', 'l', 'o']"
        /// ```
        #[rhai_fn(name = "to_chars")]
        pub fn to_chars(string: &str) -> Array {
            if string.is_empty() {
                Array::new()
            } else {
                string.chars().map(Into::into).collect()
            }
        }
        /// Split the string into segments based on whitespaces, returning an array of the segments.
        ///
        /// # Example
        ///
        /// ```rhai
        /// let text = "hello, world! hello, foo!";
        ///
        /// print(text.split());        // prints ["hello,", "world!", "hello,", "foo!"]
        /// ```
        #[rhai_fn(name = "split")]
        pub fn split_whitespace(string: &str) -> Array {
            if string.is_empty() {
                Array::new()
            } else {
                string.split_whitespace().map(Into::into).collect()
            }
        }
        /// Split the string into segments based on a `delimiter` string, returning an array of the segments.
        ///
        /// # Example
        ///
        /// ```rhai
        /// let text = "hello, world! hello, foo!";
        ///
        /// print(text.split("ll"));    // prints ["he", "o, world! he", "o, foo!"]
        /// ```
        pub fn split(string: &str, delimiter: &str) -> Array {
            string.split(delimiter).map(Into::into).collect()
        }
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
        #[rhai_fn(name = "split")]
        pub fn splitn(string: &str, delimiter: &str, segments: INT) -> Array {
            let pieces: usize = if segments < 1 { 1 } else { segments as usize };
            string.splitn(pieces, delimiter).map(Into::into).collect()
        }
        /// Split the string into segments based on a `delimiter` character, returning an array of the segments.
        ///
        /// # Example
        ///
        /// ```rhai
        /// let text = "hello, world! hello, foo!";
        ///
        /// print(text.split('l'));     // prints ["he", "", "o, wor", "d! he", "", "o, foo!"]
        /// ```
        #[rhai_fn(name = "split")]
        pub fn split_char(string: &str, delimiter: char) -> Array {
            string.split(delimiter).map(Into::into).collect()
        }
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
        #[rhai_fn(name = "split")]
        pub fn splitn_char(string: &str, delimiter: char, segments: INT) -> Array {
            let pieces: usize = if segments < 1 { 1 } else { segments as usize };
            string.splitn(pieces, delimiter).map(Into::into).collect()
        }
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
        #[rhai_fn(name = "split_rev")]
        pub fn rsplit(string: &str, delimiter: &str) -> Array {
            string.rsplit(delimiter).map(Into::into).collect()
        }
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
        #[rhai_fn(name = "split_rev")]
        pub fn rsplitn(string: &str, delimiter: &str, segments: INT) -> Array {
            let pieces: usize = if segments < 1 { 1 } else { segments as usize };
            string.rsplitn(pieces, delimiter).map(Into::into).collect()
        }
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
        #[rhai_fn(name = "split_rev")]
        pub fn rsplit_char(string: &str, delimiter: char) -> Array {
            string.rsplit(delimiter).map(Into::into).collect()
        }
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
        #[rhai_fn(name = "split_rev")]
        pub fn rsplitn_char(string: &str, delimiter: char, segments: INT) -> Array {
            let pieces: usize = if segments < 1 { 1 } else { segments as usize };
            string.rsplitn(pieces, delimiter).map(Into::into).collect()
        }
    }
}
