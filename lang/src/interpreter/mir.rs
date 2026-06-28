//! an interpreter for the MIR
//!
//! ## Store model
//!
//! Faithful to the Calculus's `BorrowedMut` semantics: a mutable borrow denotes
//! a *storage location*, not a copied value, and a write through it mutates
//! that location observably to every alias, exactly what the LLVM backend does
//! with `alloca` slots + `load`/`store`.
//!
//! We model storage as a graph of mutable **cells**. Each local owns a cell; a
//! reference value ([`MirValue::Ref`]) is a *shared handle* to a cell. `&place`
//! ([`RValue::Ref`]) yields the cell; a `[Deref]` projection
//! ([`ProjElem::Deref`]) follows the handle; reads load the cell, writes store
//! into it. Because a reference handle is passed by value into a callee's
//! parameter cell, the callee's `*r = e` mutates the *caller's* storage:
//! cross-frame write-through, just like a real pointer. The `Rc` is a
//! meta-level implementation detail of the interpreter, not language-level GC:
//! the static region/escape checker already guarantees no dangling, so the
//! interpreter never models deallocation.

use std::cell::RefCell;
use std::rc::Rc;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::internal_bug;
use crate::ir_types::mir::*;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::Bop;
use crate::lang::ops::CompOp;
use crate::lang::ops::Uop;
use crate::lang::types::AdtRef;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;

/// A storage cell: shared, interior-mutable, possibly uninitialised. A local
/// owns one; a [`MirValue::Ref`] is a shared handle to one.
type Cell<'tcx> = Rc<RefCell<Option<MirValue<'tcx>>>>;

