//! this pass combines all the modules in the program into one,
//! uniquifying function names across modules,
//! resolving function calls across modules,
//! calling uniquify for variables on every module,
//! and returning a single instance of qhir

use crate::compiler::context::CompileCtx;
use crate::compiler::optics::UniqPrism;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::FunSig;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleInfo;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::Set;
use crate::ir_types::hhir::HirFnCall;
use crate::ir_types::hhir::ProgramModule;
use crate::ir_types::hhir::{self};
use crate::ir_types::qhir::Program;
use crate::ir_types::qhir::{self};
use crate::lang::intrinsics::Intrinsic;
use crate::lang::types::EnumRef;
use crate::passes::qualify::error::QualifyError;

pub mod error;
pub mod uniquify;

struct QualfiyCtx<'qual, 'run> {
    available_functions: Map<ModuleRef<'run>, Set<FunRef<'run>>>,

    compile_ctx: &'qual mut CompileCtx<'run>,
}

impl<'qual, 'run> QualfiyCtx<'qual, 'run> {
    fn new(ctx: &'qual mut CompileCtx<'run>) -> Self {
        QualfiyCtx {
            available_functions: Map::new(),
            compile_ctx: ctx,
        }
    }

    fn get_function_by_name(
        &self,
        name: &str,
        in_mod: &ModuleRef<'run>,
        caller: Range,
        caller_module: &ModuleRef<'run>,
    ) -> Result<FunRef<'run>, QualifyError<'run>> {
        if let Some(fn_ref_set) = self.available_functions.get(in_mod) {
            for fr in fn_ref_set {
                if name == self.compile_ctx.original_fun_name(*fr) {
                    return Ok(*fr);
                }
            }
            Err(QualifyError::FunctionQualFailedFunctionNotFound {
                func: name.to_string(),
                range: caller,
                module: self.compile_ctx.module_info(in_mod),
                source_module: self.compile_ctx.module_info(caller_module),
            })
        } else {
            Err(QualifyError::FunctionQualFailedModuleNotFound {
                func: name.to_string(),
                module: self.compile_ctx.module_info(in_mod).name,
                source_module: self.compile_ctx.module_info(caller_module),
                range: caller,
            })
        }
    }

