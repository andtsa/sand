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
        (TyKind::Tuple(ds), TyKind::Tuple(acts)) if ds.len() == acts.len() => {
            for (d, a) in ds.iter().zip(*acts) {
                unify(*d, *a, mapping)?;
            }
            Ok(())
        }
        // region args are region-blind here (inferred separately); unify type args.
        (TyKind::App(de, da, _), TyKind::App(ae, aa, _)) if de == ae && da.len() == aa.len() => {
            for (d, a) in da.iter().zip(*aa) {
                unify(*d, *a, mapping)?;
            }
            Ok(())
        }
        // References and region ascriptions unify their pointee/inner types. The
        // regions are not constrained here — they carry no type parameters, are
        // erased by monomorphisation, and call-site region inference is handled
        // separately — so `&T` unifies against `&Int` regardless of region.
        (TyKind::Ref(_, di), TyKind::Ref(_, ai)) => unify(*di, *ai, mapping),
        (TyKind::RefMut(_, di), TyKind::RefMut(_, ai)) => unify(*di, *ai, mapping),
        (TyKind::Region(di, _), TyKind::Region(ai, _)) => unify(*di, *ai, mapping),
        (TyKind::Ptr(di), TyKind::Ptr(ai)) => unify(*di, *ai, mapping),
        _ => {
            if declared.type_eq(actual) {
                Ok(())
            } else {
                Err(UnifyError::Mismatch)
            }
        }
    }
}
