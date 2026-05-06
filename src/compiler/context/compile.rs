//! # the compliation context
//! the context is passed between the different passes of the compiler
//! and holds persisting data or other compilation information.

use std::marker::PhantomData;

use pest::iterators::Pair;
use thiserror::Error;

use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::structure::CodeModule;
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
use crate::ir_types::hhir::HirVar;
use crate::lang::types::Ty;
use crate::passes::parse::Rule;

/// this should not be used and is on purpose mistyped to be easily detectable.
const DEFAULT_MODULE_NAME: &str = "mAin";

pub struct CompileCtx<'run> {
    // variables
    original_variables: Vec<OriginalVar>,
    pub variable_usages: Map<OriginalVarRef, Set<Range>>,
    global_variables: Vec<UniqVar>,

    variable_types: Map<UniqVar, Ty>,

    // functions
    global_functions: Vec<OriginalFun>,
    function_signatures: Map<FunRef, FunSig>,
    pub entrypoint: Option<FunRef>,

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
}

#[derive(Debug)]
pub struct CtxEmptyError {}

impl<'run> CompileCtx<'run> {
    pub fn initial() -> Self {
        Self {
            original_variables: Default::default(),
            variable_usages: Default::default(),
            global_variables: Default::default(),
            variable_types: Default::default(),
            global_functions: Default::default(),
            function_signatures: Default::default(),
            entrypoint: None,
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

    pub fn get_var_type(&self, var: &UniqVar) -> Option<Ty> {
        debug_assert!(self.global_variables.contains(var));
        self.variable_types.get(var).cloned()
    }

    #[track_caller]
    pub fn var_type(&self, var: &UniqVar) -> Ty {
        debug_assert!(self.global_variables.contains(var));
        self.variable_types[var]
    }

    pub fn set_var_type(&mut self, var: UniqVar, ty: Ty) {
        debug_assert!(self.global_variables.contains(&var));
        let out = self.variable_types.insert(var, ty);
        debug_assert!(out.is_none());
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
    pub fn dummy_file(&self) -> FileRef {
        assert!(self.project_modules.is_empty());
        FileRef(69420)
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
