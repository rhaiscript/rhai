#[cfg(test)]
mod function_tests {
    use crate::function::ExportedFn;

    use proc_macro2::TokenStream;
    use quote::quote;

    #[test]
    fn minimal_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn do_nothing() { }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_eq!(&item_fn.name().to_string(), "do_nothing");
        assert!(!item_fn.mutable_receiver());
        assert!(item_fn.is_public());
        assert!(item_fn.return_type().is_none());
        assert_eq!(item_fn.arg_list().count(), 0);
    }

    #[test]
    fn one_arg_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn do_something(x: usize) { }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_eq!(&item_fn.name().to_string(), "do_something");
        assert_eq!(item_fn.arg_list().count(), 1);
        assert!(!item_fn.mutable_receiver());
        assert!(item_fn.is_public());
        assert!(item_fn.return_type().is_none());

        assert_eq!(
            item_fn.arg_list().next().unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { x: usize }).unwrap()
        );
    }

    #[test]
    fn two_arg_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn do_something(x: usize, y: f32) { }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_eq!(&item_fn.name().to_string(), "do_something");
        assert_eq!(item_fn.arg_list().count(), 2);
        assert!(!item_fn.mutable_receiver());
        assert!(item_fn.is_public());
        assert!(item_fn.return_type().is_none());

        assert_eq!(
            item_fn.arg_list().next().unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { x: usize }).unwrap()
        );
        assert_eq!(
            item_fn.arg_list().nth(1).unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { y: f32 }).unwrap()
        );
    }

    #[test]
    fn usize_returning_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn get_magic_number() -> usize { 42 }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_eq!(&item_fn.name().to_string(), "get_magic_number");
        assert!(!item_fn.mutable_receiver());
        assert!(item_fn.is_public());
        assert_eq!(item_fn.arg_list().count(), 0);
        assert_eq!(
            item_fn.return_type().unwrap(),
            &syn::Type::Path(syn::TypePath {
                qself: None,
                path: syn::parse2::<syn::Path>(quote! { usize }).unwrap()
            })
        );
    }

    #[test]
    fn ref_returning_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn get_magic_phrase() -> &'static str { "open sesame" }
        };

        let err = syn::parse2::<ExportedFn>(input_tokens).unwrap_err();
        assert_eq!(format!("{err}"), "Rhai functions cannot return references");
    }

    #[test]
    fn ptr_returning_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn get_magic_phrase() -> *const str { "open sesame" }
        };

        let err = syn::parse2::<ExportedFn>(input_tokens).unwrap_err();
        assert_eq!(format!("{err}"), "Rhai functions cannot return pointers");
    }

    #[test]
    fn ref_arg_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn greet(who: &Person) { }
        };

        let err = syn::parse2::<ExportedFn>(input_tokens).unwrap_err();
        assert_eq!(
            format!("{err}"),
            "references from Rhai in this position must be mutable"
        );
    }

    #[test]
    fn ref_second_arg_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn greet(count: usize, who: &Person) { }
        };

        let err = syn::parse2::<ExportedFn>(input_tokens).unwrap_err();
        assert_eq!(
            format!("{err}"),
            "function parameters other than the first one cannot be passed by reference"
        );
    }

    #[test]
    fn mut_ref_second_arg_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn give(item_name: &str, who: &mut Person) { }
        };

        let err = syn::parse2::<ExportedFn>(input_tokens).unwrap_err();
        assert_eq!(
            format!("{err}"),
            "function parameters other than the first one cannot be passed by reference"
        );
    }

    #[test]
    fn str_arg_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn log(message: &str) { }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_eq!(&item_fn.name().to_string(), "log");
        assert_eq!(item_fn.arg_list().count(), 1);
        assert!(!item_fn.mutable_receiver());
        assert!(item_fn.is_public());
        assert!(item_fn.return_type().is_none());

        assert_eq!(
            item_fn.arg_list().next().unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { message: &str }).unwrap()
        );
    }

    #[test]
    fn str_second_arg_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn log(level: usize, message: &str) { }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_eq!(&item_fn.name().to_string(), "log");
        assert_eq!(item_fn.arg_list().count(), 2);
        assert!(!item_fn.mutable_receiver());
        assert!(item_fn.is_public());
        assert!(item_fn.return_type().is_none());

        assert_eq!(
            item_fn.arg_list().next().unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { level: usize }).unwrap()
        );
        assert_eq!(
            item_fn.arg_list().nth(1).unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { message: &str }).unwrap()
        );
    }

    #[test]
    fn private_fn() {
        let input_tokens: TokenStream = quote! {
            fn do_nothing() { }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_eq!(&item_fn.name().to_string(), "do_nothing");
        assert!(!item_fn.mutable_receiver());
        assert!(!item_fn.is_public());
        assert!(item_fn.return_type().is_none());
        assert_eq!(item_fn.arg_list().count(), 0);
    }

    #[test]
    fn receiver_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn act_upon(&mut self) { }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_eq!(&item_fn.name().to_string(), "act_upon");
        assert!(item_fn.mutable_receiver());
        assert!(item_fn.is_public());
        assert!(item_fn.return_type().is_none());
        assert_eq!(item_fn.arg_list().count(), 1);
    }

    #[test]
    fn immutable_receiver_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn act_upon(&self) { }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_eq!(&item_fn.name().to_string(), "act_upon");
        assert!(item_fn.mutable_receiver());
        assert!(item_fn.is_public());
        assert!(item_fn.return_type().is_none());
        assert_eq!(item_fn.arg_list().count(), 1);
    }
}

