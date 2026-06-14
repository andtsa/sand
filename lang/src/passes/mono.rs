//! Monomorphisation: lower a generic [`TypedProgram`] to an equivalent one with
//! no type parameters.
//!
//! Every non-generic function is a root. For each generic function and enum, a
//! specialised concrete copy is created on demand for each distinct
//! instantiation it is used with, with `Ty::Param` substituted away and
//! `Ty::App` replaced by the specialised enum's `Ty::Enum`. The result contains
//! no `Ty::Param` or `Ty::App`, so MIR lowering and codegen — which are
//! unchanged — only ever see concrete types.

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::internal_bug;
use crate::ir_types::typed_hir::Expr;
use crate::ir_types::typed_hir::Expression;
use crate::ir_types::typed_hir::MatchPattern;
use crate::ir_types::typed_hir::Parameter;
use crate::ir_types::typed_hir::Statement;
use crate::ir_types::typed_hir::TypedFunction;
use crate::ir_types::typed_hir::TypedMatchArm;
use crate::ir_types::typed_hir::TypedProgram;
use crate::lang::types::EnumRef;
use crate::lang::types::Region;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;
use crate::passes::type_ast::generics::Subst;
use crate::passes::type_ast::generics::subst;
use crate::passes::type_ast::generics::unify;

/// Replace every generic function and enum with concrete specialisations.
pub fn monomorphise<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    program: &TypedProgram<'tcx>,
) -> TypedProgram<'tcx> {
    let mut mono = Mono {
        src: program.functions.clone(),
        output: Map::new(),
        fn_instances: Map::new(),
        enum_instances: Map::new(),
    };

    // Non-generic functions are the roots; generic functions are reached only
    // through instantiated calls. Every non-generic function is preserved (no
    // dead-code elimination), keeping the entry point and core library intact.
    let roots: Vec<FunRef<'tcx>> = mono
        .src
        .iter()
        .filter(|(_, f)| f.type_params.is_empty())
        .map(|(fr, _)| *fr)
        .collect();
    for fr in roots {
        mono.request_function(ctx, fr, &Subst::new());
    }

    TypedProgram {
        functions: mono.output,
    }
}

struct Mono<'tcx> {
    /// the original (possibly generic) functions, by their original ref.
    src: Map<FunRef<'tcx>, TypedFunction<'tcx>>,
    /// the specialised, fully concrete functions.
    output: Map<FunRef<'tcx>, TypedFunction<'tcx>>,
    /// `(generic fn, type args) -> specialised fn`.
    fn_instances: Map<(FunRef<'tcx>, Vec<Ty<'tcx>>), FunRef<'tcx>>,
    /// `(generic enum, type args) -> specialised enum`.
    enum_instances: Map<(EnumRef<'tcx>, Vec<Ty<'tcx>>), EnumRef<'tcx>>,
}

impl<'tcx> Mono<'tcx> {
    /// Return the specialised function for `generic_fr` under `mapping` (its
    /// type parameters' concrete values), creating and queuing it if new. A
    /// non-generic function is specialised in place under its own ref.
    fn request_function(
        &mut self,
        ctx: &mut CompileCtx<'tcx>,
        generic_fr: FunRef<'tcx>,
        mapping: &Subst<'tcx>,
    ) -> FunRef<'tcx> {
        let f = self.src[&generic_fr].clone();
        let args: Vec<Ty<'tcx>> = f.type_params.iter().map(|p| mapping[&p.id]).collect();
        let key = (generic_fr, args.clone());
        if let Some(&spec) = self.fn_instances.get(&key) {
            return spec;
        }

        // Non-generic functions keep their ref (so the entry point and any
        // already-resolved calls stay valid); specialisations get a fresh ref.
        let spec_fr = if args.is_empty() {
            generic_fr
        } else {
            let name = mangle_fn(ctx, &ctx.original_fun_name(generic_fr), &args);
            ctx.register_mono_function(name, f.src_module, f.range)
        };
        // Insert before rewriting the body so recursive instantiations resolve.
        self.fn_instances.insert(key, spec_fr);

        let parameters: Vec<Parameter<'tcx>> = f
            .parameters
            .iter()
            .map(|p| Parameter {
                name: p.name,
                ty: self.mono_ty(ctx, p.ty, mapping),
                range: p.range,
                is_mutable: p.is_mutable,
            })
            .collect();
        let ret_type = self.mono_ty(ctx, f.ret_type, mapping);
        let body = self.rewrite_expr(ctx, &f.body, mapping);

