//! Type-parameter substitution and unification
//! is the core of generic instantiation

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Map;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;
use crate::lang::types::TypeParamId;

/// A solved (or partially solved) set of type-parameter bindings.
pub type Subst<'tcx> = Map<TypeParamId, Ty<'tcx>>;

/// Replace every `Ty::Param` in `ty` according to `mapping`, recursing into
/// composite types and re-interning the result. Parameters absent from
/// `mapping` are left untouched.
pub fn subst<'tcx>(ctx: &mut CompileCtx<'tcx>, ty: Ty<'tcx>, mapping: &Subst<'tcx>) -> Ty<'tcx> {
    match ty.kind() {
        TyKind::Param(id) => mapping.get(id).copied().unwrap_or(ty),
        TyKind::Tuple(elems) => {
            let elems: Vec<Ty<'tcx>> = elems.iter().map(|e| subst(ctx, *e, mapping)).collect();
            ctx.intern_tuple(elems)
        }
        TyKind::App(er, args, regions) => {
            let er = *er;
            let regions = regions.to_vec();
            let args: Vec<Ty<'tcx>> = args.iter().map(|a| subst(ctx, *a, mapping)).collect();
            ctx.intern_app(er, args, regions)
        }
        // References and region ascriptions substitute their pointee/inner type
        // and keep their region. (Regions themselves are not type parameters, so
        // they are unaffected by `mapping`.)
        TyKind::Region(inner, r) => {
            let inner = subst(ctx, *inner, mapping);
            ctx.region_ty(inner, *r)
        }
        TyKind::Ref(r, inner) => {
            let inner = subst(ctx, *inner, mapping);
            ctx.ref_ty(*r, inner)
        }
        TyKind::RefMut(r, inner) => {
            let inner = subst(ctx, *inner, mapping);
            ctx.ref_mut_ty(*r, inner)
        }
        // raw pointers substitute their element type (e.g. `Ptr<T>` in a generic
        // strategy function like `unique_alloc<T>`).
        TyKind::Ptr(inner) => {
            let inner = subst(ctx, *inner, mapping);
            ctx.ptr_ty(inner)
        }
        // function types substitute their domain + codomain.
        TyKind::Fn(a, r, m, env) => {
            let a = subst(ctx, *a, mapping);
            let r = subst(ctx, *r, mapping);
            let env = subst(ctx, *env, mapping);
            ctx.closure_ty(a, r, *m, env)
        }
        // `F<A>`: substitute the arguments, then apply the constructor
        // `F` is bound to. A binding to the bare `Enum(er)` reconstructs the
        // concrete `App(er, ...)`; a binding to a *partial application* (a
        // constructor abstraction, Calculus §4.5) β-reduces; a binding to another
        // type-constructor parameter re-applies it; an absent binding leaves
        // `F<A>` parametric.
        TyKind::ParamApp(id, args) => {
            let args: Vec<Ty<'tcx>> = args.iter().map(|a| subst(ctx, *a, mapping)).collect();
            match mapping.get(id).copied() {
                Some(bound) => match bound.kind() {
                    // `F` bound to the bare constructor: every parameter is a hole,
                    // so the application args are the full argument list.
                    TyKind::Enum(er) => ctx.intern_app(*er, args, Vec::new()),
                    // `F` bound to a partial application `App(er, slots)` whose
                    // `slots` hold `Hole`s (and possibly fixed instance params):
                    // β-reduce — fill `Hole(i)` from `args[i]` and substitute the
                    // fixed slots through `mapping` (Calculus §4.5, the β rule).
                    TyKind::App(er, slots, regions) => {
                        let er = *er;
                        let regions = regions.to_vec();
                        let slots: Vec<Ty<'tcx>> = slots
                            .iter()
                            .map(|s| match s.kind() {
                                TyKind::Hole(i) => args[*i as usize],
                                _ => subst(ctx, *s, mapping),
                            })
                            .collect();
                        ctx.intern_app(er, slots, regions)
                    }
                    TyKind::Param(g) => ctx.param_app_ty(*g, args),
                    _ => crate::internal_bug!(
                        "higher-kinded parameter bound to a non-constructor: {bound}"
                    ),
                },
                None => ctx.param_app_ty(*id, args),
            }
        }
        _ => ty,
    }
}

/// Outcome of [`unify`] failing.
pub enum UnifyError {
    /// A parameter was forced to two incompatible types (e.g. `same(1, true)`
    /// where both arguments share one parameter).
    Conflict,
    /// The declared and actual shapes do not match at all.
    Mismatch,
}

