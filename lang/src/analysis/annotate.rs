use std::collections::HashSet;

use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::Expr;
use crate::ir_types::typed_hir::Expression;
use crate::ir_types::typed_hir::MatchPattern;
use crate::ir_types::typed_hir::Statement;
use crate::lang::types::FnMode;
use crate::lang::types::Ty;

/// Infer a closure's [`FnMode`] from how its body *uses* its captures, so the
/// mode reflects the actual environment discipline rather than the (defaulted)
/// arrow syntax:
///   - a non-`Copy` capture used **by value** (moved out of the env) makes the
///     closure single-use → `Consuming` (≈ `FnOnce`);
///   - a capture that is **mutated** (`&mut c`, `c = …`, `*c = …`) makes
///     calling it require exclusive access → `ReusableMut` (≈ `FnMut`);
///   - otherwise the closure only reads its captures → `Reusable` (≈ `Fn`).
///
/// `Consuming` dominates `ReusableMut` dominates `Reusable`.
pub fn closure_mode_from_body<'tcx>(
    body: &Expr<'tcx>,
    captures: &[(UniqVar<'tcx>, Ty<'tcx>)],
    is_copy: &dyn Fn(Ty<'tcx>) -> bool,
) -> FnMode {
    let movable: HashSet<UniqVar<'tcx>> = captures
        .iter()
        .filter(|(_, t)| !is_copy(*t))
        .map(|(v, _)| *v)
        .collect();
    let all: HashSet<UniqVar<'tcx>> = captures.iter().map(|(v, _)| *v).collect();
    let mut moved = false;
    let mut mutated = false;
    classify_capture_use(&body.expr, &movable, &all, &mut moved, &mut mutated);
    if moved {
        FnMode::Consuming
    } else if mutated {
        FnMode::ReusableMut
    } else {
        FnMode::Reusable
    }
}

/// Walk a closure body classifying how it uses its captures: set `moved` if a
/// non-`Copy` capture (`movable`) is used by value, `mutated` if any capture
/// (`all`) is written. A capture under `&`/`&mut` is borrowed, not moved.
fn classify_capture_use<'tcx>(
    expr: &Expression<'tcx>,
    movable: &HashSet<UniqVar<'tcx>>,
    all: &HashSet<UniqVar<'tcx>>,
    moved: &mut bool,
    mutated: &mut bool,
) {
    let mut go = |e: &Expression<'tcx>, m: &mut bool, mu: &mut bool| {
        classify_capture_use(e, movable, all, m, mu)
    };
    match expr {
        // A bare by-value use of a non-`Copy` capture moves it out of the env.
        Expression::Var(v) => {
            if movable.contains(v) {
                *moved = true;
            }
        }
        // `&c` / `&mut c` of a capture is a *borrow*, not a move; `&mut c` mutates.
        Expression::Borrow(inner, mutable) => {
            if let Expression::Var(v) = &inner.expr {
                if all.contains(v) {
                    if *mutable {
                        *mutated = true;
                    }
                    return; // do not descend into the borrowed capture
                }
            }
            go(&inner.expr, moved, mutated);
        }
        Expression::Deref(inner) => go(&inner.expr, moved, mutated),
        Expression::BinOp { left, right, .. } => {
            go(&left.expr, moved, mutated);
            go(&right.expr, moved, mutated);
        }
        Expression::UnOp { right, .. } => go(&right.expr, moved, mutated),
        Expression::If { cond, t, f } => {
            go(&cond.expr, moved, mutated);
            go(&t.expr, moved, mutated);
            go(&f.expr, moved, mutated);
        }
        Expression::While { cond, body } => {
            go(&cond.expr, moved, mutated);
            go(&body.expr, moved, mutated);
        }
        Expression::Call { args, .. }
        | Expression::IntrinsicCall { args, .. }
        | Expression::MethodCall { args, .. } => {
            for a in args {
                go(&a.expr, moved, mutated);
            }
        }
        Expression::Block {
            statements, expr, ..
        } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration { val, .. }
                    | Statement::LetTuple { val, .. }
                    | Statement::LetPattern { val, .. } => go(&val.expr, moved, mutated),
                    Statement::Assignment { name, val, .. } => {
                        if all.contains(name) {
                            *mutated = true;
                        }
                        go(&val.expr, moved, mutated);
                    }
                    Statement::DerefAssign {
                        reference, value, ..
                    } => {
                        // `*c = …` through a captured `&mut` mutates on each call.
                        if let Expression::Var(v) = &reference.expr {
                            if all.contains(v) {
                                *mutated = true;
                            }
                        }
                        go(&reference.expr, moved, mutated);
                        go(&value.expr, moved, mutated);
                    }
                    Statement::Expr(e) => go(&e.expr, moved, mutated),
                }
            }
            if let Some(e) = expr {
                go(&e.expr, moved, mutated);
            }
        }
        Expression::Constructor { payload, .. } => {
            if let Some(p) = payload {
                go(&p.expr, moved, mutated);
            }
        }
        Expression::Match { scrutinee, arms } => {
            go(&scrutinee.expr, moved, mutated);
            for arm in arms {
                go(&arm.body.expr, moved, mutated);
            }
        }
        Expression::Tuple(elems) => {
            for e in elems {
                go(&e.expr, moved, mutated);
            }
        }
        // A capture used inside a nested lambda is captured (moved) by it.
        Expression::Lambda { body, .. } => go(&body.expr, moved, mutated),
        Expression::Apply { func, arg } => {
            go(&func.expr, moved, mutated);
            go(&arg.expr, moved, mutated);
        }
        // `Closure` only exists after lambda-lifting (post-mono); mode inference
        // runs at type-check, so it is never reached here.
        Expression::Closure { .. } => {}
        Expression::Int(_) | Expression::Bool(_) | Expression::Unit => {}
    }
}

