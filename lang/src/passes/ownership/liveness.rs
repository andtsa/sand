//! Variable liveness for non-lexical loan tracking.
//!
//! A conservative liveness analysis over a function's `TypedHIR` body. It
//! assigns each expression a pre-order **program point** (keyed by its source
//! [`Range`], which is unique per node) and records, for every variable, the
//! point of its last use. A loan held by reference variable `r` can then be
//! released at `r`'s last use rather than at the end of the enclosing block
//! (lexical scope); i.e. non-lexical lifetimes.
//!
//! ## Escape analysis (soundness: lexical floor + audited exceptions)
//!
//! NLL is a *pure optimisation* layered on the sound lexical borrow check: the
//! ownership pass still releases loans at block exit by default, and a loan is
//! released *earlier* only when this analysis can positively prove it safe. The
//! default for **every** holder use is therefore "escaping" (→ stays lexical);
//! a holder is prunable only if *all* its uses fall in a small, **audited** set
//! of forms that provably cannot let the borrow outlive the holder's last use:
//!
//!   1. `*h` read whose result value is **reference-free** (carries no region):
//!      a deref reads *through* the reference; if the value read is itself a
//!      reference (`h : &&T`) it could be aliased, so that is *not* safe.
//!   2. `*h = e` write-through: writes through `h`, never extracting it.
//!
//! Everything else escapes: aliasing into a binding (`let g = h`), reborrowing
//! (`&(*h)`), passing to a call (a callee could stash it via a `&mut`
//! out-param), capturing it, returning it, putting it in a tuple/ctor. The
//! safety of (1)/(2) does **not** rely on references being non-`Copy` (the type
//! check stands even once shared refs become `Copy`), and a new IR variant
//! added later defaults to escaping (its inner `Var` uses hit the generic,
//! escaping arm), so forgetting to handle something costs *precision*, never
//! *soundness*. The precise, fully general version (region-liveness following
//! the loan through every value whose type mentions its region) is left to a
//! future MIR dataflow pass.
//!
//! Loop uses are widened to the loop's end (covering the back-edge). Every rule
//! only ever keeps a loan live *too long*, never too short.
//!
//! This is the *liveness substrate*; the loan/conflict *rules* live in
//! [`super::env`] / [`super`]. Keeping them separate is deliberate: a future
//! MIR borrow checker swaps this tree analysis for CFG dataflow while the rules
//! port unchanged.

use std::collections::HashSet;

use crate::compiler::structure::Map;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::Expr;
use crate::ir_types::typed_hir::Expression;
use crate::ir_types::typed_hir::Statement;
use crate::lang::types::Ty;

/// Last-use information for one function body.
pub struct Liveness<'tcx> {
    /// pre-order program point of each expression, keyed by its source range.
    points: Map<Range, usize>,
    /// the highest program point at which each variable is used (loop-widened).
    last_use: Map<UniqVar<'tcx>, usize>,
    /// variables that have at least one non-dereference use, so any loan they
    /// hold may have escaped and must not be pruned (see module docs).
    escaping: HashSet<UniqVar<'tcx>>,
}

impl<'tcx> Liveness<'tcx> {
    /// Analyse a function body, computing per-variable last-use points.
    pub fn analyze(body: &Expr<'tcx>) -> Self {
        let mut b = Builder {
            counter: 0,
            points: Map::new(),
            uses: Map::new(),
            escaping: HashSet::new(),
            loops: Vec::new(),
        };
        b.walk_expr(body);

        let mut last_use: Map<UniqVar<'tcx>, usize> = Map::new();
        for (v, ps) in &b.uses {
            if let Some(m) = ps.iter().copied().max() {
                last_use.insert(*v, m);
            }
        }
        // Loop-widening: a variable used inside a loop stays live to the loop's
        // last point (the back-edge may reuse it next iteration).
        for &(start, end) in &b.loops {
            for (v, ps) in &b.uses {
                if ps.iter().any(|&p| p >= start && p <= end) {
                    let lu = last_use.entry(*v).or_insert(end);
                    if end > *lu {
                        *lu = end;
                    }
                }
            }
        }

        Self {
            points: b.points,
            last_use,
            escaping: b.escaping,
        }
    }

    /// The program point of the expression at `range` (0 if unknown).
    pub fn point_of(&self, range: Range) -> usize {
        self.points.get(&range).copied().unwrap_or(0)
    }

    /// The last point at which `var` is used, or `None` if it is never used
    /// (in which case any loan it holds is dead immediately after declaration).
    pub fn last_use_of(&self, var: &UniqVar<'tcx>) -> Option<usize> {
        self.last_use.get(var).copied()
    }

    /// Whether `var` has a non-dereference use, so a loan it holds may have
    /// propagated elsewhere and must not be released early (kept lexical).
    pub fn holder_escapes(&self, var: &UniqVar<'tcx>) -> bool {
        self.escaping.contains(var)
    }
}

struct Builder<'tcx> {
    counter: usize,
    points: Map<Range, usize>,
    uses: Map<UniqVar<'tcx>, Vec<usize>>,
    escaping: HashSet<UniqVar<'tcx>>,
    loops: Vec<(usize, usize)>,
}