    /// search all modules for a function with the given name
    /// (used for global/unqualified lookup)
    fn find_function_globally(&self, name: &str) -> Option<FunRef<'run>> {
        for fn_ref_set in self.available_functions.values() {
            for fr in fn_ref_set {
                if name == self.compile_ctx.original_fun_name(*fr) {
                    return Some(*fr);
                }
            }
        }
        None
    }

    fn get_module_by_name(
        &self,
        name: &str,
        from_module: ModuleRef<'run>,
        range: Range,
    ) -> Result<ModuleRef<'run>, QualifyError<'run>> {
        self.compile_ctx
            .get_mod_by_name(name)
            .ok_or_else(|| QualifyError::ModuleNotFound {
                module: name.to_string(),
                source_module: self.compile_ctx.module_info(&from_module),
                range,
            })
    }

    /// Composed getter, second half: `(EnumRef, variant name)` ──▶
    /// `variant_idx`.
    ///
    /// Factored out of [`Self::resolve_constructor`] /
    /// [`Self::resolve_external_constructor`] so the only thing that differs
    /// between a local and an external constructor lookup — *how the
    /// `EnumRef` is found* — is the only thing duplicated.
    fn resolve_variant_idx(
        &self,
        enum_ref: EnumRef<'run>,
        display_name: &str,
        variant: &str,
        range: Range,
        module_name: &ModuleRef<'run>,
    ) -> Result<usize, QualifyError<'run>> {
        self.compile_ctx
            .lookup_variant(enum_ref, variant)
            .ok_or_else(|| QualifyError::UnknownVariant {
                type_name: display_name.to_string(),
                variant: variant.to_string(),
                range,
                source_module: self.compile_ctx.module_info(module_name),
            })
    }

    /// A **composed getter**: `(type_name, variant)` ──▶ `(EnumRef,
    /// variant_idx)`.
    ///
    /// This is the two-step lookup `lookup_enum_by_name` then `lookup_variant`
    /// that previously appeared, inline and slightly differently worded, in
    /// three separate places (`Constructor` / `ExternalConstructor` expressions
    /// and `Constructor` patterns). Bundling it into one getter is exactly
    /// `to lookupEnumByName . to lookupVariant` composition in `lens` terms —
    /// chaining two partial lookups into one:
    ///
    /// ```haskell
    /// resolveEnumAndVariant
    ///   :: (String -> Range -> ModuleInfo -> QualifyError)  -- "type not found" error
    ///   -> String -> String -> Qualify (EnumRef, Int)
    /// resolveEnumAndVariant notFound t v = do
    ///   er <- lookupEnumByName t  `orThrow` notFound t
    ///   ix <- lookupVariant er v  `orThrow` UnknownVariant t v
    ///   pure (er, ix)
    /// ```
    ///
    /// The only thing that differs between resolving a *constructor
    /// expression* and a *constructor pattern* is which `QualifyError`
    /// variant ("unknown constructor type" vs. "unknown pattern type") to
    /// raise when the type name doesn't resolve — so that one decision is
    /// taken as a parameter (`not_found`) rather than duplicating the whole
    /// chain. This is the getter analogue of passing a `Prism` in to pick
    /// which constructor to `review` on failure.
    fn resolve_enum_and_variant(
        &self,
        type_name: &str,
        variant: &str,
        range: Range,
        module_name: &ModuleRef<'run>,
        not_found: impl FnOnce(String, Range, ModuleInfo<'run>) -> QualifyError<'run>,
    ) -> Result<(EnumRef<'run>, usize), QualifyError<'run>> {
        let enum_ref = self
            .compile_ctx
            .lookup_enum_by_name(type_name)
            .ok_or_else(|| {
                not_found(
                    type_name.to_string(),
                    range,
                    self.compile_ctx.module_info(module_name),
                )
            })?;
        let variant_idx =
            self.resolve_variant_idx(enum_ref, type_name, variant, range, module_name)?;
        Ok((enum_ref, variant_idx))
    }

    /// [`Self::resolve_enum_and_variant`] specialised to expression-position
    /// constructors, which raise [`QualifyError::UnknownConstructorType`] on
    /// an unresolvable type name.
    fn resolve_constructor(
        &self,
        type_name: &str,
        variant: &str,
        range: Range,
        module_name: &ModuleRef<'run>,
    ) -> Result<(EnumRef<'run>, usize), QualifyError<'run>> {
        self.resolve_enum_and_variant(
            type_name,
            variant,
            range,
            module_name,
            |name, range, source_module| QualifyError::UnknownConstructorType {
                name,
                range,
                source_module,
            },
        )
    }

    /// The external-module variant of [`Self::resolve_constructor`]: first
    /// resolves `mod_name -> ModuleRef` (a getter in its own right,
    /// [`Self::get_module_by_name`]), then looks the enum up *scoped to that
    /// module* rather than globally — i.e. `lookup_enum_by_name` is replaced
    /// by `to (resolve mod_name) . lookup_enum_in_module`. The display name
    /// used in error messages is qualified as `module::type` to match the
    /// surface syntax the user wrote.
    fn resolve_external_constructor(
        &self,
        mod_name: &str,
        type_name: &str,
        variant: &str,
        range: Range,
        module_name: &ModuleRef<'run>,
    ) -> Result<(EnumRef<'run>, usize), QualifyError<'run>> {
        let mod_ref = self.get_module_by_name(mod_name, *module_name, range)?;
        let display_name = format!("{mod_name}::{type_name}");
        let enum_ref = self
            .compile_ctx
            .lookup_enum_in_module(mod_ref, type_name)
            .ok_or_else(|| QualifyError::UnknownConstructorType {
                name: display_name.clone(),
                range,
                source_module: self.compile_ctx.module_info(module_name),
            })?;
        let variant_idx =
            self.resolve_variant_idx(enum_ref, &display_name, variant, range, module_name)?;
        Ok((enum_ref, variant_idx))
    }
}

impl<'tcx> Program<'tcx> {
    pub fn combine<'qual>(
        ctx: &'qual mut CompileCtx<'tcx>,
        modules: Vec<ProgramModule<'tcx>>,
    ) -> Result<Self, QualifyError<'tcx>> {
        let mut q = QualfiyCtx::new(ctx);

        let mut main = None;
        for ProgramModule {
            functions,
            module_name,
        } in &modules
        {
            let mut fns = Set::new();
            let mut fn_names = Map::new();
            for f in functions {
                let name = q.compile_ctx.original_fun_name(f.name);
                if name == "main" {
                    if let Some((_, first, first_module)) = main {
                        return Err(QualifyError::DuplicateMain {
                            first,
                            second: f.range,
                            first_module: q.compile_ctx.module_info(first_module),
                            second_module: q.compile_ctx.module_info(module_name),
                        });
                    }
                    main = Some((f.name, f.range, module_name));
                }

                if let Some(fir) = fn_names.get(&name) {
                    return Err(QualifyError::DuplicateFunction {
                        name,
                        module: q.compile_ctx.module_info(module_name),
                        first_instance: *fir,
                        second_instance: f.range,
                    });
                }
                fn_names.insert(name, f.range);
                fns.insert(f.name);
            }
            q.available_functions
                .entry(*module_name)
                .or_default()
                .extend(fns);
        }

        q.compile_ctx.entrypoint = main.map(|(name, _, _)| name);

        let mut functions = Map::new();

        for pm in modules {
            let um = pm
                .uniquify(q.compile_ctx)
                .map_err(|e| QualifyError::UniquifyError {
                    module: q.compile_ctx.module_info(&pm.module_name),
                    source: e,
                })?;
            for function in um.functions {
                let qf = qualify_function(&mut q, &um.module_name, function)?;
                q.compile_ctx
                    .set_fun_sig(qf.name, FunSig::with(&qf.parameters, qf.ret_type));
                functions.insert(qf.name, qf);
            }
        }

        Ok(Self { functions })
    }
}

fn qualify_function<'tcx>(
    q: &mut QualfiyCtx<'_, 'tcx>,
    module_name: &ModuleRef<'tcx>,
    func: hhir::Function<'tcx>,
) -> Result<qhir::Function<'tcx>, QualifyError<'tcx>> {
    let parameters = func
        .parameters
        .into_iter()
        .map(|p| qualify_parameter(q, p))
        .collect::<Vec<_>>();

    let body = qualify_expr(q, module_name, func.body)?;

    Ok(qhir::Function {
        name: func.name,
        range: func.range,
        type_params: func.type_params,
        region_params: func.region_params,
        where_constraints: func.where_constraints,
        parameters,
        ret_type: func.ret_type,
        body,
        src_module: *module_name,
    })
}