#[derive(Debug, Clone, PartialEq)]
pub enum MirValue<'tcx> {
    Int(i64),
    Bool(bool),
    Unit,
    EnumVariant {
        enum_ref: AdtRef<'tcx>,
        variant_idx: usize,
        payload: Option<Box<MirValue<'tcx>>>,
    },
    Tuple(Vec<MirValue<'tcx>>),
    /// A reference: a shared handle to a storage location (a root cell plus a
    /// path of field indices into the value it holds). An empty path points at
    /// the whole cell (`&x`, raw pointers); a non-empty path points at an
    /// interior field (`&(*r).field`, produced by a borrowing `match`). Reads
    /// and writes through the reference navigate the path into the *live*
    /// aggregate, so a `&mut` field borrow mutates the original, never a
    /// snapshot.
    Ref(Cell<'tcx>, Vec<usize>),
    /// A closure value: the lifted function and a pointer to its captured
    /// environment (a `Ref` to a cell holding the env value: `Unit`,
    /// the single capture, or a tuple). Consumed by `CallIndirect`, which
    /// passes the env pointer as the lifted function's leading argument.
    Closure {
        fn_name: crate::compiler::structure::FunRef<'tcx>,
        env: Box<MirValue<'tcx>>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum MirInterpError {
    #[error("no entry point found")]
    NoEntryPoint,
    #[error("uninitialized local: {0:?}")]
    UninitializedLocal(LocalId),
    #[error("division by zero")]
    DivisionByZero,
    #[error("reached unreachable terminator")]
    Unreachable,
    #[error("runtime error: {0}")]
    Runtime(String),
}

impl<'tcx> MirProgram<'tcx> {
    pub fn interpret(&self, ctx: &CompileCtx<'tcx>) -> Result<MirValue<'tcx>, MirInterpError> {
        // find the main function
        let (main_ref, _main_fn) = self
            .functions
            .iter()
            .find(|(fr, _)| ctx.is_main(**fr))
            .ok_or(MirInterpError::NoEntryPoint)?;

        self.call_function(*main_ref, &[], ctx)
    }

    fn call_function(
        &self,
        fun: FunRef<'tcx>,
        args: &[MirValue<'tcx>],
        ctx: &CompileCtx<'tcx>,
    ) -> Result<MirValue<'tcx>, MirInterpError> {
        // External (FFI) functions have no MIR body; dispatch
        // the known C symbols to simulated-heap built-ins.
        if ctx.is_extern(fun) {
            let symbol = ctx
                .extern_symbol(fun)
                .expect("extern fn has a registered symbol");
            return eval_extern(symbol, args);
        }
        let func = &self.functions[&fun];

        // initialise locals: one fresh, distinct cell per local, all unset.
        let locals: Vec<Cell<'tcx>> = (0..func.locals.len())
            .map(|_| Rc::new(RefCell::new(None)))
            .collect();

        // bind parameters by storing the arg value into the param's cell. A
        // `MirValue::Ref` arg shares its `Rc` into the param cell, so the callee
        // points at the *caller's* storage (cross-frame write-through).
        for (param, val) in func.params.iter().zip(args.iter()) {
            *locals[param.local.0].borrow_mut() = Some(val.clone());
        }

        // execute blocks
        let mut current = func.entry;
        loop {
            let block = &func.blocks[current.0];

            for stmt in &block.statements {
                execute_statement(stmt, &locals, &func.locals, self, ctx)?;
            }

            match &block.terminator {
                Terminator::Goto { target } => {
                    current = *target;
                }
                Terminator::Branch {
                    cond,
                    then_bb,
                    else_bb,
                } => match eval_operand(cond, &locals)? {
                    MirValue::Bool(true) => current = *then_bb,
                    MirValue::Bool(false) => current = *else_bb,
                    v => internal_bug!("branch condition evaluated to non-bool: {:?}", v),
                },
                Terminator::Return { value: Some(op) } => {
                    return eval_operand(op, &locals);
                }
                Terminator::Return { value: None } => {
                    return Ok(MirValue::Unit);
                }
                Terminator::Unreachable => {
                    return Err(MirInterpError::Unreachable);
                }
            }
        }
    }
}

fn execute_statement<'tcx>(
    stmt: &Statement<'tcx>,
    locals: &[Cell<'tcx>],
    local_decls: &[LocalDecl<'tcx>],
    prog: &MirProgram<'tcx>,
    ctx: &CompileCtx<'tcx>,
) -> Result<(), MirInterpError> {
    match stmt {
        Statement::Assign { dst, value, .. } => {
            // The result type is the *place* type: each `[Deref]` strips a
            // reference, so a write-through `*r = e` reconstructs an aggregate at
            // the pointee type, not the reference type.
            let dst_ty = place_ty(dst, local_decls);
            let v = eval_rvalue(value, dst_ty, locals, prog, ctx)?;
            // store into the location the place names (a `[Deref]` follows the
            // reference held in the local; a `[Field]` targets an interior field;
            // write-through mutates the live aggregate).
            write_loc(&resolve_place(dst, locals)?, v);
            Ok(())
        }
        Statement::Eval { value, .. } => {
            // Eval is only for side-effecting calls: Aggregate/Field never
            // appear here, so the result_ty is irrelevant; use UNIT as dummy.
            eval_rvalue(value, ctx.types.unit, locals, prog, ctx)?;
            Ok(())
        }
        // Drop is a no-op for types without destructors; the cell's `Rc`
        // reclaims storage when it falls out of scope.
        Statement::Drop { .. } => Ok(()),
    }
}

/// A storage *location*: a root cell plus a path of field indices into the
/// value it holds. The interpreter's value model nests an aggregate's fields
/// inside one cell rather than giving each its own, so an interior place can't
/// be a bare cell; it's a cell + a path navigated live on each read/write.
type Loc<'tcx> = (Cell<'tcx>, Vec<usize>);

/// Resolve a [`Place`] to the [`Loc`] it names: each `[Deref]` follows the
/// reference at the current location (composing its path), each `[Field(i)]`
/// extends the path.
fn resolve_place<'tcx>(place: &Place, locals: &[Cell<'tcx>]) -> Result<Loc<'tcx>, MirInterpError> {
    let mut cell = locals[place.local.0].clone();
    let mut path: Vec<usize> = Vec::new();
    for elem in &place.projection {
        match elem {
            ProjElem::Deref => {
                let next = {
                    let guard = cell.borrow();
                    let root = guard
                        .as_ref()
                        .ok_or(MirInterpError::UninitializedLocal(place.local))?;
                    match nav(root, &path) {
                        MirValue::Ref(c, p) => (c.clone(), p.clone()),
                        v => internal_bug!("Deref projection on non-reference value {:?}", v),
                    }
                };
                cell = next.0;
                path = next.1;
            }
            ProjElem::Field(i) => path.push(*i),
        }
    }
    Ok((cell, path))
}

