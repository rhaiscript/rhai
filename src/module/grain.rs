//! Functions in a Grain program, looked up only when called.
//!
//! A [`Module`] made from a Grain [`Program`][crate::grain::Program] registers
//! none of its functions up front. A lookup finds the function in the
//! program's own table, and the first lookup of each one makes its
//! [`ScriptFuncDef`], which every later lookup shares. A function nothing
//! calls costs one empty slot.

use super::lazy::LazySource;
use super::{FnNamespace, Module};
use crate::func::{EncapsulatedEnviron, RhaiFunc, ScriptFuncDef, ScriptFuncPayload};
use crate::grain::{Function, SharedProgram};
use crate::types::Span;
use crate::{FnAccess, ImmutableString, Shared};
use once_cell::race::OnceBox;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// The functions of one Grain program, as a [`Module`] holds them.
#[derive(Clone)]
pub(crate) struct GrainFunctions {
    program: SharedProgram,
    /// The environment a function runs in, for a module's exports. `None`
    /// for the library a run of the program itself stacks, which is already
    /// that environment.
    env: Option<Shared<EncapsulatedEnviron>>,
    /// Whether private functions are left out, as a module's exports leave
    /// them out.
    public_only: bool,
    /// Each function's definition, made on its first lookup. Shared by
    /// every [`GrainFunctions`] made from the same one, so a module's library
    /// and its exports make each definition once between them.
    defs: Shared<[OnceBox<Shared<ScriptFuncDef>>]>,
    /// Each function's hash under no namespace, made on the first lookup and
    /// shared as `defs` is.
    hashes: Shared<OnceBox<Vec<u64>>>,
}

impl GrainFunctions {
    /// Every function of `program`, private ones too, with no environment.
    #[must_use]
    pub(crate) fn new(program: &SharedProgram) -> Self {
        let defs: Vec<_> = program.functions().iter().map(|_| OnceBox::new()).collect();
        Self {
            program: program.clone(),
            env: None,
            public_only: false,
            defs: defs.into(),
            hashes: Shared::new(OnceBox::new()),
        }
    }

    /// The public functions only, run in `env`: what a module exports.
    #[cfg(not(feature = "no_module"))]
    #[must_use]
    pub(crate) fn exports(&self, env: Shared<EncapsulatedEnviron>) -> Self {
        Self {
            program: self.program.clone(),
            env: Some(env),
            public_only: true,
            defs: self.defs.clone(),
            hashes: self.hashes.clone(),
        }
    }

    /// The functions this holds, with their index in the program.
    fn iter(&self) -> impl Iterator<Item = (usize, &Function)> {
        let public_only = self.public_only;
        self.program
            .functions()
            .iter()
            .enumerate()
            .filter(move |(_, f)| !public_only || f.access == FnAccess::Public)
    }

    /// How many functions this holds.
    #[must_use]
    pub(super) fn len(&self) -> usize {
        self.iter().count()
    }

    fn name(&self, f: &Function) -> &str {
        self.program.name(f.name).unwrap_or("")
    }

    /// A method declared for a receiver type is in the global namespace, as
    /// [`Module::set_script_fn`] puts it.
    fn namespace(f: &Function) -> FnNamespace {
        #[cfg(not(feature = "no_object"))]
        if f.this_type.is_some() {
            return FnNamespace::Global;
        }
        let _ = f;
        FnNamespace::Internal
    }

