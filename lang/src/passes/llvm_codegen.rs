//! generate llvm-ir

use std::io;
use std::path::Path;

use inkwell::basic_block::BasicBlock as LLVMBasicBlock;
use inkwell::context::Context;
use inkwell::types::BasicType;
use inkwell::values as llvm;
use inkwell::values::AnyValue;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::internal_bug;
use crate::ir_types::mir::*;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::Bop;
use crate::lang::ops::CompOp;
use crate::lang::ops::Uop;
use crate::lang::types::EnumRef;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;

pub struct LlvmCodegen<'ctx> {
    context: &'ctx inkwell::context::Context,
    module: inkwell::module::Module<'ctx>,
    builder: inkwell::builder::Builder<'ctx>,
}

/// per-function state,
/// thrown away after each function & rebuilt fresh for the next one.
struct FnCtx<'a, 'ctx, 'tcx> {
    /// LocalId  ->  alloca'd stack slot
    locals: Map<LocalId, llvm::PointerValue<'ctx>>,
    local_tys: Map<LocalId, Ty<'tcx>>,
    /// BlockId  ->  LLVM BasicBlock (pre-created so forward jumps work)
    blocks: Map<BlockId, LLVMBasicBlock<'ctx>>,
    compile_ctx: &'a CompileCtx<'tcx>,
}

#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    #[error(transparent)]
    BuilderError(#[from] inkwell::builder::BuilderError),
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error("LLVM error: {0}")]
    LlvmError(String),
    #[error("linking error: {0}")]
    LinkError(String),
}