fn qualify_parameter<'tcx>(
    _q: &mut QualfiyCtx<'_, 'tcx>,
    param: hhir::Parameter<'tcx>,
) -> qhir::Parameter<'tcx> {
    qhir::Parameter {
        name: UniqPrism::expect(
            param.name,
            "encountered unqualified variable after uniquify",
        ),
        range: param.range,
        ty: param.ty,
        is_mutable: param.is_mutable,
    }
}

fn qualify_expr<'tcx>(
    q: &mut QualfiyCtx<'_, 'tcx>,
    module_name: &ModuleRef<'tcx>,
    expr: hhir::Expr<'tcx>,
) -> Result<qhir::Expr<'tcx>, QualifyError<'tcx>> {
    let expression = match expr.expr {
        hhir::Expression::Bool(b) => qhir::Expression::Bool(b),
        hhir::Expression::Int(i) => qhir::Expression::Int(i),
        hhir::Expression::Unit => qhir::Expression::Unit,
        hhir::Expression::Borrow(inner, m) => {
            qhir::Expression::Borrow(Box::new(qualify_expr(q, module_name, *inner)?), m)
        }
        hhir::Expression::Deref(inner) => {
            qhir::Expression::Deref(Box::new(qualify_expr(q, module_name, *inner)?))
        }
        hhir::Expression::BinOp { left, op, right } => qhir::Expression::BinOp {
            left: Box::new(qualify_expr(q, module_name, *left)?),
            op,
            right: Box::new(qualify_expr(q, module_name, *right)?),
        },
        hhir::Expression::If { cond, t, f } => qhir::Expression::If {
            cond: Box::new(qualify_expr(q, module_name, *cond)?),
            t: Box::new(qualify_expr(q, module_name, *t)?),
            f: f.map(|e| qualify_expr(q, module_name, *e))
                .transpose()?
                .map(Box::new),
        },
        hhir::Expression::Block { statements, expr } => qhir::Expression::Block {
            statements: statements
                .into_iter()
                .map(|stmt| qualify_statement(q, module_name, stmt))
                .collect::<Result<Vec<_>, QualifyError<'tcx>>>()?,
            expr: {
                if let Some(e) = expr {
                    Some(Box::new(qualify_expr(q, module_name, *e)?))
                } else {
                    None
                }
            },
        },
        hhir::Expression::UnOp { op, right } => qhir::Expression::UnOp {
            op,
            right: Box::new(qualify_expr(q, module_name, *right)?),
        },
        hhir::Expression::While { cond, body } => qhir::Expression::While {
            cond: Box::new(qualify_expr(q, module_name, *cond)?),
            body: Box::new(qualify_expr(q, module_name, *body)?),
        },
        hhir::Expression::Var(v) => {
            let u = UniqPrism::expect(v, "unqualified variable after uniquify");
            qhir::Expression::Var(u)
        }
        hhir::Expression::Constructor {
            type_name,
            variant,
            payload,
        } => {
            let (enum_ref, variant_idx) =
                q.resolve_constructor(&type_name, &variant, expr.range, module_name)?;
            let q_payload = payload
                .map(|p| qualify_expr(q, module_name, *p))
                .transpose()?
                .map(Box::new);
            qhir::Expression::Constructor {
                enum_ref,
                variant_idx,
                payload: q_payload,
            }
        }
        hhir::Expression::ExternalConstructor {
            mod_name,
            type_name,
            variant,
            payload,
        } => {
            let (enum_ref, variant_idx) = q.resolve_external_constructor(
                &mod_name,
                &type_name,
                &variant,
                expr.range,
                module_name,
            )?;
            let q_payload = payload
                .map(|p| qualify_expr(q, module_name, *p))
                .transpose()?
                .map(Box::new);
            qhir::Expression::Constructor {
                enum_ref,
                variant_idx,
                payload: q_payload,
            }
        }
        hhir::Expression::Tag { variant, payload } => qhir::Expression::Tag {
            variant,
            payload: payload
                .map(|p| qualify_expr(q, module_name, *p))
                .transpose()?
                .map(Box::new),
        },
        hhir::Expression::Tuple(elems) => qhir::Expression::Tuple(
            elems
                .into_iter()
                .map(|e| qualify_expr(q, module_name, e))
                .collect::<Result<Vec<_>, QualifyError<'tcx>>>()?,
        ),
        hhir::Expression::Match { scrutinee, arms } => {
            let q_scrutinee = qualify_expr(q, module_name, *scrutinee)?;
            let q_arms = arms
                .into_iter()
                .map(|arm| {
                    let q_pattern = qualify_pattern(q, module_name, arm.pattern, arm.range)?;
                    let q_body = qualify_expr(q, module_name, arm.body)?;
                    Ok(qhir::QMatchArm {
                        pattern: q_pattern,
                        body: q_body,
                        range: arm.range,
                    })
                })
                .collect::<Result<Vec<_>, QualifyError<'tcx>>>()?;
            qhir::Expression::Match {
                scrutinee: Box::new(q_scrutinee),
                arms: q_arms,
            }
        }
        hhir::Expression::Call { fn_name, args } => {
            let qargs = args
                .into_iter()
                .map(|a| qualify_expr(q, module_name, a))
                .collect::<Result<Vec<_>, QualifyError<'tcx>>>()?;
            match fn_name {
                HirFnCall::Local(name) => {
                    if let Ok(intrinsic) = Intrinsic::try_from(name.as_str()) {
                        qhir::Expression::IntrinsicCall {
                            fn_name: intrinsic,
                            args: qargs,
                        }
                    } else {
                        // try the caller's own module first, then fall back to any module
                        // (this allows core library functions to be called without qualification)
                        let fn_ref = q
                            .get_function_by_name(&name, module_name, expr.range, module_name)
                            .or_else(|_| {
                                q.find_function_globally(&name).ok_or_else(|| {
                                    QualifyError::FunctionQualFailedFunctionNotFound {
                                        func: name.clone(),
                                        range: expr.range,
                                        module: q.compile_ctx.module_info(module_name),
                                        source_module: q.compile_ctx.module_info(module_name),
                                    }
                                })
                            })?;
                        qhir::Expression::Call {
                            fn_name: fn_ref,
                            args: qargs,
                        }
                    }
                }
                HirFnCall::External { module, name } => {
                    let mod_ref = q.get_module_by_name(&module, *module_name, expr.range)?;
                    let fn_ref =
                        q.get_function_by_name(&name, &mod_ref, expr.range, module_name)?;
                    qhir::Expression::Call {
                        fn_name: fn_ref,
                        args: qargs,
                    }
                }
            }
        }
    };

    Ok(qhir::Expr {
        expr: expression,
        range: expr.range,
    })
}

