//! generate llvm-ir

use std::io;
use std::path::Path;

use inkwell::basic_block::BasicBlock as LLVMBasicBlock;
use inkwell::context::Context;
use inkwell::types::BasicType;
use inkwell::values as llvm;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::ir_types::mir::*;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::Bop;
use crate::lang::ops::CompOp;
use crate::lang::ops::Uop;
use crate::lang::types::Ty;

pub struct LlvmCodegen<'ctx> {
    context: &'ctx inkwell::context::Context,
    module: inkwell::module::Module<'ctx>,
    builder: inkwell::builder::Builder<'ctx>,
}

/// per-function state,
/// thrown away after each function & rebuilt fresh for the next one.
struct FnCtx<'a, 'ctx> {
    /// LocalId  ->  alloca'd stack slot
    locals: Map<LocalId, llvm::PointerValue<'ctx>>,
    local_tys: Map<LocalId, Ty>,
    /// BlockId  ->  LLVM BasicBlock (pre-created so forward jumps work)
    blocks: Map<BlockId, LLVMBasicBlock<'ctx>>,
    compile_ctx: &'a CompileCtx<'a>,
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

    pub fn emit_program(&self, program: &MirProgram, ctx: &CompileCtx) -> Result<(), CodegenError> {
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
    fn declare_function(&self, f: &MirFunction, ctx: &CompileCtx) -> llvm::FunctionValue<'ctx> {
        let param_types: Vec<_> = f
            .params
            .iter()
            .map(|p| self.llvm_type(p.ty).into())
            .collect();

        let fn_type = match f.ret_type {
            Ty::Unit => self.context.void_type().fn_type(&param_types, false),
            ty => self.llvm_type(ty).fn_type(&param_types, false),
        };

        let name = ctx.original_fun_name(f.name);
        self.module.add_function(&name, fn_type, None)
    }