/// Navigate `path` (a sequence of field indices) into `v`, returning the
/// sub-value it names (tuple element `i`, or an enum's payload at index 1).
fn nav<'a, 'tcx>(v: &'a MirValue<'tcx>, path: &[usize]) -> &'a MirValue<'tcx> {
    let mut cur = v;
    for &i in path {
        cur = match cur {
            MirValue::Tuple(elems) => &elems[i],
            MirValue::EnumVariant {
                payload: Some(p), ..
            } if i == 1 => p,
            other => internal_bug!("field {} of non-aggregate value {:?}", i, other),
        };
    }
    cur
}

/// Mutable counterpart of [`nav`].
fn nav_mut<'a, 'tcx>(v: &'a mut MirValue<'tcx>, path: &[usize]) -> &'a mut MirValue<'tcx> {
    let mut cur = v;
    for &i in path {
        cur = match cur {
            MirValue::Tuple(elems) => &mut elems[i],
            MirValue::EnumVariant {
                payload: Some(p), ..
            } if i == 1 => p.as_mut(),
            _ => internal_bug!("field {i} projection on a non-aggregate value"),
        };
    }
    cur
}

/// Read the value at a location (cloned).
fn read_loc<'tcx>(loc: &Loc<'tcx>) -> Result<MirValue<'tcx>, MirInterpError> {
    let guard = loc.0.borrow();
    let root = guard
        .as_ref()
        .ok_or_else(|| MirInterpError::Runtime("read of uninitialised location".into()))?;
    Ok(nav(root, &loc.1).clone())
}

/// Write `val` to the location: replace the whole cell for an empty path, or
/// the named interior field otherwise (mutating the live aggregate in place).
fn write_loc<'tcx>(loc: &Loc<'tcx>, val: MirValue<'tcx>) {
    let mut guard = loc.0.borrow_mut();
    if loc.1.is_empty() {
        *guard = Some(val);
    } else {
        let root = guard
            .as_mut()
            .unwrap_or_else(|| internal_bug!("interior write into an uninitialised cell"));
        *nav_mut(root, &loc.1) = val;
    }
}

/// The `Ty` of a place after its projections (each `[Deref]` strips one
/// reference, yielding the pointee type). Mirrors `llvm_codegen::place_ty`.
fn place_ty<'tcx>(place: &Place, local_decls: &[LocalDecl<'tcx>]) -> Ty<'tcx> {
    let mut ty = local_decls[place.local.0].ty;
    for elem in &place.projection {
        match elem {
            ProjElem::Deref => {
                ty = match ty.kind() {
                    TyKind::Ref(_, t) | TyKind::RefMut(_, t) => *t,
                    _ => internal_bug!("Deref projection on non-reference type {ty:?}"),
                };
            }
            ProjElem::Field(i) => {
                ty = match ty.kind() {
                    TyKind::Tuple(tys) => tys[*i],
                    TyKind::Enum(_) | TyKind::App(..) if *i == 0 => {
                        // discriminant; the interpreter is layout-free, but mirror
                        // codegen's `proj_field_ty`.
                        ty
                    }
                    _ => internal_bug!("no field type for index {i} of {ty:?}"),
                };
            }
        }
    }
    ty
}

