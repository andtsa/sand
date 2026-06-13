//! an interpreter for the MIR

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::internal_bug;
use crate::ir_types::mir::*;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::Bop;
use crate::lang::ops::CompOp;
use crate::lang::ops::Uop;
use crate::lang::types::EnumRef;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;

#[derive(Debug, Clone, PartialEq)]
pub enum MirValue<'tcx> {
    Int(i64),
    Bool(bool),
    Unit,
    EnumVariant {
        enum_ref: EnumRef<'tcx>,
        variant_idx: usize,
        payload: Option<Box<MirValue<'tcx>>>,
    },
    Tuple(Vec<MirValue<'tcx>>),
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
        fun: FunRef,
        args: &[MirValue<'tcx>],
        ctx: &CompileCtx<'tcx>,
    ) -> Result<MirValue<'tcx>, MirInterpError> {
        let func = &self.functions[&fun];

        // initialise locals: all unset to start
        let mut locals: Vec<Option<MirValue<'tcx>>> = vec![None; func.locals.len()];

        // bind parameters
        for (param, val) in func.params.iter().zip(args.iter()) {
            locals[param.local.0] = Some(val.clone());
        }

        // execute blocks
        let mut current = func.entry;
        loop {
            let block = &func.blocks[current.0];

            for stmt in &block.statements {
                execute_statement(stmt, &mut locals, &func.locals, self, ctx)?;
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
    stmt: &Statement,
    locals: &mut [Option<MirValue<'tcx>>],
    local_decls: &[LocalDecl<'tcx>],
    prog: &MirProgram<'tcx>,
    ctx: &CompileCtx<'tcx>,
) -> Result<(), MirInterpError> {
    match stmt {
        Statement::Assign { dst, value, .. } => {
            let dst_ty = local_decls[dst.local.0].ty;
            let v = eval_rvalue(value, dst_ty, locals, prog, ctx)?;
            locals[dst.local.0] = Some(v);
            Ok(())
        }
        Statement::Eval { value, .. } => {
            // Eval is only for side-effecting calls: Aggregate/Field never
            // appear here, so the result_ty is irrelevant; use UNIT as dummy.
            eval_rvalue(value, ctx.types.unit, locals, prog, ctx)?;
            Ok(())
        }
    }
}

fn eval_rvalue<'tcx>(
    rv: &RValue,
    result_ty: Ty<'tcx>,
    locals: &mut [Option<MirValue<'tcx>>],
    prog: &MirProgram<'tcx>,
    ctx: &CompileCtx<'tcx>,
) -> Result<MirValue<'tcx>, MirInterpError> {
    match rv {
        RValue::Use(op) => eval_operand(op, locals),

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
    locals: &[Option<MirValue<'tcx>>],
) -> Result<MirValue<'tcx>, MirInterpError> {
    match op {
        Operand::Const(c) => Ok(match c {
            Constant::Int(i) => MirValue::Int(*i),
            Constant::Bool(b) => MirValue::Bool(*b),
            Constant::Unit => MirValue::Unit,
        }),
        Operand::Copy(place) => locals[place.local.0]
            .clone()
            .ok_or(MirInterpError::UninitializedLocal(place.local)),
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
    }
}