/// Match a declared (possibly parametric) type against an actual concrete type,
/// accumulating parameter bindings into `mapping`. A parameter binds to the
/// actual type on first encounter; a second encounter must agree.
pub fn unify<'tcx>(
    ctx: &CompileCtx<'tcx>,
    declared: Ty<'tcx>,
    actual: Ty<'tcx>,
    mapping: &mut Subst<'tcx>,
) -> Result<(), UnifyError> {
    match (declared.kind(), actual.kind()) {
        (TyKind::Param(id), _) => match mapping.get(id) {
            Some(&bound) => {
                if bound.type_eq(actual) {
                    Ok(())
                } else {
                    Err(UnifyError::Conflict)
                }
            }
            None => {
                mapping.insert(*id, actual);
                Ok(())
            }
        },
        // A hole `_` in a declared head (Calculus §4.5) is the class-operated
        // slot: it matches anything and binds nothing. (Used when matching an
        // `impl` head abstraction against a concrete receiver in
        // `resolve_instance`.)
        (TyKind::Hole(_), _) => Ok(()),
        // A bare `Enum(Base)` head is the all-holes abstraction, so it matches any
        // application of the same constructor (binding nothing).
        (TyKind::Enum(de), TyKind::App(ae, ..)) if de == ae => Ok(()),
        (TyKind::Tuple(ds), TyKind::Tuple(acts)) if ds.len() == acts.len() => {
            for (d, a) in ds.iter().zip(*acts) {
                unify(ctx, *d, *a, mapping)?;
            }
            Ok(())
        }
        // region args are region-blind here (inferred separately); unify type args.
        (TyKind::App(de, da, _), TyKind::App(ae, aa, _)) if de == ae && da.len() == aa.len() => {
            for (d, a) in da.iter().zip(*aa) {
                unify(ctx, *d, *a, mapping)?;
            }
            Ok(())
        }
        // A higher-kinded parameter `F<...>` unifies against a concrete
        // application `Base<...>` by binding `F` to the bare `Enum(Base)`
        // constructor (head only, so two uses `F<A>`, `F<B>` agree on `F`), then
        // unifying the arguments.
        (TyKind::ParamApp(fid, da), TyKind::App(ae, aa, _)) if da.len() == aa.len() => {
            let ctor = ctx.enum_ty(*ae);
            match mapping.get(fid) {
                Some(&bound) if !bound.type_eq(ctor) => return Err(UnifyError::Conflict),
                Some(_) => {}
                None => {
                    mapping.insert(*fid, ctor);
                }
            }
            for (d, a) in da.iter().zip(*aa) {
                unify(ctx, *d, *a, mapping)?;
            }
            Ok(())
        }
        (TyKind::ParamApp(f1, da), TyKind::ParamApp(f2, aa))
            if f1 == f2 && da.len() == aa.len() =>
        {
            for (d, a) in da.iter().zip(*aa) {
                unify(ctx, *d, *a, mapping)?;
            }
            Ok(())
        }
        // References and region ascriptions unify their pointee/inner types. The
        // regions are not constrained here: they carry no type parameters, are
        // erased by monomorphisation, and call-site region inference is handled
        // separately, so `&T` unifies against `&Int` regardless of region.
        (TyKind::Ref(_, di), TyKind::Ref(_, ai)) => unify(ctx, *di, *ai, mapping),
        (TyKind::RefMut(_, di), TyKind::RefMut(_, ai)) => unify(ctx, *di, *ai, mapping),
        (TyKind::Region(di, _), TyKind::Region(ai, _)) => unify(ctx, *di, *ai, mapping),
        (TyKind::Ptr(di), TyKind::Ptr(ai)) => unify(ctx, *di, *ai, mapping),
        // `declared` is the expected type, `actual` the supplied one; the actual
        // arrow may be more permissive (arrow subsumption).
        // The env is not unified (it does not participate in arrow
        // compatibility yet); only domain + codomain + mode.
        (TyKind::Fn(da, dr, dm, _), TyKind::Fn(aa, ar, am, _)) if am.usable_as(*dm) => {
            unify(ctx, *da, *aa, mapping)?;
            unify(ctx, *dr, *ar, mapping)
        }
        _ => {
            if declared.type_eq(actual) {
                Ok(())
            } else {
                Err(UnifyError::Mismatch)
            }
        }
    }
}