    /// emit one function body
    fn emit_function(
        &self,
        f: &MirFunction,
        llvm_fn: llvm::FunctionValue<'ctx>,
        fns: &Map<FunRef, llvm::FunctionValue<'ctx>>,
        ctx: &CompileCtx,
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
                    .build_alloca(self.llvm_type(decl.ty), "local")
                    .map(|ptr|
                (decl.id, ptr))
            })
            .collect::<Result<Map<LocalId, llvm::PointerValue<'ctx>>, inkwell::builder::BuilderError>>()
            ?;
        let local_tys: Map<LocalId, Ty> = f.locals.iter().map(|d| (d.id, d.ty)).collect();

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
    fn emit_block(
        &self,
        bb: &BasicBlock,
        fn_ctx: &FnCtx<'_, 'ctx>,
        fns: &Map<FunRef, llvm::FunctionValue<'ctx>>,
    ) -> Result<(), CodegenError> {
        self.builder.position_at_end(fn_ctx.blocks[&bb.id]);

        for stmt in &bb.statements {
            self.emit_statement(stmt, fn_ctx, fns)?;
        }

        self.emit_terminator(&bb.terminator, fn_ctx, fns)?;

        Ok(())
    }

    /// statements
    fn emit_statement(
        &self,
        stmt: &Statement,
        fn_ctx: &FnCtx<'_, 'ctx>,
        fns: &Map<FunRef, llvm::FunctionValue<'ctx>>,
    ) -> Result<(), CodegenError> {
        match stmt {
            Statement::Assign { dst, value, .. } => {
                let val = self.emit_rvalue(value, fn_ctx, fns)?;
                self.builder.build_store(fn_ctx.locals[&dst.local], val)?;
            }
            Statement::Eval { value, .. } => {
                self.emit_rvalue(value, fn_ctx, fns)?; // result discarded
            }
        }

        Ok(())
    }

    /// rvalues / operands
    fn emit_rvalue(
        &self,
        rv: &RValue,
        fn_ctx: &FnCtx<'_, 'ctx>,
        fns: &Map<FunRef, llvm::FunctionValue<'ctx>>,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        match rv {
            RValue::Use(op) => self.emit_operand(op, fn_ctx),

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

            RValue::IntrinsicCall { fn_name, args } => self.emit_intrinsic(*fn_name, args, fn_ctx),
        }
    }

    fn emit_operand(
        &self,
        op: &Operand,
        fn_ctx: &FnCtx<'_, 'ctx>,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        match op {
            Operand::Copy(Place { local }) => {
                let ty = self.llvm_type(fn_ctx.local_tys[local]);
                Ok(self.builder.build_load(ty, fn_ctx.locals[local], "load")?)
            }
            Operand::Const(c) => Ok(self.emit_constant(c)),
        }
    }

    fn emit_constant(&self, c: &Constant) -> llvm::BasicValueEnum<'ctx> {
        match c {
            Constant::Int(i) => self.context.i64_type().const_int(*i as u64, true).into(),
            Constant::Bool(b) => self.context.bool_type().const_int(*b as u64, false).into(),
            Constant::Unit => self.context.struct_type(&[], false).const_zero().into(),
            // enum variants are represented as a single i64 holding the variant index
            Constant::EnumVariant { variant_idx, .. } => self
                .context
                .i64_type()
                .const_int(*variant_idx as u64, false)
                .into(),
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
            Bop::Pow => {
                let ipow = self.get_or_declare_ipow();
                let call = self
                    .builder
                    .build_call(ipow, &[li.into(), ri.into()], "pow")?;
                use inkwell::values::AnyValue;
                llvm::BasicValueEnum::try_from(call.as_any_value_enum()).map_err(|_| {
                    CodegenError::LlvmError("__lang_ipow did not return a value".into())
                })?
            }
            Bop::And => self.builder.build_and(li, ri, "and")?.into(),
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
        fn_ctx: &FnCtx<'_, 'ctx>,
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
        fn_ctx: &FnCtx<'_, 'ctx>,
    ) -> Result<llvm::BasicValueEnum<'ctx>, CodegenError> {
        let printf = self.get_or_declare_printf();

        for arg in args {
            let val = self.emit_operand(arg, fn_ctx)?;
            match Self::operand_ty(arg, fn_ctx) {
                Ty::Int => {
                    let fmt = self
                        .builder
                        .build_global_string_ptr("%ld ", "fmt_int")?
                        .as_pointer_value();
                    self.builder
                        .build_call(printf, &[fmt.into(), val.into()], "")?;
                }
                Ty::Bool => {
                    // variadic call, i1 must be promoted to i32
                    let as_i32 = self.builder.build_int_z_extend(
                        val.into_int_value(),
                        self.context.i32_type(),
                        "bool_ext",
                    )?;
                    let fmt = self
                        .builder
                        .build_global_string_ptr("%d ", "fmt_bool")?
                        .as_pointer_value(); // ← .as_pointer_value()
                    self.builder
                        .build_call(printf, &[fmt.into(), as_i32.into()], "")?;
                }
                Ty::Unit => {}
                Ty::Enum(er) => {
                    // build (or reuse) a global [N x ptr] variant-name table for
                    // this enum, then index into it at runtime with the variant
                    // index stored in `val`.
                    let table = self.get_or_create_variant_table(er, fn_ctx.compile_ctx);
                    let enum_def = fn_ctx.compile_ctx.get_enum(er);
                    let n = enum_def.variants.len() as u32;
                    let table_ty = self.context.ptr_type(Default::default()).array_type(n);
                    // SAFETY: table is a private constant array we just created;
                    // the index is a valid enum variant index produced by the compiler.
                    let name_ptr = unsafe {
                        self.builder.build_in_bounds_gep(
                            table_ty,
                            table.as_pointer_value(),
                            &[self.context.i64_type().const_zero(), val.into_int_value()],
                            "variant_name_ptr",
                        )?
                    };
                    let loaded = self.builder.build_load(
                        self.context.ptr_type(Default::default()),
                        name_ptr,
                        "variant_name",
                    )?;
                    let fmt = self
                        .builder
                        .build_global_string_ptr("%s ", "fmt_enum")?
                        .as_pointer_value();
                    self.builder
                        .build_call(printf, &[fmt.into(), loaded.into()], "")?;
                }
                ty => panic!("unexpected type in intrinsic: {:?}", ty),
            }
        }

        if matches!(fn_name, Intrinsic::Println) {
            let fmt = self
                .builder
                .build_global_string_ptr("\n", "fmt_nl")?
                .as_pointer_value(); // ← .as_pointer_value()
            self.builder.build_call(printf, &[fmt.into()], "")?;
        }

        Ok(self.context.struct_type(&[], false).const_zero().into())
    }

    /// Derive the MIR `Ty` of an operand without a type-check pass.
    fn operand_ty(op: &Operand, fn_ctx: &FnCtx) -> Ty {
        match op {
            Operand::Copy(Place { local }) => fn_ctx.local_tys[local],
            Operand::Const(Constant::Int(_)) => Ty::Int,
            Operand::Const(Constant::Bool(_)) => Ty::Bool,
            Operand::Const(Constant::Unit) => Ty::Unit,
            // The variant index is stored as i64; use Int as the LLVM type for now.
            Operand::Const(Constant::EnumVariant { enum_ref, .. }) => Ty::Enum(*enum_ref),
        }
    }

    /// Return a global `[N x ptr]` constant whose elements point to
    /// null-terminated variant-name strings for the given enum.
    /// The global is named `__enum_<idx>_variants` and is created only once.
    fn get_or_create_variant_table(
        &self,
        er: crate::lang::types::EnumRef,
        ctx: &CompileCtx,
    ) -> llvm::GlobalValue<'ctx> {
        let global_name = format!("__enum_{}_variants", er.0);

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
                let str_global_name = format!("__enum_{}_variant_{}_name", er.0, i);
                // build_global_string_ptr caches by content, not by name, so use
                // add_global + set_initializer directly to guarantee our own name.
                let display = format!("{prefix}{name}");
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

    fn get_or_declare_ipow(&self) -> llvm::FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("__lang_ipow") {
            return f;
        }
        let i64_ty = self.context.i64_type().into();
        let fn_ty = self.context.i64_type().fn_type(&[i64_ty, i64_ty], false);
        self.module.add_function("__lang_ipow", fn_ty, None)
    }

    // type helpers

    fn llvm_type(&self, ty: Ty) -> inkwell::types::BasicTypeEnum<'ctx> {
        match ty {
            Ty::Int => self.context.i64_type().into(),
            Ty::Bool => self.context.bool_type().into(),
            Ty::Unit => self.context.struct_type(&[], false).into(),
            Ty::Enum(_) => self.context.i64_type().into(),
            _ => panic!("no LLVM type for {:?}", ty),
        }
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
