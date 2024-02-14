#[cfg(test)]
mod module_tests {
    use crate::module::Module;

    use proc_macro2::TokenStream;
    use quote::quote;

    #[test]
    fn empty_module() {
        let input_tokens: TokenStream = quote! {
            pub mod empty { }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns().is_empty());
        assert!(item_mod.consts().is_empty());
    }

    #[test]
    fn one_factory_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.consts().is_empty());
        assert_eq!(item_mod.fns().len(), 1);
        assert_eq!(item_mod.fns()[0].name().to_string(), "get_mystic_number");
        assert_eq!(item_mod.fns()[0].arg_count(), 0);
        assert_eq!(
            item_mod.fns()[0].return_type().unwrap(),
            &syn::parse2::<syn::Type>(quote! { INT }).unwrap()
        );
    }

    #[test]
    fn one_factory_fn_with_custom_type_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub type Hello = ();

                /// We are the world!
                pub type World = String;

                pub fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.consts().is_empty());
        assert_eq!(item_mod.custom_types().len(), 2);
        assert_eq!(item_mod.custom_types()[0].name.to_string(), "Hello");
        assert_eq!(item_mod.custom_types()[1].name.to_string(), "World");
        #[cfg(feature = "metadata")]
        assert_eq!(
            item_mod.custom_types()[1].comments[0],
            "/// We are the world!"
        );
        assert_eq!(item_mod.fns().len(), 1);
        assert_eq!(item_mod.fns()[0].name().to_string(), "get_mystic_number");
        assert_eq!(item_mod.fns()[0].arg_count(), 0);
        assert_eq!(
            item_mod.fns()[0].return_type().unwrap(),
            &syn::parse2::<syn::Type>(quote! { INT }).unwrap()
        );
    }

    #[test]
    #[cfg(feature = "metadata")]
    fn one_factory_fn_with_comments_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                /// This is a doc-comment.
                /// Another line.
                /** block doc-comment */
                // Regular comment
                /// Final line.
                /** doc-comment
                    in multiple lines
                 */
                pub fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.consts().is_empty());
        assert_eq!(item_mod.fns().len(), 1);
        assert_eq!(item_mod.fns()[0].name().to_string(), "get_mystic_number");
        assert_eq!(
            item_mod.fns()[0].comments().to_vec(),
            vec![
                "\
                /// This is a doc-comment.\n\
                /// Another line.\n\
                /// block doc-comment \n\
                /// Final line.",
                "/** doc-comment\n                    in multiple lines\n                 */"
            ]
        );
        assert_eq!(item_mod.fns()[0].arg_count(), 0);
        assert_eq!(
            item_mod.fns()[0].return_type().unwrap(),
            &syn::parse2::<syn::Type>(quote! { INT }).unwrap()
        );
    }

    #[test]
    fn one_single_arg_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.consts().is_empty());
        assert_eq!(item_mod.fns().len(), 1);
        assert_eq!(item_mod.fns()[0].name().to_string(), "add_one_to");
        assert_eq!(item_mod.fns()[0].arg_count(), 1);
        assert_eq!(
            item_mod.fns()[0].arg_list().next().unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { x: INT }).unwrap()
        );
        assert_eq!(
            item_mod.fns()[0].return_type().unwrap(),
            &syn::parse2::<syn::Type>(quote! { INT }).unwrap()
        );
    }

    #[test]
    fn one_double_arg_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn add_together(x: INT, y: INT) -> INT {
                    x + y
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        let mut args = item_mod.fns()[0].arg_list();
        assert!(item_mod.consts().is_empty());
        assert_eq!(item_mod.fns().len(), 1);
        assert_eq!(item_mod.fns()[0].name().to_string(), "add_together");
        assert_eq!(item_mod.fns()[0].arg_count(), 2);
        assert_eq!(
            args.next().unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { x: INT }).unwrap()
        );
        assert_eq!(
            args.next().unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { y: INT }).unwrap()
        );
        assert!(args.next().is_none());
        assert_eq!(
            item_mod.fns()[0].return_type().unwrap(),
            &syn::parse2::<syn::Type>(quote! { INT }).unwrap()
        );
    }

    #[test]
    fn one_constant_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                pub mod it_is {
                    pub const MYSTIC_NUMBER: INT = 42;
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns().is_empty());
        assert!(item_mod.consts().is_empty());
        assert_eq!(item_mod.sub_modules().len(), 1);
        assert_eq!(&item_mod.sub_modules()[0].consts()[0].name, "MYSTIC_NUMBER");
        assert_eq!(
            item_mod.sub_modules()[0].consts()[0].expr,
            syn::parse2::<syn::Expr>(quote! { 42 }).unwrap()
        );
    }

    #[test]
    fn one_skipped_fn_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub mod skip_this {
                    #[rhai_fn(skip)]
                    pub fn get_mystic_number() -> INT {
                        42
                    }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns().is_empty());
        assert!(item_mod.consts().is_empty());
        assert_eq!(item_mod.sub_modules().len(), 1);
        assert_eq!(item_mod.sub_modules()[0].fns().len(), 1);
        assert!(item_mod.sub_modules()[0].fns()[0].skipped());
        assert!(item_mod.sub_modules()[0].consts().is_empty());
        assert!(item_mod.sub_modules()[0].sub_modules().is_empty());
    }

    #[test]
    fn one_skipped_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[rhai_mod(skip)]
                pub mod skip_this {
                    pub fn get_mystic_number() -> INT {
                        42
                    }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns().is_empty());
        assert!(item_mod.consts().is_empty());
        assert_eq!(item_mod.sub_modules().len(), 1);
        assert!(item_mod.sub_modules()[0].skipped());
    }

    #[test]
    fn one_constant_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                pub const MYSTIC_NUMBER: INT = 42;
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns().is_empty());
        assert_eq!(item_mod.consts().len(), 1);
        assert_eq!(&item_mod.consts()[0].name, "MYSTIC_NUMBER");
        assert_eq!(
            item_mod.consts()[0].expr,
            syn::parse2::<syn::Expr>(quote! { 42 }).unwrap()
        );
    }

    #[test]
    fn one_skipped_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[rhai_fn(skip)]
                pub fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_eq!(item_mod.fns().len(), 1);
        assert!(item_mod.fns()[0].skipped());
        assert!(item_mod.consts().is_empty());
    }

    #[test]
    fn one_private_constant_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                const MYSTIC_NUMBER: INT = 42;
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns().is_empty());
        assert!(item_mod.consts().is_empty());
    }
}

