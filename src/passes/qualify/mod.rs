//! this pass combines all the modules in the program into one,
//! uniquifying function names across modules,
//! resolving function calls across modules,
//! calling uniquify for variables on every module,
//! and returning a single instance of qhir

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::FunSig;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::Set;
use crate::internal_bug;
use crate::ir_types::hhir::HirFnCall;
use crate::ir_types::hhir::HirVar;
use crate::ir_types::hhir::ProgramModule;
use crate::ir_types::hhir::{self};
use crate::ir_types::qhir::Program;
use crate::ir_types::qhir::{self};
use crate::lang::intrinsics::Intrinsic;
use crate::passes::qualify::error::QualifyError;

pub mod error;
pub mod uniquify;

struct QualfiyCtx<'qual, 'run> {
    available_functions: Map<ModuleRef, Set<FunRef>>,

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
        in_mod: &ModuleRef,
        caller: Range,
        caller_module: &ModuleRef,
    ) -> Result<FunRef, QualifyError> {
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

    fn get_module_by_name(
        &self,
        name: &str,
        from_module: ModuleRef,
        range: Range,
    ) -> Result<ModuleRef, QualifyError> {
        self.compile_ctx
            .get_mod_by_name(name)
            .ok_or_else(|| QualifyError::ModuleNotFound {
                module: name.to_string(),
                source_module: self.compile_ctx.module_info(&from_module),
                range,
            })
    }
}

impl Program {
    pub fn combine<'qual, 'run>(
        ctx: &'qual mut CompileCtx<'run>,
        modules: Vec<ProgramModule>,
    ) -> Result<Self, QualifyError> {
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
            if q.available_functions.contains_key(module_name) {
                return Err(QualifyError::DuplicateModule(
                    q.compile_ctx.module_info(module_name),
                ));
            }
            q.available_functions.insert(*module_name, fns);
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

fn qualify_function(
    q: &mut QualfiyCtx<'_, '_>,
    module_name: &ModuleRef,
    func: hhir::Function,
) -> Result<qhir::Function, QualifyError> {
    let parameters = func
        .parameters
        .into_iter()
        .map(|p| qualify_parameter(q, p))
        .collect::<Vec<_>>();

    let body = qualify_expr(q, module_name, func.body)?;

    Ok(qhir::Function {
        name: func.name,
        range: func.range,
        parameters,
        ret_type: func.ret_type,
        body,
        src_module: *module_name,
    })
}

fn qualify_parameter(_q: &mut QualfiyCtx<'_, '_>, param: hhir::Parameter) -> qhir::Parameter {
    let HirVar::Uniq(u) = param.name else {
        internal_bug!(
            "encountered unqualified variable after uniquify: {:?}",
            param.name
        );
    };
    qhir::Parameter {
        name: u,
        range: param.range,
        ty: param.ty,
    }
}

fn qualify_expr(
    q: &mut QualfiyCtx<'_, '_>,
    module_name: &ModuleRef,
    expr: hhir::Expr,
) -> Result<qhir::Expr, QualifyError> {
    let expression = match expr.expr {
        hhir::Expression::Bool(b) => qhir::Expression::Bool(b),
        hhir::Expression::Int(i) => qhir::Expression::Int(i),
        hhir::Expression::Unit => qhir::Expression::Unit,
        hhir::Expression::BinOp { left, op, right } => qhir::Expression::BinOp {
            left: Box::new(qualify_expr(q, module_name, *left)?),
            op,
            right: Box::new(qualify_expr(q, module_name, *right)?),
        },
        hhir::Expression::If { cond, t, f } => qhir::Expression::If {
            cond: Box::new(qualify_expr(q, module_name, *cond)?),
            t: Box::new(qualify_expr(q, module_name, *t)?),
            f: Box::new(qualify_expr(q, module_name, *f)?),
        },
        hhir::Expression::Block { statements, expr } => qhir::Expression::Block {
            statements: statements
                .into_iter()
                .map(|stmt| qualify_statement(q, module_name, stmt))
                .collect::<Result<Vec<_>, QualifyError>>()?,
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
            let HirVar::Uniq(u) = v else {
                internal_bug!("unqualified variable after uniquify: {v:?}");
            };
            qhir::Expression::Var(u)
        }
        hhir::Expression::Call { fn_name, args } => {
            let qargs = args
                .into_iter()
                .map(|a| qualify_expr(q, module_name, a))
                .collect::<Result<Vec<_>, QualifyError>>()?;
            match fn_name {
                HirFnCall::Local(name) => {
                    if let Ok(intrinsic) = Intrinsic::try_from(name.as_str()) {
                        qhir::Expression::IntrinsicCall {
                            fn_name: intrinsic,
                            args: qargs,
                        }
                    } else {
                        // need to find the function we're calling
                        let fn_ref =
                            q.get_function_by_name(&name, module_name, expr.range, module_name)?;
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

fn qualify_statement(
    q: &mut QualfiyCtx<'_, '_>,
    module_name: &ModuleRef,
    stmt: hhir::Statement,
) -> Result<qhir::Statement, QualifyError> {
    match stmt {
        hhir::Statement::Assignment { name, range, val } => {
            let HirVar::Uniq(uv) = name else {
                internal_bug!("unqualified variable in assignment after uniquify: {name:?}");
            };
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
            val,
        } => {
            let HirVar::Uniq(uv) = name else {
                internal_bug!("unqualified variable in declaration after uniquify: {name:?}");
            };
            Ok(qhir::Statement::Declaration {
                name: uv,
                ty,
                range,
                val: qualify_expr(q, module_name, val)?,
            })
        }
        hhir::Statement::Expr(expr) => {
            Ok(qhir::Statement::Expr(qualify_expr(q, module_name, expr)?))
        }
    }
}