fn eval_rvalue<'tcx>(
    rv: &RValue<'tcx>,
    result_ty: Ty<'tcx>,
    locals: &[Cell<'tcx>],
    prog: &MirProgram<'tcx>,
    ctx: &CompileCtx<'tcx>,
) -> Result<MirValue<'tcx>, MirInterpError> {
    match rv {
        RValue::Use(op) => eval_operand(op, locals),

        // `size_of::<T>()`: a layout-free approximation here, since the
        // interpreter's heap is a cell graph, so the exact byte size is
        // irrelevant (codegen computes the real one).
        RValue::SizeOf(ty) => Ok(MirValue::Int(crate::lang::intrinsics::interp_size_of(*ty))),

        // Address-of: yield a shared handle to the cell the place names (not a
        // copy of its value). Reads/writes through a `[Deref]` of this handle hit
        // that same cell, so write-through is observable across aliases.
        RValue::Ref(place) => {
            let (cell, path) = resolve_place(place, locals)?;
            Ok(MirValue::Ref(cell, path))
        }

        RValue::BinaryOp { op, left, right } => {
            let l = eval_operand(left, locals)?;
            let r = eval_operand(right, locals)?;
            eval_binop(*op, l, r)
        }

        RValue::UnaryOp { op, right } => {
            let v = eval_operand(right, locals)?;
            eval_unop(*op, v)
        }

        RValue::Call { fn_name, args } => {
            let arg_vals = args
                .iter()
                .map(|a| eval_operand(a, locals))
                .collect::<Result<Vec<_>, _>>()?;
            prog.call_function(*fn_name, &arg_vals, ctx)
        }

        RValue::IntrinsicCall { fn_name, args } => {
            let arg_vals = args
                .iter()
                .map(|a| eval_operand(a, locals))
                .collect::<Result<Vec<_>, _>>()?;
            eval_intrinsic(*fn_name, arg_vals, ctx)
        }

        // A closure value: pack the captured operands into the env
        // value (Unit / single / tuple), store it in a fresh cell, and keep a
        // pointer to it (so the lifted function can `__ptr_read` it).
        RValue::Closure { fn_name, env } => {
            let mut vals = env
                .iter()
                .map(|o| eval_operand(o, locals))
                .collect::<Result<Vec<_>, _>>()?;
            let env_value = match vals.len() {
                0 => MirValue::Unit,
                1 => vals.pop().unwrap(),
                _ => MirValue::Tuple(vals),
            };
            let env_ptr = MirValue::Ref(Rc::new(RefCell::new(Some(env_value))), Vec::new());
            Ok(MirValue::Closure {
                fn_name: *fn_name,
                env: Box::new(env_ptr),
            })
        }

        // Indirect call: evaluate the callee to a closure, then call
        // its lifted function with the env pointer followed by the arguments.
        RValue::CallIndirect { callee, args } => {
            let MirValue::Closure { fn_name, env } = eval_operand(callee, locals)? else {
                internal_bug!("indirect call of a non-closure value")
            };
            let mut arg_vals = vec![*env];
            for a in args {
                arg_vals.push(eval_operand(a, locals)?);
            }
            prog.call_function(fn_name, &arg_vals, ctx)
        }

        RValue::Aggregate(fields) => {
            let vals: Vec<MirValue<'tcx>> = fields
                .iter()
                .map(|f| eval_operand(f, locals))
                .collect::<Result<_, _>>()?;

            // Recover semantic type from the destination local's type (passed
            // in as `result_ty`) to reconstruct a rich `MirValue` for display.
            match result_ty.kind() {
                TyKind::Enum(enum_ref) => {
                    // field 0 = discriminant (Int), field 1 = payload (if any)
                    let MirValue::Int(disc) = vals[0] else {
                        internal_bug!("enum aggregate: field 0 is not Int (discriminant)")
                    };
                    let variant_idx = disc as usize;
                    let payload = vals.into_iter().nth(1).map(Box::new);
                    Ok(MirValue::EnumVariant {
                        enum_ref: *enum_ref,
                        variant_idx,
                        payload,
                    })
                }
                TyKind::Tuple(_) => Ok(MirValue::Tuple(vals)),
                _ => internal_bug!(
                    "Aggregate rvalue with non-aggregate destination type {:?}",
                    result_ty
                ),
            }
        }

        RValue::Field { base, index } => {
            let v = eval_operand(base, locals)?;
            match (v, *index) {
                // enum: field 0 is the discriminant, field 1 is the payload
                (MirValue::EnumVariant { variant_idx, .. }, 0) => {
                    Ok(MirValue::Int(variant_idx as i64))
                }
                (
                    MirValue::EnumVariant {
                        payload: Some(p), ..
                    },
                    1,
                ) => Ok(*p),
                (MirValue::EnumVariant { payload: None, .. }, 1) => {
                    internal_bug!("payload field (index 1) accessed on a nullary enum variant")
                }
                // tuple: field i is element i
                (MirValue::Tuple(elems), i) if i < elems.len() => {
                    Ok(elems.into_iter().nth(i).unwrap())
                }
                (v, i) => internal_bug!(
                    "Field[{}] applied to incompatible value {:?} \
                     (type checker / lowering should have prevented this)",
                    i,
                    v
                ),
            }
        }
    }
}