#[cfg(test)]
mod generate_tests {
    use crate::function::ExportedFn;

    use proc_macro2::TokenStream;
    use quote::quote;

    fn assert_streams_eq(actual: TokenStream, expected: TokenStream) {
        let actual = actual.to_string();
        let expected = expected.to_string();
        if actual != expected {
            let mut counter = 0;
            let _iter = actual.chars().zip(expected.chars()).skip_while(|(a, e)| {
                if *a == *e {
                    counter += 1;
                    true
                } else {
                    false
                }
            });
            let (_actual_diff, _expected_diff) = {
                let mut actual_diff = String::new();
                let mut expected_diff = String::new();
                for (a, e) in _iter.take(50) {
                    actual_diff.push(a);
                    expected_diff.push(e);
                }
                (actual_diff, expected_diff)
            };
            eprintln!("actual != expected, diverge at char {counter}");
            // eprintln!("  actual: {}", _actual_diff);
            // eprintln!("expected: {}", _expected_diff);
            // assert!(false);
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn minimal_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn do_nothing() { }
        };

        let expected_tokens = quote! {
            #[automatically_derived]
            pub mod rhai_fn_do_nothing {
                use super::*;
                #[doc(hidden)]
                pub struct Token();
                #[doc(hidden)]
                impl Token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 0usize] { [] }
                }
                impl PluginFunc for Token {
                    #[inline(always)] fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        Ok(Dynamic::from(do_nothing()))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
                #[allow(unused)]
                #[doc(hidden)]
                #[inline(always)] pub fn dynamic_result_fn() -> RhaiResult {
                    Ok(Dynamic::from(do_nothing()))
                }
            }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_streams_eq(item_fn.generate(), expected_tokens);
    }

    #[test]
    fn one_arg_usize_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn do_something(x: usize) { }
        };

        let expected_tokens = quote! {
            #[automatically_derived]
            pub mod rhai_fn_do_something {
                use super::*;
                #[doc(hidden)]
                pub struct Token();
                #[doc(hidden)]
                impl Token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: usize", "()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<usize>()] }
                }
                impl PluginFunc for Token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).cast::<usize>();
                        Ok(Dynamic::from(do_something(arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
                #[allow(unused)]
                #[doc(hidden)]
                #[inline(always)] pub fn dynamic_result_fn(x: usize) -> RhaiResult {
                    Ok(Dynamic::from(do_something(x)))
                }
            }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_streams_eq(item_fn.generate(), expected_tokens);
    }

    #[test]
    fn one_arg_fn_with_context() {
        let input_tokens: TokenStream = quote! {
            pub fn do_something(context: NativeCallContext, x: usize) {}
        };

        let expected_tokens = quote! {
            #[automatically_derived]
            pub mod rhai_fn_do_something {
                use super::*;
                #[doc(hidden)]
                pub struct Token();
                #[doc(hidden)]
                impl Token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: usize", "()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<usize>()] }
                }
                impl PluginFunc for Token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).cast::<usize>();
                        Ok(Dynamic::from(do_something(context.unwrap(), arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { true }
                }
                #[allow(unused)]
                #[doc(hidden)]
                #[inline(always)] pub fn dynamic_result_fn(context: NativeCallContext, x: usize) -> RhaiResult {
                    Ok(Dynamic::from(do_something(context, x)))
                }
            }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert!(item_fn.pass_context());
        assert_streams_eq(item_fn.generate(), expected_tokens);
    }

    #[test]
    fn return_dynamic() {
        let input_tokens: TokenStream = quote! {
            pub fn return_dynamic() -> (((rhai::Dynamic))) {
                ().into()
            }
        };

        let expected_tokens = quote! {
            #[automatically_derived]
            pub mod rhai_fn_return_dynamic {
                use super::*;
                #[doc(hidden)]
                pub struct Token();
                #[doc(hidden)]
                impl Token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["rhai::Dynamic"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 0usize] { [] }
                }
                impl PluginFunc for Token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        Ok(Dynamic::from(return_dynamic()))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
                #[allow(unused)]
                #[doc(hidden)]
                #[inline(always)] pub fn dynamic_result_fn() -> RhaiResult {
                    Ok(Dynamic::from(return_dynamic()))
                }
            }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_streams_eq(item_fn.generate(), expected_tokens);
    }

    #[test]
    fn one_arg_usize_fn_impl() {
        let input_tokens: TokenStream = quote! {
            pub fn do_something(x: usize) { }
        };

        let expected_tokens = quote! {
            #[doc(hidden)]
            impl TestStruct {
                pub const PARAM_NAMES: &'static [&'static str] = &["x: usize", "()"];
                #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<usize>()] }
            }
            impl PluginFunc for TestStruct {
                #[inline(always)]
                fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                    let arg0 = mem::take(args[0usize]).cast::<usize>();
                    Ok(Dynamic::from(do_something(arg0)))
                }

                #[inline(always)] fn is_method_call(&self) -> bool { false }
                #[inline(always)] fn is_pure(&self) -> bool { true }
                #[inline(always)] fn is_volatile(&self) -> bool { false }
                #[inline(always)] fn has_context(&self) -> bool { false }
            }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_streams_eq(item_fn.generate_impl("TestStruct"), expected_tokens);
    }

    #[test]
    fn two_arg_returning_usize_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn add_together(x: usize, y: usize) -> usize { x + y }
        };

        let expected_tokens = quote! {
            #[automatically_derived]
            pub mod rhai_fn_add_together {
                use super::*;
                #[doc(hidden)]
                pub struct Token();
                #[doc(hidden)]
                impl Token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: usize", "y: usize", "usize"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 2usize] { [TypeId::of::<usize>(), TypeId::of::<usize>()] }
                }
                impl PluginFunc for Token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).cast::<usize>();
                        let arg1 = mem::take(args[1usize]).cast::<usize>();
                        Ok(Dynamic::from(add_together(arg0, arg1)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
                #[allow(unused)]
                #[doc(hidden)]
                #[inline(always)] pub fn dynamic_result_fn(x: usize, y: usize) -> RhaiResult {
                    Ok(Dynamic::from(add_together(x, y)))
                }
            }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert_streams_eq(item_fn.generate(), expected_tokens);
    }

    #[test]
    fn mut_arg_usize_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn increment(x: &mut usize, y: usize) { *x += y; }
        };

        let expected_tokens = quote! {
            #[automatically_derived]
            pub mod rhai_fn_increment {
                use super::*;
                #[doc(hidden)]
                pub struct Token();
                #[doc(hidden)]
                impl Token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut usize", "y: usize", "()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 2usize] { [TypeId::of::<usize>(), TypeId::of::<usize>()] }
                }
                impl PluginFunc for Token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg1 = mem::take(args[1usize]).cast::<usize>();
                        let arg0 = &mut args[0usize].write_lock::<usize>().unwrap();
                        Ok(Dynamic::from(increment(arg0, arg1)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
                #[allow(unused)]
                #[doc(hidden)]
                #[inline(always)] pub fn dynamic_result_fn(x: &mut usize, y: usize) -> RhaiResult {
                    Ok(Dynamic::from(increment(x, y)))
                }
            }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert!(item_fn.mutable_receiver());
        assert_streams_eq(item_fn.generate(), expected_tokens);
    }

    #[test]
    fn str_arg_fn() {
        let input_tokens: TokenStream = quote! {
            pub fn special_print(message: &str) { eprintln!("----{}----", message); }
        };

        let expected_tokens = quote! {
            #[automatically_derived]
            pub mod rhai_fn_special_print {
                use super::*;
                #[doc(hidden)]
                pub struct Token();
                #[doc(hidden)]
                impl Token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["message: &str", "()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<ImmutableString>()] }
                }
                impl PluginFunc for Token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).into_immutable_string().unwrap();
                        Ok(Dynamic::from(special_print(&arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
                #[allow(unused)]
                #[doc(hidden)]
                #[inline(always)] pub fn dynamic_result_fn(message: &str) -> RhaiResult {
                    Ok(Dynamic::from(special_print(message)))
                }
            }
        };

        let item_fn = syn::parse2::<ExportedFn>(input_tokens).unwrap();
        assert!(!item_fn.mutable_receiver());
        assert_streams_eq(item_fn.generate(), expected_tokens);
    }
}