fn qualify_pattern<'tcx>(
    q: &mut QualfiyCtx<'_, 'tcx>,
    module_name: &ModuleRef<'tcx>,
    pattern: hhir::HirPattern<'tcx>,
    range: Range,
) -> Result<qhir::QPattern<'tcx>, QualifyError<'tcx>> {
    match pattern {
        hhir::HirPattern::Constructor {
            type_name,
            variant,
            payload,
        } => {
            let (enum_ref, variant_idx) = q.resolve_enum_and_variant(
                &type_name,
                &variant,
                range,
                module_name,
                |name, range, source_module| QualifyError::UnknownPatternType {
                    name,
                    range,
                    source_module,
                },
            )?;
            let payload = payload
                .map(|p| qualify_pattern(q, module_name, *p, range))
                .transpose()?
                .map(Box::new);
            Ok(qhir::QPattern::Variant {
                enum_ref,
                variant_idx,
                payload,
            })
        }
        hhir::HirPattern::Tag { variant, payload } => {
            let payload = payload
                .map(|p| qualify_pattern(q, module_name, *p, range))
                .transpose()?
                .map(Box::new);
            Ok(qhir::QPattern::Tag { variant, payload })
        }
        hhir::HirPattern::Tuple(elems) => Ok(qhir::QPattern::Tuple(
            elems
                .into_iter()
                .map(|p| qualify_pattern(q, module_name, p, range))
                .collect::<Result<Vec<_>, QualifyError<'tcx>>>()?,
        )),
        hhir::HirPattern::Binding { var, range: brange } => {
            let uv = UniqPrism::expect(var, "unqualified variable in pattern after uniquify");
            Ok(qhir::QPattern::Binding {
                var: uv,
                range: brange,
            })
        }
        hhir::HirPattern::IntLit(n) => Ok(qhir::QPattern::IntLit(n)),
        hhir::HirPattern::BoolLit(b) => Ok(qhir::QPattern::BoolLit(b)),
        hhir::HirPattern::Wildcard => Ok(qhir::QPattern::Wildcard),
    }
}