        self.output.insert(
            spec_fr,
            TypedFunction {
                name: spec_fr,
                range: f.range,
                type_params: Vec::new(),
                region_params: Vec::new(),
                where_constraints: Vec::new(),
                type_constraints: Vec::new(),
                parameters,
                ret_type,
                body,
                src_module: f.src_module,
            },
        );
        spec_fr
    }

    /// Return the specialised enum for `base_er<args>`, creating it (and
    /// substituting its variant payloads) if new.
    fn request_enum(
        &mut self,
        ctx: &mut CompileCtx<'tcx>,
        base_er: EnumRef<'tcx>,
        args: Vec<Ty<'tcx>>,
    ) -> EnumRef<'tcx> {
        let key = (base_er, args.clone());
        if let Some(&spec) = self.enum_instances.get(&key) {
            return spec;
        }

        let base = ctx.get_enum(base_er);
        // A region-only instantiation (no type args) still needs a distinct
        // monomorphic copy, as its payloads may contain generic `App`s to specialise
        // and its borrow regions to erase, but `mangle_enum` would collide with
        // the base name, so give it a deterministic suffix. (Regions are erased,
        // so all region-instantiations of one base collapse to a single spec.)
        let mangled = if args.is_empty() {
            format!("{}$", base.name)
        } else {
            mangle_enum(ctx, base_er, &args)
        };
        let variant_names: Vec<String> = base.variants.iter().map(|v| v.name.clone()).collect();
        let base_mapping: Subst<'tcx> = base
            .type_params
            .iter()
            .map(|p| p.id)
            .zip(args.iter().copied())
            .collect();
        let raw_payloads: Vec<Option<Ty<'tcx>>> =
            base.variants.iter().map(|v| v.payload.get()).collect();
        let module = base.src_module;
        let range = base.range;

        let spec_er = ctx
            .register_enum(
                &mangled,
                variant_names,
                Vec::new(),
                Vec::new(),
                range,
                module,
            )
            .unwrap_or_else(|e| internal_bug!("mono enum registration failed: {e}"));
        // Insert before resolving payloads so recursive enums terminate.
        self.enum_instances.insert(key, spec_er);

        for (idx, raw) in raw_payloads.into_iter().enumerate() {
            if let Some(p) = raw {
                let concrete = self.mono_ty(ctx, p, &base_mapping);
                ctx.set_variant_payload(spec_er, idx, concrete);
            }
        }
        spec_er
    }

    /// Substitute parameters per `mapping` and replace every generic-enum
    /// instantiation with its specialised concrete enum. The result is free of
    /// `Ty::Param` and `Ty::App`.
    fn mono_ty(
        &mut self,
        ctx: &mut CompileCtx<'tcx>,
        ty: Ty<'tcx>,
        mapping: &Subst<'tcx>,
    ) -> Ty<'tcx> {
        match ty.kind() {
            TyKind::Param(id) => match mapping.get(id) {
                Some(&t) => t,
                None => internal_bug!("unbound type parameter {id:?} during monomorphisation"),
            },
            TyKind::Tuple(elems) => {
                let elems: Vec<Ty<'tcx>> = elems
                    .iter()
                    .map(|e| self.mono_ty(ctx, *e, mapping))
                    .collect();
                ctx.intern_tuple(elems)
            }
            // Region args are dropped: regions are compile-time, and the enum is
            // specialised on its type arguments only.
            TyKind::App(base, args, _) => {
                let base = *base;
                let args: Vec<Ty<'tcx>> = args
                    .iter()
                    .map(|a| self.mono_ty(ctx, *a, mapping))
                    .collect();
                let spec_er = self.request_enum(ctx, base, args);
                ctx.enum_ty(spec_er)
            }
            // Region ascription `T @ 'r` has no runtime representation: erase to
            // `T`. References ARE real pointers (R2): keep the `Ref`/`RefMut`
            // constructor (so codegen/MIR see "pointer to T"), canonicalising the
            // compile-time-only region to `'static` so distinct source regions
            // monomorphise to the same runtime type.
            TyKind::Region(inner, _) => self.mono_ty(ctx, *inner, mapping),
            TyKind::Ref(_, inner) => {
                let inner = self.mono_ty(ctx, *inner, mapping);
                ctx.ref_ty(Region::Static, inner)
            }
            TyKind::RefMut(_, inner) => {
                let inner = self.mono_ty(ctx, *inner, mapping);
                ctx.ref_mut_ty(Region::Static, inner)
            }
            // Raw pointers have a real runtime representation (A): keep the `Ptr`
            // constructor, substituting only the element type.
            TyKind::Ptr(inner) => {
                let inner = self.mono_ty(ctx, *inner, mapping);
                ctx.ptr_ty(inner)
            }
            _ => ty,
        }
    }

    fn rewrite_expr(
        &mut self,
        ctx: &mut CompileCtx<'tcx>,
        expr: &Expr<'tcx>,
        mapping: &Subst<'tcx>,
    ) -> Expr<'tcx> {
        let ty = self.mono_ty(ctx, expr.ty, mapping);
        let new = match &expr.expr {
            Expression::Int(i) => Expression::Int(*i),
            Expression::Bool(b) => Expression::Bool(*b),
            Expression::Unit => Expression::Unit,
            Expression::Var(v) => Expression::Var(*v),
            Expression::Borrow(inner, m) => Expression::Borrow(self.boxed(ctx, inner, mapping), *m),
            Expression::Deref(inner) => Expression::Deref(self.boxed(ctx, inner, mapping)),
            Expression::If { cond, t, f } => Expression::If {
                cond: self.boxed(ctx, cond, mapping),
                t: self.boxed(ctx, t, mapping),
                f: self.boxed(ctx, f, mapping),
            },
            Expression::While { cond, body } => Expression::While {
                cond: self.boxed(ctx, cond, mapping),
                body: self.boxed(ctx, body, mapping),
            },
            Expression::BinOp { left, op, right } => Expression::BinOp {
                left: self.boxed(ctx, left, mapping),
                op: *op,
                right: self.boxed(ctx, right, mapping),
            },
            Expression::UnOp { op, right } => Expression::UnOp {
                op: *op,
                right: self.boxed(ctx, right, mapping),
            },
            Expression::Tuple(elems) => Expression::Tuple(
                elems
                    .iter()
                    .map(|e| self.rewrite_expr(ctx, e, mapping))
                    .collect(),
            ),
            Expression::IntrinsicCall { fn_name, args } => Expression::IntrinsicCall {
                fn_name: *fn_name,
                args: args
                    .iter()
                    .map(|a| self.rewrite_expr(ctx, a, mapping))
                    .collect(),
            },
            Expression::Call { fn_name, args } => {
                let new_args: Vec<Expr<'tcx>> = args
                    .iter()
                    .map(|a| self.rewrite_expr(ctx, a, mapping))
                    .collect();
                let spec = self.specialise_callee(ctx, *fn_name, args, expr.ty, mapping);
                Expression::Call {
                    fn_name: spec,
                    args: new_args,
                }
            }
            // Deferred typeclass dispatch (Step 10b): the receiver type is now
            // concrete (the enclosing generic function has been specialised), so
            // resolve the instance and lower to a direct call of its method.
            Expression::MethodCall {
                class,
                method,
                self_ty,
                args,
            } => {
                let new_args: Vec<Expr<'tcx>> = args
                    .iter()
                    .map(|a| self.rewrite_expr(ctx, a, mapping))
                    .collect();
                let concrete = self.mono_ty(ctx, *self_ty, mapping);
                let head = ctx.type_head(concrete).unwrap_or_else(|| {
                    internal_bug!("method receiver {concrete:?} has no instance head at mono")
                });
                let fn_name = ctx
                    .lookup_instance(*class, head)
                    .and_then(|d| d.methods.get(method).copied())
                    .unwrap_or_else(|| {
                        internal_bug!("no instance for '{method}' on {concrete:?} at mono")
                    });
                Expression::Call {
                    fn_name,
                    args: new_args,
                }
            }
            Expression::Block {
                statements,
                expr,
                drops,
            } => Expression::Block {
                statements: statements
                    .iter()
                    .map(|s| self.rewrite_stmt(ctx, s, mapping))
                    .collect(),
                expr: expr.as_ref().map(|e| self.boxed(ctx, e, mapping)),
                // drops name locals, which monomorphisation does not rename.
                drops: drops.clone(),
            },
            Expression::Constructor {
                variant_idx,
                payload,
                ..
            } => {
                let payload = payload.as_ref().map(|p| self.boxed(ctx, p, mapping));
                // `ty` is the concrete enum type, from which the specialised
                // enum ref is read directly.
                let enum_ref = match ty.kind() {
                    TyKind::Enum(er) => *er,
                    _ => internal_bug!("constructor type is not an enum after mono: {ty}"),
                };
                Expression::Constructor {
                    enum_ref,
                    variant_idx: *variant_idx,
                    payload,
                }
            }
            Expression::Match { scrutinee, arms } => Expression::Match {
                scrutinee: self.boxed(ctx, scrutinee, mapping),
                arms: arms
                    .iter()
                    .map(|arm| TypedMatchArm {
                        pattern: self.rewrite_pattern(ctx, &arm.pattern, mapping),
                        body: self.rewrite_expr(ctx, &arm.body, mapping),
                        range: arm.range,
                    })
                    .collect(),
            },
        };
        Expr {
            expr: new,
            ty,
            kind: expr.kind,
            range: expr.range,
        }
    }

    fn boxed(
        &mut self,
        ctx: &mut CompileCtx<'tcx>,
        expr: &Expr<'tcx>,
        mapping: &Subst<'tcx>,
    ) -> Box<Expr<'tcx>> {
        Box::new(self.rewrite_expr(ctx, expr, mapping))
    }

    /// Resolve the specialised callee for a call. The callee's instantiation is
    /// recovered by unifying its (parametric) signature against the call's
    /// argument and result types — taken in their *original*, `App`-preserving
    /// form with the caller's own substitution applied, so the structure needed
    /// to bind the callee's parameters survives. The recovered arguments are
    /// then fully monomorphised (`App` → concrete enum) to form the
    /// specialisation key.
    fn specialise_callee(
        &mut self,
        ctx: &mut CompileCtx<'tcx>,
        callee: FunRef<'tcx>,
        orig_args: &[Expr<'tcx>],
        orig_result_ty: Ty<'tcx>,
        caller_mapping: &Subst<'tcx>,
    ) -> FunRef<'tcx> {
        // External (FFI) functions (Memory Step A) have no body to specialise;
        // they are monomorphic C symbols. Pass the callee through unchanged.
        if ctx.is_extern(callee) {
            return callee;
        }
        let f = self.src[&callee].clone();
        if f.type_params.is_empty() {
            return self.request_function(ctx, callee, &Subst::new());
        }
        let mut recovered: Subst<'tcx> = Subst::new();
        for (param, arg) in f.parameters.iter().zip(orig_args) {
            let arg_ty = subst(ctx, arg.ty, caller_mapping);
            unify(param.ty, arg_ty, &mut recovered)
                .unwrap_or_else(|_| internal_bug!("mono: argument unification failed"));
        }
        let result_ty = subst(ctx, orig_result_ty, caller_mapping);
        unify(f.ret_type, result_ty, &mut recovered)
            .unwrap_or_else(|_| internal_bug!("mono: return unification failed"));

        // The recovered bindings may still carry `App`s; lower them to concrete
        // enums so the specialisation is fully monomorphic.
        let mapping: Subst<'tcx> = recovered
            .iter()
            .map(|(id, t)| (*id, self.mono_ty(ctx, *t, &Subst::new())))
            .collect();
        self.request_function(ctx, callee, &mapping)
    }

    fn rewrite_stmt(
        &mut self,
        ctx: &mut CompileCtx<'tcx>,
        stmt: &Statement<'tcx>,
        mapping: &Subst<'tcx>,
    ) -> Statement<'tcx> {
        match stmt {
            Statement::Declaration {
                name,
                range,
                ty,
                val,
            } => Statement::Declaration {
                name: *name,
                range: *range,
                ty: self.mono_ty(ctx, *ty, mapping),
                val: self.rewrite_expr(ctx, val, mapping),
            },
            Statement::Assignment { name, range, val } => Statement::Assignment {
                name: *name,
                range: *range,
                val: self.rewrite_expr(ctx, val, mapping),
            },
            Statement::DerefAssign {
                reference,
                value,
                range,
            } => Statement::DerefAssign {
                reference: self.rewrite_expr(ctx, reference, mapping),
                value: self.rewrite_expr(ctx, value, mapping),
                range: *range,
            },
            Statement::LetTuple { elems, range, val } => Statement::LetTuple {
                elems: elems
                    .iter()
                    .map(|(n, t, m, r)| (*n, self.mono_ty(ctx, *t, mapping), *m, *r))
                    .collect(),
                range: *range,
                val: self.rewrite_expr(ctx, val, mapping),
            },
            Statement::LetPattern {
                pattern,
                val,
                else_branch,
                range,
            } => Statement::LetPattern {
                pattern: self.rewrite_pattern(ctx, pattern, mapping),
                val: self.rewrite_expr(ctx, val, mapping),
                else_branch: self.rewrite_expr(ctx, else_branch, mapping),
                range: *range,
            },
            Statement::Expr(e) => Statement::Expr(self.rewrite_expr(ctx, e, mapping)),
        }
    }

    fn rewrite_pattern(
        &mut self,
        ctx: &mut CompileCtx<'tcx>,
        pattern: &MatchPattern<'tcx>,
        mapping: &Subst<'tcx>,
    ) -> MatchPattern<'tcx> {
        match pattern {
            MatchPattern::Variant {
                ty,
                variant_idx,
                payload,
                ..
            } => {
                let ty = self.mono_ty(ctx, *ty, mapping);
                let enum_ref = match ty.kind() {
                    TyKind::Enum(er) => *er,
                    _ => internal_bug!("variant pattern type is not an enum after mono: {ty}"),
                };
                let payload = payload.as_ref().map(|(pty, sub)| {
                    (
                        self.mono_ty(ctx, *pty, mapping),
                        Box::new(self.rewrite_pattern(ctx, sub, mapping)),
                    )
                });
                MatchPattern::Variant {
                    ty,
                    enum_ref,
                    variant_idx: *variant_idx,
                    payload,
                }
            }
            MatchPattern::Tuple { ty, elems } => MatchPattern::Tuple {
                ty: self.mono_ty(ctx, *ty, mapping),
                elems: elems
                    .iter()
                    .map(|e| self.rewrite_pattern(ctx, e, mapping))
                    .collect(),
            },
            MatchPattern::Binding { var, ty, range } => MatchPattern::Binding {
                var: *var,
                ty: self.mono_ty(ctx, *ty, mapping),
                range: *range,
            },
            MatchPattern::IntLit(n) => MatchPattern::IntLit(*n),
            MatchPattern::BoolLit(b) => MatchPattern::BoolLit(*b),
            MatchPattern::Wildcard => MatchPattern::Wildcard,
        }
    }
}