    /// The hash [`Module::set_script_fn`] gives `f` under no namespace, or
    /// [`Module::build_index`] gives it under `path`.
    fn hash<'a>(&self, f: &Function, path: impl IntoIterator<Item = &'a str>) -> u64 {
        let hash = crate::calc_fn_hash(path, self.name(f), f.params.len());
        #[cfg(not(feature = "no_object"))]
        if let Some(this_type) = f.this_type.and_then(|i| self.program.name(i)) {
            return crate::calc_typed_method_hash(hash, this_type);
        }
        hash
    }

    /// Function `i`'s hash under no namespace, as [`hash`](Self::hash) gives it.
    fn unqualified_hash(&self, i: usize) -> u64 {
        self.hashes.get_or_init(|| {
            let functions = self.program.functions();
            Box::new(functions.iter().map(|f| self.hash(f, None)).collect())
        })[i]
    }

    /// Function `i`'s definition, made on the first call.
    fn def(&self, i: usize) -> &Shared<ScriptFuncDef> {
        self.defs[i].get_or_init(|| {
            let program = &self.program;
            let f = &program.functions()[i];
            let name = |index: u32| -> ImmutableString { program.name(index).unwrap_or("").into() };
            Box::new(
                ScriptFuncDef {
                    body: ScriptFuncPayload::GrainVM {
                        program: program.clone(),
                        params: f.params.iter().copied().collect(),
                        chunk: f.chunk,
                        span: Span::new(
                            program.position(f.chunk.entry() as usize),
                            program.position(f.chunk.end() as usize),
                        ),
                    },
                    name: name(f.name),
                    access: f.access,
                    #[cfg(not(feature = "no_object"))]
                    this_type: f.this_type.map(name),
                    params: f.params.iter().map(|&p| name(p)).collect(),
                    #[cfg(feature = "metadata")]
                    comments: <_>::default(),
                }
                .into(),
            )
        })
    }

    fn func(&self, i: usize) -> RhaiFunc {
        RhaiFunc::Script {
            fn_def: self.def(i).clone(),
            env: self.env.clone(),
        }
    }

    /// The hashes [`Module::set_script_fn`] gives the functions.
    pub(super) fn hashes(&self) -> impl Iterator<Item = u64> + '_ {
        self.iter().map(|(i, _)| self.unqualified_hash(i))
    }

    /// The function [`Module::set_script_fn`] would have put under `hash`.
    #[must_use]
    pub(super) fn get(&self, hash: u64) -> Option<RhaiFunc> {
        self.iter()
            .find(|&(i, _)| self.unqualified_hash(i) == hash)
            .map(|(i, _)| self.func(i))
    }

    /// The definition of the function [`Module::set_script_fn`] would have
    /// put under `hash`, and the environment it runs in.
    #[must_use]
    pub(super) fn get_script(
        &self,
        hash: u64,
    ) -> Option<(&Shared<ScriptFuncDef>, Option<&Shared<EncapsulatedEnviron>>)> {
        self.iter()
            .find(|&(i, _)| self.unqualified_hash(i) == hash)
            .map(|(i, _)| (self.def(i), self.env.as_ref()))
    }

    /// The function [`Module::build_index`] would have put under `hash`, for
    /// a module reached by `path`.
    #[must_use]
    pub(super) fn get_qualified(&self, path: &[&str], hash: u64) -> Option<RhaiFunc> {
        self.iter()
            .find(|&(i, f)| {
                self.hash(f, path.iter().copied()) == hash
                    || (Self::namespace(f) == FnNamespace::Global
                        && self.unqualified_hash(i) == hash)
            })
            .map(|(i, _)| self.func(i))
    }

    /// Whether any function is in the global namespace.
    #[must_use]
    pub(super) fn has_global(&self) -> bool {
        self.iter()
            .any(|(_, f)| Self::namespace(f) == FnNamespace::Global)
    }

    /// The first function named `name` that takes `num_params`, or any
    /// number of parameters when that is `None`.
    #[must_use]
    pub(super) fn get_named(
        &self,
        name: &str,
        num_params: Option<usize>,
    ) -> Option<&Shared<ScriptFuncDef>> {
        self.iter()
            .find(|(_, f)| num_params.map_or(true, |n| f.params.len() == n) && self.name(f) == name)
            .map(|(i, _)| self.def(i))
    }

    /// Every function: its namespace, access, name, parameter count and
    /// definition. Makes every definition.
    #[allow(dead_code)]
    pub(super) fn iter_defs(
        &self,
    ) -> impl Iterator<Item = (FnNamespace, FnAccess, &str, usize, &Shared<ScriptFuncDef>)> {
        self.iter().map(move |(i, f)| {
            (
                Self::namespace(f),
                f.access,
                self.name(f),
                f.params.len(),
                self.def(i),
            )
        })
    }

    /// Register every function into `module` as [`Module::set_script_fn`]
    /// does, in place of any it already has.
    pub(super) fn register_into(&self, module: &mut Module) {
        for (i, _) in self.iter() {
            let hash = module.set_script_fn(self.def(i).clone());
            if let Some(RhaiFunc::Script { env, .. }) = module.get_script_fn_by_hash_mut(hash) {
                env.clone_from(&self.env);
            }
        }
    }
}

