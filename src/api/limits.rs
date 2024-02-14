//! Settings for [`Engine`]'s limitations.
#![cfg(not(feature = "unchecked"))]

use crate::Engine;
use std::num::{NonZeroU64, NonZeroUsize};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(debug_assertions)]
pub mod default_limits {
    /// Maximum levels of function calls.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    pub const MAX_CALL_STACK_DEPTH: usize = 8;
    /// Maximum levels of expressions.
    pub const MAX_EXPR_DEPTH: usize = 32;
    /// Maximum levels of expressions in function bodies.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    pub const MAX_FUNCTION_EXPR_DEPTH: usize = 16;
}
#[cfg(not(debug_assertions))]
pub mod default_limits {
    /// Maximum levels of function calls.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    pub const MAX_CALL_STACK_DEPTH: usize = 64;
    /// Maximum levels of expressions.
    pub const MAX_EXPR_DEPTH: usize = 64;
    /// Maximum levels of expressions in function bodies.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    pub const MAX_FUNCTION_EXPR_DEPTH: usize = 32;
}

/// A type containing all the limits imposed by the [`Engine`].
///
/// Not available under `unchecked`.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Limits {
    /// Maximum levels of call-stack to prevent infinite recursion.
    ///
    /// Set to zero to effectively disable function calls.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    pub call_stack_depth: usize,
    /// Maximum depth of statements/expressions at global level.
    pub expr_depth: Option<NonZeroUsize>,
    /// Maximum depth of statements/expressions in functions.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    pub function_expr_depth: Option<NonZeroUsize>,
    /// Maximum number of operations allowed to run.
    pub num_operations: Option<NonZeroU64>,
    /// Maximum number of variables allowed at any instant.
    ///
    /// Set to zero to effectively disable creating variables.
    pub num_variables: usize,
    /// Maximum number of scripted functions allowed.
    ///
    /// Set to zero to effectively disable defining any function.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    pub num_functions: usize,
    /// Maximum number of [modules][crate::Module] allowed to load.
    ///
    /// Set to zero to effectively disable loading any [module][crate::Module].
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    pub num_modules: usize,
    /// Maximum length of a [string][crate::ImmutableString].
    pub string_len: Option<NonZeroUsize>,
    /// Maximum length of an [array][crate::Array].
    ///
    /// Not available under `no_index`.
    #[cfg(not(feature = "no_index"))]
    pub array_size: Option<NonZeroUsize>,
    /// Maximum number of properties in an [object map][crate::Map].
    ///
    /// Not available under `no_object`.
    #[cfg(not(feature = "no_object"))]
    pub map_size: Option<NonZeroUsize>,
}

impl Limits {
    /// Create a new [`Limits`] with default values.
    ///
    /// Not available under `unchecked`.
    #[inline]
    pub const fn new() -> Self {
        Self {
            #[cfg(not(feature = "no_function"))]
            call_stack_depth: default_limits::MAX_CALL_STACK_DEPTH,
            expr_depth: NonZeroUsize::new(default_limits::MAX_EXPR_DEPTH),
            #[cfg(not(feature = "no_function"))]
            function_expr_depth: NonZeroUsize::new(default_limits::MAX_FUNCTION_EXPR_DEPTH),
            num_operations: None,
            num_variables: usize::MAX,
            #[cfg(not(feature = "no_function"))]
            num_functions: usize::MAX,
            #[cfg(not(feature = "no_module"))]
            num_modules: usize::MAX,
            string_len: None,
            #[cfg(not(feature = "no_index"))]
            array_size: None,
            #[cfg(not(feature = "no_object"))]
            map_size: None,
        }
    }
}