fn eval_operand<'tcx>(
    op: &Operand,
    locals: &[Cell<'tcx>],
) -> Result<MirValue<'tcx>, MirInterpError> {
    match op {
        Operand::Const(c) => Ok(match c {
            Constant::Int(i) => MirValue::Int(*i),
            Constant::Bool(b) => MirValue::Bool(*b),
            Constant::Unit => MirValue::Unit,
        }),
        // Read the location the place names. A `[Deref]` loads *through* the
        // reference held in the local (the inverse of `RValue::Ref`); a `[Field]`
        // reads an interior field of the live aggregate.
        Operand::Copy(place) => resolve_place(place, locals).and_then(|loc| read_loc(&loc)),
    }
}

fn eval_binop<'tcx>(
    op: Bop,
    l: MirValue<'tcx>,
    r: MirValue<'tcx>,
) -> Result<MirValue<'tcx>, MirInterpError> {
    match (op, l, r) {
        (Bop::Plus, MirValue::Int(a), MirValue::Int(b)) => {
            Ok(MirValue::Int(a.overflowing_add(b).0))
        }
        (Bop::Minus, MirValue::Int(a), MirValue::Int(b)) => {
            Ok(MirValue::Int(a.overflowing_sub(b).0))
        }
        (Bop::Mult, MirValue::Int(a), MirValue::Int(b)) => {
            Ok(MirValue::Int(a.overflowing_mul(b).0))
        }
        (Bop::Div, MirValue::Int(a), MirValue::Int(b)) => {
            if b == 0 {
                Err(MirInterpError::DivisionByZero)
            } else {
                Ok(MirValue::Int(a / b))
            }
        }
        (Bop::Pow, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a.pow(b as u32))),
        (Bop::BitAnd, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a & b)),
        (Bop::Or, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a | b)),
        (Bop::Xor, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a ^ b)),
        (Bop::And, MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a && b)),
        (Bop::Or, MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a || b)),
        (Bop::Xor, MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a ^ b)),
        (Bop::Comp(cop), MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Bool(match cop {
            CompOp::Eq => a == b,
            CompOp::Ne => a != b,
            CompOp::Lt => a < b,
            CompOp::Le => a <= b,
            CompOp::Gt => a > b,
            CompOp::Ge => a >= b,
        })),
        (Bop::Comp(CompOp::Eq), MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a == b)),
        (Bop::Comp(CompOp::Ne), MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a != b)),
        (
            Bop::Comp(cop @ (CompOp::Eq | CompOp::Ne)),
            MirValue::EnumVariant {
                enum_ref: er_l,
                variant_idx: vi_l,
                payload: pl_l,
            },
            MirValue::EnumVariant {
                enum_ref: er_r,
                variant_idx: vi_r,
                payload: pl_r,
            },
        ) => {
            // Tag equality is always required. Payloads are compared only
            // when *both* sides actually carry one. This is sound because a
            // variant's "payload-carrying-ness" is fixed by its declaration:
            // real values of a payload variant are always `Some`, so `None`
            // only ever appears for nullary variants (where both sides agree).
            let eq = er_l == er_r
                && vi_l == vi_r
                && match (&pl_l, &pl_r) {
                    (Some(a), Some(b)) => a == b,
                    _ => true,
                };
            Ok(MirValue::Bool(match cop {
                CompOp::Eq => eq,
                CompOp::Ne => !eq,
                _ => unreachable!(),
            }))
        }
        (Bop::Comp(CompOp::Eq), l @ MirValue::Tuple(_), r @ MirValue::Tuple(_)) => {
            Ok(MirValue::Bool(l == r))
        }
        (Bop::Comp(CompOp::Ne), l @ MirValue::Tuple(_), r @ MirValue::Tuple(_)) => {
            Ok(MirValue::Bool(l != r))
        }
        (op, l, r) => internal_bug!("type error in MIR binop: {:?} {:?} {:?}", l, op, r),
    }
}