#[cfg(test)]
mod generate_tests {
    use super::super::assert_streams_eq;
    use crate::module::Module;

    use proc_macro2::TokenStream;
    use quote::quote;

    #[test]
    fn empty_module() {
        let input_tokens: TokenStream = quote! {
            pub mod empty { }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod empty {
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    if flatten {} else {}
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_factory_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("get_mystic_number").with_namespace(FnNamespace::Internal).with_params_info(get_mystic_number_token::PARAM_NAMES)
                        .set_into_module_raw(m, &get_mystic_number_token::param_types(), get_mystic_number_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct get_mystic_number_token();
                #[doc(hidden)]
                impl get_mystic_number_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["INT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 0usize] { [] }
                }
                impl PluginFunc for get_mystic_number_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        Ok(Dynamic::from(get_mystic_number()))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_factory_fn_with_comments_module() {
        let input_tokens: TokenStream = quote! {
            /// This is the one_fn module!
            /** block doc-comment
             *  multi-line
             */
            /// Another line!
            /// Final line!
            pub mod one_fn {
                /// We are the world!
                pub type World = String;

                /// This is a doc-comment.
                /// Another line.
                /** block doc-comment */
                // Regular comment
                /// Final line.
                /** doc-comment
                    in multiple lines
                 */
                pub fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let expected_tokens = quote! {
            /// This is the one_fn module!
            /** block doc-comment
             *  multi-line
             */
            /// Another line!
            /// Final line!
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                /// We are the world!
                pub type World = String;

                /// This is a doc-comment.
                /// Another line.
                /** block doc-comment */
                // Regular comment
                /// Final line.
                /** doc-comment
                    in multiple lines
                 */
                pub fn get_mystic_number() -> INT {
                    42
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m.set_doc("/// This is the one_fn module!\n/** block doc-comment\n             *  multi-line\n             */\n/// Another line!\n/// Final line!");
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("get_mystic_number").with_namespace(FnNamespace::Internal).with_params_info(get_mystic_number_token::PARAM_NAMES)
                        .with_comments(&[
                            "/// This is a doc-comment.\n/// Another line.\n/// block doc-comment \n/// Final line.",
                            "/** doc-comment\n                    in multiple lines\n                 */"
                        ])
                        .set_into_module_raw(m, &get_mystic_number_token::param_types(), get_mystic_number_token().into());
                    m.set_custom_type_with_comments::<String>("World", &["/// We are the world!"]);
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct get_mystic_number_token();
                #[doc(hidden)]
                impl get_mystic_number_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["INT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 0usize] { [] }
                }
                impl PluginFunc for get_mystic_number_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        Ok(Dynamic::from(get_mystic_number()))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_single_arg_global_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_global_fn {
                #[rhai_fn(global)]
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_global_fn {
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("add_one_to").with_namespace(FnNamespace::Global).with_params_info(add_one_to_token::PARAM_NAMES)
                        .set_into_module_raw(m, &add_one_to_token::param_types(), add_one_to_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct add_one_to_token();
                #[doc(hidden)]
                impl add_one_to_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: INT", "INT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<INT>()] }
                }
                impl PluginFunc for add_one_to_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).cast::<INT>();
                        Ok(Dynamic::from(add_one_to(arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_single_arg_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("add_one_to").with_namespace(FnNamespace::Internal).with_params_info(add_one_to_token::PARAM_NAMES)
                        .set_into_module_raw(m, &add_one_to_token::param_types(), add_one_to_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct add_one_to_token();
                #[doc(hidden)]
                impl add_one_to_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: INT", "INT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<INT>()] }
                }
                impl PluginFunc for add_one_to_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).cast::<INT>();
                        Ok(Dynamic::from(add_one_to(arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn two_fn_overload_module() {
        let input_tokens: TokenStream = quote! {
            pub mod two_fns {
                #[rhai_fn(name = "add_n")]
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }

                #[rhai_fn(name = "add_n")]
                pub fn add_n_to(x: INT, y: INT) -> INT {
                    x + y
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod two_fns {
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }

                pub fn add_n_to(x: INT, y: INT) -> INT {
                    x + y
                }

                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("add_n").with_namespace(FnNamespace::Internal).with_params_info(add_one_to_token::PARAM_NAMES)
                        .set_into_module_raw(m, &add_one_to_token::param_types(), add_one_to_token().into());
                    FuncRegistration::new("add_n").with_namespace(FnNamespace::Internal).with_params_info(add_n_to_token::PARAM_NAMES)
                        .set_into_module_raw(m, &add_n_to_token::param_types(), add_n_to_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct add_one_to_token();
                #[doc(hidden)]
                impl add_one_to_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: INT", "INT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<INT>()] }
                }
                impl PluginFunc for add_one_to_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).cast::<INT>();
                        Ok(Dynamic::from(add_one_to(arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }

                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct add_n_to_token();
                #[doc(hidden)]
                impl add_n_to_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: INT", "y: INT", "INT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 2usize] { [TypeId::of::<INT>(), TypeId::of::<INT>()] }
                }
                impl PluginFunc for add_n_to_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).cast::<INT>();
                        let arg1 = mem::take(args[1usize]).cast::<INT>();
                        Ok(Dynamic::from(add_n_to(arg0, arg1)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_double_arg_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn add_together(x: INT, y: INT) -> INT {
                    x + y
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                pub fn add_together(x: INT, y: INT) -> INT {
                    x + y
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("add_together").with_namespace(FnNamespace::Internal).with_params_info(add_together_token::PARAM_NAMES)
                        .set_into_module_raw(m, &add_together_token::param_types(), add_together_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct add_together_token();
                #[doc(hidden)]
                impl add_together_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: INT", "y: INT", "INT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 2usize] { [TypeId::of::<INT>(), TypeId::of::<INT>()] }
                }
                impl PluginFunc for add_together_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).cast::<INT>();
                        let arg1 = mem::take(args[1usize]).cast::<INT>();
                        Ok(Dynamic::from(add_together(arg0, arg1)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_double_rename_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[rhai_fn(name = "add", name = "+", name = "add_together")]
                pub fn add_together(x: INT, y: INT) -> INT {
                    x + y
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                pub fn add_together(x: INT, y: INT) -> INT {
                    x + y
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("add").with_namespace(FnNamespace::Internal).with_params_info(add_together_token::PARAM_NAMES)
                        .set_into_module_raw(m, &add_together_token::param_types(), add_together_token().into());
                    FuncRegistration::new("+").with_namespace(FnNamespace::Internal).with_params_info(add_together_token::PARAM_NAMES)
                        .set_into_module_raw(m, &add_together_token::param_types(), add_together_token().into());
                    FuncRegistration::new("add_together").with_namespace(FnNamespace::Internal).with_params_info(add_together_token::PARAM_NAMES)
                        .set_into_module_raw(m, &add_together_token::param_types(), add_together_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct add_together_token();
                #[doc(hidden)]
                impl add_together_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: INT", "y: INT", "INT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 2usize] { [TypeId::of::<INT>(), TypeId::of::<INT>()] }
                }
                impl PluginFunc for add_together_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).cast::<INT>();
                        let arg1 = mem::take(args[1usize]).cast::<INT>();
                        Ok(Dynamic::from(add_together(arg0, arg1)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_constant_type_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                #[derive(Debug, Clone)]
                pub struct Foo(pub INT);

                pub type Hello = Foo;

                pub const MYSTIC_NUMBER: Foo = Foo(42);

                pub fn get_mystic_number(x: &mut Hello) -> INT {
                    x.0
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_constant {
                #[derive(Debug, Clone)]
                pub struct Foo(pub INT);

                pub type Hello = Foo;

                pub const MYSTIC_NUMBER: Foo = Foo(42);

                pub fn get_mystic_number(x: &mut Hello) -> INT {
                    x.0
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("get_mystic_number").with_namespace(FnNamespace::Internal).with_params_info(get_mystic_number_token::PARAM_NAMES)
                        .set_into_module_raw(m, &get_mystic_number_token::param_types(), get_mystic_number_token().into());
                    m.set_var("MYSTIC_NUMBER", MYSTIC_NUMBER);
                    m.set_custom_type::<Foo>("Hello");
                    if flatten {} else {}
                }

                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct get_mystic_number_token();
                #[doc(hidden)]
                impl get_mystic_number_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut Hello", "INT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<Hello>()] }
                }
                impl PluginFunc for get_mystic_number_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = &mut args[0usize].write_lock::<Hello>().unwrap();
                        Ok(Dynamic::from(get_mystic_number(arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_constant_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                pub const MYSTIC_NUMBER: INT = 42;
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_constant {
                pub const MYSTIC_NUMBER: INT = 42;
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    m.set_var("MYSTIC_NUMBER", MYSTIC_NUMBER);
                    if flatten {} else {}
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_constant_module_imports_preserved() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                pub use rhai::INT;
                pub const MYSTIC_NUMBER: INT = 42;
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_constant {
                pub use rhai::INT;
                pub const MYSTIC_NUMBER: INT = 42;
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    m.set_var("MYSTIC_NUMBER", MYSTIC_NUMBER);
                    if flatten {} else {}
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_private_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                fn get_mystic_number() -> INT {
                    42
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    if flatten {} else {}
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_skipped_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[rhai_fn(skip)]
                pub fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    if flatten {} else {}
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_skipped_sub_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
                #[rhai_mod(skip)]
                pub mod inner_secrets {
                    pub const SECRET_NUMBER: INT = 86;
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
                pub mod inner_secrets {
                    pub const SECRET_NUMBER: INT = 86;
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("get_mystic_number").with_namespace(FnNamespace::Internal).with_params_info(get_mystic_number_token::PARAM_NAMES)
                        .set_into_module_raw(m, &get_mystic_number_token::param_types(), get_mystic_number_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct get_mystic_number_token();
                #[doc(hidden)]
                impl get_mystic_number_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["INT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 0usize] { [] }
                }
                impl PluginFunc for get_mystic_number_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        Ok(Dynamic::from(get_mystic_number()))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_private_constant_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                const MYSTIC_NUMBER: INT = 42;
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_constant {
                const MYSTIC_NUMBER: INT = 42;
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    if flatten {} else {}
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_str_arg_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod str_fn {
                pub fn print_out_to(x: &str) {
                    x + 1
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod str_fn {
                pub fn print_out_to(x: &str) {
                    x + 1
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("print_out_to").with_namespace(FnNamespace::Internal).with_params_info(print_out_to_token::PARAM_NAMES)
                         .set_into_module_raw(m, &print_out_to_token::param_types(), print_out_to_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct print_out_to_token();
                #[doc(hidden)]
                impl print_out_to_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &str", "()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<ImmutableString>()] }
                }
                impl PluginFunc for print_out_to_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).into_immutable_string().unwrap();
                        Ok(Dynamic::from(print_out_to(&arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_string_arg_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod str_fn {
                pub fn print_out_to(x: String) {
                    x + 1
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod str_fn {
                pub fn print_out_to(x: String) {
                    x + 1
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("print_out_to").with_namespace(FnNamespace::Internal).with_params_info(print_out_to_token::PARAM_NAMES)
                         .set_into_module_raw(m, &print_out_to_token::param_types(), print_out_to_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct print_out_to_token();
                #[doc(hidden)]
                impl print_out_to_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: String", "()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<ImmutableString>()] }
                }
                impl PluginFunc for print_out_to_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = mem::take(args[0usize]).into_string().unwrap();
                        Ok(Dynamic::from(print_out_to(arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { false }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn mut_ref_pure_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod ref_fn {
                #[rhai_fn(pure)]
                pub fn foo(x: &mut FLOAT, y: INT) -> FLOAT {
                    *x + y as FLOAT
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod ref_fn {
                pub fn foo(x: &mut FLOAT, y: INT) -> FLOAT {
                    *x + y as FLOAT
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("foo").with_namespace(FnNamespace::Internal).with_params_info(foo_token::PARAM_NAMES)
                         .set_into_module_raw(m, &foo_token::param_types(), foo_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct foo_token();
                #[doc(hidden)]
                impl foo_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut FLOAT", "y: INT", "FLOAT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 2usize] { [TypeId::of::<FLOAT>(), TypeId::of::<INT>()] }
                }
                impl PluginFunc for foo_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg1 = mem::take(args[1usize]).cast::<INT>();
                        let arg0 = &mut args[0usize].write_lock::<FLOAT>().unwrap();
                        Ok(Dynamic::from(foo(arg0, arg1)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { true }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_mut_ref_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod ref_fn {
                pub fn increment(x: &mut FLOAT) {
                    *x += 1.0 as FLOAT;
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod ref_fn {
                pub fn increment(x: &mut FLOAT) {
                    *x += 1.0 as FLOAT;
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("increment").with_namespace(FnNamespace::Internal).with_params_info(increment_token::PARAM_NAMES)
                         .set_into_module_raw(m, &increment_token::param_types(), increment_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct increment_token();
                #[doc(hidden)]
                impl increment_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut FLOAT", "()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<FLOAT>()] }
                }
                impl PluginFunc for increment_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = &mut args[0usize].write_lock::<FLOAT>().unwrap();
                        Ok(Dynamic::from(increment(arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_fn_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub mod it_is {
                    pub fn increment(x: &mut FLOAT) {
                        *x += 1.0 as FLOAT;
                    }
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                pub mod it_is {
                    pub fn increment(x: &mut FLOAT) {
                        *x += 1.0 as FLOAT;
                    }
                    #[allow(unused_imports)]
                    use super::*;

                    #[doc(hidden)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        rhai_generate_into_module(&mut m, false);
                        m.build_index();
                        m
                    }
                    #[allow(unused_mut)]
                    #[doc(hidden)]
                    pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("increment").with_namespace(FnNamespace::Internal).with_params_info(increment_token::PARAM_NAMES)
                             .set_into_module_raw(m, &increment_token::param_types(), increment_token().into());
                        if flatten {} else {}
                    }
                    #[allow(non_camel_case_types)]
                    #[doc(hidden)]
                    pub struct increment_token();
                    #[doc(hidden)]
                    impl increment_token {
                        pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut FLOAT", "()"];
                        #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<FLOAT>()] }
                    }
                    impl PluginFunc for increment_token {
                        #[inline(always)]
                        fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                            let arg0 = &mut args[0usize].write_lock::<FLOAT>().unwrap();
                            Ok(Dynamic::from(increment(arg0)))
                        }

                        #[inline(always)] fn is_method_call(&self) -> bool { true }
                        #[inline(always)] fn is_pure(&self) -> bool { false }
                        #[inline(always)] fn is_volatile(&self) -> bool { false }
                        #[inline(always)] fn has_context(&self) -> bool { false }
                    }
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    if flatten {
                        { self::it_is::rhai_generate_into_module(m, flatten); }
                    } else {
                        { m.set_sub_module("it_is", self::it_is::rhai_module_generate()); }
                    }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_fn_with_cfg_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[cfg(not(feature = "no_float"))]
                pub mod it_is {
                    pub fn increment(x: &mut FLOAT) {
                        *x += 1.0 as FLOAT;
                    }
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                #[cfg(not(feature = "no_float"))]
                #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                pub mod it_is {
                    pub fn increment(x: &mut FLOAT) {
                        *x += 1.0 as FLOAT;
                    }
                    #[allow(unused_imports)]
                    use super::*;

                    #[doc(hidden)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        rhai_generate_into_module(&mut m, false);
                        m.build_index();
                        m
                    }
                    #[allow(unused_mut)]
                    #[doc(hidden)]
                    pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("increment").with_namespace(FnNamespace::Internal).with_params_info(increment_token::PARAM_NAMES)
                             .set_into_module_raw(m, &increment_token::param_types(), increment_token().into());
                        if flatten {} else {}
                    }
                    #[allow(non_camel_case_types)]
                    #[doc(hidden)]
                    pub struct increment_token();
                    #[doc(hidden)]
                    impl increment_token {
                        pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut FLOAT", "()"];
                        #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<FLOAT>()] }
                    }
                    impl PluginFunc for increment_token {
                        #[inline(always)]
                        fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                            let arg0 = &mut args[0usize].write_lock::<FLOAT>().unwrap();
                            Ok(Dynamic::from(increment(arg0)))
                        }

                        #[inline(always)] fn is_method_call(&self) -> bool { true }
                        #[inline(always)] fn is_pure(&self) -> bool { false }
                        #[inline(always)] fn is_volatile(&self) -> bool { false }
                        #[inline(always)] fn has_context(&self) -> bool { false }
                    }
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    if flatten {
                        {
                            #[cfg(not(feature = "no_float"))]
                            self::it_is::rhai_generate_into_module(m, flatten);
                        }
                    } else {
                        {
                            #[cfg(not(feature = "no_float"))]
                            m.set_sub_module("it_is", self::it_is::rhai_module_generate());
                        }
                    }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_getter_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[rhai_fn(get = "square")]
                pub fn int_foo(x: &mut u64) -> u64 {
                    (*x) * (*x)
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                pub fn int_foo(x: &mut u64) -> u64 {
                    (*x) * (*x)
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("get$square").with_namespace(FnNamespace::Global).with_params_info(int_foo_token::PARAM_NAMES)
                         .set_into_module_raw(m, &int_foo_token::param_types(), int_foo_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct int_foo_token();
                #[doc(hidden)]
                impl int_foo_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut u64", "u64"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<u64>()] }
                }
                impl PluginFunc for int_foo_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = &mut args[0usize].write_lock::<u64>().unwrap();
                        Ok(Dynamic::from(int_foo(arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_getter_and_rename_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[rhai_fn(name = "square", get = "square")]
                pub fn int_foo(x: &mut u64) -> u64 {
                    (*x) * (*x)
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                pub fn int_foo(x: &mut u64) -> u64 {
                    (*x) * (*x)
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("square").with_namespace(FnNamespace::Internal).with_params_info(int_foo_token::PARAM_NAMES)
                         .set_into_module_raw(m, &int_foo_token::param_types(), int_foo_token().into());
                    FuncRegistration::new("get$square").with_namespace(FnNamespace::Global).with_params_info(int_foo_token::PARAM_NAMES)
                         .set_into_module_raw(m, &int_foo_token::param_types(), int_foo_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct int_foo_token();
                #[doc(hidden)]
                impl int_foo_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut u64", "u64"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 1usize] { [TypeId::of::<u64>()] }
                }
                impl PluginFunc for int_foo_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg0 = &mut args[0usize].write_lock::<u64>().unwrap();
                        Ok(Dynamic::from(int_foo(arg0)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_setter_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[rhai_fn(set = "squared")]
                pub fn int_foo(x: &mut u64, y: u64) {
                    *x = y * y
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                pub fn int_foo(x: &mut u64, y: u64) {
                    *x = y * y
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("set$squared").with_namespace(FnNamespace::Global).with_params_info(int_foo_token::PARAM_NAMES)
                         .set_into_module_raw(m, &int_foo_token::param_types(), int_foo_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct int_foo_token();
                #[doc(hidden)]
                impl int_foo_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut u64", "y: u64", "()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 2usize] { [TypeId::of::<u64>(), TypeId::of::<u64>()] }
                }
                impl PluginFunc for int_foo_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg1 = mem::take(args[1usize]).cast::<u64>();
                        let arg0 = &mut args[0usize].write_lock::<u64>().unwrap();
                        Ok(Dynamic::from(int_foo(arg0, arg1)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_setter_and_rename_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[rhai_fn(name = "set_sq", set = "squared")]
                pub fn int_foo(x: &mut u64, y: u64) {
                    *x = y * y
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_fn {
                pub fn int_foo(x: &mut u64, y: u64) {
                    *x = y * y
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("set_sq").with_namespace(FnNamespace::Internal).with_params_info(int_foo_token::PARAM_NAMES)
                         .set_into_module_raw(m, &int_foo_token::param_types(), int_foo_token().into());
                    FuncRegistration::new("set$squared").with_namespace(FnNamespace::Global).with_params_info(int_foo_token::PARAM_NAMES)
                         .set_into_module_raw(m, &int_foo_token::param_types(), int_foo_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct int_foo_token();
                #[doc(hidden)]
                impl int_foo_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut u64", "y: u64", "()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 2usize] { [TypeId::of::<u64>(), TypeId::of::<u64>()] }
                }
                impl PluginFunc for int_foo_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg1 = mem::take(args[1usize]).cast::<u64>();
                        let arg0 = &mut args[0usize].write_lock::<u64>().unwrap();
                        Ok(Dynamic::from(int_foo(arg0, arg1)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_index_getter_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_index_fn {
                #[rhai_fn(index_get)]
                pub fn get_by_index(x: &mut MyCollection, i: u64) -> FLOAT {
                    x.get(i)
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_index_fn {
                pub fn get_by_index(x: &mut MyCollection, i: u64) -> FLOAT {
                    x.get(i)
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("index$get$").with_namespace(FnNamespace::Global).with_params_info(get_by_index_token::PARAM_NAMES)
                         .set_into_module_raw(m, &get_by_index_token::param_types(), get_by_index_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct get_by_index_token();
                #[doc(hidden)]
                impl get_by_index_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut MyCollection", "i: u64", "FLOAT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 2usize] { [TypeId::of::<MyCollection>(), TypeId::of::<u64>()] }
                }
                impl PluginFunc for get_by_index_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg1 = mem::take(args[1usize]).cast::<u64>();
                        let arg0 = &mut args[0usize].write_lock::<MyCollection>().unwrap();
                        Ok(Dynamic::from(get_by_index(arg0, arg1)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_index_getter_fn_with_cfg_attr_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_index_fn {
                #[cfg(hello)]
                #[rhai_fn(index_get)]
                #[some_other_attr]
                pub fn get_by_index(x: &mut MyCollection, i: u64) -> FLOAT {
                    x.get(i)
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_index_fn {
                #[cfg(hello)]
                #[some_other_attr]
                pub fn get_by_index(x: &mut MyCollection, i: u64) -> FLOAT {
                    x.get(i)
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    #[cfg(hello)]
                    FuncRegistration::new("index$get$").with_namespace(FnNamespace::Global).with_params_info(get_by_index_token::PARAM_NAMES)
                         .set_into_module_raw(m, &get_by_index_token::param_types(), get_by_index_token().into());
                    if flatten {} else {}
                }
                #[cfg(hello)]
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct get_by_index_token();
                #[cfg(hello)]
                #[doc(hidden)]
                impl get_by_index_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut MyCollection", "i: u64", "FLOAT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 2usize] { [TypeId::of::<MyCollection>(), TypeId::of::<u64>()] }
                }
                #[cfg(hello)]
                impl PluginFunc for get_by_index_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg1 = mem::take(args[1usize]).cast::<u64>();
                        let arg0 = &mut args[0usize].write_lock::<MyCollection>().unwrap();
                        Ok(Dynamic::from(get_by_index(arg0, arg1)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_index_getter_and_rename_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_index_fn {
                #[rhai_fn(name = "get", index_get)]
                pub fn get_by_index(x: &mut MyCollection, i: u64) -> FLOAT {
                    x.get(i)
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_index_fn {
                pub fn get_by_index(x: &mut MyCollection, i: u64) -> FLOAT {
                    x.get(i)
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("get").with_namespace(FnNamespace::Internal).with_params_info(get_by_index_token::PARAM_NAMES)
                         .set_into_module_raw(m, &get_by_index_token::param_types(), get_by_index_token().into());
                    FuncRegistration::new("index$get$").with_namespace(FnNamespace::Global).with_params_info(get_by_index_token::PARAM_NAMES)
                         .set_into_module_raw(m, &get_by_index_token::param_types(), get_by_index_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct get_by_index_token();
                #[doc(hidden)]
                impl get_by_index_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut MyCollection", "i: u64", "FLOAT"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 2usize] { [TypeId::of::<MyCollection>(), TypeId::of::<u64>()] }
                }
                impl PluginFunc for get_by_index_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg1 = mem::take(args[1usize]).cast::<u64>();
                        let arg0 = &mut args[0usize].write_lock::<MyCollection>().unwrap();
                        Ok(Dynamic::from(get_by_index(arg0, arg1)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_index_setter_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_index_fn {
                #[rhai_fn(index_set)]
                pub fn set_by_index(x: &mut MyCollection, i: u64, item: FLOAT) {
                    x.entry(i).set(item)
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_index_fn {
                pub fn set_by_index(x: &mut MyCollection, i: u64, item: FLOAT) {
                    x.entry(i).set(item)
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("index$set$").with_namespace(FnNamespace::Global).with_params_info(set_by_index_token::PARAM_NAMES)
                         .set_into_module_raw(m, &set_by_index_token::param_types(), set_by_index_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct set_by_index_token();
                #[doc(hidden)]
                impl set_by_index_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut MyCollection", "i: u64", "item: FLOAT", "()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 3usize] { [TypeId::of::<MyCollection>(), TypeId::of::<u64>(), TypeId::of::<FLOAT>()] }
                }
                impl PluginFunc for set_by_index_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg1 = mem::take(args[1usize]).cast::<u64>();
                        let arg2 = mem::take(args[2usize]).cast::<FLOAT>();
                        let arg0 = &mut args[0usize].write_lock::<MyCollection>().unwrap();
                        Ok(Dynamic::from(set_by_index(arg0, arg1, arg2)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_index_setter_and_rename_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_index_fn {
                #[rhai_fn(name = "set", index_set)]
                pub fn set_by_index(x: &mut MyCollection, i: u64, item: FLOAT) {
                    x.entry(i).set(item)
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_index_fn {
                pub fn set_by_index(x: &mut MyCollection, i: u64, item: FLOAT) {
                    x.entry(i).set(item)
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    FuncRegistration::new("set").with_namespace(FnNamespace::Internal).with_params_info(set_by_index_token::PARAM_NAMES)
                         .set_into_module_raw(m, &set_by_index_token::param_types(), set_by_index_token().into());
                    FuncRegistration::new("index$set$").with_namespace(FnNamespace::Global).with_params_info(set_by_index_token::PARAM_NAMES)
                         .set_into_module_raw(m, &set_by_index_token::param_types(), set_by_index_token().into());
                    if flatten {} else {}
                }
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                pub struct set_by_index_token();
                #[doc(hidden)]
                impl set_by_index_token {
                    pub const PARAM_NAMES: &'static [&'static str] = &["x: &mut MyCollection", "i: u64", "item: FLOAT", "()"];
                    #[inline(always)] pub fn param_types() -> [TypeId; 3usize] { [TypeId::of::<MyCollection>(), TypeId::of::<u64>(), TypeId::of::<FLOAT>()] }
                }
                impl PluginFunc for set_by_index_token {
                    #[inline(always)]
                    fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                        let arg1 = mem::take(args[1usize]).cast::<u64>();
                        let arg2 = mem::take(args[2usize]).cast::<FLOAT>();
                        let arg0 = &mut args[0usize].write_lock::<MyCollection>().unwrap();
                        Ok(Dynamic::from(set_by_index(arg0, arg1, arg2)))
                    }

                    #[inline(always)] fn is_method_call(&self) -> bool { true }
                    #[inline(always)] fn is_pure(&self) -> bool { false }
                    #[inline(always)] fn is_volatile(&self) -> bool { false }
                    #[inline(always)] fn has_context(&self) -> bool { false }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_constant_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                pub mod it_is {
                    pub const MYSTIC_NUMBER: INT = 42;
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod one_constant {
                #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                pub mod it_is {
                    pub const MYSTIC_NUMBER: INT = 42;
                    #[allow(unused_imports)]
                    use super::*;

                    #[doc(hidden)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        rhai_generate_into_module(&mut m, false);
                        m.build_index();
                        m
                    }
                    #[allow(unused_mut)]
                    #[doc(hidden)]
                    pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                        m.set_var("MYSTIC_NUMBER", MYSTIC_NUMBER);
                        if flatten {} else {}
                    }
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    if flatten {
                        { self::it_is::rhai_generate_into_module(m, flatten); }
                    } else {
                        { m.set_sub_module("it_is", self::it_is::rhai_module_generate()); }
                    }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn dual_constant_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod two_constants {
                pub mod first_is {
                    pub const MYSTIC_NUMBER: INT = 42;
                }
                pub mod second_is {
                    #[cfg(hello)]
                    pub const SPECIAL_CPU_NUMBER: INT = 68000;
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod two_constants {
                #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                pub mod first_is {
                    pub const MYSTIC_NUMBER: INT = 42;
                    #[allow(unused_imports)]
                    use super::*;

                    #[doc(hidden)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        rhai_generate_into_module(&mut m, false);
                        m.build_index();
                        m
                    }
                    #[allow(unused_mut)]
                    #[doc(hidden)]
                    pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                        m.set_var("MYSTIC_NUMBER", MYSTIC_NUMBER);
                        if flatten {} else {}
                    }
                }
                #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                pub mod second_is {
                    #[cfg(hello)]
                    pub const SPECIAL_CPU_NUMBER: INT = 68000;
                    #[allow(unused_imports)]
                    use super::*;

                    #[doc(hidden)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        rhai_generate_into_module(&mut m, false);
                        m.build_index();
                        m
                    }
                    #[allow(unused_mut)]
                    #[doc(hidden)]
                    pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                        #[cfg(hello)]
                        m.set_var("SPECIAL_CPU_NUMBER", SPECIAL_CPU_NUMBER);
                        if flatten {} else {}
                    }
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    if flatten {
                        { self::first_is::rhai_generate_into_module(m, flatten); }
                        { self::second_is::rhai_generate_into_module(m, flatten); }
                    } else {
                        { m.set_sub_module("first_is", self::first_is::rhai_module_generate()); }
                        { m.set_sub_module("second_is", self::second_is::rhai_module_generate()); }
                    }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn deep_tree_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod heap_root {
                pub const VALUE: INT = 100;
                pub mod left {
                    pub const VALUE: INT = 19;
                    pub mod left {
                        pub const VALUE: INT = 17;
                        pub mod left {
                            pub const VALUE: INT = 2;
                        }
                        pub mod right {
                            pub const VALUE: INT = 7;
                        }
                    }
                    pub mod right {
                        pub const VALUE: INT = 3;
                    }
                }
                pub mod right {
                    pub const VALUE: INT = 36;
                    pub mod left {
                        pub const VALUE: INT = 25;
                    }
                    pub mod right {
                        pub const VALUE: INT = 1;
                    }
                }
            }
        };

        let expected_tokens = quote! {
            #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
            pub mod heap_root {
                pub const VALUE: INT = 100;
                #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                pub mod left {
                    pub const VALUE: INT = 19;
                    #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                    pub mod left {
                        pub const VALUE: INT = 17;
                        #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                        pub mod left {
                            pub const VALUE: INT = 2;
                            #[allow(unused_imports)]
                            use super::*;

                            #[doc(hidden)]
                            pub fn rhai_module_generate() -> Module {
                                let mut m = Module::new();
                                rhai_generate_into_module(&mut m, false);
                                m.build_index();
                                m
                            }
                            #[allow(unused_mut)]
                            #[doc(hidden)]
                            pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                                m.set_var("VALUE", VALUE);
                                if flatten {} else {}
                            }
                        }
                        #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                        pub mod right {
                            pub const VALUE: INT = 7;
                            #[allow(unused_imports)]
                            use super::*;

                            #[doc(hidden)]
                            pub fn rhai_module_generate() -> Module {
                                let mut m = Module::new();
                                rhai_generate_into_module(&mut m, false);
                                m.build_index();
                                m
                            }
                            #[allow(unused_mut)]
                            #[doc(hidden)]
                            pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                                m.set_var("VALUE", VALUE);
                                if flatten {} else {}
                            }
                        }
                        #[allow(unused_imports)]
                        use super::*;

                        #[doc(hidden)]
                        pub fn rhai_module_generate() -> Module {
                            let mut m = Module::new();
                            rhai_generate_into_module(&mut m, false);
                            m.build_index();
                            m
                        }
                        #[allow(unused_mut)]
                        #[doc(hidden)]
                        pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                            m.set_var("VALUE", VALUE);

                            if flatten {
                                { self::left::rhai_generate_into_module(m, flatten); }
                                { self::right::rhai_generate_into_module(m, flatten); }
                            } else {
                                { m.set_sub_module("left", self::left::rhai_module_generate()); }
                                { m.set_sub_module("right", self::right::rhai_module_generate()); }
                            }
                        }
                    }
                    #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                    pub mod right {
                        pub const VALUE: INT = 3;
                        #[allow(unused_imports)]
                        use super::*;

                        #[doc(hidden)]
                        pub fn rhai_module_generate() -> Module {
                            let mut m = Module::new();
                            rhai_generate_into_module(&mut m, false);
                            m.build_index();
                            m
                        }
                        #[allow(unused_mut)]
                        #[doc(hidden)]
                        pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                            m.set_var("VALUE", VALUE);
                            if flatten {} else {}
                        }
                    }
                    #[allow(unused_imports)]
                    use super::*;

                    #[doc(hidden)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        rhai_generate_into_module(&mut m, false);
                        m.build_index();
                        m
                    }
                    #[allow(unused_mut)]
                    #[doc(hidden)]
                    pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                        m.set_var("VALUE", VALUE);

                        if flatten {
                            { self::left::rhai_generate_into_module(m, flatten); }
                            { self::right::rhai_generate_into_module(m, flatten); }
                        } else {
                            { m.set_sub_module("left", self::left::rhai_module_generate()); }
                            { m.set_sub_module("right", self::right::rhai_module_generate()); }
                        }
                    }
                }
                #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                pub mod right {
                    pub const VALUE: INT = 36;
                    #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                    pub mod left {
                        pub const VALUE: INT = 25;
                        #[allow(unused_imports)]
                        use super::*;

                        #[doc(hidden)]
                        pub fn rhai_module_generate() -> Module {
                            let mut m = Module::new();
                            rhai_generate_into_module(&mut m, false);
                            m.build_index();
                            m
                        }
                        #[allow(unused_mut)]
                        #[doc(hidden)]
                        pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                            m.set_var("VALUE", VALUE);
                            if flatten {} else {}
                        }
                    }
                    #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                    pub mod right {
                        pub const VALUE: INT = 1;
                        #[allow(unused_imports)]
                        use super::*;

                        #[doc(hidden)]
                        pub fn rhai_module_generate() -> Module {
                            let mut m = Module::new();
                            rhai_generate_into_module(&mut m, false);
                            m.build_index();
                            m
                        }
                        #[allow(unused_mut)]
                        #[doc(hidden)]
                        pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                            m.set_var("VALUE", VALUE);
                            if flatten {} else {}
                        }
                    }
                    #[allow(unused_imports)]
                    use super::*;

                    #[doc(hidden)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        rhai_generate_into_module(&mut m, false);
                        m.build_index();
                        m
                    }
                    #[allow(unused_mut)]
                    #[doc(hidden)]
                    pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                        m.set_var("VALUE", VALUE);

                        if flatten {
                            { self::left::rhai_generate_into_module(m, flatten); }
                            { self::right::rhai_generate_into_module(m, flatten); }
                        } else {
                            { m.set_sub_module("left", self::left::rhai_module_generate()); }
                            { m.set_sub_module("right", self::right::rhai_module_generate()); }
                        }
                    }
                }
                #[allow(unused_imports)]
                use super::*;

                #[doc(hidden)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    rhai_generate_into_module(&mut m, false);
                    m.build_index();
                    m
                }
                #[allow(unused_mut)]
                #[doc(hidden)]
                pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                    m.set_var("VALUE", VALUE);

                    if flatten {
                        { self::left::rhai_generate_into_module(m, flatten); }
                        { self::right::rhai_generate_into_module(m, flatten); }
                    } else {
                        { m.set_sub_module("left", self::left::rhai_module_generate()); }
                        { m.set_sub_module("right", self::right::rhai_module_generate()); }
                    }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }
}