/// A mangled suffix encoding a list of concrete type arguments, e.g.
/// `$Int$Bool`. Stable and collision-free for distinct instantiations.
fn mangle_args<'tcx>(ctx: &CompileCtx<'tcx>, args: &[Ty<'tcx>]) -> String {
    let mut s = String::new();
    for a in args {
        s.push('$');
        s.push_str(&mangle_ty(ctx, *a));
    }
    s
}

fn mangle_fn<'tcx>(ctx: &CompileCtx<'tcx>, name: &str, args: &[Ty<'tcx>]) -> String {
    format!("{name}{}", mangle_args(ctx, args))
}

fn mangle_enum<'tcx>(ctx: &CompileCtx<'tcx>, base: EnumRef<'tcx>, args: &[Ty<'tcx>]) -> String {
    format!("{}{}", ctx.get_enum(base).name, mangle_args(ctx, args))
}

fn mangle_ty<'tcx>(ctx: &CompileCtx<'tcx>, ty: Ty<'tcx>) -> String {
    match ty.kind() {
        TyKind::Int => "Int".to_string(),
        TyKind::Bool => "Bool".to_string(),
        TyKind::Unit => "Unit".to_string(),
        TyKind::Top => "Top".to_string(),
        TyKind::Enum(er) => ctx.get_enum(*er).name.clone(),
        TyKind::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(|e| mangle_ty(ctx, *e)).collect();
            format!("Tup{}_{}", elems.len(), inner.join("_"))
        }
        // References are real pointers (R2) and may appear as type arguments.
        TyKind::Ref(_, inner) => format!("Ref_{}", mangle_ty(ctx, *inner)),
        TyKind::RefMut(_, inner) => format!("RefMut_{}", mangle_ty(ctx, *inner)),
        // Raw pointers survive monomorphisation (A); the element type is mangled.
        TyKind::Ptr(inner) => format!("Ptr_{}", mangle_ty(ctx, *inner)),
        // `Param`/`App`/`Region` are substituted / erased before mangling.
        TyKind::Param(_) | TyKind::App(..) | TyKind::Region(..) => {
            internal_bug!("type argument is not concrete during mangling: {ty}")
        }
    }
}