impl<'ctx> LlvmCodegen<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        Self {
            context,
            module: context.create_module(module_name),
            builder: context.create_builder(),
        }
    }

    // public entry point

    pub fn emit_program<'tcx>(
        &self,
        program: &MirProgram<'tcx>,
        ctx: &CompileCtx<'tcx>,
    ) -> Result<(), CodegenError> {
        // Pass 1: declare every function signature (handles forward calls)
        let fns: Map<FunRef, llvm::FunctionValue<'ctx>> = program
            .functions
            .iter()
            .map(|(fref, f)| (*fref, self.declare_function(f, ctx)))
            .collect();

        // Pass 2: fill in each body
        for (fref, f) in &program.functions {
            self.emit_function(f, fns[fref], &fns, ctx)?;
        }

        Ok(())
    }

    /// declare functions without bodies (for now)
    fn declare_function<'tcx>(
        &self,
        f: &MirFunction<'tcx>,
        ctx: &CompileCtx<'tcx>,
    ) -> llvm::FunctionValue<'ctx> {
        let param_types: Vec<_> = f
            .params
            .iter()
            .map(|p| self.llvm_type(ctx, p.ty).into())
            .collect();

        let fn_type = if matches!(f.ret_type.kind(), TyKind::Unit) {
            self.context.void_type().fn_type(&param_types, false)
        } else {
            self.llvm_type(ctx, f.ret_type).fn_type(&param_types, false)
        };

        let name = ctx.original_fun_name(f.name);
        self.module.add_function(&name, fn_type, None)
    }

    /// emit one function body
    fn emit_function<'tcx>(
        &self,
        f: &MirFunction<'tcx>,
        llvm_fn: llvm::FunctionValue<'ctx>,
        fns: &Map<FunRef<'tcx>, llvm::FunctionValue<'ctx>>,
        ctx: &CompileCtx<'tcx>,
    ) -> Result<(), CodegenError> {
        // 1. entry block for allocas
        let entry_bb = self.context.append_basic_block(llvm_fn, "entry");
        self.builder.position_at_end(entry_bb);

        // 2. alloca one slot per local
        let locals: Map<LocalId, llvm::PointerValue<'ctx>> = f
            .locals
            .iter()
            .map(|decl| {
                self
                    .builder
                    .build_alloca(self.llvm_type(ctx, decl.ty), "local")
                    .map(|ptr|
                (decl.id, ptr))
            })
            .collect::<Result<Map<LocalId, llvm::PointerValue<'ctx>>, inkwell::builder::BuilderError>>()
            ?;
        let local_tys: Map<LocalId, Ty<'tcx>> = f.locals.iter().map(|d| (d.id, d.ty)).collect();

        // 3. store incoming params into their alloca'd slots
        for (param, llvm_arg) in f.params.iter().zip(llvm_fn.get_params()) {
            self.builder.build_store(locals[&param.local], llvm_arg)?;
        }

        // 4. pre-create all MIR basic blocks (forward jumps need them to exist)
        let blocks: Map<BlockId, LLVMBasicBlock<'ctx>> = f
            .blocks
            .iter()
            .map(|bb| {
                let label = format!("bb{}", bb.id.0);
                (bb.id, self.context.append_basic_block(llvm_fn, &label))
            })
            .collect();

        // 5. jump from alloca-entry into the MIR entry block
        self.builder.build_unconditional_branch(blocks[&f.entry])?;

        let fn_ctx = FnCtx {
            locals,
            local_tys,
            blocks,
            compile_ctx: ctx,
        };

        // 6. fill each block
        for bb in &f.blocks {
            self.emit_block(bb, &fn_ctx, fns)?;
        }

        Ok(())
    }

    /// one basic block
    fn emit_block<'tcx>(
        &self,
        bb: &BasicBlock<'tcx>,
        fn_ctx: &FnCtx<'_, 'ctx, 'tcx>,
        fns: &Map<FunRef<'tcx>, llvm::FunctionValue<'ctx>>,
    ) -> Result<(), CodegenError> {
        self.builder.position_at_end(fn_ctx.blocks[&bb.id]);

        for stmt in &bb.statements {
            self.emit_statement(stmt, fn_ctx, fns)?;
        }

        self.emit_terminator(&bb.terminator, fn_ctx, fns)?;

        Ok(())
    }

    /// statements
    fn emit_statement<'tcx>(
        &self,
        stmt: &Statement<'tcx>,
        fn_ctx: &FnCtx<'_, 'ctx, 'tcx>,
        fns: &Map<FunRef<'tcx>, llvm::FunctionValue<'ctx>>,
    ) -> Result<(), CodegenError> {
        match stmt {
            Statement::Assign { dst, value, .. } => {
                // The destination type is the place's type (a `[Deref]` dst stores
                // the *pointee* type, through the pointer's address). For a bare
                // local this is the local's own type/slot, as before (R2/R3).
                let dst_ty = Self::place_ty(dst, fn_ctx);
                let val = self.emit_rvalue(value, dst_ty, fn_ctx, fns)?;
                let addr = self.place_address(dst, fn_ctx)?;
                self.builder.build_store(addr, val)?;
            }
            Statement::Eval { value, .. } => {
                // Side-effecting call only; Aggregate/Field never appear here,
                // so dst_ty is irrelevant — use unit as a dummy.
                self.emit_rvalue(value, fn_ctx.compile_ctx.types.unit, fn_ctx, fns)?;
            }
        }

        Ok(())
    }

    /// rvalues / operands
    /// `dst_ty` is the Sand type of the destination local — needed by
    /// `Aggregate` (to distinguish enum from tuple) and `Field` (to know
    /// what type to load).
    fn emit_rvalue<'tcx>(
        &self,
        rv: &RValue<'tcx>,
        dst_ty: Ty<'tcx>,
        fn_ctx: &FnCtx<'_, 'ctx, 'tcx>,
        fns: &Map<FunRef<'tcx>, llvm::FunctionValue<'ctx>>,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        match rv {
            RValue::Use(op) => self.emit_operand(op, fn_ctx),

            // `&place` / `&mut place`: the address of the place's storage (R2).
            RValue::Ref(place) => Ok(self.place_address(place, fn_ctx)?.into()),

            RValue::BinaryOp {
                op: Bop::Pow,
                left,
                right,
            } => {
                // `^` desugars to the `pow` function defined in core.sand.
                let base = self.emit_operand(left, fn_ctx)?;
                let exp = self.emit_operand(right, fn_ctx)?;
                let pow_fn = fns
                    .iter()
                    .find(|(fr, _)| fn_ctx.compile_ctx.original_fun_name(**fr) == "pow")
                    .map(|(_, fv)| *fv)
                    .expect("core 'pow' function not found in compiled program");
                let call = self
                    .builder
                    .build_call(pow_fn, &[base.into(), exp.into()], "pow")?;
                Ok(call.try_as_basic_value().basic().unwrap())
            }

            RValue::BinaryOp { op, left, right } => {
                let l = self.emit_operand(left, fn_ctx)?;
                let r = self.emit_operand(right, fn_ctx)?;
                self.emit_binop(*op, l, r)
            }

            RValue::UnaryOp { op, right } => {
                let v = self.emit_operand(right, fn_ctx)?;
                self.emit_unop(*op, v)
            }

            RValue::Call { fn_name, args } => {
                let callee = fns[fn_name];
                let arg_vals: Vec<llvm::BasicMetadataValueEnum> = args
                    .iter()
                    .map(|a| self.emit_operand(a, fn_ctx).map(Into::into))
                    .collect::<Result<_, _>>()?;
                let call = self.builder.build_call(callee, &arg_vals, "call")?;
                // void functions → return the () unit struct instead
                Ok(call
                    .try_as_basic_value()
                    .basic()
                    .unwrap_or_else(|| self.context.struct_type(&[], false).const_zero().into()))
            }

            RValue::IntrinsicCall { fn_name, args } => match fn_name {
                Intrinsic::Print | Intrinsic::Println => {
                    self.emit_intrinsic(*fn_name, args, fn_ctx)
                }
                _ => self.emit_intrinsic_value(*fn_name, args, fn_ctx),
            },

            RValue::Aggregate(fields) => self.emit_aggregate(fields, dst_ty, fn_ctx),

            RValue::Field { base, index } => self.emit_field(base, *index, dst_ty, fn_ctx),
        }
    }

    fn emit_operand(
        &self,
        op: &Operand,
        fn_ctx: &FnCtx<'_, 'ctx, '_>,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        match op {
            Operand::Copy(place) => {
                let ty = self.llvm_type(fn_ctx.compile_ctx, Self::place_ty(place, fn_ctx));
                let addr = self.place_address(place, fn_ctx)?;
                Ok(self.builder.build_load(ty, addr, "load")?)
            }
            Operand::Const(c) => Ok(self.emit_constant(c)),
        }
    }

    fn emit_constant(&self, c: &Constant) -> llvm::BasicValueEnum<'ctx> {
        match c {
            Constant::Int(i) => self.context.i64_type().const_int(*i as u64, true).into(),
            Constant::Bool(b) => self.context.bool_type().const_int(*b as u64, false).into(),
            Constant::Unit => self.context.struct_type(&[], false).const_zero().into(),
        }
    }

    fn emit_binop(
        &self,
        op: Bop,
        l: llvm::BasicValueEnum<'ctx>,
        r: llvm::BasicValueEnum<'ctx>,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        use inkwell::IntPredicate::*;
        let li = l.into_int_value();
        let ri = r.into_int_value();
        Ok(match op {
            Bop::Plus => self.builder.build_int_add(li, ri, "add")?.into(),
            Bop::Minus => self.builder.build_int_sub(li, ri, "sub")?.into(),
            Bop::Mult => self.builder.build_int_mul(li, ri, "mul")?.into(),
            Bop::Div => self.builder.build_int_signed_div(li, ri, "div")?.into(),
            // pow is intercepted in emit_rvalue before reaching here
            Bop::Pow => internal_bug!("Bop::Pow should be handled before emit_binop"),
            // bitwise AND on i64 and logical AND on i1 are the same LLVM `and`.
            Bop::BitAnd | Bop::And => self.builder.build_and(li, ri, "and")?.into(),
            Bop::Or => self.builder.build_or(li, ri, "or")?.into(),
            Bop::Xor => self.builder.build_xor(li, ri, "xor")?.into(),
            Bop::Comp(cop) => {
                let pred = match cop {
                    CompOp::Eq => EQ,
                    CompOp::Ne => NE,
                    CompOp::Lt => SLT,
                    CompOp::Le => SLE,
                    CompOp::Gt => SGT,
                    CompOp::Ge => SGE,
                };
                self.builder.build_int_compare(pred, li, ri, "cmp")?.into()
            }
        })
    }

    fn emit_unop(
        &self,
        op: Uop,
        v: llvm::BasicValueEnum<'ctx>,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        let vi = v.into_int_value();
        Ok(match op {
            Uop::Neg => self.builder.build_int_neg(vi, "neg")?.into(),
            Uop::Not => self.builder.build_not(vi, "not")?.into(), /* bitwise NOT on i1 = logical
                                                                    * NOT */
        })
    }

    /// terminator
    fn emit_terminator(
        &self,
        term: &Terminator,
        fn_ctx: &FnCtx<'_, 'ctx, '_>,
        _fns: &Map<FunRef, llvm::FunctionValue<'ctx>>,
    ) -> Result<(), CodegenError> {
        match term {
            Terminator::Goto { target } => {
                self.builder
                    .build_unconditional_branch(fn_ctx.blocks[target])?;
            }
            Terminator::Branch {
                cond,
                then_bb,
                else_bb,
            } => {
                let cond_val = self.emit_operand(cond, fn_ctx)?.into_int_value();
                self.builder.build_conditional_branch(
                    cond_val,
                    fn_ctx.blocks[then_bb],
                    fn_ctx.blocks[else_bb],
                )?;
            }
            Terminator::Return { value: Some(op) } => {
                let val = self.emit_operand(op, fn_ctx)?;
                self.builder.build_return(Some(&val))?;
            }
            Terminator::Return { value: None } => {
                self.builder.build_return(None)?;
            }
            Terminator::Unreachable => {
                self.builder.build_unreachable()?;
            }
        }

        Ok(())
    }

    fn emit_intrinsic(
        &self,
        fn_name: Intrinsic,
        args: &[Operand],
        fn_ctx: &FnCtx<'_, 'ctx, '_>,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        let printf = self.get_or_declare_printf();

        for arg in args {
            let val = self.emit_operand(arg, fn_ctx)?;
            let ty = Self::operand_ty(arg, fn_ctx);
            self.emit_print_value(val, ty, printf, fn_ctx)?;
        }

        if matches!(fn_name, Intrinsic::Println) {
            let fmt = self
                .builder
                .build_global_string_ptr("\n", "fmt_nl")?
                .as_pointer_value();
            self.builder.build_call(printf, &[fmt.into()], "")?;
        }

        Ok(self.context.struct_type(&[], false).const_zero().into())
    }

    fn emit_intrinsic_value(
        &self,
        fn_name: Intrinsic,
        args: &[Operand],
        fn_ctx: &FnCtx<'_, 'ctx, '_>,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        let i64_ty = self.context.i64_type();
        match fn_name {
            Intrinsic::Abs => {
                debug_assert_eq!(args.len(), 1, "Abs expects 1 arg");
                let val = self.emit_operand(&args[0], fn_ctx)?.into_int_value();
                // llvm.abs.i64(val, is_int_min_poison: false)
                // The second argument tells LLVM whether INT_MIN is poison (UB).
                // We pass false to keep defined behaviour for all inputs.
                let is_int_min_poison = self.context.bool_type().const_int(0, false);
                self.call_llvm_intrinsic(
                    "llvm.abs",
                    i64_ty,
                    &[val.into(), is_int_min_poison.into()],
                    "abs",
                )
            }
            Intrinsic::Min => {
                debug_assert_eq!(args.len(), 2, "Min expects 2 args");
                let a = self.emit_operand(&args[0], fn_ctx)?.into_int_value();
                let b = self.emit_operand(&args[1], fn_ctx)?.into_int_value();
                self.call_llvm_intrinsic("llvm.smin", i64_ty, &[a.into(), b.into()], "min")
            }
            Intrinsic::Max => {
                debug_assert_eq!(args.len(), 2, "Max expects 2 args");
                let a = self.emit_operand(&args[0], fn_ctx)?.into_int_value();
                let b = self.emit_operand(&args[1], fn_ctx)?.into_int_value();
                self.call_llvm_intrinsic("llvm.smax", i64_ty, &[a.into(), b.into()], "max")
            }
            Intrinsic::ReadInt => {
                let scanf = self.get_or_declare_scanf();
                let slot = self.builder.build_alloca(i64_ty, "read_int_slot")?;
                let fmt = self
                    .builder
                    .build_global_string_ptr("%ld", "fmt_read_int")?
                    .as_pointer_value();
                self.builder
                    .build_call(scanf, &[fmt.into(), slot.into()], "")?;
                Ok(self.builder.build_load(i64_ty, slot, "read_int_val")?)
            }
            Intrinsic::Exit => {
                debug_assert_eq!(args.len(), 1, "Exit expects 1 arg");
                let code = self.emit_operand(&args[0], fn_ctx)?.into_int_value();
                let code_i32 =
                    self.builder
                        .build_int_truncate(code, self.context.i32_type(), "exit_code")?;
                let exit_fn = self.get_or_declare_exit();
                self.builder.build_call(exit_fn, &[code_i32.into()], "")?;
                Ok(self.context.struct_type(&[], false).const_zero().into())
            }
            _ => unreachable!("emit_intrinsic_value called for print/println"),
        }
    }

    /// Derive the MIR `Ty` of an operand without a type-check pass.
    fn operand_ty<'tcx>(op: &Operand, fn_ctx: &FnCtx<'_, '_, 'tcx>) -> Ty<'tcx> {
        match op {
            Operand::Copy(place) => Self::place_ty(place, fn_ctx),
            Operand::Const(Constant::Int(_)) => fn_ctx.compile_ctx.types.int,
            Operand::Const(Constant::Bool(_)) => fn_ctx.compile_ctx.types.bool,
            Operand::Const(Constant::Unit) => fn_ctx.compile_ctx.types.unit,
        }
    }

    /// The LLVM address denoted by `place`: start at the local's alloca slot,
    /// then follow one load per `Deref` projection (each `Deref` reads the
    /// pointer stored there to get the next address). Empty projection → the
    /// local's slot itself (so `&local` is its address, a read is a plain
    /// load).
    fn place_address<'tcx>(
        &self,
        place: &Place,
        fn_ctx: &FnCtx<'_, 'ctx, 'tcx>,
    ) -> Result<llvm::PointerValue<'ctx>, CodegenError> {
        let mut addr = fn_ctx.locals[&place.local];
        for elem in &place.projection {
            match elem {
                ProjElem::Deref => {
                    let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
                    addr = self
                        .builder
                        .build_load(ptr_ty, addr, "deref")?
                        .into_pointer_value();
                }
            }
        }
        Ok(addr)
    }

    /// The MIR `Ty` of a place after its projections (each `Deref` strips one
    /// reference, yielding the pointee type).
    fn place_ty<'tcx>(place: &Place, fn_ctx: &FnCtx<'_, '_, 'tcx>) -> Ty<'tcx> {
        let mut ty = fn_ctx.local_tys[&place.local];
        for elem in &place.projection {
            match elem {
                ProjElem::Deref => {
                    ty = match ty.kind() {
                        TyKind::Ref(_, t) | TyKind::RefMut(_, t) => *t,
                        _ => internal_bug!("Deref projection on non-reference {ty:?}"),
                    };
                }
            }
        }
        ty
    }

    /// Return a global `[N x ptr]` constant whose elements point to
    /// null-terminated variant-name strings for the given enum.
    /// The global is named `__enum_<idx>_variants` and is created only once.
    fn get_or_create_variant_table<'tcx>(
        &self,
        er: crate::lang::types::EnumRef<'tcx>,
        ctx: &CompileCtx<'tcx>,
    ) -> llvm::GlobalValue<'ctx> {
        let global_name = format!("__enum_{}_variants", er.0.id);

        // Reuse if already emitted (e.g. the same enum printed in multiple places).
        if let Some(g) = self.module.get_global(&global_name) {
            return g;
        }

        let enum_def = ctx.get_enum(er);
        let ptr_ty = self.context.ptr_type(Default::default());

        // One global i8 array per variant name.
        // For anonymous tag-union types, prefix names with `#` so that
        // `println(tag_val)` produces e.g. `#gt` rather than bare `gt`.
        let prefix = if enum_def.is_anonymous { "#" } else { "" };
        let name_ptrs: Vec<llvm::BasicValueEnum<'ctx>> = enum_def
            .variants
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let str_global_name = format!("__enum_{}_variant_{}_name", er.0.id, i);
                // build_global_string_ptr caches by content, not by name, so use
                // add_global + set_initializer directly to guarantee our own name.
                let display = format!("{prefix}{}", name.name);
                let s = self.context.const_string(display.as_bytes(), true);
                let g = self.module.add_global(s.get_type(), None, &str_global_name);
                g.set_initializer(&s);
                g.set_constant(true);
                g.set_linkage(inkwell::module::Linkage::Private);
                g.as_pointer_value().into()
            })
            .collect();

        let array_const = ptr_ty.const_array(
            &name_ptrs
                .iter()
                .map(|v| v.into_pointer_value())
                .collect::<Vec<_>>(),
        );
        let table = self
            .module
            .add_global(array_const.get_type(), None, &global_name);
        table.set_initializer(&array_const);
        table.set_constant(true);
        table.set_linkage(inkwell::module::Linkage::Private);
        table
    }

    /// helper to call an LLVM intrinsic by name
    fn call_llvm_intrinsic(
        &self,
        intrinsic_name: &str,
        param_type: inkwell::types::IntType<'ctx>,
        args: &[llvm::BasicMetadataValueEnum<'ctx>],
        result_name: &str,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        let intrinsic = inkwell::intrinsics::Intrinsic::find(intrinsic_name)
            .ok_or_else(|| CodegenError::LlvmError(format!("{} not found", intrinsic_name)))?;
        let decl = intrinsic
            .get_declaration(&self.module, &[param_type.into()])
            .ok_or_else(|| {
                CodegenError::LlvmError(format!("{} declaration failed", intrinsic_name))
            })?;
        let call = self.builder.build_call(decl, args, result_name)?;
        llvm::BasicValueEnum::try_from(call.as_any_value_enum())
            .map_err(|_| CodegenError::LlvmError(format!("{} returned no value", intrinsic_name)))
    }

    fn get_or_declare_printf(&self) -> llvm::FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("printf") {
            return f;
        }
        let ptr_ty = self
            .context
            .ptr_type(inkwell::AddressSpace::default())
            .into();
        let fn_ty = self
            .context
            .i32_type()
            .fn_type(&[ptr_ty], /* variadic= */ true);
        self.module.add_function("printf", fn_ty, None)
    }

    fn get_or_declare_scanf(&self) -> llvm::FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("scanf") {
            return f;
        }
        let ptr_ty = self
            .context
            .ptr_type(inkwell::AddressSpace::default())
            .into();
        let fn_ty = self
            .context
            .i32_type()
            .fn_type(&[ptr_ty], /* variadic= */ true);
        self.module.add_function("scanf", fn_ty, None)
    }

    fn get_or_declare_exit(&self) -> llvm::FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("exit") {
            return f;
        }
        let fn_ty = self
            .context
            .void_type()
            .fn_type(&[self.context.i32_type().into()], false);
        self.module.add_function("exit", fn_ty, None)
    }

    fn get_or_declare_malloc(&self) -> llvm::FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("malloc") {
            return f;
        }
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_ty = ptr_ty.fn_type(&[self.context.i64_type().into()], false);
        self.module.add_function("malloc", fn_ty, None)
    }

    // aggregate type helpers

    /// Returns `true` if any variant of `er` carries a payload.
    ///
    /// All-nullary enums are represented as a bare `i64` discriminant.
    /// Enums where at least one variant has a payload are heap-allocated:
    /// every value is a `ptr` to a `{ i64, ptr }` cell (field 0 = discriminant,
    /// field 1 = separately-malloc'd payload, or null for nullary variants).
    fn enum_has_payload<'tcx>(ctx: &CompileCtx<'tcx>, er: EnumRef<'tcx>) -> bool {
        ctx.get_enum(er)
            .variants
            .iter()
            .any(|v| v.payload.get().is_some())
    }

    /// LLVM struct type for a heap-allocated enum cell: `{ i64, ptr }`.
    /// Field 0 = discriminant, field 1 = opaque ptr to payload (or null).
    fn enum_cell_type(&self) -> inkwell::types::StructType<'ctx> {
        self.context.struct_type(
            &[
                self.context.i64_type().into(),
                self.context
                    .ptr_type(inkwell::AddressSpace::default())
                    .into(),
            ],
            false,
        )
    }

    // type helpers

    fn llvm_type<'tcx>(
        &self,
        ctx: &CompileCtx<'tcx>,
        ty: Ty<'tcx>,
    ) -> inkwell::types::BasicTypeEnum<'ctx> {
        match ty.kind() {
            TyKind::Int => self.context.i64_type().into(),
            TyKind::Bool => self.context.bool_type().into(),
            TyKind::Unit => self.context.struct_type(&[], false).into(),
            TyKind::Enum(er) if Self::enum_has_payload(ctx, *er) => {
                // heap-allocated cell; we only store/pass/load an opaque ptr
                self.context
                    .ptr_type(inkwell::AddressSpace::default())
                    .into()
            }
            TyKind::Enum(_) => self.context.i64_type().into(),
            TyKind::Tuple(tys) => {
                let tys = *tys;
                let field_tys: Vec<_> = tys.iter().map(|t| self.llvm_type(ctx, *t)).collect();
                self.context.struct_type(&field_tys, false).into()
            }
            // References are real pointers (R2); the region carries no runtime data.
            TyKind::Ref(..) | TyKind::RefMut(..) => self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into(),
            _ => internal_bug!("no LLVM type for {:?}", ty),
        }
    }

    // aggregate rvalue helpers

    /// Emit an `RValue::Aggregate` construction.
    ///
    /// - **Enum, all-nullary**: fields = [discriminant] → emit the `i64` disc.
    /// - **Enum, payload**: fields = [discriminant, payload?] → malloc a `{
    ///   i64, ptr }` cell, store the discriminant in field 0 and (optionally)
    ///   the malloc'd payload ptr in field 1 (null if nullary).
    /// - **Tuple**: fields = [elem0, …] → build a stack-allocated struct value.
    fn emit_aggregate<'tcx>(
        &self,
        fields: &[Operand],
        dst_ty: Ty<'tcx>,
        fn_ctx: &FnCtx<'_, 'ctx, 'tcx>,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        let ctx = fn_ctx.compile_ctx;
        match dst_ty.kind() {
            TyKind::Enum(er) => {
                let er = *er;
                // fields[0] is always Const::Int(variant_idx)
                let disc_val = self.emit_operand(&fields[0], fn_ctx)?.into_int_value();

                if Self::enum_has_payload(ctx, er) {
                    // ── heap cell ─────────────────────────────────────────────
                    let cell_ty = self.enum_cell_type();
                    let cell_size = cell_ty.size_of().expect("cell_ty has known size");
                    let malloc = self.get_or_declare_malloc();
                    let cell_ptr = self
                        .builder
                        .build_call(malloc, &[cell_size.into()], "enum_cell")?
                        .try_as_basic_value()
                        .basic()
                        .unwrap()
                        .into_pointer_value();

                    // store discriminant into cell[0]
                    let disc_slot =
                        self.builder
                            .build_struct_gep(cell_ty, cell_ptr, 0, "disc_slot")?;
                    self.builder.build_store(disc_slot, disc_val)?;

                    // store payload ptr into cell[1]
                    let payload_slot =
                        self.builder
                            .build_struct_gep(cell_ty, cell_ptr, 1, "payload_slot")?;
                    let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());

                    if fields.len() == 2 {
                        // non-nullary variant: malloc payload and store it
                        let payload_val = self.emit_operand(&fields[1], fn_ctx)?;
                        let payload_sand_ty = Self::operand_ty(&fields[1], fn_ctx);
                        let payload_llvm_ty = self.llvm_type(ctx, payload_sand_ty);
                        let payload_size = payload_llvm_ty
                            .size_of()
                            .expect("payload type has known size");
                        let payload_ptr = self
                            .builder
                            .build_call(malloc, &[payload_size.into()], "payload_cell")?
                            .try_as_basic_value()
                            .basic()
                            .unwrap()
                            .into_pointer_value();
                        self.builder.build_store(payload_ptr, payload_val)?;
                        self.builder.build_store(payload_slot, payload_ptr)?;
                    } else {
                        // nullary variant: null payload ptr
                        self.builder
                            .build_store(payload_slot, ptr_ty.const_null())?;
                    }

                    Ok(cell_ptr.into())
                } else {
                    // ── all-nullary: just the discriminant ────────────────────
                    Ok(disc_val.into())
                }
            }

            TyKind::Tuple(tys) => {
                let tys = *tys;
                let field_tys: Vec<_> = tys.iter().map(|t| self.llvm_type(ctx, *t)).collect();
                let struct_ty = self.context.struct_type(&field_tys, false);

                // Materialise the struct via a temporary alloca, then load it
                // so the result is a value (not a pointer).  LLVM's mem2reg
                // pass eliminates the alloca in almost all real cases.
                let slot = self.builder.build_alloca(struct_ty, "tuple_tmp")?;
                for (i, field) in fields.iter().enumerate() {
                    let val = self.emit_operand(field, fn_ctx)?;
                    let field_ptr = self.builder.build_struct_gep(
                        struct_ty,
                        slot,
                        i as u32,
                        "tuple_field_slot",
                    )?;
                    self.builder.build_store(field_ptr, val)?;
                }
                Ok(self.builder.build_load(struct_ty, slot, "tuple_val")?)
            }

            _ => internal_bug!(
                "emit_aggregate called with non-aggregate dst_ty {:?}",
                dst_ty
            ),
        }
    }

    /// Emit an `RValue::Field` extraction.
    ///
    /// - **Enum (payload)**: base is a `ptr` to `{ i64, ptr }` cell; field 0 =
    ///   `i64` discriminant, field 1 = payload (load through ptr).
    /// - **Enum (all-nullary)**: base is the `i64` discriminant; only field 0
    ///   is valid.
    /// - **Tuple**: base is a struct value; extract element at `index`.
    fn emit_field<'tcx>(
        &self,
        base: &Operand,
        index: usize,
        dst_ty: Ty<'tcx>,
        fn_ctx: &FnCtx<'_, 'ctx, 'tcx>,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        let ctx = fn_ctx.compile_ctx;
        let base_sand_ty = Self::operand_ty(base, fn_ctx);
        let base_val = self.emit_operand(base, fn_ctx)?;

        match base_sand_ty.kind() {
            TyKind::Enum(er) if Self::enum_has_payload(ctx, *er) => {
                let cell_ty = self.enum_cell_type();
                let cell_ptr = base_val.into_pointer_value();
                match index {
                    0 => {
                        // discriminant
                        let disc_slot =
                            self.builder
                                .build_struct_gep(cell_ty, cell_ptr, 0, "disc_slot")?;
                        Ok(self
                            .builder
                            .build_load(self.context.i64_type(), disc_slot, "disc")?)
                    }
                    1 => {
                        // payload: load the payload ptr from cell[1], then
                        // load the actual value through that ptr
                        let payload_ptr_slot = self.builder.build_struct_gep(
                            cell_ty,
                            cell_ptr,
                            1,
                            "payload_ptr_slot",
                        )?;
                        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
                        let payload_ptr = self
                            .builder
                            .build_load(ptr_ty, payload_ptr_slot, "payload_ptr")?
                            .into_pointer_value();
                        let payload_llvm_ty = self.llvm_type(ctx, dst_ty);
                        Ok(self
                            .builder
                            .build_load(payload_llvm_ty, payload_ptr, "payload")?)
                    }
                    _ => internal_bug!("enum field index {} out of range (valid: 0 or 1)", index),
                }
            }

            TyKind::Enum(_) => {
                // all-nullary: base_val *is* the i64 discriminant
                debug_assert_eq!(index, 0, "all-nullary enum only has field 0");
                Ok(base_val)
            }

            TyKind::Tuple(tys) => {
                let tys = *tys;
                let field_tys: Vec<_> = tys.iter().map(|t| self.llvm_type(ctx, *t)).collect();
                // build_extract_value needs to know the struct type so LLVM
                // can verify the index; we rebuild it here (it's the same
                // interned struct type, so LLVM deduplicates it).
                let _struct_ty = self.context.struct_type(&field_tys, false);
                let struct_val = base_val.into_struct_value();
                Ok(self
                    .builder
                    .build_extract_value(struct_val, index as u32, "field")?)
            }

            _ => internal_bug!(
                "emit_field called with non-aggregate base type {:?}",
                base_sand_ty
            ),
        }
    }

    /// Emit printf calls to print a single Sand value of any type.
    ///
    /// Values are printed in the following format:
    /// - `Int`   → `%ld ` (e.g. `42 `)
    /// - `Bool`  → `%d ` promoted to i32 (e.g. `1 ` / `0 `)
    /// - `Unit`  → nothing
    /// - `Enum` (all-nullary) → `%s ` (variant name)
    /// - `Enum` (payload)     → `%s ` (variant name; payload is not printed)
    /// - `Tuple` → each element printed in declaration order, space-separated
    fn emit_print_value<'tcx>(
        &self,
        val: llvm::BasicValueEnum<'ctx>,
        ty: Ty<'tcx>,
        printf: llvm::FunctionValue<'ctx>,
        fn_ctx: &FnCtx<'_, 'ctx, 'tcx>,
    ) -> Result<(), CodegenError> {
        let ctx = fn_ctx.compile_ctx;
        match ty.kind() {
            TyKind::Int => {
                let fmt = self
                    .builder
                    .build_global_string_ptr("%ld ", "fmt_int")?
                    .as_pointer_value();
                self.builder
                    .build_call(printf, &[fmt.into(), val.into()], "")?;
            }
            TyKind::Bool => {
                // Variadic call: i1 must be widened to i32 for printf
                let as_i32 = self.builder.build_int_z_extend(
                    val.into_int_value(),
                    self.context.i32_type(),
                    "bool_ext",
                )?;
                let fmt = self
                    .builder
                    .build_global_string_ptr("%d ", "fmt_bool")?
                    .as_pointer_value();
                self.builder
                    .build_call(printf, &[fmt.into(), as_i32.into()], "")?;
            }
            TyKind::Unit => {
                // nothing to print
            }
            TyKind::Enum(er) => {
                let er = *er;
                let disc_int = if Self::enum_has_payload(ctx, er) {
                    // val is a ptr to `{ i64, ptr }`; load discriminant from [0]
                    let cell_ty = self.enum_cell_type();
                    let cell_ptr = val.into_pointer_value();
                    let disc_slot =
                        self.builder
                            .build_struct_gep(cell_ty, cell_ptr, 0, "disc_slot")?;
                    self.builder
                        .build_load(self.context.i64_type(), disc_slot, "disc")?
                        .into_int_value()
                } else {
                    // all-nullary: val is already the i64 discriminant
                    val.into_int_value()
                };
                let table = self.get_or_create_variant_table(er, ctx);
                let enum_def = ctx.get_enum(er);
                let n = enum_def.variants.len() as u32;
                let ptr_ty = self.context.ptr_type(Default::default());
                let table_ty = ptr_ty.array_type(n);
                let i64_zero = self.context.i64_type().const_zero();
                let name_ptr = unsafe {
                    self.builder.build_in_bounds_gep(
                        table_ty,
                        table.as_pointer_value(),
                        &[i64_zero, disc_int],
                        "variant_name_ptr",
                    )?
                };
                let loaded = self.builder.build_load(ptr_ty, name_ptr, "variant_name")?;
                let fmt = self
                    .builder
                    .build_global_string_ptr("%s ", "fmt_enum")?
                    .as_pointer_value();
                self.builder
                    .build_call(printf, &[fmt.into(), loaded.into()], "")?;
            }
            TyKind::Tuple(tys) => {
                let tys = *tys;
                let struct_val = val.into_struct_value();
                for (i, &field_ty) in tys.iter().enumerate() {
                    let field_val =
                        self.builder
                            .build_extract_value(struct_val, i as u32, "print_field")?;
                    self.emit_print_value(field_val, field_ty, printf, fn_ctx)?;
                }
            }
            _ => {} // Top doesn't appear at runtime
        }
        Ok(())
    }

    // output

    /// Write human-readable LLVM IR (.ll) — useful for debugging.
    pub fn write_ir<P: AsRef<Path>>(&self, path: P, dry: bool) -> Result<(), CodegenError> {
        if dry {
            self.module.print_to_stderr();
            return Ok(());
        }
        self.module
            .print_to_file(path)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Write a native object file (`.o`);
    ///
    /// `[!]` link with `cc` to get a binary
    pub fn write_object<P: AsRef<Path>>(&self, path: P, dry: bool) -> Result<(), CodegenError> {
        use inkwell::targets::*;
        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let triple = TargetMachine::get_default_triple();
        let target =
            Target::from_triple(&triple).map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                inkwell::OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            )
            .ok_or_else(|| {
                CodegenError::LlvmError("Failed to create target machine".to_string())
            })?;
        if dry {
            return Ok(());
        }
        machine
            .write_to_file(&self.module, FileType::Object, path.as_ref())
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Link a native object file (`.o`) into an executable binary.
    ///
    /// todo: do this in rust instead of calling `cc`
    pub fn link(object_path: &str, output_path: &str) -> Result<(), CodegenError> {
        let status = std::process::Command::new("cc")
            .args([object_path, "-o", output_path])
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(CodegenError::LinkError(format!(
                "linking failed: {}",
                status
            )))
        }
    }
}