fn qualify_statement<'tcx>(
    q: &mut QualfiyCtx<'_, 'tcx>,
    module_name: &ModuleRef<'tcx>,
    stmt: hhir::Statement<'tcx>,
) -> Result<qhir::Statement<'tcx>, QualifyError<'tcx>> {
    match stmt {
        hhir::Statement::Assignment { name, range, val } => {
            let uv = UniqPrism::expect(name, "unqualified variable in assignment after uniquify");
            Ok(qhir::Statement::Assignment {
                name: uv,
                range,
                val: qualify_expr(q, module_name, val)?,
            })
        }
        hhir::Statement::Declaration {
            name,
            range,
            ty,
            is_mutable,
            val,
        } => {
            let uv = UniqPrism::expect(name, "unqualified variable in declaration after uniquify");
            Ok(qhir::Statement::Declaration {
                name: uv,
                ty,
                is_mutable,
                range,
                val: qualify_expr(q, module_name, val)?,
            })
        }
        hhir::Statement::LetTuple {
            elems,
            ty,
            val,
            range,
        } => Ok(qhir::Statement::LetTuple {
            elems: elems
                .into_iter()
                .map(|(name, is_mutable, elem_range)| {
                    (
                        UniqPrism::expect(name, "unqualified variable in let-tuple after uniquify"),
                        is_mutable,
                        elem_range,
                    )
                })
                .collect(),
            ty,
            val: qualify_expr(q, module_name, val)?,
            range,
        }),

        hhir::Statement::LetPattern {
            pattern,
            ty,
            val,
            else_branch,
            range,
        } => Ok(qhir::Statement::LetPattern {
            pattern: qualify_pattern(q, module_name, pattern, range)?,
            ty,
            val: qualify_expr(q, module_name, val)?,
            else_branch: qualify_expr(q, module_name, else_branch)?,
            range,
        }),

        hhir::Statement::Expr(expr) => {
            Ok(qhir::Statement::Expr(qualify_expr(q, module_name, expr)?))
        }
    }
}