fn eval_unop<'tcx>(op: Uop, v: MirValue<'tcx>) -> Result<MirValue<'tcx>, MirInterpError> {
    match (op, v) {
        (Uop::Neg, MirValue::Int(i)) => Ok(MirValue::Int(-i)),
        (Uop::Not, MirValue::Bool(b)) => Ok(MirValue::Bool(!b)),
        (Uop::Not, MirValue::Int(i)) => Ok(MirValue::Int(!i)),
        (op, v) => internal_bug!("type error in MIR unop: {:?} {:?}", op, v),
    }
}

/// Execute a known external (FFI) function in the interpreter.
/// There is no real heap; a `malloc`'d cell is a fresh interpreter cell and a
/// pointer is a `MirValue::Ref` handle to it. `free` is a no-op (the `Rc` drop
/// reclaims the cell).
fn eval_extern<'tcx>(
    symbol: &str,
    _args: &[MirValue<'tcx>],
) -> Result<MirValue<'tcx>, MirInterpError> {
    match symbol {
        // size argument is ignored: one cell holds one value of any type.
        "malloc" | "calloc" => Ok(MirValue::Ref(Rc::new(RefCell::new(None)), Vec::new())),
        "free" => Ok(MirValue::Unit),
        other => Err(MirInterpError::Runtime(format!(
            "extern function '{other}' is not supported by the interpreter"
        ))),
    }
}

