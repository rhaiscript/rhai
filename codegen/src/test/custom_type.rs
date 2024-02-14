#[cfg(test)]
mod custom_type_tests {
    use crate::test::assert_streams_eq;
    use quote::quote;

    #[test]
    fn test_custom_type_tuple_struct() {
        let input = quote! {
            #[derive(Clone, CustomType)]
            pub struct Bar(
                #[rhai_type(skip)]
                #[cfg(not(feature = "no_float"))]
                rhai::FLOAT,
                INT,
                #[rhai_type(name = "boo", readonly)]
                String,
                Vec<INT>
            );
        };

        let result = crate::custom_type::derive_custom_type_impl(
            syn::parse2::<syn::DeriveInput>(input).unwrap(),
        );

        let expected = quote! {
            impl CustomType for Bar {
                fn build(mut builder: TypeBuilder<Self>) {
                    builder.with_name(stringify!(Bar));
                    builder.with_get_set("field1",
                        |obj: &mut Self| obj.1.clone(),
                        |obj: &mut Self, val| obj.1 = val
                    );
                    builder.with_get("boo", |obj: &mut Self| obj.2.clone());
                    builder.with_get_set("field3",
                        |obj: &mut Self| obj.3.clone(),
                        |obj: &mut Self, val| obj.3 = val
                    );
                }
            }
        };

        assert_streams_eq(result, expected);
    }

    #[test]
    fn test_custom_type_struct() {
        let input = quote! {
            #[derive(CustomType)]
            #[rhai_type(skip, name = "MyFoo", extra = Self::build_extra)]
            pub struct Foo {
                #[cfg(not(feature = "no_float"))]
                #[rhai_type(skip)]
                _dummy: rhai::FLOAT,
                #[rhai_type(get = get_bar)]
                pub bar: INT,
                #[rhai_type(name = "boo", readonly)]
                pub(crate) baz: String,
                #[rhai_type(set = Self::set_qux)]
                pub qux: Vec<INT>
            }
        };

        let result = crate::custom_type::derive_custom_type_impl(
            syn::parse2::<syn::DeriveInput>(input).unwrap(),
        );

        let expected = quote! {
            impl CustomType for Foo {
                fn build(mut builder: TypeBuilder<Self>) {
                    builder.with_name("MyFoo");
                    builder.with_get_set(stringify!(bar),
                        |obj: &mut Self| get_bar(&*obj),
                        |obj: &mut Self, val| obj.bar = val
                    );
                    builder.with_get("boo", |obj: &mut Self| obj.baz.clone());
                    builder.with_get_set(stringify!(qux),
                        |obj: &mut Self| obj.qux.clone(),
                        Self::set_qux
                    );
                    Self::build_extra(&mut builder);
                }
            }
        };

        assert_streams_eq(result, expected);
    }
}