/// Visit every variable bound by `pattern`, calling `f` with the bound variable
/// and its type. The shared traversal behind the per-pass binding handlers
/// (collecting into a set here, declaring into an ownership env, ..) which only
/// differ in what they do at each `Binding` leaf.
pub fn for_each_binding<'tcx>(
    pattern: &MatchPattern<'tcx>,
    f: &mut impl FnMut(UniqVar<'tcx>, Ty<'tcx>),
) {
    match pattern {
        MatchPattern::Binding { var, ty, .. } => f(*var, *ty),
        MatchPattern::Tuple { elems, .. } => {
            for e in elems {
                for_each_binding(e, f);
            }
        }
        MatchPattern::Variant { payload, .. } => {
            if let Some((_, sub)) = payload {
                for_each_binding(sub, f);
            }
        }
        MatchPattern::Wildcard | MatchPattern::IntLit(_) | MatchPattern::BoolLit(_) => {}
    }
}

/// Collect all variable names bound by a `LetPattern`'s match pattern.
pub fn collect_let_pattern_bindings<'tcx>(pattern: &MatchPattern<'tcx>) -> HashSet<UniqVar<'tcx>> {
    let mut set = HashSet::new();
    for_each_binding(pattern, &mut |var, _| {
        set.insert(var);
    });
    set
}

pub fn get_dependencies<'tcx>(expr: &Expr<'tcx>) -> HashSet<UniqVar<'tcx>> {
    let mut dependencies = HashSet::new();
    collect_dependencies(&expr.expr, &mut dependencies);
    dependencies
}