impl Default for Limits {
    #[inline(always)]
    #[must_use]
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// Is there a data size limit set?
    #[inline(always)]
    pub(crate) const fn has_data_size_limit(&self) -> bool {
        self.limits.string_len.is_some()
            || {
                #[cfg(not(feature = "no_index"))]
                {
                    self.limits.array_size.is_some()
                }
                #[cfg(feature = "no_index")]
                false
            }
            || {
                #[cfg(not(feature = "no_object"))]
                {
                    self.limits.map_size.is_some()
                }
                #[cfg(feature = "no_object")]
                false
            }
    }
    /// Set the maximum levels of function calls allowed for a script in order to avoid
    /// infinite recursion and stack overflows.
    ///
    /// Not available under `unchecked` or `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn set_max_call_levels(&mut self, levels: usize) -> &mut Self {
        self.limits.call_stack_depth = levels;
        self
    }
    /// The maximum levels of function calls allowed for a script.
    ///
    /// Not available under `unchecked` or `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    #[must_use]
    pub const fn max_call_levels(&self) -> usize {
        self.limits.call_stack_depth
    }
    /// Set the maximum number of operations allowed for a script to run to avoid
    /// consuming too much resources (0 for unlimited).
    ///
    /// Not available under `unchecked`.
    #[inline(always)]
    pub fn set_max_operations(&mut self, operations: u64) -> &mut Self {
        self.limits.num_operations = NonZeroU64::new(operations);
        self
    }
    /// The maximum number of operations allowed for a script to run (0 for unlimited).
    ///
    /// Not available under `unchecked`.
    #[inline]
    #[must_use]
    pub const fn max_operations(&self) -> u64 {
        match self.limits.num_operations {
            Some(n) => n.get(),
            None => 0,
        }
    }
    /// Set the maximum number of variables allowed for a script at any instant.
    ///
    /// Not available under `unchecked`.
    #[inline(always)]
    pub fn set_max_variables(&mut self, variables: usize) -> &mut Self {
        self.limits.num_variables = variables;
        self
    }
    /// The maximum number of variables allowed for a script at any instant.
    ///
    /// Not available under `unchecked`.
    #[inline(always)]
    #[must_use]
    pub const fn max_variables(&self) -> usize {
        self.limits.num_variables
    }
    /// Set the maximum number of scripted functions allowed for a script at any instant.
    ///
    /// Not available under `unchecked` or `no_function`
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn set_max_functions(&mut self, functions: usize) -> &mut Self {
        self.limits.num_functions = functions;
        self
    }
    /// The maximum number of scripted functions allowed for a script at any instant.
    ///
    /// Not available under `unchecked` or `no_function`
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    #[must_use]
    pub const fn max_functions(&self) -> usize {
        self.limits.num_functions
    }
    /// Set the maximum number of imported [modules][crate::Module] allowed for a script.
    ///
    /// Not available under `unchecked` or `no_module`.
    #[cfg(not(feature = "no_module"))]
    #[inline(always)]
    pub fn set_max_modules(&mut self, modules: usize) -> &mut Self {
        self.limits.num_modules = modules;
        self
    }
    /// The maximum number of imported [modules][crate::Module] allowed for a script.
    ///
    /// Not available under `unchecked` or `no_module`.
    #[cfg(not(feature = "no_module"))]
    #[inline(always)]
    #[must_use]
    pub const fn max_modules(&self) -> usize {
        self.limits.num_modules
    }
    /// Set the depth limits for expressions (0 for unlimited).
    ///
    /// Not available under `unchecked`.
    #[inline(always)]
    pub fn set_max_expr_depths(
        &mut self,
        max_expr_depth: usize,
        #[cfg(not(feature = "no_function"))] max_function_expr_depth: usize,
    ) -> &mut Self {
        self.limits.expr_depth = NonZeroUsize::new(max_expr_depth);
        #[cfg(not(feature = "no_function"))]
        {
            self.limits.function_expr_depth = NonZeroUsize::new(max_function_expr_depth);
        }
        self
    }
    /// The depth limit for expressions (0 for unlimited).
    ///
    /// Not available under `unchecked`.
    #[inline]
    #[must_use]
    pub const fn max_expr_depth(&self) -> usize {
        match self.limits.expr_depth {
            Some(n) => n.get(),
            None => 0,
        }
    }
    /// The depth limit for expressions in functions (0 for unlimited).
    ///
    /// Not available under `unchecked` or `no_function`.
    #[inline]
    #[must_use]
    pub const fn max_function_expr_depth(&self) -> usize {
        #[cfg(not(feature = "no_function"))]
        return match self.limits.function_expr_depth {
            Some(n) => n.get(),
            None => 0,
        };
        #[cfg(feature = "no_function")]
        return 0;
    }
    /// Set the maximum length, in bytes, of [strings][crate::ImmutableString] (0 for unlimited).
    ///
    /// Not available under `unchecked`.
    #[inline(always)]
    pub fn set_max_string_size(&mut self, max_len: usize) -> &mut Self {
        self.limits.string_len = NonZeroUsize::new(max_len);
        self
    }
    /// The maximum length, in bytes, of [strings][crate::ImmutableString] (0 for unlimited).
    ///
    /// Not available under `unchecked`.
    #[inline]
    #[must_use]
    pub const fn max_string_size(&self) -> usize {
        match self.limits.string_len {
            Some(n) => n.get(),
            None => 0,
        }
    }
    /// Set the maximum length of [arrays][crate::Array] (0 for unlimited).
    ///
    /// Not available under `unchecked` or `no_index`.
    #[cfg(not(feature = "no_index"))]
    #[inline(always)]
    pub fn set_max_array_size(&mut self, max_size: usize) -> &mut Self {
        self.limits.array_size = NonZeroUsize::new(max_size);
        self
    }
    /// The maximum length of [arrays][crate::Array] (0 for unlimited).
    ///
    /// Not available under `unchecked` or `no_index`.
    #[inline]
    #[must_use]
    pub const fn max_array_size(&self) -> usize {
        #[cfg(not(feature = "no_index"))]
        return match self.limits.array_size {
            Some(n) => n.get(),
            None => 0,
        };
        #[cfg(feature = "no_index")]
        return 0;
    }
    /// Set the maximum size of [object maps][crate::Map] (0 for unlimited).
    ///
    /// Not available under `unchecked` or `no_object`.
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn set_max_map_size(&mut self, max_size: usize) -> &mut Self {
        self.limits.map_size = NonZeroUsize::new(max_size);
        self
    }
    /// The maximum size of [object maps][crate::Map] (0 for unlimited).
    ///
    /// Not available under `unchecked` or `no_object`.
    #[inline]
    #[must_use]
    pub const fn max_map_size(&self) -> usize {
        #[cfg(not(feature = "no_object"))]
        return match self.limits.map_size {
            Some(n) => n.get(),
            None => 0,
        };
        #[cfg(feature = "no_object")]
        return 0;
    }
}
