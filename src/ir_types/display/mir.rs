//! inspect the MIR

use std::fmt::Write as _;

use crate::compiler::context::CompileCtx;
use crate::ir_types::mir::*;

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bb{}", self.0)
    }
}

impl MirProgram {
    pub fn dump(&self, ctx: &CompileCtx) -> String {
        let mut out = String::new();
        for func in self.functions.values() {
            out.push_str(&func.dump(ctx));
            out.push('\n');
        }
        out
    }
}

impl MirFunction {
    pub fn dump(&self, ctx: &CompileCtx) -> String {
        let mut out = String::new();

        writeln!(
            out,
            "fn {}() -> {}  [entry: {}]",
            ctx.original_fun_name(self.name),
            self.ret_type,
            self.entry,
        )
        .unwrap();

        // locals
        writeln!(out, "  locals:").unwrap();
        for local in &self.locals {
            let name = match &local.name {
                LocalName::User(uv) => {
                    format!("{} ({})", ctx.uniq_variable_name(*uv), local.range,)
                }
                LocalName::Temp(i, hint) => format!("_tmp{i} [{hint}]"),
            };
            writeln!(out, "    {:?}: {:?}  // {}", local.id, local.ty, name).unwrap();
        }

        // blocks
        for block in &self.blocks {
            writeln!(out, "  {}:", block.id).unwrap();
            for stmt in &block.statements {
                writeln!(out, "    {}", fmt_statement(stmt, ctx)).unwrap();
            }
            writeln!(out, "    {}", fmt_terminator(&block.terminator)).unwrap();
        }

        out
    }
}

fn fmt_local(id: &LocalId) -> String {
    format!("_{}", id.0)
}

fn fmt_place(p: &Place) -> String {
    fmt_local(&p.local)
}

fn fmt_constant(c: &Constant) -> String {
    match c {
        Constant::Int(i) => i.to_string(),
        Constant::Bool(b) => b.to_string(),
        Constant::Unit => "()".to_string(),
    }
}

fn fmt_operand(o: &Operand) -> String {
    match o {
        Operand::Copy(p) => fmt_place(p),
        Operand::Const(c) => fmt_constant(c),
    }
}

fn fmt_rvalue(rv: &RValue, ctx: &CompileCtx) -> String {
    match rv {
        RValue::Use(o) => fmt_operand(o),
        RValue::BinaryOp { op, left, right } => {
            format!("{} {} {}", fmt_operand(left), op, fmt_operand(right))
        }
        RValue::UnaryOp { op, right } => format!("{} {}", op, fmt_operand(right)),
        RValue::Call { fn_name, args } => {
            let args: Vec<_> = args.iter().map(fmt_operand).collect();
            format!("{}({})", ctx.original_fun_name(*fn_name), args.join(", "))
        }
        RValue::IntrinsicCall { fn_name, args } => {
            let args: Vec<_> = args.iter().map(fmt_operand).collect();
            format!("{}({})", fn_name, args.join(", "))
        }
    }
}

fn fmt_statement(stmt: &Statement, ctx: &CompileCtx) -> String {
    match stmt {
        Statement::Assign { dst, value, .. } => {
            format!("{} = {}", fmt_place(dst), fmt_rvalue(value, ctx))
        }
        Statement::Eval { value, .. } => fmt_rvalue(value, ctx).to_string(),
    }
}

fn fmt_terminator(term: &Terminator) -> String {
    match term {
        Terminator::Goto { target } => format!("goto {}", target),
        Terminator::Branch {
            cond,
            then_bb,
            else_bb,
        } => format!("if {} then {} else {}", fmt_operand(cond), then_bb, else_bb),
        Terminator::Return { value: Some(v) } => format!("return {}", fmt_operand(v)),
        Terminator::Return { value: None } => "return ()".to_string(),
        Terminator::Unreachable => "unreachable".to_string(),
    }
}