impl Module {
    /// The functions of Grain programs this [`Module`] holds, looked up only
    /// when called, from the lowest precedence to the highest.
    pub(super) fn grain_functions(&self) -> impl DoubleEndedIterator<Item = &GrainFunctions> {
        self.lazy_functions.iter().flat_map(|f| f.grain())
    }

    /// Does this [`Module`] hold the functions of a Grain program, looked up
    /// only when called?
    #[must_use]
    fn has_grain_functions(&self) -> bool {
        self.grain_functions().next().is_some()
    }

    /// The number of functions of Grain programs this [`Module`] holds.
    #[must_use]
    pub(super) fn num_grain_functions(&self) -> usize {
        self.grain_functions().map(GrainFunctions::len).sum()
    }

    /// Does this [`Module`] hold a Grain function under `hash`, looked up
    /// only when called?
    #[must_use]
    pub(super) fn has_grain_fn(&self, hash: u64) -> bool {
        self.grain_functions()
            .any(|g| g.hashes().any(|h| h == hash))
    }

    /// Remove the functions registered in this [`Module`] that the Grain
    /// functions `other` holds would take precedence over once combined
    /// into it, so that lookups fall through to them.
    pub(super) fn remove_fns_hidden_by_grain(&mut self, other: &Self) {
        let Some(ref mut functions) = self.functions else {
            return;
        };
        for hash in other.grain_functions().flat_map(GrainFunctions::hashes) {
            functions.remove(&hash);
        }
    }

    /// A [`Module`] whose functions are `functions`, looked up only when
    /// called.
    #[must_use]
    pub(crate) fn from_grain_functions(functions: GrainFunctions) -> Self {
        let mut module = Self::new();
        if let Some(source) = functions.program.source() {
            module.set_id(source.clone());
        }
        module.add_grain_functions(functions);
        module.build_index();
        module
    }

    /// Hold `functions`, looked up only when called, taking precedence over
    /// any functions looked up only when called held before.
    pub(crate) fn add_grain_functions(&mut self, functions: GrainFunctions) {
        self.push_lazy_sources([LazySource::Grain(functions)]);
    }

    /// Register the Grain functions this holds as ordinary script functions.
    /// Functions already registered take precedence.
    pub(super) fn register_grain_functions(&mut self) {
        if !self.has_grain_functions() {
            return;
        }
        let sources = self.lazy_functions.take().map(|f| f.sources.into_vec());
        let functions = self.functions.take();
        let mut rest = Vec::new();
        for source in sources.into_iter().flatten() {
            match source {
                LazySource::Grain(grain) => grain.register_into(self),
                LazySource::Manifest(..) => rest.push(source),
            }
        }
        if let Some(functions) = functions {
            self.functions
                .get_or_insert_with(|| super::new_hash_map(functions.len()))
                .extend(functions);
        }
        self.push_lazy_sources(rest);
    }

    /// This [`Module`] with its Grain functions registered, cloning it if it
    /// holds any.
    #[must_use]
    pub(super) fn with_grain_registered(&self) -> std::borrow::Cow<'_, Self> {
        if self.has_grain_functions() {
            let mut module = self.clone();
            module.register_grain_functions();
            std::borrow::Cow::Owned(module)
        } else {
            std::borrow::Cow::Borrowed(self)
        }
    }
}