impl<'tcx> Builder<'tcx> {
    fn record_use(&mut self, v: UniqVar<'tcx>, point: usize, escaping: bool) {
        self.uses.entry(v).or_default().push(point);
        if escaping {
            self.escaping.insert(v);
        }
    }

    /// Record a `*v` read at `result_ty` (`*v`'s type). This is a *safe*
    /// (non-escaping) use of `v` **only** when the read value is
    /// reference-free; otherwise it could yield an aliasable reference (`v
    /// : &&T`), so `v` escapes. The check is on the *type*, not on
    /// `Copy`-ness, so it stays sound even if shared references later
    /// become `Copy` (their *purpose*).
    fn record_deref_read(&mut self, v: UniqVar<'tcx>, point: usize, result_ty: Ty<'tcx>) {
        let mut regions = Vec::new();
        result_ty.free_regions(&mut regions);
        // a reference-free value carries no region; anything with a region might
        // *be* / *contain* a reference, so conservatively escape.
        self.record_use(v, point, !regions.is_empty());
    }

    fn walk_expr(&mut self, e: &Expr<'tcx>) {
        let point = self.counter;
        self.counter += 1;
        self.points.insert(e.range, point);

        match &e.expr {
            // A bare variable reference: a non-dereference use, so it escapes.
            Expression::Var(v) => self.record_use(*v, point, true),

            // `*inner`: a read through a reference. Safe (terminal) only when
            // `inner` is a variable *and* the read value is reference-free.
            Expression::Deref(inner) => {
                if let Expression::Var(v) = &inner.expr {
                    let p = self.counter;
                    self.counter += 1;
                    self.points.insert(inner.range, p);
                    self.record_deref_read(*v, p, e.ty);
                } else {
                    self.walk_expr(inner);
                }
            }

            // `&(*h)` / `&mut (*h)` reborrows *through* `h`, producing a fresh
            // reference that aliases `h`'s referent, so `h` escapes. (A plain
            // `&x` borrows the place `x`, handled by recursing.)
            Expression::Borrow(inner, _) => {
                if let Expression::Deref(d) = &inner.expr
                    && let Expression::Var(h) = &d.expr
                {
                    self.record_use(*h, point, true);
                } else {
                    self.walk_expr(inner);
                }
            }

            Expression::If { cond, t, f } => {
                self.walk_expr(cond);
                self.walk_expr(t);
                self.walk_expr(f);
            }
            Expression::While { cond, body } => {
                let start = point;
                self.walk_expr(cond);
                self.walk_expr(body);
                let end = self.counter - 1;
                self.loops.push((start, end));
            }
            Expression::BinOp { left, right, .. } => {
                self.walk_expr(left);
                self.walk_expr(right);
            }
            Expression::UnOp { right, .. } => self.walk_expr(right),
            Expression::Call { args, .. }
            | Expression::IntrinsicCall { args, .. }
            | Expression::MethodCall { args, .. } => {
                for a in args {
                    self.walk_expr(a);
                }
            }
            Expression::Constructor { payload, .. } => {
                if let Some(p) = payload {
                    self.walk_expr(p);
                }
            }
            Expression::Tuple(elems) => {
                for el in elems {
                    self.walk_expr(el);
                }
            }
            Expression::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee);
                for arm in arms {
                    self.walk_expr(&arm.body);
                }
            }
            Expression::Block {
                statements, expr, ..
            } => {
                for s in statements {
                    self.walk_stmt(s);
                }
                if let Some(tail) = expr {
                    self.walk_expr(tail);
                }
            }
            // A captured variable is moved into the closure: it escapes.
            Expression::Lambda { body, captures, .. } => {
                for (v, _) in captures {
                    self.record_use(*v, point, true);
                }
                self.walk_expr(body);
            }
            Expression::Closure { captures, .. } => {
                for (v, _) in captures {
                    self.record_use(*v, point, true);
                }
            }
            Expression::Apply { func, arg } => {
                self.walk_expr(func);
                self.walk_expr(arg);
            }
            Expression::Int(_) | Expression::Bool(_) | Expression::Unit => {}
        }
    }

    fn walk_stmt(&mut self, s: &Statement<'tcx>) {
        match s {
            // A declaration/assignment's *name* is a write, not a use, so only
            // its initialiser is walked.
            Statement::Declaration { val, .. }
            | Statement::Assignment { val, .. }
            | Statement::LetTuple { val, .. } => self.walk_expr(val),
            // `*reference = value`: write-through is a safe terminal use of the
            // reference; it writes *through* it, never extracting/aliasing it,
            // so it's safe regardless of the pointee type.
            Statement::DerefAssign {
                reference, value, ..
            } => {
                if let Expression::Var(h) = &reference.expr {
                    let p = self.counter;
                    self.counter += 1;
                    self.points.insert(reference.range, p);
                    self.record_use(*h, p, false);
                } else {
                    self.walk_expr(reference);
                }
                self.walk_expr(value);
            }
            Statement::LetPattern {
                val, else_branch, ..
            } => {
                self.walk_expr(val);
                self.walk_expr(else_branch);
            }
            Statement::Expr(e) => self.walk_expr(e),
        }
    }
}