pub fn collect_dependencies<'tcx>(
    expr: &Expression<'tcx>,
    dependencies: &mut HashSet<UniqVar<'tcx>>,
) {
    match expr {
        Expression::Var(name) => {
            dependencies.insert(*name);
        }
        Expression::Borrow(inner, _) => collect_dependencies(&inner.expr, dependencies),
        Expression::Deref(inner) => collect_dependencies(&inner.expr, dependencies),
        Expression::BinOp { left, right, .. } => {
            collect_dependencies(&left.expr, dependencies);
            collect_dependencies(&right.expr, dependencies);
        }
        Expression::UnOp { right, .. } => {
            collect_dependencies(&right.expr, dependencies);
        }
        Expression::If { cond, f, t } => {
            collect_dependencies(&cond.expr, dependencies);
            collect_dependencies(&f.expr, dependencies);
            collect_dependencies(&t.expr, dependencies);
        }
        Expression::While { cond, body } => {
            collect_dependencies(&cond.expr, dependencies);
            collect_dependencies(&body.expr, dependencies);
        }
        Expression::Call { args, .. }
        | Expression::IntrinsicCall { args, .. }
        | Expression::MethodCall { args, .. } => {
            for arg in args {
                collect_dependencies(&arg.expr, dependencies);
            }
        }
        Expression::Block {
            statements, expr, ..
        } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration { val, .. } => {
                        collect_dependencies(&val.expr, dependencies);
                    }
                    Statement::Assignment { val, .. } => {
                        collect_dependencies(&val.expr, dependencies);
                    }
                    Statement::DerefAssign {
                        reference, value, ..
                    } => {
                        collect_dependencies(&reference.expr, dependencies);
                        collect_dependencies(&value.expr, dependencies);
                    }
                    Statement::LetTuple { val, .. } | Statement::LetPattern { val, .. } => {
                        collect_dependencies(&val.expr, dependencies);
                    }
                    Statement::Expr(e) => {
                        collect_dependencies(&e.expr, dependencies);
                    }
                }
            }
            if let Some(e) = expr {
                collect_dependencies(&e.expr, dependencies);
            }
        }
        Expression::Int(_) | Expression::Bool(_) | Expression::Unit => {}
        Expression::Constructor { payload, .. } => {
            if let Some(p) = payload {
                collect_dependencies(&p.expr, dependencies);
            }
        }
        Expression::Tuple(elems) => {
            for e in elems {
                collect_dependencies(&e.expr, dependencies);
            }
        }
        Expression::Lambda { body, .. } => collect_dependencies(&body.expr, dependencies),
        Expression::Apply { func, arg } => {
            collect_dependencies(&func.expr, dependencies);
            collect_dependencies(&arg.expr, dependencies);
        }
        Expression::Closure { captures, .. } => {
            for (c, _) in captures {
                dependencies.insert(*c);
            }
        }
        Expression::Match { scrutinee, arms } => {
            collect_dependencies(&scrutinee.expr, dependencies);
            for arm in arms {
                collect_dependencies(&arm.body.expr, dependencies);
            }
        }
    }
}

pub fn get_mutations_stmt<'tcx>(stmt: &Statement<'tcx>) -> HashSet<UniqVar<'tcx>> {
    match stmt {
        Statement::Declaration { name, .. } => HashSet::from([*name]),
        Statement::Assignment { name, .. } => HashSet::from([*name]),
        // write-through mutates through a reference, not a named local.
        Statement::DerefAssign { .. } => HashSet::new(),
        Statement::LetTuple { elems, .. } => elems.iter().map(|(n, ..)| *n).collect(),
        Statement::LetPattern { pattern, .. } => collect_let_pattern_bindings(pattern),
        Statement::Expr(_) => HashSet::new(),
    }
}

pub fn get_mutations_expr<'tcx>(expr: &Expr<'tcx>) -> HashSet<UniqVar<'tcx>> {
    let mut mutations = HashSet::new();
    collect_mutations(&expr.expr, &mut mutations);
    mutations
}

fn collect_mutations<'tcx>(expr: &Expression<'tcx>, mutations: &mut HashSet<UniqVar<'tcx>>) {
    match expr {
        Expression::Block {
            statements, expr, ..
        } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration { name, .. } => {
                        mutations.insert(*name);
                    }
                    Statement::Assignment { name, .. } => {
                        mutations.insert(*name);
                    }
                    Statement::DerefAssign { .. } => {}
                    Statement::LetTuple { elems, .. } => {
                        for (name, ..) in elems {
                            mutations.insert(*name);
                        }
                    }
                    Statement::LetPattern { pattern, .. } => {
                        mutations.extend(collect_let_pattern_bindings(pattern));
                    }
                    Statement::Expr(e) => {
                        collect_mutations(&e.expr, mutations);
                    }
                }
            }
            if let Some(e) = expr {
                collect_dependencies(&e.expr, mutations);
            }
        }
        Expression::If { t, f, .. } => {
            collect_mutations(&t.expr, mutations);
            collect_mutations(&f.expr, mutations);
        }
        Expression::While { body, .. } => {
            collect_mutations(&body.expr, mutations);
        }
        Expression::Match { arms, .. } => {
            for arm in arms {
                collect_mutations(&arm.body.expr, mutations);
            }
        }
        _ => {}
    }
}