fn eval_intrinsic<'tcx>(
    fn_name: Intrinsic,
    args: Vec<MirValue<'tcx>>,
    ctx: &CompileCtx<'tcx>,
) -> Result<MirValue<'tcx>, MirInterpError> {
    match fn_name {
        Intrinsic::Println => {
            for arg in &args {
                print!("{} ", fmt_value(arg, ctx));
            }
            println!();
            Ok(MirValue::Unit)
        }
        Intrinsic::Print => {
            for arg in &args {
                print!("{} ", fmt_value(arg, ctx));
            }
            Ok(MirValue::Unit)
        }
        Intrinsic::Abs => {
            debug_assert_eq!(args.len(), 1, "Abs expects 1 arg");
            match args[0] {
                MirValue::Int(n) => Ok(MirValue::Int(n.abs())),
                ref v => Err(MirInterpError::Runtime(format!(
                    "__abs: expected Int, got {:?}",
                    v
                ))),
            }
        }
        Intrinsic::Min => {
            debug_assert_eq!(args.len(), 2, "Min expects 2 args");
            match (&args[0], &args[1]) {
                (MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(*a.min(b))),
                _ => Err(MirInterpError::Runtime("__min: expected (Int, Int)".into())),
            }
        }
        Intrinsic::Max => {
            debug_assert_eq!(args.len(), 2, "Max expects 2 args");
            match (&args[0], &args[1]) {
                (MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(*a.max(b))),
                _ => Err(MirInterpError::Runtime("__max: expected (Int, Int)".into())),
            }
        }
        Intrinsic::ReadInt => {
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .map_err(|e| MirInterpError::Runtime(format!("__read_int: io error: {e}")))?;
            let n = line
                .trim()
                .parse::<i64>()
                .map_err(|e| MirInterpError::Runtime(format!("__read_int: parse error: {e}")))?;
            Ok(MirValue::Int(n))
        }
        Intrinsic::Exit => {
            debug_assert_eq!(args.len(), 1, "Exit expects 1 arg");
            match args[0] {
                MirValue::Int(code) => std::process::exit(code as i32),
                ref v => Err(MirInterpError::Runtime(format!(
                    "__exit: expected Int, got {:?}",
                    v
                ))),
            }
        }
        // Raw-pointer ops. A `Ptr<T>` is a cell handle, exactly
        // like a reference, so `read`/`write` are a load/store of that cell and
        // `cast` is the identity.
        Intrinsic::PtrRead => {
            debug_assert_eq!(args.len(), 1, "__ptr_read expects 1 arg");
            match &args[0] {
                MirValue::Ref(cell, path) => read_loc(&(cell.clone(), path.clone())),
                v => Err(MirInterpError::Runtime(format!(
                    "__ptr_read: expected a pointer, got {v:?}"
                ))),
            }
        }
        Intrinsic::PtrWrite => {
            debug_assert_eq!(args.len(), 2, "__ptr_write expects 2 args");
            match &args[0] {
                MirValue::Ref(cell, path) => {
                    write_loc(&(cell.clone(), path.clone()), args[1].clone());
                    Ok(MirValue::Unit)
                }
                v => Err(MirInterpError::Runtime(format!(
                    "__ptr_write: expected a pointer, got {v:?}"
                ))),
            }
        }
        Intrinsic::PtrCast => {
            debug_assert_eq!(args.len(), 1, "__ptr_cast expects 1 arg");
            Ok(args.into_iter().next().unwrap())
        }
        // No-op for types without destructors; the value is simply
        // discarded (its `Rc`-backed cells, if any, drop here).
        Intrinsic::DropInPlace => Ok(MirValue::Unit),
        // `size_of` is lowered to `RValue::SizeOf`, never an intrinsic call.
        Intrinsic::SizeOf => internal_bug!("size_of should lower to RValue::SizeOf"),
    }
}

fn fmt_value<'tcx>(v: &MirValue<'tcx>, ctx: &CompileCtx<'tcx>) -> String {
    match v {
        MirValue::Int(i) => i.to_string(),
        MirValue::Bool(b) => b.to_string(),
        MirValue::Unit => "()".to_string(),
        MirValue::EnumVariant {
            enum_ref,
            variant_idx,
            payload,
        } => {
            let def = ctx.get_enum(*enum_ref);
            let name = &def.variants[*variant_idx].name;
            let tag = if def.is_anonymous {
                format!("#{name}")
            } else {
                name.clone()
            };
            match payload {
                Some(p) => format!("{tag}({})", fmt_value(p, ctx)),
                None => tag,
            }
        }
        MirValue::Tuple(elems) => {
            let inner = elems
                .iter()
                .map(|e| fmt_value(e, ctx))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({inner})")
        }
        // a reference prints as the value it points at (matches its transparent
        // display in the typed-HIR interpreter and the `&`-erased surface).
        MirValue::Ref(cell, path) => match read_loc(&(cell.clone(), path.clone())) {
            Ok(v) => format!("&{}", fmt_value(&v, ctx)),
            Err(_) => "&<uninit>".to_string(),
        },
        MirValue::Closure { fn_name, .. } => {
            format!("<closure {}>", ctx.original_fun_name(*fn_name))
        }
    }
}
