//! # the compliation context
//! the context is passed between the different passes of the compiler
//! and holds persisting data or other compilation information.

use std::marker::PhantomData;

use pest::iterators::Pair;
use thiserror::Error;

use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::structure::CodeModule;
use crate::compiler::structure::EnumDef;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::FunSig;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleInfo;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::OriginalFun;
use crate::compiler::structure::OriginalVar;
use crate::compiler::structure::OriginalVarRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::Set;
use crate::compiler::structure::UniqVar;
use crate::compiler::structure::VarName;
use crate::internal_bug;
use crate::ir_types::hhir::HirVar;
use crate::lang::types::EnumRef;
use crate::lang::types::Ty;
use crate::passes::parse::Rule;

/// this should not be used and is on purpose mistyped to be easily detectable.
const DEFAULT_MODULE_NAME: &str = "mAin";

pub struct CompileCtx<'run> {
    // variables
    original_variables: Vec<OriginalVar>,
    pub variable_usages: Map<OriginalVarRef, Set<Range>>,
    global_variables: Vec<UniqVar>,

    // functions
    global_functions: Vec<OriginalFun>,
    function_signatures: Map<FunRef, FunSig>,
    pub entrypoint: Option<FunRef>,

    // enums
    enum_defs: Vec<EnumDef>,
    enum_names: Map<String, EnumRef>,
    /// Interning table for ad-hoc tag-union types, keyed by sorted tag list.
    anon_tag_types: Map<Vec<String>, EnumRef>,
    /// Scratch: the module currently being built (set in `build_function`).
    cur_build_module: Option<ModuleRef>,

    // modules
    project_modules: Vec<CodeModule>,

    // defaults
    file_defaults: Map<FileRef, ModuleRef>,
    default_module: Option<ModuleRef>,

    // diagnostics
    pub diagnostics: Vec<SandDiagnostic>,

    phantom: PhantomData<&'run ()>,
}

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("use of undeclared variable: {name} at {range}")]
    UndeclaredVariable { name: VarName, range: Range },

    #[error("cannot register variable with rule {rule:?}")]
    IllegalVariableRegistration { rule: Rule },

    #[error("cannot register function with rule {rule:?}")]
    IllegalFunctionRegistration { rule: Rule },

    #[error("duplicate enum type '{name}'")]
    DuplicateEnum {
        name: String,
        first: Range,
        second: Range,
    },
}

#[derive(Debug)]
pub struct CtxEmptyError {}

impl<'run> CompileCtx<'run> {
    pub fn initial() -> Self {
        Self {
            original_variables: Default::default(),
            variable_usages: Default::default(),
            global_variables: Default::default(),
            global_functions: Default::default(),
            function_signatures: Default::default(),
            entrypoint: None,
            enum_defs: Default::default(),
            enum_names: Default::default(),
            anon_tag_types: Default::default(),
            cur_build_module: None,
            // project_config: Default::default(),
            // code_files: Vec::new(),
            project_modules: Default::default(),
            default_module: None,
            file_defaults: Default::default(),
            // default_file: None,
            diagnostics: Vec::new(),
            phantom: Default::default(),
        }
    }

    // ========================== Variables ==============================
    pub fn new_original_variable(
        &mut self,
        pair: &Pair<'_, Rule>,
        rule: Rule,
    ) -> Result<OriginalVarRef, ContextError> {
        if !matches!(rule, Rule::declaration | Rule::parameter) {
            return Err(ContextError::IllegalVariableRegistration { rule });
        }

        let ovref = OriginalVarRef(self.original_variables.len());
        let var = OriginalVar::create(pair, ovref, rule.into());
        self.original_variables.push(var);

        Ok(ovref)
    }

    pub fn uniquify_original_variable(&mut self, ovref: OriginalVarRef) -> UniqVar {
        let idx = self.global_variables.len();
        let uv = UniqVar { idx, orig: ovref };
        self.global_variables.push(uv);
        uv
    }

    pub fn original_var_name(&self, ovref: &OriginalVarRef) -> String {
        self.original_variables[ovref.0].name.name()
    }

    pub fn uniq_variable_name(&self, uv: &UniqVar) -> String {
        debug_assert!(self.global_variables.contains(uv));

        self.original_variables[uv.orig.0].name.name()
    }

    pub fn uniq_var_declaration(&self, uv: &UniqVar) -> Range {
        self.original_variables[uv.orig.0].declaration
    }

    /// Returns the friendly name of the given HIR variable,
    /// should not be used for logic, just for display
    pub fn hir_var_name(&self, hv: &HirVar) -> String {
        match hv {
            HirVar::Decl(ovref) => self.original_var_name(ovref),
            HirVar::Unqualified(name) => name.clone(),
            HirVar::Uniq(uv) => self.uniq_variable_name(uv),
        }
    }

    /// might be used later
    #[allow(dead_code)]
    fn register_variable_usage(&mut self, var: OriginalVarRef, range: Range) {
        self.variable_usages
            .entry(var)
            .and_modify(|e| {
                e.insert(range);
            })
            .or_insert(Set::from([range]));
    }

    // ============================= Functions ================================

    pub fn function_count(&self) -> usize {
        self.global_functions.len()
    }

    pub fn all_functions(&self) -> impl Iterator<Item = FunRef> + '_ {
        self.global_functions.iter().map(|f| f.index)
    }

    pub fn register_function(
        &mut self,
        pair: &Pair<'_, Rule>,
        module: &ModuleRef,
    ) -> Result<FunRef, ContextError> {
        let ofref = FunRef(self.global_functions.len());
        let fun = OriginalFun::create(pair, ofref, *module);
        self.global_functions.push(fun);

        Ok(ofref)
    }

    #[track_caller]
    pub fn original_fun_name(&self, fun: FunRef) -> String {
        debug_assert!(
            self.global_functions.len() >= fun.0,
            "{fun:?}:{:?}",
            self.global_functions
        );
        self.global_functions[fun.0].name.name()
    }

    pub fn original_fun(&self, fun: &FunRef) -> &OriginalFun {
        &self.global_functions[fun.0]
    }

    pub fn get_fun_sig(&self, fun: &FunRef) -> Option<FunSig> {
        debug_assert!(self.global_functions.len() >= fun.0);
        self.function_signatures.get(fun).cloned()
    }

    #[track_caller]
    pub fn fun_sig(&self, fun: &FunRef) -> FunSig {
        debug_assert!(self.global_functions.len() >= fun.0);
        self.function_signatures[fun].clone()
    }

    pub fn set_fun_sig(&mut self, fun: FunRef, sig: FunSig) {
        debug_assert!(self.global_functions.len() >= fun.0);
        let out = self.function_signatures.insert(fun, sig);
        debug_assert!(out.is_none());
    }

    pub fn is_main(&self, fun: FunRef) -> bool {
        self.entrypoint == Some(fun)
    }

    // ============================ Enums =====================================

    pub fn register_enum(
        &mut self,
        name: &str,
        variants: Vec<String>,
        range: Range,
        module: ModuleRef,
    ) -> Result<EnumRef, ContextError> {
        if let Some(existing) = self.enum_names.get(name) {
            return Err(ContextError::DuplicateEnum {
                name: name.to_string(),
                first: self.enum_defs[existing.0].range,
                second: range,
            });
        }
        let er = EnumRef(self.enum_defs.len());
        self.enum_defs.push(EnumDef {
            name: name.to_string(),
            variants,
            range,
            src_module: module,
            index: er,
            is_anonymous: false,
        });
        self.enum_names.insert(name.to_string(), er);
        Ok(er)
    }

    pub fn get_enum(&self, er: EnumRef) -> &EnumDef {
        &self.enum_defs[er.0]
    }

    /// returns a [`Display`]-able wrapper for [`Ty`] that resolves enum names via
    /// this context
    pub fn display_ty(&self, ty: Ty) -> TyDisplay<'_> {
        TyDisplay { ty, ctx: self }
    }

    pub fn lookup_enum_by_name(&self, name: &str) -> Option<EnumRef> {
        self.enum_names.get(name).copied()
    }

    pub fn lookup_variant(&self, er: EnumRef, variant: &str) -> Option<usize> {
        self.enum_defs[er.0]
            .variants
            .iter()
            .position(|v| v == variant)
    }

    pub fn lookup_enum_in_module(&self, module: ModuleRef, name: &str) -> Option<EnumRef> {
        self.enum_defs
            .iter()
            .enumerate()
            .find(|(_, def)| def.src_module == module && def.name == name)
            .map(|(i, _)| EnumRef(i))
    }

    /// Record the module currently being processed during AST building so that
    /// anonymous tag-union types can be attributed to the right module.
    /// Call this at the start of `build_function`.
    pub fn set_build_module(&mut self, m: ModuleRef) {
        self.cur_build_module = Some(m);
    }

    /// intern an ad-hoc tag-union type from a sorted, deduplicated list of tag
    /// names. returns the same `EnumRef` for any two call sites with the
    /// same tag set (structural identity)
    ///
    /// the resulting `EnumDef` uses a sorted `variants` list so
    /// that variant indices are stable regardless of the order in which the
    /// tags were written
    pub fn register_or_get_anon_enum(&mut self, mut tags: Vec<String>, range: Range) -> EnumRef {
        tags.sort();
        tags.dedup();
        if let Some(&er) = self.anon_tag_types.get(&tags) {
            return er;
        }
        let display_name = tags
            .iter()
            .map(|t| format!("#{t}"))
            .collect::<Vec<_>>()
            .join(" | ");
        let src_module = self
            .cur_build_module
            .or(self.default_module)
            .unwrap_or(ModuleRef(0));
        let er = EnumRef(self.enum_defs.len());
        self.enum_defs.push(EnumDef {
            name: display_name,
            variants: tags.clone(),
            range,
            src_module,
            index: er,
            is_anonymous: true,
        });
        // NOTE: we do NOT insert into `enum_names`, anonymous enums are only
        // looked up via `anon_tag_types`
        self.anon_tag_types.insert(tags, er);
        er
    }

    pub fn enum_display(&self, enum_ref: EnumRef, variant: usize) -> String {
        let variants = &self.enum_defs[enum_ref.0].variants;
        if variants.len() <= variant {
            internal_bug!("enum display indexed with oob variant: {enum_ref:?}[{variant}]");
        }
        variants[variant].clone()
    }

    // ============================ Modules ===================================
    pub fn create_dummy_module(&mut self, for_file: FileRef) -> Result<ModuleRef, CtxEmptyError> {
        if self.default_module.is_some() {
            Err(CtxEmptyError {})
        } else {
            let mr = self.register_module(DEFAULT_MODULE_NAME, for_file);
            self.default_module = Some(mr);
            Ok(mr)
        }
    }

    pub fn register_module(&mut self, name: &str, in_file: FileRef) -> ModuleRef {
        let idx = self.project_modules.len();
        let mr = ModuleRef(idx);
        let cm = CodeModule {
            index: mr,
            from_file: in_file,
            name: name.to_string(),
        };
        self.project_modules.push(cm);
        mr
    }

    pub fn create_default_module(&mut self, for_file: FileRef, name: &str) -> ModuleRef {
        let mr = self.register_module(name, for_file);
        self.file_defaults.insert(for_file, mr);
        mr
    }

    pub fn default_module(&mut self, for_file: FileRef) -> ModuleRef {
        if let Some(dm) = self.file_defaults.get(&for_file) {
            *dm
        } else {
            let name = format!("{DEFAULT_MODULE_NAME}_{}", for_file.0);
            let mr = self.register_module(&name, for_file);
            self.file_defaults.insert(for_file, mr);
            mr
        }
    }

    /// return a reference to a dummy file.
    /// this should ONLY EVER be invoked at the start of a file-less
    /// compilation.
    pub fn stub_file(&self) -> FileRef {
        assert!(self.project_modules.is_empty());
        FileRef(usize::MAX)
    }

    /// register (idempotently) the synthetic core library module and return its
    /// FileRef
    ///
    /// uses a reserved sentinel FileRef(usize::MAX - 1) that never
    /// conflicts with real files or with test stubs
    pub fn ensure_core_module(&mut self) -> FileRef {
        let core_file = FileRef(usize::MAX - 1);
        if !self.file_defaults.contains_key(&core_file) {
            self.create_default_module(core_file, "__core");
        }
        core_file
    }

    pub fn get_mod_by_name(&self, name: &str) -> Option<ModuleRef> {
        self.project_modules
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.index)
    }

    pub fn module_info(&self, mr: &ModuleRef) -> ModuleInfo {
        let cm = &self.project_modules[mr.0];
        ModuleInfo {
            name: cm.name.clone(),
            index: *mr,
        }
    }

    pub fn file_of_module(&self, mr: ModuleRef) -> FileRef {
        self.project_modules[mr.0].from_file
    }
}

/// a [`Display`]-able wrapper for [`Ty`] that resolves enum names via a
/// [`CompileCtx`]
///
/// obtain via [`CompileCtx::display_ty`]
pub struct TyDisplay<'a> {
    ty: Ty,
    ctx: &'a CompileCtx<'a>,
}

impl std::fmt::Display for TyDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            Ty::Enum(er) => write!(f, "{}", self.ctx.get_enum(er).name),
            other => write!(f, "{}", other),
        }
    }
}
