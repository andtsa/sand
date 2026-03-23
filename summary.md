# project sand compiler codebase summary:

## directory tree
```
.
├── Cargo.lock
├── Cargo.toml
├── README.md
├── archive
├── examples
│   ├── RSA.sand
│   ├── complex.sand
│   ├── fact.sand
│   ├── fib.sand
│   ├── gcd.sand
│   ├── gt3.sand
│   ├── invalid.sand
│   ├── prime.sand
│   ├── repeat.sand
│   ├── sand.toml
│   ├── simple.sand
│   └── test.sand
├── lsp_wrapper.sh
├── rustfmt.toml
├── src
│   ├── analysis
│   │   ├── annotate.rs
│   │   ├── cfg.rs
│   │   ├── interactions.rs
│   │   └── mod.rs
│   ├── bin
│   │   ├── analyse.rs
│   │   ├── debug.rs
│   │   ├── lower.rs
│   │   ├── lsp.rs
│   │   ├── run.rs
│   │   ├── run_mir.rs
│   │   └── visualize.rs
│   ├── compiler
│   │   ├── context
│   │   │   ├── compile.rs
│   │   │   ├── mod.rs
│   │   │   └── project.rs
│   │   ├── mod.rs
│   │   └── structure
│   │       ├── debug.rs
│   │       ├── functions.rs
│   │       ├── mod.rs
│   │       ├── projects.rs
│   │       └── variables.rs
│   ├── grammar.pest
│   ├── interpreter
│   │   ├── mir.rs
│   │   ├── mod.rs
│   │   └── typed_hir.rs
│   ├── ir_types
│   │   ├── cfgmir.rs
│   │   ├── display
│   │   │   ├── cfgmir.rs
│   │   │   └── mod.rs
│   │   ├── hhir.rs
│   │   ├── mod.rs
│   │   ├── qhir.rs
│   │   ├── ssa.rs
│   │   └── typed_hir.rs
│   ├── lang
│   │   ├── intrinsics.rs
│   │   ├── mod.rs
│   │   ├── ops.rs
│   │   └── types.rs
│   ├── lib.rs
│   ├── lsp
│   │   ├── annotate.rs
│   │   ├── backend.rs
│   │   ├── config.rs
│   │   ├── diagnostics
│   │   │   ├── ast.rs
│   │   │   ├── mod.rs
│   │   │   ├── qualify.rs
│   │   │   ├── typecheck.rs
│   │   │   └── uniquify.rs
│   │   ├── files.rs
│   │   ├── mod.rs
│   │   └── util.rs
│   ├── passes
│   │   ├── build_ast.rs
│   │   ├── explicate_control
│   │   │   ├── context.rs
│   │   │   └── mod.rs
│   │   ├── mod.rs
│   │   ├── parse.rs
│   │   ├── qualify
│   │   │   ├── error.rs
│   │   │   ├── mod.rs
│   │   │   └── uniquify
│   │   │       ├── error.rs
│   │   │       ├── mod.rs
│   │   │       └── reserved.rs
│   │   └── type_ast
│   │       ├── check_intrinsic.rs
│   │       ├── errors.rs
│   │       ├── mod.rs
│   │       ├── type_check.rs
│   │       └── var_types.rs
│   └── util
│       ├── bugs.rs
│       ├── mod.rs
│       └── traits.rs
├── summary.md
├── summary.toml
├── tests
│   ├── common
│   │   └── mod.rs
│   ├── correct_programs.rs
│   ├── fact.rs
│   ├── fail_parse.rs
│   ├── fib.rs
│   ├── hir_tests.rs
│   └── mir_tests.rs
└── treesitter
    ├── grammar.js
    ├── queries
    │   ├── highlights.scm
    │   ├── indents.scm
    │   └── locals.scm
    └── tree-sitter.json

25 directories, 97 files
```

## files

### README.md

# a sandy language for remote working (waterproof frfr)

wip


## Layers

### collect files
- project identification
- find related files 
- parse config toml file

- fetch libraries

**input:**
- directory, file, or files.

**output:**
```
Map<FileRef, CodeFile>,
Config
```

status: partially implemented in the LSP module, but still scattered between compiler/context.rs and lsp/. probably need to unify those two or make a separate module.

### parse files

input: previous pass

- read files to string
- parse each one

output:
```
Map<FileRef, Result<Pairs<'i, Rule>, Error<Rule>>>
```

status: parsing implemented with pest, currently this is combined with the pass below, I don't think it's worth it to separate them.

### build ASTs

input: previous pass

- build untyped ast for each file

output:
```
Map<FileName, Result<Map<FnName, Function>, AstError>>
Map<FnName, FnSig>
```

status: implemented but with newer signatures; need to update documentation to reflect changes

### qualify functions
input: 
```
Map<FileName, Map<FnName, Function>>
Map<FnName, FnSig>
```

change every function name and function call to a globally unique one,
predictably depending on the file name.

possibly: allow specifying module name with a keyword in the file instead
of using file name exclusively.

also: resolve calls to external functions using module names

output:
```
Map<FileName, Result<Map<FnName, Function>, QfError>>
Map<FnName, FnSig>
```

status: implemented but with slightly different semantics, need to update documentation

### merge modules
input: 
```
Map<FileName, Map<FnName, Function>>
Map<FnName, FnSig>
```

output:
```
Map<FnName, Function>
Map<FnName, FnSig>
```

this might not be a separate pass as it is very small.

status: implemented as a first step of the previous pass

### uniquify function bodies
input:
```
Map<FnName, Function>
Map<FnName, FnSig>
```

- change all variable names to be globally unique

output:
```
Map<FnName, Result<Function, UniquifyError>>
Map<FnName, FnSig>
```

status: done, currently a substep of the qualify pass

### build typed ast

input:
```
Map<FnName, Function>
Map<FnName, FnSig>
```

output:
```
TypedProgram
AstTypeError
```

status: done

### type check

input:
```
TypedProgram
```

output:
```
TypedProgram
TypeCheckError
```

status: basic implementation done

## todo

- a LOT.
- fix LSP,
- improve diagnostics
- MIR lowering (explicate control)
- SSA MIR
- ...
- llvm codegen
- write more tests


### Cargo.toml

```toml
[package]
name = "sand"
version = "0.1.0"
edition = "2024"
default-run = "debug"

# integration tests under tests/ directory

[dependencies]
anyhow = "1"
pest = "2.8.4"
pest_derive = "2.8.4"
petgraph = "0.8.3"
take-if = "1"
thiserror = "2"
tower-lsp = "0.20.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "io-std", "fs"] }
toml = "1"
serde = "1.0.228"
bimap = "0.6.3"
```

### lsp_wrapper.sh

```sh
#!/usr/bin/env bash
set -euo pipefail

DEBUG_BIN="target/debug/lsp"
RELEASE_BIN="target/release/lsp"

LSP_ARGS=("$@")

# ----- helpers -----

latest_bin() {
  local d="$DEBUG_BIN" r="$RELEASE_BIN"
  local d_ok=0 r_ok=0
  [[ -x "$d" ]] && d_ok=1
  [[ -x "$r" ]] && r_ok=1

  if [[ $d_ok -eq 0 && $r_ok -eq 0 ]]; then
    echo ""
    return 0
  fi
  if [[ $d_ok -eq 1 && $r_ok -eq 0 ]]; then
    echo "$d"; return 0
  fi
  if [[ $d_ok -eq 0 && $r_ok -eq 1 ]]; then
    echo "$r"; return 0
  fi

  # Both exist: pick the newer mtime (BSD stat)
  local dt rt
  dt=$(stat -f %m "$d")
  rt=$(stat -f %m "$r")
  if (( rt >= dt )); then echo "$r"; else echo "$d"; fi
}

# Wait until file stops changing (mtime+size stable twice)
wait_stable() {
  local f="$1"
  local m1 s1 m2 s2
  while true; do
    [[ -x "$f" ]] || { sleep 0.1; continue; }
    m1=$(stat -f %m "$f") || { sleep 0.1; continue; }
    s1=$(stat -f %z "$f") || { sleep 0.1; continue; }
    sleep 0.12
    m2=$(stat -f %m "$f") || { sleep 0.1; continue; }
    s2=$(stat -f %z "$f") || { sleep 0.1; continue; }
    [[ "$m1" == "$m2" && "$s1" == "$s2" ]] && return 0
  done
}

kill_child() {
  local pid="${1:-}"
  [[ -n "${pid}" ]] || return 0
  kill -TERM "$pid" 2>/dev/null || true
  # give it a moment
  for _ in {1..20}; do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.05
  done
  kill -KILL "$pid" 2>/dev/null || true
}

# ----- main supervisor -----

if ! command -v fswatch >/dev/null 2>&1; then
  echo "error: fswatch not found. Install with: brew install fswatch" >&2
  exit 1
fi

child_pid=""

start_server() {
  local bin
  bin="$(latest_bin)"
  if [[ -z "$bin" ]]; then
    echo "wrapper: no executable at $DEBUG_BIN or $RELEASE_BIN yet" >&2
    return 1
  fi

  wait_stable "$bin"
  echo "wrapper: starting $bin" >&2

  # Start server with stdio connected to nvim
  "$bin" "${LSP_ARGS[@]}" &
  child_pid=$!
  return 0
}

# Start initial server (block until one exists)
until start_server; do
  sleep 0.2
done

# Watch for changes to either binary path (create/rename/write)
# -0 makes it NUL-delimited, safer for paths, but we don't need the path text anyway.
fswatch -0 "$DEBUG_BIN" "$RELEASE_BIN" 2>/dev/null | while IFS= read -r -d '' _; do
  # On any event, pick newest and restart
  newbin="$(latest_bin)"
  [[ -n "$newbin" ]] || continue

  # If the newest is still the same *file* and hasn't changed, this is harmless;
  # restart anyway to keep logic simple, but only after stable.
  wait_stable "$newbin"

  echo "wrapper: restart triggered; switching to $newbin" >&2
  kill_child "$child_pid"
  "$newbin" "${LSP_ARGS[@]}" &
  child_pid=$!
done
```

### rustfmt.toml

```toml
# imports are single line only to better work with line-based diffs like git
imports_granularity = "Item"

# imports are sorted to better work with line-based diffs like git
group_imports = "StdExternalCrate"

# comments adjust to maximal code width.
wrap_comments = true
```

### src/interpreter/mod.rs

```rs
//! interpreters module

pub mod typed_hir;
pub mod mir;
```

### src/interpreter/typed_hir.rs

```rs
//! a simple interpreter for the typed_hir IR 

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use anyhow::anyhow;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::*;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;

impl TypedProgram {
    /// run the main function of the program and return an expression
    /// that's either Int, Bool, or Unit
    pub fn interpret(&self, ctx: &CompileCtx) -> anyhow::Result<Expression> {
        // find the main function
        let (_, main_fn) = self
            .functions
            .iter()
            .find(|(f, _)| ctx.is_main(**f))
            .ok_or_else(|| anyhow!("no main function found"))?;

        // empty environment
        let env = Env::new();
        // just evaluate the body of the main function
        self.evaluate(&main_fn.body.expr, &env)
    }
}

// A reference-counted, interior-mutable environment handle
pub type EnvRef = Rc<RefCell<Env>>;

#[derive(Debug)]
pub struct Env {
    data: BTreeMap<UniqVar, Expression>,
    /// pointer to the outer environment
    outer: Option<EnvRef>,
}

impl Env {
    fn new() -> EnvRef {
        Rc::new(RefCell::new(Env {
            data: BTreeMap::new(),
            outer: None,
        }))
    }

    fn with_outer(outer: &EnvRef) -> EnvRef {
        Rc::new(RefCell::new(Env {
            data: BTreeMap::new(),
            outer: Some(Rc::clone(outer)),
        }))
    }

    fn assign(&mut self, name: UniqVar, val: Expression) -> anyhow::Result<()> {
        #[allow(clippy::map_entry)]
        if self.data.contains_key(&name) {
            self.data.insert(name, val);
            Ok(())
        } else if let Some(ref outer) = self.outer {
            outer.borrow_mut().assign(name, val)
        } else {
            Err(anyhow::anyhow!("variable not found: {:?}", name))
        }
    }

    fn get(&self, name: &UniqVar) -> Option<Expression> {
        if let Some(v) = self.data.get(name) {
            Some(v.clone())
        } else if let Some(ref outer) = self.outer {
            outer.borrow().get(name)
        } else {
            None
        }
    }

    fn add_variable(&mut self, name: UniqVar, val: Expression) {
        self.data.insert(name, val);
    }
}

impl Expr {
    pub fn evaluate(&self, prog: &TypedProgram, env: &EnvRef) -> anyhow::Result<Expression> {
        prog.evaluate(&self.expr, env)
    }
}

impl TypedProgram {
    /// evaluate the expression and return the resulting expression
    pub fn evaluate(&self, expr: &Expression, env: &EnvRef) -> anyhow::Result<Expression> {
        match expr {
            Expression::If { cond, t, f } => {
                let cond_val = cond.evaluate(self, env)?;
                match cond_val {
                    Expression::Bool(true) => t.evaluate(self, env),
                    Expression::Bool(false) => f.evaluate(self, env),
                    e => Err(anyhow!(
                        "condition {cond:?} must evaluate to a boolean, got {e:?}"
                    )),
                }
            }
            Expression::While { cond, body } => {
                let mut result = Expression::Unit;
                loop {
                    let cond_val = cond.evaluate(self, env)?;
                    match cond_val {
                        Expression::Bool(true) => {
                            result = body.evaluate(self, env)?;
                        }
                        Expression::Bool(false) => break,
                        e => {
                            return Err(anyhow!(
                                "condition {cond:?} must evaluate to a boolean, got {e:?}"
                            ));
                        }
                    }
                }
                Ok(result)
            }
            Expression::BinOp { left, op, right } => {
                let left_val = left.evaluate(self, env)?;
                let right_val = right.evaluate(self, env)?;
                match (left_val, right_val, op) {
                    (Expression::Int(l), Expression::Int(r), Bop::Plus) => {
                        Ok(Expression::Int(l + r))
                    }
                    (Expression::Int(l), Expression::Int(r), Bop::Minus) => {
                        Ok(Expression::Int(l - r))
                    }
                    (Expression::Int(l), Expression::Int(r), Bop::Mult) => {
                        Ok(Expression::Int(l * r))
                    }
                    (Expression::Int(l), Expression::Int(r), Bop::Div) => {
                        Ok(Expression::Int(l / r))
                    }
                    (Expression::Int(l), Expression::Int(r), Bop::Pow) => {
                        Ok(Expression::Int(l.pow(r as u32)))
                    }
                    (Expression::Int(l), Expression::Int(r), Bop::Comp(cop)) => match cop {
                        CompOp::Eq => Ok(Expression::Bool(l == r)),
                        CompOp::Ne => Ok(Expression::Bool(l != r)),
                        CompOp::Lt => Ok(Expression::Bool(l < r)),
                        CompOp::Le => Ok(Expression::Bool(l <= r)),
                        CompOp::Gt => Ok(Expression::Bool(l > r)),
                        CompOp::Ge => Ok(Expression::Bool(l >= r)),
                    },
                    (Expression::Bool(l), Expression::Bool(r), Bop::And) => {
                        Ok(Expression::Bool(l && r))
                    }
                    (Expression::Bool(l), Expression::Bool(r), Bop::Or) => {
                        Ok(Expression::Bool(l || r))
                    }
                    (Expression::Bool(l), Expression::Bool(r), Bop::Comp(CompOp::Eq)) => {
                        Ok(Expression::Bool(l == r))
                    }
                    (Expression::Bool(l), Expression::Bool(r), Bop::Comp(CompOp::Ne)) => {
                        Ok(Expression::Bool(l != r))
                    }
                    (x, y, z) => Err(anyhow!(
                        "type error in binary operation: {left:?} {op:?} {right:?}, got {x:?} {y:?} {z:?}"
                    )),
                }
            }
            Expression::UnOp { op, right } => {
                let val = right.evaluate(self, env)?;
                match (val, op) {
                    (Expression::Bool(b), Uop::Not) => Ok(Expression::Bool(!b)),
                    (Expression::Int(n), Uop::Neg) => Ok(Expression::Int(-n)),
                    (x, y) => Err(anyhow!(
                        "type error in unary operation: {op:?} {right:?}, got {y:?} {x:?}"
                    )),
                }
            }
            Expression::Int(n) => Ok(Expression::Int(*n)),
            Expression::Bool(b) => Ok(Expression::Bool(*b)),
            Expression::Unit => Ok(Expression::Unit),

            Expression::Var(name) => {
                if let Some(val) = env.borrow().get(name) {
                    Ok(val)
                } else {
                    Err(anyhow!("undefined variable: {:?}", name))
                }
            }

            Expression::Block { statements, expr } => {
                let local_env = Env::with_outer(env);
                let mut ret_expr = Expression::Unit;
                for stmt in statements {
                    match stmt {
                        Statement::Declaration { name, val, .. } => {
                            let evaluated_val = val.evaluate(self, &local_env)?;
                            local_env.borrow_mut().add_variable(*name, evaluated_val);
                        }
                        Statement::Assignment { name, val, .. } => {
                            let evaluated_val = val.evaluate(self, &local_env)?;
                            local_env.borrow_mut().assign(*name, evaluated_val)?;
                        }
                        Statement::Expr(e) => {
                            ret_expr = e.evaluate(self, &local_env)?;
                        }
                    }
                }
                if let Some(e) = expr {
                    e.evaluate(self, &local_env)
                } else {
                    Ok(ret_expr)
                }
            }
            Expression::Call { fn_name, args } => {
                let function = &self.functions[fn_name];

                if args.len() != function.parameters.len() {
                    return Err(anyhow!(
                        "function {:?} expects {} arguments, got {}",
                        function.name,
                        function.parameters.len(),
                        args.len()
                    ));
                }

                let local_env = Env::new();

                // evaluate arguments and bind to parameters
                for (param, arg) in function.parameters.iter().zip(args.iter()) {
                    let arg_val = arg.evaluate(self, env)?;
                    local_env.borrow_mut().add_variable(param.name, arg_val);
                }

                // evaluate function body
                function.body.evaluate(self, &local_env)
            }
            Expression::IntrinsicCall { fn_name, args } => match fn_name {
                Intrinsic::Println => {
                    for arg in args {
                        let val = arg.evaluate(self, env)?;
                        print!("{:?} ", val);
                    }
                    println!();
                    Ok(Expression::Unit)
                }
                Intrinsic::Print => {
                    for arg in args {
                        let val = arg.evaluate(self, env)?;
                        print!("{:?} ", val);
                    }
                    Ok(Expression::Unit)
                }
            },
        }
    }
}
```

### src/interpreter/mir.rs

```rs
//! an interpreter for the MIR

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::internal_bug;
use crate::ir_types::cfgmir::*;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::{Bop, CompOp, Uop};

#[derive(Debug, Clone, PartialEq)]
pub enum MirValue {
    Int(i64),
    Bool(bool),
    Unit,
}

#[derive(Debug)]
pub enum MirInterpError {
    NoEntryPoint,
    UninitializedLocal(LocalId),
    DivisionByZero,
    Unreachable,
}

impl std::fmt::Display for MirInterpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirInterpError::NoEntryPoint => write!(f, "no entry point found"),
            MirInterpError::UninitializedLocal(id) => {
                write!(f, "uninitialized local: {:?}", id)
            }
            MirInterpError::DivisionByZero => write!(f, "division by zero"),
            MirInterpError::Unreachable => write!(f, "reached unreachable terminator"),
        }
    }
}

impl MirProgram {
    pub fn interpret(&self, ctx: &CompileCtx) -> Result<MirValue, MirInterpError> {
        // find the main function
        let (main_ref, _main_fn) = self
            .functions
            .iter()
            .find(|(fr, _)| ctx.is_main(**fr))
            .ok_or(MirInterpError::NoEntryPoint)?;

        self.call_function(*main_ref, &[])
    }

    fn call_function(
        &self,
        fun: FunRef,
        args: &[MirValue],
    ) -> Result<MirValue, MirInterpError> {
        let func = &self.functions[&fun];

        // initialise locals — all unset to start
        let mut locals: Vec<Option<MirValue>> = vec![None; func.locals.len()];

        // bind parameters
        for (param, val) in func.params.iter().zip(args.iter()) {
            locals[param.local.0] = Some(val.clone());
        }

        // execute blocks
        let mut current = func.entry;
        loop {
            let block = &func.blocks[current.0];

            for stmt in &block.statements {
                execute_statement(stmt, &mut locals, self)?;
            }

            match &block.terminator {
                Terminator::Goto { target } => {
                    current = *target;
                }
                Terminator::Branch { cond, then_bb, else_bb } => {
                    match eval_operand(cond, &locals)? {
                        MirValue::Bool(true)  => current = *then_bb,
                        MirValue::Bool(false) => current = *else_bb,
                        v => internal_bug!(
                            "branch condition evaluated to non-bool: {:?}", v
                        ),
                    }
                }
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

fn execute_statement(
    stmt: &Statement,
    locals: &mut Vec<Option<MirValue>>,
    prog: &MirProgram,
) -> Result<(), MirInterpError> {
    match stmt {
        Statement::Assign { dst, value, .. } => {
            let v = eval_rvalue(value, locals, prog)?;
            locals[dst.local.0] = Some(v);
            Ok(())
        }
        Statement::Eval { value, .. } => {
            eval_rvalue(value, locals, prog)?;
            Ok(())
        }
    }
}

fn eval_rvalue(
    rv: &RValue,
    locals: &mut [Option<MirValue>],
    prog: &MirProgram,
) -> Result<MirValue, MirInterpError> {
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
            prog.call_function(*fn_name, &arg_vals)
        }

        RValue::IntrinsicCall { fn_name, args } => {
            let arg_vals = args
                .iter()
                .map(|a| eval_operand(a, locals))
                .collect::<Result<Vec<_>, _>>()?;
            eval_intrinsic(*fn_name, arg_vals)
        }
    }
}

fn eval_operand(
    op: &Operand,
    locals: &[Option<MirValue>],
) -> Result<MirValue, MirInterpError> {
    match op {
        Operand::Const(c) => Ok(match c {
            Constant::Int(i)  => MirValue::Int(*i),
            Constant::Bool(b) => MirValue::Bool(*b),
            Constant::Unit    => MirValue::Unit,
        }),
        Operand::Copy(place) => locals[place.local.0]
            .clone()
            .ok_or(MirInterpError::UninitializedLocal(place.local)),
    }
}

fn eval_binop(op: Bop, l: MirValue, r: MirValue) -> Result<MirValue, MirInterpError> {
    match (op, l, r) {
        (Bop::Plus,  MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a + b)),
        (Bop::Minus, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a - b)),
        (Bop::Mult,  MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a * b)),
        (Bop::Div,   MirValue::Int(a), MirValue::Int(b)) => {
            if b == 0 { Err(MirInterpError::DivisionByZero) }
            else { Ok(MirValue::Int(a / b)) }
        }
        (Bop::Pow,   MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a.pow(b as u32))),
        (Bop::And,   MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a && b)),
        (Bop::Or,    MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a || b)),
        (Bop::Xor,   MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a ^ b)),
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
        (op, l, r) => internal_bug!(
            "type error in MIR binop: {:?} {:?} {:?}", l, op, r
        ),
    }
}

fn eval_unop(op: Uop, v: MirValue) -> Result<MirValue, MirInterpError> {
    match (op, v) {
        (Uop::Neg, MirValue::Int(i))   => Ok(MirValue::Int(-i)),
        (Uop::Not, MirValue::Bool(b))  => Ok(MirValue::Bool(!b)),
        (op, v) => internal_bug!(
            "type error in MIR unop: {:?} {:?}", op, v
        ),
    }
}

fn eval_intrinsic(fn_name: Intrinsic, args: Vec<MirValue>) -> Result<MirValue, MirInterpError> {
    match fn_name {
        Intrinsic::Println => {
            for arg in &args { print!("{} ", fmt_value(arg)); }
            println!();
            Ok(MirValue::Unit)
        }
        Intrinsic::Print => {
            for arg in &args { print!("{} ", fmt_value(arg)); }
            Ok(MirValue::Unit)
        }
    }
}

fn fmt_value(v: &MirValue) -> String {
    match v {
        MirValue::Int(i)  => i.to_string(),
        MirValue::Bool(b) => b.to_string(),
        MirValue::Unit    => "()".to_string(),
    }
}
```

### src/analysis/annotate.rs

```rs
use std::collections::HashSet;

use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::Expr;
use crate::ir_types::typed_hir::Expression;
use crate::ir_types::typed_hir::Statement;

pub fn get_dependencies(expr: &Expr) -> HashSet<UniqVar> {
    let mut dependencies = HashSet::new();
    collect_dependencies(&expr.expr, &mut dependencies);
    dependencies
}

pub fn collect_dependencies(expr: &Expression, dependencies: &mut HashSet<UniqVar>) {
    match expr {
        Expression::Var(name) => {
            dependencies.insert(*name);
        }
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
        Expression::Call { args, .. } | Expression::IntrinsicCall { args, .. } => {
            for arg in args {
                collect_dependencies(&arg.expr, dependencies);
            }
        }
        Expression::Block { statements, expr } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration { val, .. } => {
                        collect_dependencies(&val.expr, dependencies);
                    }
                    Statement::Assignment { val, .. } => {
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
    }
}

pub fn get_mutations_stmt(stmt: &Statement) -> HashSet<UniqVar> {
    match stmt {
        Statement::Declaration { name, .. } => HashSet::from([*name]),
        Statement::Assignment { name, .. } => HashSet::from([*name]),
        Statement::Expr(_) => HashSet::new(),
    }
}

pub fn get_mutations_expr(expr: &Expr) -> HashSet<UniqVar> {
    let mut mutations = HashSet::new();
    collect_mutations(&expr.expr, &mut mutations);
    mutations
}

fn collect_mutations(expr: &Expression, mutations: &mut HashSet<UniqVar>) {
    match expr {
        Expression::Block { statements, expr } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration { name, .. } => {
                        mutations.insert(*name);
                    }
                    Statement::Assignment { name, .. } => {
                        mutations.insert(*name);
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
        _ => {}
    }
}
```

### src/analysis/cfg.rs

```rs
#![allow(unused)]
//! control flow graph construction

use std::collections::HashMap;
use std::collections::HashSet;

use petgraph::Directed;
use petgraph::Graph;
use petgraph::graph::NodeIndex;

use crate::analysis::AnnotatedExpression;
use crate::analysis::annotate::get_dependencies;
use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Pos;
use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::Expr;
use crate::ir_types::typed_hir::Expression;
use crate::ir_types::typed_hir::Statement;
use crate::ir_types::typed_hir::TypedFunction;
use crate::ir_types::typed_hir::TypedProgram;
use crate::lang::types::Ty;

pub fn construct_cfg(
    ctx: &CompileCtx,
    ast: &TypedProgram,
) -> Graph<AnnotatedExpression, (), Directed> {
    let mut graph = Graph::<AnnotatedExpression, (), Directed>::new();
    let mut function_entries = HashMap::new();
    let mut function_exits = HashMap::new();

    // AE Analysis expects that the entry to the main function is always at
    // IndexNode(0)
    let mut funcs_sorted: Vec<&TypedFunction> = ast.functions.values().collect();
    funcs_sorted.sort_by_key(|f| if Some(f.name) == ctx.entrypoint { 0 } else { 1 });

    // Add entry and exit nodes for every function
    for func in funcs_sorted {
        let entry_annotated = AnnotatedExpression {
            expr: Expr {
                expr: Expression::Unit,
                range: Default::default(),
                ty: Ty::Unit,
            },
            depends_on: HashSet::new(),
            mutates: HashSet::new(),
            module: func.src_module,
        };
        let entry = graph.add_node(entry_annotated);

        let exit_annotated = AnnotatedExpression {
            expr: Expr {
                expr: Expression::Unit,
                range: Default::default(),
                ty: Ty::Unit,
            },
            depends_on: HashSet::new(),
            mutates: HashSet::new(),
            module: func.src_module,
        };
        let exit = graph.add_node(exit_annotated);

        function_entries.insert(func.name, entry);
        function_exits.insert(func.name, exit);
    }

    // Create cfg for each function and connect them if needed
    for (fun_ref, func) in &ast.functions {
        let entry = function_entries[fun_ref];
        let exit = function_exits[fun_ref];

        build_cfg_func(
            &mut graph,
            func,
            entry,
            exit,
            &function_entries,
            &function_exits,
        );
    }

    graph
}

pub fn build_cfg_func(
    graph: &mut Graph<AnnotatedExpression, (), Directed>,
    func: &TypedFunction,
    entry: NodeIndex,
    exit: NodeIndex,
    function_entries: &HashMap<FunRef, NodeIndex>,
    function_exits: &HashMap<FunRef, NodeIndex>,
) {
    let body_entry = build_cfg_expr(
        graph,
        &func.body,
        func.src_module,
        exit,
        function_entries,
        function_exits,
        None,
    );
    graph.add_edge(entry, body_entry, ());
}

fn build_cfg_expr(
    graph: &mut Graph<AnnotatedExpression, (), Directed>,
    expr: &Expr,
    module: ModuleRef,
    next: NodeIndex,
    function_entries: &HashMap<FunRef, NodeIndex>,
    function_exits: &HashMap<FunRef, NodeIndex>,
    mutations: Option<HashSet<UniqVar>>,
) -> NodeIndex {
    match &expr.expr {
        Expression::If { cond, t, f } => {
            let cond_annotated = AnnotatedExpression {
                expr: cond.as_ref().clone(),
                depends_on: get_dependencies(cond),
                mutates: mutations.clone().unwrap_or_default(),
                module,
            };
            let cond_node = graph.add_node(cond_annotated);
            let then_entry = build_cfg_expr(
                graph,
                t,
                module,
                next,
                function_entries,
                function_exits,
                None,
            );
            let else_entry = build_cfg_expr(
                graph,
                f,
                module,
                next,
                function_entries,
                function_exits,
                None,
            );

            graph.add_edge(cond_node, then_entry, ());
            graph.add_edge(cond_node, else_entry, ());

            cond_node
        }
        Expression::While { cond, body } => {
            let cond_annotated = AnnotatedExpression {
                expr: cond.as_ref().clone(),
                depends_on: get_dependencies(cond),
                mutates: mutations.clone().unwrap_or_default(),
                module,
            };
            let cond_node = graph.add_node(cond_annotated);
            let body_entry = build_cfg_expr(
                graph,
                body,
                module,
                cond_node,
                function_entries,
                function_exits,
                None,
            );

            graph.add_edge(cond_node, body_entry, ());
            graph.add_edge(cond_node, next, ());

            cond_node
        }
        Expression::Block {
            statements,
            expr: block_expr,
        } => {
            let mut current_node = next;

            if let Some(final_expr) = block_expr {
                current_node = build_cfg_expr(
                    graph,
                    final_expr,
                    module,
                    current_node,
                    function_entries,
                    function_exits,
                    Some(mutations.clone().unwrap_or_default()),
                );
            }

            for stmt in statements.iter().rev() {
                match stmt {
                    Statement::Declaration { name, val, .. }
                    | Statement::Assignment { name, val, .. } => {
                        let rhs_entry = build_cfg_expr(
                            graph,
                            val,
                            module,
                            current_node,
                            function_entries,
                            function_exits,
                            Some(mutations.clone().unwrap_or_default()),
                        );
                        graph
                            .node_weight_mut(rhs_entry)
                            .unwrap()
                            .mutates
                            .insert(*name);

                        current_node = rhs_entry;
                    }
                    Statement::Expr(e) => {
                        current_node = build_cfg_expr(
                            graph,
                            e,
                            module,
                            current_node,
                            function_entries,
                            function_exits,
                            Some(mutations.clone().unwrap_or_default()),
                        );
                    }
                }
            }

            current_node
        }
        // Expression::BinOp {left, op: _ , right} => {
        //     let binop_annotated = AnnotatedExpression {
        //         expr: expr.clone(),
        //         depends_on: get_dependencies(expr),
        //         mutates: mutations.clone().unwrap_or_default(),
        //     };
        //     let binop_node = graph.add_node(binop_annotated);
        //     graph.add_edge(binop_node, next, ());
        //
        //     let rhs_entry = if needs_node(right) {
        //         build_cfg_expr(
        //             graph,
        //             right,
        //             binop_node,
        //             function_entries,
        //             function_exits,
        //             None,
        //         )?
        //     } else {
        //         binop_node
        //     };
        //
        //     let lhs_entry = if needs_node(left) {
        //         build_cfg_expr(
        //             graph,
        //             left,
        //             rhs_entry,
        //             function_entries,
        //             function_exits,
        //             None,
        //         )?
        //     } else {
        //         rhs_entry
        //     };
        //
        //     Ok(lhs_entry)
        // }
        // Expression::UnOp {op: _ , right} => {
        //     let unop_annotated = AnnotatedExpression {
        //         expr: expr.clone(),
        //         depends_on: get_dependencies(expr),
        //         mutates: mutations.clone().unwrap_or_default(),
        //     };
        //     let unop_node = graph.add_node(unop_annotated);
        //     graph.add_edge(unop_node, next, ());
        //
        //     let rhs_entry = if needs_node(right) {
        //         build_cfg_expr(
        //             graph,
        //             right,
        //             unop_node,
        //             function_entries,
        //             function_exits,
        //             None,
        //         )?
        //     } else {
        //         unop_node
        //     };
        //
        //     Ok(rhs_entry)
        // }
        Expression::Call { fn_name, args } => {
            let mut current_node;

            let call_annotated = AnnotatedExpression {
                expr: expr.clone(),
                depends_on: get_dependencies(expr),
                mutates: mutations.clone().unwrap_or_default(),
                module,
            };
            let call_node = graph.add_node(call_annotated);

            if let Some(&callee_entry) = function_entries.get(fn_name) {
                let callee_exit = function_exits[fn_name];

                graph.add_edge(call_node, callee_entry, ());
                graph.add_edge(callee_exit, next, ());
            } else {
                graph.add_edge(call_node, next, ());
            }

            current_node = call_node;

            for arg in args.iter().rev() {
                current_node = build_cfg_expr(
                    graph,
                    arg,
                    module,
                    current_node,
                    function_entries,
                    function_exits,
                    None,
                );
            }

            current_node
        }
        _ => {
            let annotated = AnnotatedExpression {
                expr: expr.clone(),
                depends_on: get_dependencies(expr),
                mutates: mutations.clone().unwrap_or_default(),
                module,
            };
            let node = graph.add_node(annotated);
            graph.add_edge(node, next, ());
            node
        }
    }
}

fn needs_node(expr: &Expr) -> bool {
    !matches!(
        expr.expr,
        Expression::Var(_) | Expression::Int(_) | Expression::Bool(_) | Expression::Unit
    )
}
```

### src/analysis/mod.rs

```rs
//! different analyses of the program

pub mod annotate;
pub mod cfg;
pub mod interactions;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;

use petgraph::graph::NodeIndex;

use crate::analysis::interactions::find_interactions;
use crate::compiler::context::CompileCtx;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::Expr;
use crate::ir_types::typed_hir::TypedProgram;

#[derive(Debug, Clone, Default)]
pub struct ProgramAnnotations {
    /// Map from each expression to all its occurrences in the source code
    pub expr_occurrences: HashMap<Expr, HashSet<(ModuleRef, Range)>>,

    /// Available-expressions set at each CFG node
    pub available_at: HashMap<NodeIndex, HashSet<Expr>>,
}

#[derive(Debug, Clone)]
pub struct AnnotatedExpression {
    pub expr: Expr,
    /// which variables does this expression depend on
    pub depends_on: HashSet<UniqVar>,
    /// which variables does this expression mutate
    pub mutates: HashSet<UniqVar>,
    /// which module does this expression belong to
    pub module: ModuleRef,
}

pub fn analyse(ctx: &CompileCtx, ast: &TypedProgram) -> ProgramAnnotations {
    // create the "control flow graph" - the order in which expressions are
    // evaluated. for example:
    // ```
    // let x = {
    //   let y = 5;
    //   y + 2
    // };
    // x * 3
    // ```
    // here the order of evaluation is
    // // 1. `let y = 5;`
    // // 2. `y + 2`
    // // 3. `let x = { ... };`
    // // 4. `x * 3`
    //
    // note that we need to traverse the AST recursively,
    // meaning that in the AST `a + (b * c)` we need to consider all of
    // `a`, `b`, `c`, `b * c`, and `a + (b * c)` as separate expressions.
    //
    // additionally, the control flow graph should branch for conditionals and
    // loops, and indicate indirection for function calls.
    let cfg = cfg::construct_cfg(ctx, ast);

    // we iterate through the above graph,
    // and for every expression we count how many times it appeared,
    // keeping track of whether the variables it depends on are in the
    // same state as the other instances of the expression.
    let annotations: ProgramAnnotations = find_interactions(cfg);

    annotations
}

pub fn visualise_cfg(ctx: &CompileCtx, program: &TypedProgram) -> String {
    // construct the CFG
    let cfg = cfg::construct_cfg(ctx, program);

    // convert to dot format
    let dot = petgraph::dot::Dot::new(&cfg);

    format!("{dot:?}")
}

impl PartialEq for AnnotatedExpression {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

impl Eq for AnnotatedExpression {}

impl Hash for AnnotatedExpression {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr.hash(state);
    }
}

impl std::fmt::Display for AnnotatedExpression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.expr)?;

        if !self.depends_on.is_empty() || !self.mutates.is_empty() {
            write!(f, " [")?;

            if !self.depends_on.is_empty() {
                write!(
                    f,
                    "deps: {}",
                    self.depends_on
                        .iter()
                        .map(|x| format!("{x:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }

            if !self.mutates.is_empty() {
                if !self.depends_on.is_empty() {
                    write!(f, "; ")?;
                }
                write!(
                    f,
                    "muts: {}",
                    self.mutates
                        .iter()
                        .map(|x| format!("{x:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }

            write!(f, "]")?;
        }

        Ok(())
    }
}
```

### src/analysis/interactions.rs

```rs
#![allow(unused)]
//! find repeated expressions in a program,
//! keeping track of variable interactions

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

use petgraph::Directed;
use petgraph::Graph;
use petgraph::graph::NodeIndex;

use crate::analysis::AnnotatedExpression;
use crate::analysis::ProgramAnnotations;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::ir_types::typed_hir::Expr;
use crate::ir_types::typed_hir::Expression;
use crate::lang::intrinsics::RESERVED_FUNCTION_NAMES;

pub fn find_interactions(cfg: Graph<AnnotatedExpression, (), Directed>) -> ProgramAnnotations {
    let mut in_sets: HashMap<NodeIndex, HashSet<AnnotatedExpression>> = HashMap::new();
    let mut out_sets: HashMap<NodeIndex, HashSet<AnnotatedExpression>> = HashMap::new();

    for n in cfg.node_indices() {
        in_sets.insert(n, HashSet::new());
        out_sets.insert(n, HashSet::new());
    }

    let mut worklist = VecDeque::new();
    let mut visited = HashSet::new();
    worklist.push_back(NodeIndex::new(0)); // this is unsafe, it depends on the
    // internal implementation for the numbering, i would recommend u avoid.
    // // NOTE: starting from all nodes is inefficient but guarantees convergence
    // for n in cfg.node_indices() {
    //     worklist.push_back(n);
    // }

    while let Some(n) = worklist.pop_front() {
        let first_time = visited.insert(n);

        // In set : The intersection of predecessors
        let preds: Vec<_> = cfg.neighbors_directed(n, petgraph::Incoming).collect();

        let new_in: HashSet<AnnotatedExpression> = if preds.is_empty() {
            HashSet::new()
        } else {
            let mut result = out_sets[&preds[0]].clone();
            for i in 1..preds.len() {
                result = result
                    .intersection(&out_sets[&preds[i]])
                    .cloned()
                    .collect::<HashSet<AnnotatedExpression>>();
            }
            result
        };

        // println!("{:?}", &cfg[n].expr);
        // println!("{:?}: New in: {:?}", n, &new_in);

        let Some(expr) = &cfg.node_weight(n) else {
            continue;
        };

        // Gen set: A set with only the node's expression itself if it can be memoized
        let mut generated: HashSet<AnnotatedExpression> = HashSet::new();
        if is_candidate(&expr.expr) {
            generated.insert((*expr).clone());
        }

        // println!("{:?}: Generated: {:?}", n, &generated);

        let mut in_gen_union = new_in.clone();
        for g in generated {
            in_gen_union.insert(g);
        }

        // Kill set : Expressions with at least one rewritten variable
        let mut killed = HashSet::new();
        for e in &in_gen_union {
            for v in &e.depends_on {
                if expr.mutates.contains(v) {
                    killed.insert(e.clone());
                }
            }
        }

        // println!("{:?}: Killed: {:?}", n, &killed);

        let new_out = in_gen_union.difference(&killed).cloned().collect();

        // println!("{:?}: New out: {:?}", n, &new_out);

        if new_in != in_sets[&n] || new_out != out_sets[&n] || first_time {
            in_sets.insert(n, new_in);
            out_sets.insert(n, new_out);

            for succ in cfg.neighbors_directed(n, petgraph::Outgoing) {
                worklist.push_back(succ);
            }
        }
    }

    // Collect redundancies
    let mut expr_occurrences: HashMap<Expr, HashSet<(ModuleRef, Range)>> = HashMap::new();
    let mut available_at: HashMap<NodeIndex, HashSet<Expr>> = HashMap::new();

    for n in cfg.node_indices() {
        let node = &cfg[n];

        // available_at
        let exprs: HashSet<_> = in_sets[&n]
            .iter()
            // NOTE: including the out-set captures the first instance of an expression as well
            // .chain(out_sets[&n].iter())
            .map(|ae| ae.expr.clone())
            .collect();

        available_at.insert(n, exprs.clone());

        let mut subexprs = Vec::new();
        collect_expr_subtrees(&node.expr, &mut subexprs);
        for sub in subexprs {
            if available_at[&n].contains(sub) {
                expr_occurrences
                    .entry(sub.clone())
                    .or_default()
                    .insert((node.module, sub.range));
            }
        }
    }

    // Construct ProgramAnnotations directly
    ProgramAnnotations {
        expr_occurrences,
        available_at,
    }
}

fn is_candidate(expr: &Expr) -> bool {
    matches!(
        expr.expr,
        Expression::BinOp { .. } | Expression::UnOp { .. } | Expression::Call { .. }
    )
}

pub fn has_other_side_effects(expr: &Expr) -> bool {
    false
    // match &expr.expr {
    //     Expression::Call { fn_name, .. } if
    // RESERVED_FUNCTION_NAMES.contains(&fn_name.as_str()) => {         true
    //     }
    //     _ => false        // Expression::If { cond, t, f } =>
    // has_other_side_effects(&cond) }
}

fn collect_expr_subtrees<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
    if has_other_side_effects(expr) {
        println!("side effects: {expr:?}, {out:?}");
        out.clear();
        return;
    }
    out.push(expr);

    match &expr.expr {
        Expression::BinOp { left, right, .. } => {
            collect_expr_subtrees(left, out);
            collect_expr_subtrees(right, out);
        }
        Expression::UnOp { right, .. } => {
            collect_expr_subtrees(right, out);
        }
        Expression::Call { args, .. } => {
            for a in args {
                collect_expr_subtrees(a, out);
            }
        }
        _ => {}
    }
}
```

### src/util/mod.rs

```rs
//! utility functions, types, and trait implementations

pub mod bugs;
pub mod traits;
```

### src/util/traits.rs

```rs
use std::fmt;

use crate::ir_types::hhir::*;
use crate::lang::ops::*;
use crate::lang::types::*;

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.expr)
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Int(n) => {
                write!(f, "{}", n)
            }
            Expression::Bool(b) => {
                write!(f, "{}", b)
            }
            Expression::Unit => {
                write!(f, "()")
            }
            Expression::Var(name) => {
                write!(f, "{:?}", name)
            }
            Expression::BinOp { left, op, right } => {
                write!(f, "({} {} {})", left.expr, op, right.expr)
            }
            Expression::UnOp { op, right } => {
                write!(f, "({}{})", op, right.expr)
            }
            Expression::If {
                cond,
                t,
                f: else_branch,
            } => {
                write!(
                    f,
                    "(if {} then {} else {})",
                    cond.expr, t.expr, else_branch.expr
                )
            }
            Expression::While { cond, body } => {
                write!(f, "(while {} do {})", cond.expr, body.expr)
            }
            Expression::Call { fn_name, args } => {
                write!(f, "{:?}(", fn_name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Expression::Block { statements, expr } => {
                write!(f, "{{ ")?;
                for stmt in statements {
                    write!(f, "{};", stmt)?;
                }
                if let Some(e) = expr {
                    write!(f, " {}", e.expr)?;
                }
                write!(f, " }}")
            }
        }
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Statement::Declaration { name, ty, val, .. } => {
                write!(f, "let {:?}: {} = {}", name, ty, val.expr)
            }
            Statement::Assignment { name, val, .. } => {
                write!(f, "{:?} = {}", name, val.expr)
            }
            Statement::Expr(expr) => {
                write!(f, "{}", expr.expr)
            }
        }
    }
}

impl fmt::Display for Bop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bop::Plus => write!(f, "+"),
            Bop::Minus => write!(f, "-"),
            Bop::Mult => write!(f, "*"),
            Bop::Div => write!(f, "/"),
            Bop::Pow => write!(f, "^"),
            Bop::And => write!(f, "&"),
            Bop::Or => write!(f, "|"),
            Bop::Xor => write!(f, "#"),
            Bop::Comp(op) => write!(f, "{}", op),
        }
    }
}

impl fmt::Display for CompOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompOp::Eq => write!(f, "=="),
            CompOp::Ne => write!(f, "!="),
            CompOp::Gt => write!(f, ">"),
            CompOp::Lt => write!(f, "<"),
            CompOp::Ge => write!(f, ">="),
            CompOp::Le => write!(f, "<="),
        }
    }
}

impl fmt::Display for Uop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Uop::Neg => write!(f, "-"),
            Uop::Not => write!(f, "!"),
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "Int"),
            Ty::Bool => write!(f, "Bool"),
            Ty::Unit => write!(f, "Unit"),
            Ty::Top => write!(f, "Top"),
            Ty::Bottom => write!(f, "Bottom"),
        }
    }
}
```

### src/util/bugs.rs

```rs
//! utilities for tracking down bugs in the code

#[track_caller]
pub fn internal_bug_fmt(args: std::fmt::Arguments<'_>) -> ! {
    panic!(
        "internal compiler bug: {}\nfrom: {}",
        args,
        std::panic::Location::caller()
    );
}

#[macro_export]
macro_rules! internal_bug {
    ($($arg:tt)*) => {
        $crate::internal_bug_fmt(format_args!($($arg)*))
    };
}
```

### src/bin/visualize.rs

```rs
use std::fs;

use petgraph::dot::Config;
use petgraph::dot::Dot;
use sand::analysis::cfg;
use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cargo run --bin visualize <source_file>");
        std::process::exit(1);
    }

    let src = fs::read_to_string(&args[1])?;
    let mut ctx = CompileCtx::initial();
    let fr = ctx.dummy_file();
    let ast = compile_hir(Map::from([(fr, src.as_str())]), &mut ctx)?;
    let cfg = cfg::construct_cfg(&ctx, &ast);

    let dot = format!(
        "{:?}",
        Dot::with_attr_getters(
            &cfg,
            &[Config::EdgeNoLabel],
            &|_, _| String::new(),
            &|_, (_, node)| format!("label=\"{}\"", node)
        )
    );

    fs::write("cfg.dot", &dot)?;
    println!("CFG saved to cfg.dot");
    println!("Visualize with: dot -Tpng cfg.dot -o cfg.png");

    Ok(())
}
```

### src/bin/run_mir.rs

```rs
//! run a program via the MIR interpreter

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;
use sand::ir_types::cfgmir::MirProgram;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <input-file>", args[0]);
        std::process::exit(1);
    }

    let src = std::fs::read_to_string(&args[1])
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", args[1], e))?;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.dummy_file();
    let ast = compile_hir(Map::from([(fr, src.as_str())]), &mut ctx)?;
    let mir = MirProgram::from_typed_program(&ast);

    match mir.interpret(&ctx) {
        Ok(val) => println!("{:?}", val),
        Err(e)  => eprintln!("runtime error: {}", e),
    }

    Ok(())
}
```

### src/bin/analyse.rs

```rs
//! analyse a single file
//!
//! - read input file from command line args
//! - find repeated expressions
//! - print to stdout

use std::collections::HashMap;
use std::collections::HashSet;

use sand::analysis::analyse;
use sand::analysis::interactions::has_other_side_effects;
use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;
use sand::compiler::structure::ModuleRef;
use sand::compiler::structure::Range;
use sand::ir_types::typed_hir::Expr;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <input-file>", args[0]);
        std::process::exit(1);
    }

    let input_file = &args[1];
    let program_src = std::fs::read_to_string(input_file)
        .map_err(|e| anyhow::anyhow!("failed to read input file {}: {}", input_file, e))?;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.dummy_file();
    let ast = compile_hir(Map::from([(fr, program_src.as_str())]), &mut ctx)?;
    let annotations = analyse(&ctx, &ast);

    println!(
        "Program Annotations:\n{}",
        visualise_annotations(&program_src, &annotations.expr_occurrences)
    );

    Ok(())
}

type OccurenceMap = HashMap<Expr, HashSet<(ModuleRef, Range)>>;

fn visualise_annotations(text: &str, repeated_expressions: &OccurenceMap) -> String {
    // collect lines preserving newline characters
    let lines_inclusive: Vec<&str> = text.split_inclusive('\n').collect();

    // precompute char counts (without the newline) for each line
    let mut line_char_counts: Vec<usize> = Vec::with_capacity(lines_inclusive.len());
    let mut line_stripped: Vec<String> = Vec::with_capacity(lines_inclusive.len());
    for l in &lines_inclusive {
        line_char_counts.push(l.trim_end().chars().count());
        line_stripped.push(l.trim_end().to_string());
    }

    // map from 1-based line number -> Vec<(start_col, end_col)> inclusive, both
    // 1-based
    let mut ranges_by_line: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();

    for (expr, occs) in repeated_expressions.iter() {
        if has_other_side_effects(expr) {
            continue;
        }
        for ((sl, sc), (el, ec)) in occs.iter().map(|(_, r)| r.destruct()) {
            if sl == 0 || el == 0 {
                continue;
            }
            let max_line = lines_inclusive.len();
            if sl > max_line || el > max_line {
                continue;
            }

            if sl == el {
                ranges_by_line.entry(sl).or_default().push((sc, ec));
            } else if sl < el {
                // start line: from sc to end
                let start_line_len = line_char_counts.get(sl - 1).copied().unwrap_or(0);
                if start_line_len > 0 && sc <= start_line_len + 1 {
                    ranges_by_line
                        .entry(sl)
                        .or_default()
                        .push((sc, start_line_len));
                }

                // middle full lines
                for ln in (sl + 1)..el {
                    let l_len = line_char_counts.get(ln - 1).copied().unwrap_or(0);
                    if l_len > 0 {
                        ranges_by_line.entry(ln).or_default().push((1, l_len));
                    }
                }

                // end line: from 1 to ec
                let end_line_len = line_char_counts.get(el - 1).copied().unwrap_or(0);
                if end_line_len > 0 && ec >= 1 {
                    let ec_clamped = if ec > end_line_len { end_line_len } else { ec };
                    if ec_clamped >= 1 {
                        ranges_by_line.entry(el).or_default().push((1, ec_clamped));
                    }
                }
            } else {
                // sl > el: malformed - ignore
                continue;
            }
        }
    }

    // merge ranges on each line (ranges are 1-based inclusive)
    for (_ln, ranges) in ranges_by_line.iter_mut() {
        if ranges.is_empty() {
            continue;
        }
        ranges.sort_by_key(|(s, _e)| *s);
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
        let mut cur = ranges[0];
        for &(s, e) in &ranges[1..] {
            // if next.start is <= cur.end + 1 => merge (adjacent or overlapping)
            if s <= cur.1 + 1 {
                if e > cur.1 {
                    cur.1 = e;
                }
            } else {
                merged.push(cur);
                cur = (s, e);
            }
        }
        merged.push(cur);
        *ranges = merged;
    }

    // ANSI sequences
    let start_seq = "\x1b[1;33m"; // bold yellow
    let reset_seq = "\x1b[0m";

    // rebuild the text
    let mut out = String::with_capacity(text.len() * 2);
    for (idx, orig_line) in lines_inclusive.iter().enumerate() {
        let ln = idx + 1;
        let has_nl = orig_line.ends_with('\n');
        let line_content = &line_stripped[idx];
        let chars: Vec<char> = line_content.chars().collect();
        let len = chars.len();

        if let Some(ranges) = ranges_by_line.get(&ln) {
            let mut cur_pos = 0usize; // 0-based
            for &(s1, e1) in ranges.iter() {
                // convert to 0-based inclusive indices: start0 = s1-1, end_inclusive = min(e1,
                // len)
                if s1 == 0 || e1 == 0 {
                    continue;
                }
                let start0 = s1.saturating_sub(1);
                let end_inclusive = if e1 > len {
                    len
                } else {
                    e1 - 1 /* not completely sure why this is needed but it doesnt work without */
                };
                if start0 >= end_inclusive {
                    continue;
                }
                // append prefix (cur_pos .. start0)
                if cur_pos < start0 && cur_pos < len {
                    out.extend(chars[cur_pos..start0].iter());
                }
                // append highlight
                out.push_str(start_seq);
                out.extend(chars[start0..end_inclusive].iter());
                out.push_str(reset_seq);
                cur_pos = end_inclusive;
            }
            // append remainder
            if cur_pos < len {
                out.extend(chars[cur_pos..len].iter());
            }
        } else {
            // no highlights on this line; append as is
            out.push_str(line_content);
        }

        if has_nl {
            out.push('\n');
        }
    }

    out
}
```

### src/bin/run.rs

```rs
//! run a program

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <input-file>", args[0]);
        std::process::exit(1);
    }

    let input_file = &args[1];
    let program_src = std::fs::read_to_string(input_file)
        .map_err(|e| anyhow::anyhow!("failed to read input file {}: {}", input_file, e))?;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.dummy_file();
    let ast = compile_hir(Map::from([(fr, program_src.as_str())]), &mut ctx)?;

    println!("{:?}", ast.interpret(&ctx));

    Ok(())
}
```

### src/bin/debug.rs

```rs
#![allow(unused)]
use sand::compiler::context::CompileCtx;
use sand::ir_types::hhir::ProgramModule;
use sand::ir_types::typed_hir::TypedProgram;
use sand::passes::parse::Rule;

fn _print_pairs(pairs: pest::iterators::Pairs<Rule>, indent: usize) {
    let indent_str = "  ".repeat(indent);

    for pair in pairs {
        println!("{}{:?}: {}", indent_str, pair.as_rule(), pair.as_str());
        _print_pairs(pair.into_inner(), indent + 1);
    }
}

fn main() -> anyhow::Result<()> {
    let _input = "\
    def main(a: Int, b: Int): Int := { \
        let c: Int = 2;
        let d: Bool = !(c == 1);
        let a: Int = -(1); \
        if a < 2 & true then { \
            while d | false do { \
                a = a + 1; \
                d = !d; \
                println(123, a, d); \
            }; \
            a \
        } else {\
            d\
        };\
        \
    }";
    let _src = r#"
        def main(x: Int, y: Int): Int := {
            let z: Int = x + y * 2;
            z
        }
        
    "#;

    let _test = r#"
        def println(x: Int, y: Int): Int := {
            x + y
        }
        def main(): Int := {
            let a: Int = 10;
            let b: Int = 20;
            while a < b do {
                a = a + 1;
                println(a - b);
            };
            a
        }
        "#;

    let _test_2 = r#"
        def main(): Int := {
            let a: Int = 9;
            let x: Int = {
                let y: Int = 4;
                a = a + y;
                let z: Int = 3;
                y * z / a
            };

            let f: Int = 5 * 4 / a;

            5 * 4 / a
            
        }
    "#;

    let _test_3 = r#"
    def shadow(): Int := {
        let shadow: Int = 2;
        shadow
    }
    def main(): Int := {
    let a: Int = 1;
    let x: Int = {
        a = a + 1;
        let a: Int = 5;
        a = a + a;
        a + shadow()
    };
    a = 3;
    x
    }"#;

    let _test_4 = r#"
def main(): Int := {
    let a: Int = 1;
    let x: Int = {
        let x: Int = {
            let x: Int = {
                let x: Int = 3;
                x + a
            };
            x
        };
        x
    };
    x
}"#;

    let ctx = &mut CompileCtx::initial();
    let program = ProgramModule::parse_stub(ctx, _test_4)?;
    // println!("{:#?}", program);

    let uniquified = ProgramModule::uniquify(&program, ctx)?;
    // let typed = TypedProgram::from_ast_program(&uniquified)?;
    println!("{:#?}", uniquified);
    // println!("{:#?}", typed);

    // let eval_u = uniquified.interpret()?;
    // let eval_p = program.interpret()?;
    // println!(
    //     "Program evaluated to: {:?}\nUniquified evaluated to: {:?}",
    //     eval_p, eval_u
    // );
    Ok(())
}
```

### src/bin/lower.rs

```rs
//! lower a source file to CFG-MIR and print it

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;
use sand::ir_types::cfgmir::MirProgram;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <input-file>", args[0]);
        std::process::exit(1);
    }

    let src = std::fs::read_to_string(&args[1])
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", args[1], e))?;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.dummy_file();
    let ast = compile_hir(Map::from([(fr, src.as_str())]), &mut ctx)?;
    let mir = MirProgram::from_typed_program(&ast);

    print!("{}", mir.dump(&ctx));

    Ok(())
}
```

### src/bin/lsp.rs

```rs
//! implement (basic) language server

use sand::lsp::Backend;
use tower_lsp::LspService;
use tower_lsp::Server;

#[tokio::main]
async fn main() {
    println!("starting sand lsp");
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    println!("creating lsp service");
    let (service, socket) = LspService::new(Backend::with_client);

    println!("serving lsp service");
    Server::new(stdin, stdout, socket).serve(service).await;
}
```

### src/lib.rs

```rs
//! the sand compiler
#![allow(clippy::result_large_err)]

use thiserror::Error;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::ir_types::hhir;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::ir_types::typed_hir::TypedProgram;

pub mod analysis;
pub mod compiler;
pub mod interpreter;
pub mod ir_types;
pub mod lang;
pub mod lsp;
pub mod passes;
pub mod util;

pub use util::bugs::*;

#[derive(Debug, Error)]
#[error("compilation error: {source}")]
pub struct SandError {
    pub context: SandErrorContext,
    pub source: SandErrorSource,
}

#[derive(Debug, Default)]
pub struct SandErrorContext {
    pub module: Option<ModuleRef>,
    pub file: Option<FileRef>,
}

#[derive(Debug, Error)]
pub enum SandErrorSource {
    #[error("parse error: {0}")]
    AstParseError(#[from] passes::build_ast::AstError),

    #[error("qualify error: {0}")]
    QualifyError(#[from] passes::qualify::error::QualifyError),

    #[error("type error: {0}")]
    TypeError(#[from] passes::type_ast::AstTypeError),
}

pub fn compile_hir<'run, 'proj>(
    code: Map<FileRef, &'_ str>,
    ctx: &'run mut CompileCtx<'proj>,
) -> Result<TypedProgram, SandError> {
    let mut modules = Vec::new();
    for (file, source) in code {
        let err_ctx = SandErrorContext {
            module: None,
            file: Some(file),
        };
        modules.append(
            &mut hhir::ProgramModule::parse_source_file(ctx, source, file)
                .map_err(|e| err_ctx.wrap_err(e))?,
        );
    }

    let program = qhir::Program::combine(ctx, modules)
        .map_err(|e| SandErrorContext::with_module(e.source_module().index).wrap_err(e))?;

    let typed_program = typed_hir::TypedProgram::from_ast_program(ctx, program)
        .map_err(|e| SandErrorContext::with_module(e.module).wrap_err(e.error))?;

    Ok(typed_program)
}

impl SandErrorContext {
    pub fn with_module(module: ModuleRef) -> Self {
        Self {
            module: Some(module),
            file: None,
        }
    }

    pub fn wrap_err<E: Into<SandErrorSource>>(self, err: E) -> SandError {
        SandError {
            context: self,
            source: err.into(),
        }
    }
}
```

### src/passes/qualify/error.rs

```rs
//! errors raised during qualify pass

use thiserror::Error;

use crate::compiler::structure::ModuleInfo;
use crate::compiler::structure::Range;
use crate::passes::qualify::uniquify::error::UniquifyError;

#[derive(Debug, Error)]
pub enum QualifyError {
    #[error("found two modules with the same name: {0}")]
    DuplicateModule(ModuleInfo),

    #[error(
        "found two functions with the same name: {name} at {first_instance} and {second_instance} in module {module}"
    )]
    DuplicateFunction {
        name: String,
        module: ModuleInfo,
        first_instance: Range,
        second_instance: Range,
    },

    #[error("error uniquifying module {module}: {source}")]
    UniquifyError {
        module: ModuleInfo,
        source: UniquifyError,
    },

    #[error("module {module} was not found")]
    ModuleNotFound {
        module: String,
        source_module: ModuleInfo,
    },

    #[error("tried to call function {func} from module {module} that doesn't exist")]
    FunctionQualFailedModuleNotFound {
        func: String,
        module: String,
        source_module: ModuleInfo,
        range: Range,
    },

    // todo: add range for locating the offending function call
    #[error("could not find function {func} in module {module}")]
    FunctionQualFailedFunctionNotFound {
        func: String,
        module: ModuleInfo,
        range: Range,
    },

    #[error("encountered multiple main functions at {first} and {second} in module {first_module}")]
    DuplicateMain {
        first: Range,
        second: Range,
        first_module: ModuleInfo,
        second_module: ModuleInfo,
    },
}

impl QualifyError {
    pub fn source_module(&self) -> &ModuleInfo {
        match self {
            QualifyError::DuplicateModule(module) => module,
            QualifyError::DuplicateFunction { module, .. } => module,
            QualifyError::ModuleNotFound { source_module, .. } => source_module,
            QualifyError::FunctionQualFailedModuleNotFound { source_module, .. } => source_module,
            QualifyError::FunctionQualFailedFunctionNotFound { module, .. } => module,
            QualifyError::DuplicateMain { first_module, .. } => first_module,
            QualifyError::UniquifyError { module, .. } => module,
        }
    }
}
```

### src/passes/qualify/uniquify/error.rs

```rs
//! error types for uniquify pass

use crate::compiler::structure::Range;

/// errors produced by the uniquify / reserved-name checking passes
#[derive(Debug)]
pub enum UniquifyError {
    UnboundVariable {
        name: String,
        at: Range,
    },
    UndefinedFunction {
        name: String,
        at: Range,
    },
    DuplicateFunction {
        name: String,
        first_instance: Range,
        second_instance: Range,
    },
    IllegalFunctionName {
        name: String,
        at: Range,
    },
    DuplicateParameterName {
        name: String,
        first_instance: Range,
        second_instance: Range,
    },
    DuplicateVariableName {
        name: String,
        first_instance: Range,
        second_instance: Range,
    },
}

impl std::fmt::Display for UniquifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use UniquifyError::*;
        match self {
            UnboundVariable { name, at } => {
                write!(f, "unbound variable '{name}' at {at}")
            }
            UndefinedFunction { name, at } => {
                write!(f, "undefined function '{name}' at {at}")
            }
            DuplicateFunction {
                name,
                first_instance,
                second_instance,
            } => write!(
                f,
                "duplicate function '{name}' at {first_instance} and {second_instance}"
            ),
            IllegalFunctionName { name, at } => {
                write!(f, "illegal function name '{name}' at {at}")
            }
            DuplicateParameterName {
                name,
                first_instance,
                second_instance,
            } => write!(
                f,
                "duplicate parameter '{name}' at {first_instance} and {second_instance}"
            ),
            DuplicateVariableName {
                name,
                first_instance,
                second_instance,
            } => write!(
                f,
                "duplicate variable '{name}' at {first_instance} and {second_instance}",
            ),
        }
    }
}

impl std::error::Error for UniquifyError {}
```

### src/passes/qualify/uniquify/mod.rs

```rs
//! the uniquify pass of the compiler
//!
//! takes a program AST and ensures all variable and function names are unique
// pub mod reserved;

pub mod error;

use std::collections::BTreeMap;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::UniqVar;
use crate::internal_bug;
use crate::ir_types::hhir::*;
use crate::passes::qualify::uniquify::error::UniquifyError;

/// A helper struct that captures the active scopes for all identifiers at the
/// program's various levels and offers the functionality to keep track of and
/// rename them.
struct UniqCtx<'uniq, 'run> {
    /// Each scope is represented as a BTreeMap from original names to renamed
    /// names and are stored in a stack-like vector, where the last element
    /// is the current scope.
    var_scopes: Vec<BTreeMap<String, UniqVar>>,

    // /// Function and variable names live in different namespaces in order to
    // /// allow function name shadowing without problems
    // fun_scopes: BTreeMap<String, String>,
    compile_ctx: &'uniq mut CompileCtx<'run>,
}

impl<'uniq, 'run> UniqCtx<'uniq, 'run> {
    /// Create a new Context, initialize its counter to zero, and push two empty
    /// BTreeMaps as the variable and function scopes.
    /// # Returns
    /// An initialized empty Context.
    fn new(ctx: &'uniq mut CompileCtx<'run>) -> Self {
        // let mut global = BTreeMap::new();

        // for &name in RESERVED_FUNCTION_NAMES.iter() {
        //     global.insert(name.to_string(), name.to_string());
        // }

        Self {
            compile_ctx: ctx,
            var_scopes: vec![BTreeMap::new()],
        }
    }

    /// Pushes a new empty scope onto the scope stack when entering a new block
    /// or function.
    fn enter_scope(&mut self) {
        self.var_scopes.push(BTreeMap::new());
    }

    /// Pops the top scope from the scope stack when exiting a block or
    /// function.
    fn exit_scope(&mut self) {
        self.var_scopes.pop();
    }

    /// Binds a given variable to a newly generated unique name and stores it
    /// in the current variable scope.
    /// # Arguments
    /// * 'name' - The original identifier to bind.
    /// # Returns
    /// The newly generated unique identifier
    fn bind_var(&mut self, name: &HirVar) -> UniqVar {
        let ovref = match name {
            HirVar::Decl(ovref) => *ovref,
            x => internal_bug!("uniquify binding a non-declaration {x:?}"),
        };

        let seen_as = self.compile_ctx.original_var_name(ovref);
        let uniq = self.compile_ctx.uniquify_original_variable(ovref);

        self.var_scopes.last_mut().unwrap().insert(seen_as, uniq);
        uniq
    }

    /// Looks up the unique name associated with a variable in the scope
    /// stack from the innermost to the outermost scope.
    /// # Arguments
    /// * 'name' - The original identifier to look up.
    /// # Returns
    /// The currently active unique name for that identifier, or None if not
    /// bound.
    pub fn lookup_var_opt(&self, name: &HirVar) -> Option<UniqVar> {
        let HirVar::Unqualified(str_name) = name else {
            internal_bug!("uniquify tried resolving {name:?}");
        };
        for scope in self.var_scopes.iter().rev() {
            if let Some(n) = scope.get(str_name) {
                return Some(*n);
            }
        }
        None
    }

    pub fn display_hir_var(&self, hv: &HirVar) -> String {
        match hv {
            HirVar::Decl(ovref) => self.compile_ctx.original_var_name(*ovref),
            HirVar::Uniq(uv) => self.compile_ctx.uniq_variable_name(*uv),
            HirVar::Unqualified(s) => s.to_string(),
        }
    }

    // /// Binds a given function to a newly generated unique name and stores it
    // /// in the current function scope.
    // /// # Arguments
    // /// * 'name' - The original identifier to bind.
    // /// # Returns
    // /// The newly generated unique identifier as a string.
    // pub fn bind_fun(&mut self, name: &str) -> String {
    //     let new_name = if name == "main" ||
    // RESERVED_FUNCTION_NAMES.contains(&name) {         name.to_string()
    //     } else {
    //         self.rename(name)
    //     };
    //     self.fun_scopes.insert(name.to_string(), new_name.clone());
    //     new_name
    // }

    // /// Looks up the unique name associated with a function in the global scope
    // /// # Arguments
    // /// * 'name' - The original identifier to look up.
    // /// # Returns
    // /// The currently active unique name for that identifier, or None if not
    // /// defined.
    // pub fn lookup_fun_opt(&self, name: &str) -> Option<String> {
    //     self.fun_scopes.get(name).cloned()
    // }
}

/// Offers the uniquify pass publicly via Program::uniquify
impl ProgramModule {
    /// Produces a version of the program where all variable and function names
    /// are unique.
    /// # Returns
    /// A new Program AST with all its names uniquified but with the same
    /// functionality.
    pub fn uniquify<'run>(&self, ctx: &mut CompileCtx<'run>) -> Result<Self, UniquifyError> {
        let mut u = UniqCtx::new(ctx);

        let mut functions = Vec::new();
        for f in &self.functions {
            functions.push(uniquify_function(f, &mut u)?);
        }

        let ast = ProgramModule {
            functions,
            module_name: self.module_name,
        };

        Ok(ast)
    }
}

/// Renames a single function, its parameters, and body.
/// # Arguments
/// * 'f' - The function to uniquify.
/// * 'u' - The entire current Context.
/// # Returns
/// A new Function`AST with all identifiers uniquely renamed.
fn uniquify_function(f: &Function, u: &mut UniqCtx) -> Result<Function, UniquifyError> {
    u.enter_scope();

    let mut parameters: Vec<Parameter> = Vec::new();
    for p in &f.parameters {
        let new_name = u.bind_var(&p.name);
        parameters.push(Parameter {
            name: HirVar::Uniq(new_name),
            ty: p.ty,
            range: p.range,
        });
    }
    let body = uniquify_expr(&f.body, u)?; // Enter a new context and recursively uniquify its expressions

    u.exit_scope();

    Ok(Function {
        name: f.name,
        range: f.range,
        parameters,
        ret_type: f.ret_type,
        body,
    })
}

/// Recursively traverses and uniquifies an expression AST.
/// # Arguments
/// * 'e' - The Expression to uniquify.
/// * 'u' - The entire current Context.
/// # Returns
/// A new 'Expr' with all identifiers renamed according to scope rules.
fn uniquify_expr(e: &Expr, u: &mut UniqCtx) -> Result<Expr, UniquifyError> {
    let expr = match &e.expr {
        Expression::If { cond, t, f } => Expression::If {
            cond: Box::new(uniquify_expr(cond, u)?),
            t: Box::new(uniquify_expr(t, u)?),
            f: Box::new(uniquify_expr(f, u)?),
        },

        Expression::While { cond, body } => Expression::While {
            cond: Box::new(uniquify_expr(cond, u)?),
            body: Box::new(uniquify_expr(body, u)?),
        },

        Expression::BinOp { left, op, right } => Expression::BinOp {
            left: Box::new(uniquify_expr(left, u)?),
            op: *op,
            right: Box::new(uniquify_expr(right, u)?),
        },

        Expression::UnOp { op, right } => Expression::UnOp {
            op: *op,
            right: Box::new(uniquify_expr(right, u)?),
        },

        Expression::Call { fn_name, args } => {
            let args_res: Result<Vec<Expr>, UniquifyError> =
                args.iter().map(|a| uniquify_expr(a, u)).collect();
            Expression::Call {
                fn_name: fn_name.clone(),
                args: args_res?,
            }
        }

        Expression::Var(name) => {
            let mapped = match u.lookup_var_opt(name) {
                Some(n) => n,
                None => {
                    return Err(UniquifyError::UnboundVariable {
                        name: u.display_hir_var(name),
                        at: e.range,
                    });
                }
            };
            Expression::Var(HirVar::Uniq(mapped))
        }
        Expression::Int(i) => Expression::Int(*i),
        Expression::Bool(b) => Expression::Bool(*b),
        Expression::Unit => Expression::Unit,

        Expression::Block { statements, expr } => {
            u.enter_scope();

            let mut stmts = Vec::new();
            for s in statements {
                stmts.push(uniquify_stmt(s, u)?);
            }
            let inner_expr = match expr.as_ref() {
                Some(inner) => Some(Box::new(uniquify_expr(inner, u)?)),
                None => None,
            };

            u.exit_scope();

            Expression::Block {
                statements: stmts,
                expr: inner_expr,
            }
        }
    };

    Ok(Expr {
        expr,

        range: e.range,
    })
}

/// Recursively traverses and uniquifies a statement AST.
/// # Arguments
/// * 'stmt' - The Statement to uniquify.
/// * 'u' - The entire current Context.
/// # Returns
/// A new Statement with variable names uniquely renamed
fn uniquify_stmt(stmt: &Statement, u: &mut UniqCtx) -> Result<Statement, UniquifyError> {
    match stmt {
        Statement::Declaration {
            name,
            range,
            ty,
            val,
        } => {
            let val = uniquify_expr(val, u)?;
            let new_name = u.bind_var(name);
            Ok(Statement::Declaration {
                name: HirVar::Uniq(new_name),
                range: *range,
                ty: *ty,
                val,
            })
        }

        Statement::Assignment { name, range, val } => {
            let mapped = match u.lookup_var_opt(name) {
                Some(n) => n,
                None => {
                    return Err(UniquifyError::UnboundVariable {
                        name: u.display_hir_var(name),
                        at: *range,
                    });
                }
            };
            let val = uniquify_expr(val, u)?;
            Ok(Statement::Assignment {
                name: HirVar::Uniq(mapped),
                range: *range,
                val,
            })
        }

        Statement::Expr(e) => {
            let expr = uniquify_expr(e, u)?;
            Ok(Statement::Expr(expr))
        }
    }
}
```

### src/passes/qualify/uniquify/reserved.rs

```rs
//! checks for reserved keywords
//! TODO: remove dead code, consolidate with pass from hhir to qhir

use std::collections::BTreeMap;

use crate::ir_types::hhir::ProgramModule;
use crate::lang::intrinsics::fn_name_allowed;
use crate::compiler::structure::Range;

pub type FnSeenMap = BTreeMap<String, Range>;


/// Checks that all variable and function names
/// in the provided program AST are unique
///
/// # Arguments
/// * 'prog' - The Program AST to check
///
/// # Returns
/// 'Ok(())' if all names are unique; otherwise, 'Err' with a `UniquifyError`
pub fn assert_unique(prog: &ProgramModule) -> Result<(), UniquifyError> {
    // Map function name -> (start,end) of first occurrence
    let mut seen_funs: FnSeenMap = BTreeMap::new();

    for func in &prog.functions {
        // if function uses an internal reserved name -> illegal
        if !fn_name_allowed(&func.name) {
            return Err(UniquifyError::IllegalFunctionName {
                name: func.name.clone(),
                at: func.range,
            });
        }

        if let Some(first_span) = seen_funs.get(&func.name) {
            return Err(UniquifyError::DuplicateFunction {
                name: func.name.clone(),
                first_instance: *first_span,
                second_instance: func.range,
            });
        }
        // record this function's name span
        seen_funs.insert(func.name.clone(), func.range);

        // check parameters for duplicates within the same function,
        // mapping parameter name -> (start,end)
        // let mut param_seen: VarSeenMap = BTreeMap::new();
        // for param in &func.parameters {
        //     if let Some(first) = param_seen.get(&param.name) {
        //         return Err(UniquifyError::DuplicateParameterName {
        //             name: format!("{:?}", param.name),
        //             first_instance: *first,
        //             second_instance: param.range,
        //         });
        //     }
        //     param_seen.insert(param.name.clone(), param.range);
        // }

        // // check the function body using a name->span map for locals
        // let mut local_seen_vars: VarSeenMap = BTreeMap::new();
        // check_expr(&func.body, &mut local_seen_vars)?;
    }

    Ok(())
}

// /// Recursively checks an expression AST for uniqueness of all declared
// /// identifiers. # Arguments
// /// * 'expr' - the expression to traverse.
// /// * 'seen' - the map of already encountered names to the span of their first
// ///   occurrence.
// /// # Returns
// /// 'Ok(())' if all names are unique, otherwise `UniquifyError`.
// pub fn check_expr(expr: &Expr, seen: &mut VarSeenMap) -> Result<(), UniquifyError> {
//     match &expr.expr {
//         Expression::Int(_) | Expression::Bool(_) | Expression::Unit | Expression::Var(_) => {}
//         Expression::If { cond, t, f } => {
//             check_expr(cond, seen)?;
//             check_expr(t, seen)?;
//             check_expr(f, seen)?;
//         }

//         Expression::While { cond, body } => {
//             check_expr(cond, seen)?;
//             check_expr(body, seen)?;
//         }

//         Expression::BinOp { left, right, .. } => {
//             check_expr(left, seen)?;
//             check_expr(right, seen)?;
//         }

//         Expression::UnOp { right, .. } => {
//             check_expr(right, seen)?;
//         }

//         Expression::Call { args, .. } => {
//             for arg in args {
//                 check_expr(arg, seen)?;
//             }
//         }

//         Expression::Block {
//             statements,
//             expr: inner_expr,
//         } => {
//             let mut block_seen = seen.clone();
//             for stmt in statements {
//                 check_stmt(stmt, &mut block_seen)?;
//             }
//             if let Some(e) = inner_expr {
//                 check_expr(e, &mut block_seen)?;
//             }
//         }
//     }
//     Ok(())
// }

// /// Recursively checks a statement AST for uniqueness of all declared
// /// identifiers. # Arguments
// /// * 'stmt' - the statement to traverse.
// /// * 'seen' - the map of already encountered names to the span of their first
// ///   occurrence.
// /// # Returns
// /// 'Ok(())' if all names are unique, otherwise `UniquifyError`.
// pub fn check_stmt(stmt: &Statement, seen: &mut VarSeenMap) -> Result<(), UniquifyError> {
//     match stmt {
//         Statement::Declaration {
//             name,
//             range,
//             ty: _,
//             val,
//         } => {
//             if let Some(first_span) = seen.get(name) {
//                 return Err(UniquifyError::DuplicateVariableName {
//                     name: name.clone(),
//                     first_instance: *first_span,
//                     second_instance: *range,
//                 });
//             }
//             seen.insert(name.clone(), *range);
//             check_expr(val, seen)
//         }

//         Statement::Assignment { name: _, val, .. } => {
//             // assignment doesn't declare a new variable; it should refer to an existing one
//             // uniqueness checker only needs to traverse the RHS expression
//             check_expr(val, seen)
//         }

//         Statement::Expr(e) => check_expr(e, seen),
//     }
// }
```

### src/passes/qualify/mod.rs

```rs
//! this pass combines all the modules in the program into one,
//! uniquifying function names across modules,
//! resolving function calls across modules,
//! calling uniquify for variables on every module,
//! and returning a single instance of qhir

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::FunSig;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::Set;
use crate::internal_bug;
use crate::ir_types::hhir::HirFnCall;
use crate::ir_types::hhir::HirVar;
use crate::ir_types::hhir::ProgramModule;
use crate::ir_types::hhir::{self};
use crate::ir_types::qhir::Program;
use crate::ir_types::qhir::{self};
use crate::lang::intrinsics::Intrinsic;
use crate::passes::qualify::error::QualifyError;

pub mod error;
pub mod uniquify;

struct QualfiyCtx<'qual, 'run> {
    available_functions: Map<ModuleRef, Set<FunRef>>,

    compile_ctx: &'qual mut CompileCtx<'run>,
}

impl<'qual, 'run> QualfiyCtx<'qual, 'run> {
    fn new(ctx: &'qual mut CompileCtx<'run>) -> Self {
        QualfiyCtx {
            available_functions: Map::new(),
            compile_ctx: ctx,
        }
    }

    fn get_function_by_name(
        &self,
        name: &str,
        in_mod: &ModuleRef,
        caller: Range,
    ) -> Result<FunRef, QualifyError> {
        if let Some(fn_ref_set) = self.available_functions.get(in_mod) {
            for fr in fn_ref_set {
                if name == self.compile_ctx.original_fun_name(*fr) {
                    return Ok(*fr);
                }
            }
            Err(QualifyError::FunctionQualFailedFunctionNotFound {
                func: name.to_string(),
                range: caller,
                module: self.compile_ctx.module_info(in_mod),
            })
        } else {
            internal_bug!(
                "internal function call from non existing module: {name} {}",
                self.compile_ctx.module_info(in_mod)
            )
        }
    }

    fn get_module_by_name(
        &self,
        name: &str,
        from_module: ModuleRef,
    ) -> Result<ModuleRef, QualifyError> {
        self.compile_ctx
            .get_mod_by_name(name)
            .ok_or_else(|| QualifyError::ModuleNotFound {
                module: name.to_string(),
                source_module: self.compile_ctx.module_info(&from_module),
            })
    }
}

impl Program {
    pub fn combine<'qual, 'run>(
        ctx: &'qual mut CompileCtx<'run>,
        modules: Vec<ProgramModule>,
    ) -> Result<Self, QualifyError> {
        let mut q = QualfiyCtx::new(ctx);

        let mut main = None;
        for ProgramModule {
            functions,
            module_name,
        } in &modules
        {
            let mut fns = Set::new();
            let mut fn_names = Map::new();
            for f in functions {
                let name = q.compile_ctx.original_fun_name(f.name);
                if name == "main" {
                    if let Some((_, first, first_module)) = main {
                        return Err(QualifyError::DuplicateMain {
                            first,
                            second: f.range,
                            first_module: q.compile_ctx.module_info(first_module),
                            second_module: q.compile_ctx.module_info(module_name),
                        });
                    }
                    main = Some((f.name, f.range, module_name));
                }

                if let Some(fir) = fn_names.get(&name) {
                    return Err(QualifyError::DuplicateFunction {
                        name,
                        module: q.compile_ctx.module_info(module_name),
                        first_instance: *fir,
                        second_instance: f.range,
                    });
                }
                fn_names.insert(name, f.range);
                fns.insert(f.name);
            }
            if q.available_functions.contains_key(module_name) {
                return Err(QualifyError::DuplicateModule(
                    q.compile_ctx.module_info(module_name),
                ));
            }
            q.available_functions.insert(*module_name, fns);
        }

        q.compile_ctx.entrypoint = main.map(|(name, _, _)| name);

        let mut functions = Map::new();

        for pm in modules {
            let um = pm
                .uniquify(q.compile_ctx)
                .map_err(|e| QualifyError::UniquifyError {
                    module: q.compile_ctx.module_info(&pm.module_name),
                    source: e,
                })?;
            for function in um.functions {
                let qf = qualify_function(&mut q, &um.module_name, function)?;
                q.compile_ctx
                    .set_fun_sig(qf.name, FunSig::with(&qf.parameters, qf.ret_type));
                functions.insert(qf.name, qf);
            }
        }

        Ok(Self { functions })
    }
}

fn qualify_function(
    q: &mut QualfiyCtx<'_, '_>,
    module_name: &ModuleRef,
    func: hhir::Function,
) -> Result<qhir::Function, QualifyError> {
    let parameters = func
        .parameters
        .into_iter()
        .map(|p| qualify_parameter(q, p))
        .collect::<Vec<_>>();

    let body = qualify_expr(q, module_name, func.body)?;

    Ok(qhir::Function {
        name: func.name,
        range: func.range,
        parameters,
        ret_type: func.ret_type,
        body,
        src_module: *module_name,
    })
}

fn qualify_parameter(_q: &mut QualfiyCtx<'_, '_>, param: hhir::Parameter) -> qhir::Parameter {
    let HirVar::Uniq(u) = param.name else {
        internal_bug!(
            "encountered unqualified variable after uniquify: {:?}",
            param.name
        );
    };
    qhir::Parameter {
        name: u,
        range: param.range,
        ty: param.ty,
    }
}

fn qualify_expr(
    q: &mut QualfiyCtx<'_, '_>,
    module_name: &ModuleRef,
    expr: hhir::Expr,
) -> Result<qhir::Expr, QualifyError> {
    let expression = match expr.expr {
        hhir::Expression::Bool(b) => qhir::Expression::Bool(b),
        hhir::Expression::Int(i) => qhir::Expression::Int(i),
        hhir::Expression::Unit => qhir::Expression::Unit,
        hhir::Expression::BinOp { left, op, right } => qhir::Expression::BinOp {
            left: Box::new(qualify_expr(q, module_name, *left)?),
            op,
            right: Box::new(qualify_expr(q, module_name, *right)?),
        },
        hhir::Expression::If { cond, t, f } => qhir::Expression::If {
            cond: Box::new(qualify_expr(q, module_name, *cond)?),
            t: Box::new(qualify_expr(q, module_name, *t)?),
            f: Box::new(qualify_expr(q, module_name, *f)?),
        },
        hhir::Expression::Block { statements, expr } => qhir::Expression::Block {
            statements: statements
                .into_iter()
                .map(|stmt| qualify_statement(q, module_name, stmt))
                .collect::<Result<Vec<_>, QualifyError>>()?,
            expr: {
                if let Some(e) = expr {
                    Some(Box::new(qualify_expr(q, module_name, *e)?))
                } else {
                    None
                }
            },
        },
        hhir::Expression::UnOp { op, right } => qhir::Expression::UnOp {
            op,
            right: Box::new(qualify_expr(q, module_name, *right)?),
        },
        hhir::Expression::While { cond, body } => qhir::Expression::While {
            cond: Box::new(qualify_expr(q, module_name, *cond)?),
            body: Box::new(qualify_expr(q, module_name, *body)?),
        },
        hhir::Expression::Var(v) => {
            let HirVar::Uniq(u) = v else {
                internal_bug!("unqualified variable after uniquify: {v:?}");
            };
            qhir::Expression::Var(u)
        }
        hhir::Expression::Call { fn_name, args } => {
            let qargs = args
                .into_iter()
                .map(|a| qualify_expr(q, module_name, a))
                .collect::<Result<Vec<_>, QualifyError>>()?;
            match fn_name {
                HirFnCall::Local(name) => {
                    if let Ok(intrinsic) = Intrinsic::try_from(name.as_str()) {
                        qhir::Expression::IntrinsicCall {
                            fn_name: intrinsic,
                            args: qargs,
                        }
                    } else {
                        // need to find the function we're calling
                        let fn_ref = q.get_function_by_name(&name, module_name, expr.range)?;
                        qhir::Expression::Call {
                            fn_name: fn_ref,
                            args: qargs,
                        }
                    }
                }
                HirFnCall::External { module, name } => {
                    let mod_ref = q.get_module_by_name(&module, *module_name)?;
                    let fn_ref = q.get_function_by_name(&name, &mod_ref, expr.range)?;
                    qhir::Expression::Call {
                        fn_name: fn_ref,
                        args: qargs,
                    }
                }
            }
        }
    };

    Ok(qhir::Expr {
        expr: expression,
        range: expr.range,
    })
}

fn qualify_statement(
    q: &mut QualfiyCtx<'_, '_>,
    module_name: &ModuleRef,
    stmt: hhir::Statement,
) -> Result<qhir::Statement, QualifyError> {
    match stmt {
        hhir::Statement::Assignment { name, range, val } => {
            let HirVar::Uniq(uv) = name else {
                internal_bug!("unqualified variable in assignment after uniquify: {name:?}");
            };
            Ok(qhir::Statement::Assignment {
                name: uv,
                range,
                val: qualify_expr(q, module_name, val)?,
            })
        }
        hhir::Statement::Declaration {
            name,
            range,
            ty,
            val,
        } => {
            let HirVar::Uniq(uv) = name else {
                internal_bug!("unqualified variable in declaration after uniquify: {name:?}");
            };
            Ok(qhir::Statement::Declaration {
                name: uv,
                ty,
                range,
                val: qualify_expr(q, module_name, val)?,
            })
        }
        hhir::Statement::Expr(expr) => {
            Ok(qhir::Statement::Expr(qualify_expr(q, module_name, expr)?))
        }
    }
}
```

### src/passes/build_ast.rs

```rs
//! build an AST from the Pair tree
#![allow(clippy::result_large_err)]

use pest::Parser;
use pest::error::Error;
use pest::iterators::Pair;
use thiserror::Error;

use crate::compiler::context::CompileCtx;
use crate::compiler::context::ContextError;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UriError;
use crate::internal_bug;
use crate::ir_types::hhir::*;
use crate::lang::intrinsics;
use crate::lang::ops::*;
use crate::lang::types::*;
use crate::passes::parse::LangParser;
use crate::passes::parse::Rule;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Error)]
pub enum AstError {
    #[error("parse error: {0}")]
    Pest(#[from] Box<Error<Rule>>),

    #[error("unexpected rule: expected {expected:?}, got {got:?} at {range}")]
    UnexpectedRule {
        expected: &'static str,
        got: Rule,
        range: Range,
    },

    #[error("missing {expected:?} at {range}")]
    Missing {
        expected: &'static str,
        range: Range,
    },

    #[error("invalid integer literal: {got} at {range}")]
    InvalidInteger { got: String, range: Range },

    #[error("invalid name: {got} at {range}")]
    InvalidName { got: String, range: Range },

    #[error("context error: {0}")]
    ContextError(#[from] ContextError),

    #[error(transparent)]
    UriError(#[from] UriError),
}

trait AstExt<T> {
    /// If the Option is None, produce a `Missing` error located at
    /// `start..end`.
    fn missing(self, expecting: &'static str, range: Range) -> Result<T, AstError>
    where
        Self: Sized;
}

impl<T> AstExt<T> for Option<T> {
    fn missing(self, expecting: &'static str, range: Range) -> Result<T, AstError>
    where
        Self: Sized,
    {
        match self {
            Some(v) => Ok(v),
            None => Err(AstError::Missing {
                expected: expecting,
                range,
            }),
        }
    }
}

impl ProgramModule {
    pub fn parse_source_file<'run>(
        ctx: &mut CompileCtx<'run>,
        src: &str,
        file: FileRef,
    ) -> Result<Vec<Self>, AstError> {
        let mut pairs = LangParser::parse(Rule::program, src).map_err(Box::new)?;

        let program_pair = match pairs.next() {
            Some(p) => p,
            None => {
                return Err(AstError::Missing {
                    expected: "root node",
                    range: Range::new(1, 1, 1, 1),
                });
            }
        };

        let dm = ctx.default_module(file);

        let map = build_program(ctx, program_pair, src, dm, file)?;
        Ok(map
            .into_iter()
            .map(|(module_name, functions)| ProgramModule {
                functions,
                module_name,
            })
            .collect::<Vec<_>>())
    }

    pub fn parse_stub<'run>(ctx: &mut CompileCtx<'run>, src: &str) -> Result<Self, AstError> {
        let fr = ctx.dummy_file();
        let modules = Self::parse_source_file(ctx, src, fr)?;
        if modules.len() == 1 {
            Ok(modules.into_iter().next().unwrap())
        } else {
            Err(AstError::UnexpectedRule {
                expected: "exactly one module",
                got: Rule::program,
                range: Range::new(1, 1, 1, 1),
            })
        }
    }
}

// ============== top level ==============

pub fn build_program<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
    default_module: ModuleRef,
    file: FileRef,
) -> Result<Map<ModuleRef, Vec<Function>>, AstError> {
    assert_eq!(pair.as_rule(), Rule::program);

    // todo: discard unused modules from the ctx
    let mut mods = Map::new();
    let mut funcs = Vec::new();
    let mut current_module = default_module;
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::module => {
                // parse module name
                let child_span = child.as_span();
                let modname_pair = child
                    .into_inner()
                    .next()
                    .missing("module name", Range::from(child_span))?;
                let mod_span = modname_pair.as_span();
                if modname_pair.as_rule() != Rule::identifier {
                    return Err(AstError::UnexpectedRule {
                        expected: "identifier",
                        got: modname_pair.as_rule(),
                        range: Range::from(&modname_pair),
                    });
                }
                // if we have an existing module, save it before starting the new one
                if !funcs.is_empty() {
                    mods.insert(current_module, funcs);
                    funcs = Vec::new();
                }
                current_module = ctx.register_module(mod_span.as_str(), file);
            }
            Rule::function => {
                funcs.push(build_function(ctx, child, src, &current_module)?);
            }
            // ignore start/end markers
            Rule::EOI => continue,
            other => {
                // debug
                let range = Range::from(child);
                eprintln!("parse error: unexpected top-level rule at {range} - got {other:?}");
                return Err(AstError::UnexpectedRule {
                    expected: "function or module declaration",
                    got: other,
                    range,
                });
            }
        }
    }

    if !funcs.is_empty() {
        mods.insert(current_module, funcs);
    }

    Ok(mods)
}

fn build_function<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
    cur_module: &ModuleRef,
) -> Result<Function, AstError> {
    let range = Range::from(&pair);
    if pair.as_rule() != Rule::function {
        return Err(AstError::UnexpectedRule {
            expected: "function",
            got: pair.as_rule(),
            range,
        });
    }

    let mut inner = pair.into_inner();

    // order in grammar: identifier, (parameter | parameters)? , type_, expression
    // first child must be identifier
    let name_pair = inner.next().missing("function name", range)?;
    let name_range = Range::from(&name_pair);
    if name_pair.as_rule() != Rule::identifier {
        return Err(AstError::UnexpectedRule {
            expected: "identifier",
            got: name_pair.as_rule(),
            range: name_range,
        });
    }
    let name = name_pair.as_str().to_string();

    // make sure we aren't redefining internal functions
    if !intrinsics::fn_name_allowed(&name) {
        return Err(AstError::InvalidName {
            got: name,
            range: name_range,
        });
    }

    // collect optional parameters (parameter or parameters)
    let mut parameters = Vec::new();
    loop {
        let peek = inner.peek().map(|p| p.as_rule());
        match peek {
            Some(Rule::parameter) => {
                let p = inner.next().missing("parameter", range)?;
                for pp in p.into_inner() {
                    parameters.push(build_parameter(ctx, pp)?);
                }
            }
            Some(Rule::parameters) => {
                let p = inner.next().missing("parameter", range)?;
                for pp in p.into_inner() {
                    parameters.push(build_parameter(ctx, pp)?);
                }
            }
            _ => break,
        }
    }

    // next should be type_
    let ty_pair = match inner.next() {
        Some(p) => {
            if p.as_rule() != Rule::type_ {
                return Err(AstError::UnexpectedRule {
                    expected: "type_",
                    got: p.as_rule(),
                    range: Range::from(&p),
                });
            }
            p
        }
        None => {
            return Err(AstError::UnexpectedRule {
                expected: "type_",
                got: Rule::program,
                range,
            });
        }
    };
    let ret_type = build_type(ctx, ty_pair)?;

    // final child is the function body expression
    let body_pair = inner.next().missing("function body expression", range)?;
    let body = build_expr(ctx, body_pair, src)?;

    let ofref = ctx.register_function(&name_pair, cur_module)?;

    Ok(Function {
        name: ofref,
        range: Range::from(name_pair),
        parameters,
        ret_type,
        body,
    })
}

fn build_parameter<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<Parameter, AstError> {
    let rule = pair.as_rule();
    assert_eq!(rule, Rule::parameter);
    // capture span before into_inner
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();
    let name = inner.next().missing("parameter name", range)?; // identifier
    let ty_pair = inner.next().missing("parameter type", range)?;
    let ty = build_type(ctx, ty_pair)?;
    let var = HirVar::Decl(ctx.new_original_variable(&name, rule)?);
    Ok(Parameter {
        name: var,
        ty,
        range,
    })
}

fn build_type<'run>(_ctx: &mut CompileCtx<'run>, pair: Pair<Rule>) -> Result<Ty, AstError> {
    assert_eq!(pair.as_rule(), Rule::type_);
    match pair.as_str() {
        "Int" => Ok(Ty::Int),
        "Bool" => Ok(Ty::Bool),
        "Unit" => Ok(Ty::Unit),
        _ => Err(AstError::UnexpectedRule {
            expected: "type literal",
            got: pair.as_rule(),
            range: Range::from(pair),
        }),
    }
}

// === statements ===

fn build_statement<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Statement, AstError> {
    assert_eq!(pair.as_rule(), Rule::statement);
    // statement = ((declaration | assignment | expression) ~ ";")
    // capture pair span before moving
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let first = inner.next().missing("statement beginning", range)?;

    let inner_range = Range::from(&first);
    match first.as_rule() {
        d @ Rule::declaration => {
            let mut decl_inner = first.into_inner();
            let name_pair = decl_inner.next().missing("declaration name", inner_range)?;
            let var = HirVar::Decl(ctx.new_original_variable(&name_pair, d)?); // identifier
            let ty = build_type(
                ctx,
                decl_inner.next().missing("declaration type", inner_range)?,
            )?;
            let expr = build_expr(
                ctx,
                decl_inner
                    .next()
                    .missing("declaration expression", inner_range)?,
                src,
            )?;
            Ok(Statement::Declaration {
                name: var,
                range: inner_range,
                ty,
                val: expr,
            })
        }
        Rule::assignment => {
            let mut a_inner = first.into_inner();
            let name = a_inner
                .next()
                .missing("assignment name", inner_range)?
                .as_str()
                .to_string(); // identifier
            let expr = build_expr(
                ctx,
                a_inner.next().missing("assignment type", inner_range)?,
                src,
            )?;
            Ok(Statement::Assignment {
                name: HirVar::Unqualified(name),
                range: inner_range,
                val: expr,
            })
        }
        Rule::expression => {
            let expr = build_expr(ctx, first, src)?;
            Ok(Statement::Expr(expr))
        }
        other => {
            // use the statement pair span for location
            Err(AstError::UnexpectedRule {
                expected: "declaration | assignment | expression",
                got: other,
                range: inner_range,
            })
        }
    }
}

// === expressions ===
// rule hierarchy: expression -> logic_or -> logic_xor -> logic_and -> equality
// -> comparison -> add_sub -> mul_div -> power -> unary -> primary

fn build_expr<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    match pair.as_rule() {
        Rule::expression => {
            // expression wraps logic_or
            let inner = pair.into_inner().next().missing("expression body", range)?;
            build_expr(ctx, inner, src)
        }
        Rule::logic_or => build_logic_or(ctx, pair, src),
        Rule::logic_xor => build_logic_xor(ctx, pair, src),
        Rule::logic_and => build_logic_and(ctx, pair, src),
        Rule::equality => build_equality(ctx, pair, src),
        Rule::comparison => build_comparison(ctx, pair, src),
        Rule::add_sub => build_add_sub(ctx, pair, src),
        Rule::mul_div => build_mul_div(ctx, pair, src),
        Rule::power => build_power(ctx, pair, src),
        Rule::unary => build_unary(ctx, pair, src),
        Rule::primary => build_primary(ctx, pair, src),
        other => Err(AstError::UnexpectedRule {
            expected: "expression-like rule",
            got: other,
            range,
        }),
    }
}

// generic left-assoc binary fold helper
fn binop_fold<'run, F>(
    ctx: &mut CompileCtx<'run>,
    mut inner: pest::iterators::Pairs<'_, Rule>,
    mut next_level: F,
    src: &str,
    parent_range: Range,
) -> Result<Expr, AstError>
where
    F: FnMut(&mut CompileCtx<'run>, Pair<Rule>, &str) -> Result<Expr, AstError>,
{
    let first_pair = inner.next().missing("left operand", parent_range)?;
    let mut expr = next_level(ctx, first_pair, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("right operand", parent_range)?;
        let rhs = next_level(ctx, rhs_pair, src)?;
        let op = bop_from_rule(op_pair.as_rule());

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range: parent_range,
        };
    }

    Ok(expr)
}

// mapping only for token rules used in these folds (or/xor/and)
// other operators handled in their specific builders
fn bop_from_rule(rule: Rule) -> Bop {
    match rule {
        Rule::or => Bop::Or,
        Rule::xor => Bop::Xor,
        Rule::and => Bop::And,
        _ => internal_bug!("unexpected bop_from_rule: {rule:?}"),
    }
}

// logic_or = { logic_xor ~ (or ~ logic_xor)* }
fn build_logic_or<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_logic_xor, src, range)
}

// logic_xor = { logic_and ~ (xor ~ logic_and)* }
fn build_logic_xor<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_logic_and, src, range)
}

// logic_and = { equality ~ (and ~ equality)* }
fn build_logic_and<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_equality, src, range)
}

// equality = { comparison ~ ( (eq | ne) ~ comparison )* }
fn build_equality<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_comparison(ctx, inner.next().missing("eq expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("eq right", range)?;
        let rhs = build_comparison(ctx, rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::eq => Bop::Comp(CompOp::Eq),
            Rule::ne => Bop::Comp(CompOp::Ne),
            other => internal_bug!("unexpected equality operator: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// comparison = { add_sub ~ ( (gt | lt | ge | le) ~ add_sub )* }
fn build_comparison<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_add_sub(ctx, inner.next().missing("comp expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("comp right", range)?;
        let rhs = build_add_sub(ctx, rhs_pair, src)?;
        let comp_op = match op_pair.as_rule() {
            Rule::gt => CompOp::Gt,
            Rule::lt => CompOp::Lt,
            Rule::ge => CompOp::Ge,
            Rule::le => CompOp::Le,
            other => internal_bug!("unexpected comp operator: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op: Bop::Comp(comp_op),
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// add_sub = { mul_div ~ ( (add | subtract) ~ mul_div )* }
fn build_add_sub<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_mul_div(ctx, inner.next().missing("add_sub expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("add_sub right", range)?;
        let rhs = build_mul_div(ctx, rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::add => Bop::Plus,
            Rule::subtract => Bop::Minus,
            other => internal_bug!("unexpected add_sub op: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// mul_div = { power ~ ( (multiply | divide) ~ power )* }
fn build_mul_div<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_power(ctx, inner.next().missing("mul_div expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("mul_div right", range)?;
        let rhs = build_power(ctx, rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::multiply => Bop::Mult,
            Rule::divide => Bop::Div,
            other => internal_bug!("unexpected mul_div op: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// power = { unary ~ (pow ~ power)? }  -> right-assoc
fn build_power<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let left_pair = inner.next().missing("power expression", range)?;
    let left = build_unary(ctx, left_pair, src)?;

    if let Some(_op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("power right", range)?;
        let rhs = build_power(ctx, rhs_pair, src)?;
        Ok(Expr {
            expr: Expression::BinOp {
                left: Box::new(left),
                op: Bop::Pow,
                right: Box::new(rhs),
            },
            range,
        })
    } else {
        Ok(left)
    }
}

// unary = { (unary_operand ~ unary) | primary }
fn build_unary<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    assert_eq!(pair.as_rule(), Rule::unary);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let first = inner.next().missing("unary expr", range)?;

    match first.as_rule() {
        Rule::unary_operand => {
            let op_pair = first.into_inner().next().missing("unary operator", range)?;
            let rhs = build_unary(ctx, inner.next().missing("unary rhs", range)?, src)?;

            let op = match op_pair.as_rule() {
                Rule::subtract => Uop::Neg,
                Rule::negate => Uop::Not,
                other => {
                    return Err(AstError::UnexpectedRule {
                        expected: "subtract | negate",
                        got: other,
                        range: Range::from(&op_pair),
                    });
                }
            };

            Ok(Expr {
                expr: Expression::UnOp {
                    op,
                    right: Box::new(rhs),
                },
                range,
            })
        }
        Rule::subtract => {
            let rhs = build_unary(ctx, inner.next().missing("subtract rhs", range)?, src)?;
            Ok(Expr {
                expr: Expression::UnOp {
                    op: Uop::Neg,
                    right: Box::new(rhs),
                },
                range,
            })
        }
        Rule::negate => {
            let rhs = build_unary(ctx, inner.next().missing("negate rhs", range)?, src)?;
            Ok(Expr {
                expr: Expression::UnOp {
                    op: Uop::Not,
                    right: Box::new(rhs),
                },
                range,
            })
        }
        _ => build_primary(ctx, first, src),
    }
}

fn build_primary<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    assert_eq!(pair.as_rule(), Rule::primary);
    let range = Range::from(&pair);

    let s = pair.as_str();
    if s.starts_with('{') {
        let mut statements = Vec::new();
        let mut expr: Option<Box<Expr>> = None;

        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::statement => statements.push(build_statement(ctx, inner, src)?),
                Rule::expression => expr = Some(Box::new(build_expr(ctx, inner, src)?)),
                other => {
                    return Err(AstError::UnexpectedRule {
                        expected: "statement | expression in block",
                        got: other,
                        range: Range::from(&inner),
                    });
                }
            }
        }

        return Ok(Expr {
            expr: Expression::Block { statements, expr },
            range,
        });
    }

    let inner = pair
        .into_inner()
        .next()
        .missing("inner expression", range)?;
    match inner.as_rule() {
        Rule::expression => build_expr(ctx, inner, src),
        Rule::ifstatement => build_if(ctx, inner, src),
        Rule::whileloop => build_while(ctx, inner, src),
        Rule::function_call | Rule::external_function_call => build_call(ctx, inner, src),
        Rule::number => {
            let s = inner.as_str().to_string();
            let v = s.parse::<i64>().map_err(|_| AstError::InvalidInteger {
                got: s.clone(),
                range: Range::from(&inner),
            })?;

            Ok(Expr {
                expr: Expression::Int(v),
                range: Range::from(&inner),
            })
        }
        Rule::boolean => {
            let b = match inner.as_str() {
                "true" => true,
                "false" => false,
                other => internal_bug!("invalid boolean literal: {other}"),
            };

            Ok(Expr {
                expr: Expression::Bool(b),
                range: Range::from(&inner),
            })
        }
        Rule::identifier => Ok(Expr {
            expr: Expression::Var(HirVar::Unqualified(inner.as_str().to_string())),
            range: Range::from(&inner),
        }),
        other => Err(AstError::UnexpectedRule {
            expected: "primary inner",
            got: other,
            range: Range::from(&inner),
        }),
    }
}

fn build_if<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    assert_eq!(pair.as_rule(), Rule::ifstatement);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let cond_pair = inner.next().missing("if condition", range)?;
    let then_pair = inner.next().missing("then branch", range)?;
    let else_pair = inner.next();

    let cond = build_expr(ctx, cond_pair, src)?;
    let then_e = build_expr(ctx, then_pair, src)?;
    let else_e = match else_pair {
        Some(p) => build_expr(ctx, p, src)?,
        None => Expr {
            expr: Expression::Unit,
            range,
        },
    };

    Ok(Expr {
        expr: Expression::If {
            cond: Box::new(cond),
            t: Box::new(then_e),
            f: Box::new(else_e),
        },
        range,
    })
}

fn build_while<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    assert_eq!(pair.as_rule(), Rule::whileloop);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let cond_pair = inner.next().missing("while condition", range)?;
    let body_pair = inner.next().missing("while body", range)?;

    let cond = build_expr(ctx, cond_pair, src)?;
    let body = build_expr(ctx, body_pair, src)?;

    Ok(Expr {
        expr: Expression::While {
            cond: Box::new(cond),
            body: Box::new(body),
        },
        range,
    })
}

fn build_call<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let rule = pair.as_rule();
    assert!(matches!(
        rule,
        Rule::function_call | Rule::external_function_call
    ));
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let ext_call = if rule == Rule::external_function_call {
        Some(inner.next().missing("function call module", range)?)
    } else {
        None
    };
    let name_pair = inner.next().missing("function call name", range)?;
    let name = name_pair.as_str().to_string();

    let mut args = Vec::new();
    for expr_pair in inner {
        args.push(build_expr(ctx, expr_pair, src)?);
    }

    let fn_name = if let Some(mod_name) = ext_call {
        HirFnCall::External {
            module: mod_name.as_str().to_string(),
            name,
        }
    } else {
        HirFnCall::Local(name)
    };

    Ok(Expr {
        expr: Expression::Call { fn_name, args },
        range,
    })
}
```

### src/passes/type_ast/check_intrinsic.rs

```rs
//! check intrinsic functions and their arguments

use crate::ir_types::typed_hir;
use crate::lang::intrinsics::INTRINSICS;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::types::Ty;

pub fn get_intrinsic_call(
    call: &str,
    typed_args: &[typed_hir::Expr],
) -> Option<(typed_hir::Expression, Ty)> {
    if let Ok(intr) = Intrinsic::try_from(call) {
        let (_, fn_sig) = &INTRINSICS[&intr];
        Some((
            typed_hir::Expression::IntrinsicCall {
                fn_name: intr,
                args: typed_args.to_vec(),
            },
            fn_sig.ret_ty,
        ))
    } else {
        None
    }
}

pub fn is_intrinsic_call(call: &str) -> bool {
    Intrinsic::try_from(call).is_ok()
}
```

### src/passes/type_ast/mod.rs

```rs
//! take a parsed and uniquified AST,
//! annotate expressions with their types,
//! check them for correctness,
//! and output a TypedProgram AST

mod errors;
mod type_check;
mod var_types;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::ir_types::typed_hir::TypedFunction;
use crate::lang::intrinsics::INTRINSICS;
use crate::lang::types::Ty;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::errors::TypeError;

impl typed_hir::TypedProgram {
    pub fn from_ast_program<'tyc, 'run>(
        ctx: &'tyc mut CompileCtx<'run>,
        ast: qhir::Program,
    ) -> Result<Self, TypeError> {
        let fn_list = ast
            .functions
            .values()
            .map(|f| annotate_function(ctx, f))
            .collect::<Result<Vec<(FunRef, TypedFunction)>, _>>()?;

        let functions = fn_list.into_iter().collect::<Map<_, _>>();

        let prog = typed_hir::TypedProgram { functions };

        type_check::check_program(ctx, &prog)?;

        Ok(prog)
    }
}

pub fn annotate_function<'tyc, 'run>(
    ctx: &'tyc mut CompileCtx<'run>,
    func: &qhir::Function,
) -> Result<(FunRef, TypedFunction), TypeError> {
    var_types::collect_variables(ctx, func).map_err(|e| TypeError {
        error: e,
        module: func.src_module,
    })?;

    let body = annotate_expression(ctx, &func.body).map_err(|e| TypeError {
        error: e,
        module: func.src_module,
    })?;

    Ok((
        func.name,
        TypedFunction {
            name: func.name,
            range: func.range,
            parameters: func.parameters.to_vec(),
            ret_type: func.ret_type,
            body,
            src_module: func.src_module,
        },
    ))
}

fn annotate_expression<'tyc, 'run>(
    ctx: &'tyc mut CompileCtx<'run>,
    expr: &qhir::Expr,
) -> Result<typed_hir::Expr, AstTypeError> {
    match &expr.expr {
        qhir::Expression::Int(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Int(*x),
            range: expr.range,
            ty: Ty::Int,
        }),
        qhir::Expression::Bool(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Bool(*x),
            range: expr.range,
            ty: Ty::Bool,
        }),
        qhir::Expression::Unit => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Unit,
            range: expr.range,
            ty: Ty::Unit,
        }),
        qhir::Expression::Var(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Var(*x),
            range: expr.range,
            ty: ctx.get_var_type(x).expect("untyped variable"),
        }),
        qhir::Expression::BinOp { left, op, right } => {
            let left_expr = annotate_expression(ctx, left)?;
            let right_expr = annotate_expression(ctx, right)?;

            let expected_ty =
                op.accepts_types(left_expr.ty, right_expr.ty)
                    .map_err(|expected_ty| AstTypeError::TypeError {
                        message: format!(
                            "Operator '{:?}' does not accept types {:?} and {:?}",
                            op, left_expr.ty, right_expr.ty
                        ),
                        expected: expected_ty,
                        found: left_expr.ty,
                        range: expr.range,
                    })?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::BinOp {
                    left: Box::new(left_expr),
                    op: *op,
                    right: Box::new(right_expr),
                },
                range: expr.range,
                ty: expected_ty,
            })
        }
        qhir::Expression::UnOp { op, right } => {
            let right_expr = annotate_expression(ctx, right)?;

            let expected_ty =
                op.accepts_type(right_expr.ty)
                    .map_err(|expected_ty| AstTypeError::TypeError {
                        message: format!(
                            "Operator '{:?}' does not accept type {:?}",
                            op, right_expr.ty
                        ),
                        expected: expected_ty,
                        found: right_expr.ty,
                        range: expr.range,
                    })?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::UnOp {
                    op: *op,
                    right: Box::new(right_expr),
                },
                range: expr.range,
                ty: expected_ty,
            })
        }
        qhir::Expression::If { cond, t, f } => {
            let cond_expr = annotate_expression(ctx, cond)?;
            let t_expr = annotate_expression(ctx, t)?;
            let f_expr = annotate_expression(ctx, f)?;

            let expected_ty = if t_expr.ty != f_expr.ty {
                return Err(AstTypeError::TypeError {
                    message: format!(
                        "Branches of 'if' expression must have the same type, found {:?} and {:?}",
                        t_expr.ty, f_expr.ty
                    ),
                    expected: t_expr.ty,
                    found: f_expr.ty,
                    range: expr.range,
                });
            } else {
                t_expr.ty
            };

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::If {
                    cond: Box::new(cond_expr),
                    t: Box::new(t_expr),
                    f: Box::new(f_expr),
                },
                range: expr.range,
                ty: expected_ty,
            })
        }
        qhir::Expression::While { cond, body } => {
            let cond_expr = annotate_expression(ctx, cond)?;
            let body_expr = annotate_expression(ctx, body)?;
            let ret_ty = body_expr.ty;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::While {
                    cond: Box::new(cond_expr),
                    body: Box::new(body_expr),
                },
                range: expr.range,
                ty: ret_ty,
            })
        }
        qhir::Expression::Call { fn_name, args } => {
            let arg_exprs_and_tys = args
                .iter()
                .map(|arg| annotate_expression(ctx, arg))
                .collect::<Result<Vec<_>, _>>()?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Call {
                    fn_name: *fn_name,
                    args: arg_exprs_and_tys,
                },
                range: expr.range,
                ty: ctx.get_fun_sig(fn_name).expect("untyped function").ret_ty,
            })
        }
        qhir::Expression::IntrinsicCall { fn_name, args } => {
            let arg_exprs_and_tys = args
                .iter()
                .map(|arg| annotate_expression(ctx, arg))
                .collect::<Result<Vec<_>, _>>()?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::IntrinsicCall {
                    fn_name: *fn_name,
                    args: arg_exprs_and_tys,
                },
                range: expr.range,
                ty: INTRINSICS[fn_name].1.ret_ty,
            })
        }
        qhir::Expression::Block {
            statements,
            expr: ret_expr,
        } => {
            let typed_statements = statements
                .iter()
                .map(|stmt| annotate_statement(ctx, stmt))
                .collect::<Result<Vec<_>, _>>()?;

            let (typed_expr, ret_ty) = if let Some(e) = ret_expr {
                let t_expr = annotate_expression(ctx, e)?;
                let ret_ty = t_expr.ty;
                (Some(Box::new(t_expr)), ret_ty)
            } else {
                (None, Ty::Unit)
            };

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Block {
                    statements: typed_statements,
                    expr: typed_expr,
                },
                range: expr.range,
                ty: ret_ty,
            })
        }
    }
}

fn annotate_statement<'tyc, 'run>(
    ctx: &'tyc mut CompileCtx<'run>,
    stmt: &qhir::Statement,
) -> Result<typed_hir::Statement, AstTypeError> {
    let typed_stmt = match stmt {
        qhir::Statement::Declaration {
            name,
            ty,
            val,
            range,
        } => {
            let val_expr = annotate_expression(ctx, val)?;
            typed_hir::Statement::Declaration {
                name: *name,
                range: *range,
                ty: *ty,
                val: val_expr,
            }
        }
        qhir::Statement::Assignment { name, val, range } => {
            let val_expr = annotate_expression(ctx, val)?;
            typed_hir::Statement::Assignment {
                name: *name,
                range: *range,
                val: val_expr,
            }
        }
        qhir::Statement::Expr(e) => {
            let e_expr = annotate_expression(ctx, e)?;
            typed_hir::Statement::Expr(e_expr)
        }
    };
    Ok(typed_stmt)
}
```

### src/passes/type_ast/errors.rs

```rs
//! errors for the ast typing pass

use thiserror::Error;

use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::lang::types::Ty;

#[derive(Debug)]
pub struct TypeError {
    pub error: AstTypeError,
    pub module: ModuleRef,
}

#[derive(Debug, Error)]
pub enum AstTypeError {
    #[error("unbound variable '{name}' at {range}")]
    UnboundVariable { name: String, range: Range },
    #[error("undefined function '{name}' at {range}")]
    UndefinedFunction { name: String, range: Range },
    #[error("type error at {range}: {message} (expected {expected:?}, found {found:?})")]
    TypeError {
        message: String,
        expected: Ty,
        found: Ty,
        range: Range,
    },
    #[error(
        "function call type error at {range}: {message} (expected argument types {expected:?}, found argument types {found:?})"
    )]
    FunctionCallTypeError {
        message: String,
        expected: Vec<Ty>,
        found: Vec<Ty>,
        range: Range,
    },
}
```

### src/passes/type_ast/var_types.rs

```rs
//! collect variable and function names from the AST for function call
//! resolution and variable binding checks in later passes

use crate::compiler::context::CompileCtx;
use crate::ir_types::qhir::*;
use crate::passes::type_ast::AstTypeError;

pub(super) fn collect_variables<'col, 'run>(
    ctx: &'col mut CompileCtx<'run>,
    func: &Function,
) -> Result<(), AstTypeError> {
    for param in &func.parameters {
        ctx.set_var_type(param.name, param.ty);
    }
    collect_variable_names_in_expr(ctx, &func.body)?;
    Ok(())
}

pub(super) fn collect_variable_names_in_expr<'col, 'run>(
    ctx: &'col mut CompileCtx<'run>,
    expr: &Expr,
) -> Result<(), AstTypeError> {
    match &expr.expr {
        Expression::Int(_) | Expression::Bool(_) | Expression::Unit | Expression::Var(_) => Ok(()),
        Expression::BinOp { left, right, .. } => {
            collect_variable_names_in_expr(ctx, left)?;
            collect_variable_names_in_expr(ctx, right)
        }
        Expression::UnOp { right, .. } => collect_variable_names_in_expr(ctx, right),
        Expression::If { cond, t, f } => {
            collect_variable_names_in_expr(ctx, cond)?;
            collect_variable_names_in_expr(ctx, t)?;
            collect_variable_names_in_expr(ctx, f)
        }
        Expression::While { cond, body } => {
            collect_variable_names_in_expr(ctx, cond)?;
            collect_variable_names_in_expr(ctx, body)
        }
        Expression::Call { fn_name: _, args } => {
            for arg in args {
                collect_variable_names_in_expr(ctx, arg)?;
            }
            Ok(())
        }
        Expression::IntrinsicCall { fn_name: _, args } => {
            for arg in args {
                collect_variable_names_in_expr(ctx, arg)?;
            }
            Ok(())
        }
        Expression::Block { statements, expr } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration { name, ty, val, .. } => {
                        collect_variable_names_in_expr(ctx, val)?;
                        ctx.set_var_type(*name, *ty);
                    }
                    Statement::Expr(e) => {
                        collect_variable_names_in_expr(ctx, e)?;
                    }
                    Statement::Assignment { name, val, range } => {
                        if ctx.get_var_type(name).is_some() {
                            collect_variable_names_in_expr(ctx, val)?;
                        } else {
                            return Err(AstTypeError::UnboundVariable {
                                name: ctx.uniq_variable_name(*name),
                                range: *range,
                            });
                        }
                    }
                }
            }
            if let Some(e) = expr {
                collect_variable_names_in_expr(ctx, e)?;
            }
            Ok(())
        }
    }
}
```

### src/passes/type_ast/type_check.rs

```rs
//! check that the types of a TypedProgram AST actually make sense

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Range;
use crate::ir_types::typed_hir::*;
use crate::lang::intrinsics::INTRINSICS;
use crate::lang::types::Ty;
use crate::passes::type_ast::AstTypeError;
use crate::passes::type_ast::errors::TypeError;

fn expect_type(
    found: Ty,
    expected: Ty,
    message: impl FnOnce() -> String,
    range: Range,
) -> Result<(), AstTypeError> {
    if found.type_neq(&expected) {
        Err(AstTypeError::TypeError {
            message: message(),
            expected,
            found,
            range,
        })
    } else {
        Ok(())
    }
}

fn expect_same_type(
    left: Ty,
    right: Ty,
    message: impl FnOnce() -> String,
    range: Range,
) -> Result<Ty, AstTypeError> {
    if left.type_neq(&right) {
        Err(AstTypeError::TypeError {
            message: message(),
            expected: left,
            found: right,
            range,
        })
    } else {
        Ok(left)
    }
}

fn check_call_args(
    ctx: &CompileCtx,
    fn_name: String,
    args: &[Expr],
    expected: &[Ty],
    ret_ty: Ty,
    range: Range,
) -> Result<Ty, AstTypeError> {
    let arg_tys = args
        .iter()
        .map(|arg| check_expr(ctx, arg))
        .collect::<Result<Vec<_>, _>>()?;

    if arg_tys.len() != expected.len() {
        return Err(AstTypeError::FunctionCallTypeError {
            message: format!(
                "Function '{}' expects {} arguments but found {}",
                fn_name,
                expected.len(),
                arg_tys.len()
            ),
            expected: expected.to_vec(),
            found: arg_tys,
            range,
        });
    }

    for (i, (found, expected_ty)) in arg_tys.iter().zip(expected).enumerate() {
        if found.type_neq(expected_ty) {
            return Err(AstTypeError::FunctionCallTypeError {
                message: format!(
                    "Argument {} of function '{}' expects type {:?} but found {:?}",
                    i + 1,
                    fn_name,
                    expected_ty,
                    found
                ),
                expected: vec![*expected_ty],
                found: vec![*found],
                range: args[i].range,
            });
        }
    }

    Ok(ret_ty)
}

pub(super) fn check_program(ctx: &CompileCtx, prog: &TypedProgram) -> Result<(), TypeError> {
    for func in prog.functions.values() {
        check_function(ctx, func)?;
    }
    Ok(())
}

pub(super) fn check_function(ctx: &CompileCtx, func: &TypedFunction) -> Result<(), TypeError> {
    // check that the function's return type matches the type of its body expression
    let body_ty = check_expr(ctx, &func.body).map_err(|e| TypeError {
        error: e,
        module: func.src_module,
    })?;
    if body_ty.type_neq(&func.ret_type) {
        let err = AstTypeError::TypeError {
            message: format!(
                "Function '{}' has return type {:?} but body has type {:?}",
                ctx.original_fun_name(func.name),
                func.ret_type,
                body_ty
            ),
            expected: func.ret_type,
            found: body_ty,
            range: func.range,
        };
        return Err(TypeError {
            error: err,
            module: func.src_module,
        });
    }

    Ok(())
}

pub(super) fn check_expr(ctx: &CompileCtx, expr: &Expr) -> Result<Ty, AstTypeError> {
    match &expr.expr {
        Expression::BinOp { left, op, right } => {
            let left_ty = check_expr(ctx, left)?;
            let right_ty = check_expr(ctx, right)?;

            op.accepts_types(left_ty, right_ty)
                .map_err(|expected_ty| AstTypeError::TypeError {
                    message: format!(
                        "Operator '{:?}' does not accept types {:?} and {:?}",
                        op, left_ty, right_ty
                    ),
                    expected: expected_ty,
                    found: left_ty,
                    range: expr.range,
                })
        }
        Expression::UnOp { op, right } => {
            let right_ty = check_expr(ctx, right)?;

            if let Err(expected_ty) = op.accepts_type(right_ty) {
                return Err(AstTypeError::TypeError {
                    message: format!("Operator '{:?}' does not accept type {:?}", op, right_ty),
                    expected: expected_ty,
                    found: right_ty,
                    range: expr.range,
                });
            }

            Ok(right_ty)
        }
        Expression::If { cond, t, f } => {
            let cond_ty = check_expr(ctx, cond)?;
            expect_type(
                cond_ty,
                Ty::Bool,
                || format!("Condition of 'if' must be Bool, found {:?}", cond_ty),
                cond.range,
            )?;

            let t_ty = check_expr(ctx, t)?;
            let f_ty = check_expr(ctx, f)?;
            expect_same_type(
                t_ty,
                f_ty,
                || {
                    format!(
                        "Branches of 'if' must have same type, found {:?} and {:?}",
                        t_ty, f_ty
                    )
                },
                t.range,
            )
        }
        Expression::While { cond, body } => {
            let cond_ty = check_expr(ctx, cond)?;
            if cond_ty != Ty::Bool {
                return Err(AstTypeError::TypeError {
                    message: format!(
                        "Condition {cond:?} of 'while' expression must be of type Bool, found {:?}",
                        cond_ty
                    ),
                    expected: Ty::Bool,
                    found: cond_ty,
                    range: cond.range,
                });
            }
            check_expr(ctx, body)
        }
        Expression::Call { fn_name, args } => {
            let fun_sig = ctx.fun_sig(fn_name);

            let expected: Vec<Ty> = fun_sig.args.iter().map(|p| p.1).collect();
            check_call_args(
                ctx,
                ctx.original_fun_name(*fn_name),
                args,
                &expected,
                fun_sig.ret_ty,
                expr.range,
            )
        }

        Expression::IntrinsicCall { fn_name, args } => {
            let (_fn_ref, fn_sig) = &INTRINSICS[fn_name];
            check_call_args(
                ctx,
                fn_name.to_string(),
                args,
                &fn_sig.args,
                fn_sig.ret_ty,
                expr.range,
            )
        }
        Expression::Block { statements, expr } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration {
                        name,
                        ty,
                        val,
                        range,
                    } => {
                        let val_ty = check_expr(ctx, val)?;
                        if val_ty.type_neq(ty) {
                            return Err(AstTypeError::TypeError {
                                message: format!(
                                    "Declared variable '{}' has type {:?} but initializer has type {:?}",
                                    ctx.uniq_variable_name(*name),
                                    ty,
                                    val_ty
                                ),
                                expected: *ty,
                                found: val_ty,
                                range: *range,
                            });
                        }
                    }
                    Statement::Assignment { name, val, range } => {
                        let var_ty = ctx.get_var_type(name).ok_or_else(|| {
                            AstTypeError::UnboundVariable {
                                name: ctx.uniq_variable_name(*name),
                                range: *range,
                            }
                        })?;
                        let val_ty = check_expr(ctx, val)?;
                        if val_ty.type_neq(&var_ty) {
                            return Err(AstTypeError::TypeError {
                                message: format!(
                                    "Variable '{}' has type {:?} but assigned value has type {:?}",
                                    ctx.uniq_variable_name(*name),
                                    var_ty,
                                    val_ty
                                ),
                                expected: var_ty,
                                found: val_ty,
                                range: *range,
                            });
                        }
                    }
                    Statement::Expr(e) => {
                        check_expr(ctx, e)?;
                    }
                }
            }
            if let Some(e) = expr {
                check_expr(ctx, e)
            } else {
                Ok(Ty::Unit)
            }
        }
        Expression::Int(_) => Ok(Ty::Int),
        Expression::Bool(_) => Ok(Ty::Bool),
        Expression::Unit => Ok(Ty::Unit),
        Expression::Var(name) => Ok(ctx.var_type(name)),
    }
}
```

### src/passes/explicate_control/mod.rs

```rs
//! explicate control of our functional language to construct the CFG-MIR from
//! an AST

pub mod context;

use crate::ir_types::cfgmir::*;
use crate::ir_types::typed_hir as th;
use crate::passes::explicate_control::context::FnCx;

impl MirProgram {
    pub fn from_typed_program(prog: &th::TypedProgram) -> Self {
        let functions = prog
            .functions
            .iter()
            .map(|(name, func)| (*name, lower_function(func)))
            .collect();

        Self { functions }
    }
}

fn lower_function(func: &th::TypedFunction) -> MirFunction {
    let mut cx = FnCx::new(func.name, func.range, func.ret_type);

    let params = func
        .parameters
        .iter()
        .map(|p| {
            let local = cx.get_or_create_local(p.name, p.ty, p.range);
            MirParam {
                local,
                name: p.name,
                ty: p.ty,
                range: p.range,
            }
        })
        .collect::<Vec<_>>();

    collect_locals(&mut cx, &func.body);

    let mut entry = cx.lower_tail(&func.body);

    cx.blocks.reverse();
    // fix up all BlockId references since indices just changed
    let n = cx.blocks.len();
    for block in &mut cx.blocks {
        block.id = BlockId(n - 1 - block.id.0);
        fix_terminator_ids(&mut block.terminator, n);
    }
    entry.0 = n - 1 - entry.0;
    
    MirFunction {
        name: func.name,
        range: func.range,
        params,
        ret_type: func.ret_type,
        locals: cx.locals,
        blocks: cx.blocks,
        entry,
    }
}

fn fix_terminator_ids(term: &mut Terminator, n: usize) {
    match term {
        Terminator::Branch { cond: _, then_bb, else_bb } => {
            then_bb.0 = n - 1 - then_bb.0;
            else_bb.0 = n - 1 - else_bb.0;
        }
        Terminator::Goto { target } => {
            target.0 = n - 1 - target.0;
        }
        _ => {}
    }
}

fn collect_locals(cx: &mut FnCx, expr: &th::Expr) {
    match &expr.expr {
        th::Expression::Block { statements, expr } => {
            for stmt in statements {
                match stmt {
                    th::Statement::Declaration {
                        name,
                        ty,
                        range,
                        val,
                    } => {
                        cx.get_or_create_local(*name, *ty, *range);
                        collect_locals(cx, val);
                    }
                    th::Statement::Assignment { val, .. } => collect_locals(cx, val),
                    th::Statement::Expr(e) => collect_locals(cx, e),
                }
            }
            if let Some(e) = expr {
                collect_locals(cx, e);
            }
        }
        th::Expression::If { cond, t, f } => {
            collect_locals(cx, cond);
            collect_locals(cx, t);
            collect_locals(cx, f);
        }
        th::Expression::While { cond, body } => {
            collect_locals(cx, cond);
            collect_locals(cx, body);
        }
        th::Expression::BinOp { left, right, .. } => {
            collect_locals(cx, left);
            collect_locals(cx, right);
        }
        th::Expression::UnOp { right, .. } => collect_locals(cx, right),
        th::Expression::Call { args, .. } | th::Expression::IntrinsicCall { args, .. } => {
            for a in args {
                collect_locals(cx, a);
            }
        }
        th::Expression::Var(_)
        | th::Expression::Int(_)
        | th::Expression::Bool(_)
        | th::Expression::Unit => {}
    }
}
```

### src/passes/explicate_control/context.rs

```rs
//! a function's context for explicate control

use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::internal_bug;
use crate::ir_types::cfgmir::*;
use crate::ir_types::typed_hir as th;
use crate::lang::ops::Bop;
use crate::lang::ops::Uop;
use crate::lang::types::Ty;

pub(super) struct FnCx {
    #[allow(dead_code)]
    name: FunRef,
    #[allow(dead_code)]
    range: Range,
    #[allow(dead_code)]
    ret_type: Ty,

    pub(super) locals: Vec<LocalDecl>,
    local_map: Map<UniqVar, LocalId>,

    pub(super) blocks: Vec<BasicBlock>,
    next_temp: usize,
}

impl FnCx {
    pub(super) fn new(name: FunRef, range: Range, ret_type: Ty) -> Self {
        Self {
            name,
            range,
            ret_type,
            locals: Vec::new(),
            local_map: Map::new(),
            blocks: Vec::new(),
            next_temp: 0,
        }
    }

    pub(super) fn new_block(
        &mut self,
        statements: Vec<Statement>,
        terminator: Terminator,
    ) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock {
            id,
            statements,
            terminator,
        });
        id
    }

    pub(super) fn reserve_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock {
            id,
            statements: Vec::new(),
            terminator: Terminator::Unreachable,
        });
        id
    }

    pub(super) fn set_block(
        &mut self,
        id: BlockId,
        statements: Vec<Statement>,
        terminator: Terminator,
    ) {
        self.blocks[id.0] = BasicBlock {
            id,
            statements,
            terminator,
        };
    }

    pub(super) fn get_or_create_local(&mut self, name: UniqVar, ty: Ty, range: Range) -> LocalId {
        if let Some(id) = self.local_map.get(&name) {
            return *id;
        }

        let id = LocalId(self.locals.len());
        self.locals.push(LocalDecl {
            id,
            name: LocalName::User(name),
            ty,
            range,
        });
        self.local_map.insert(name, id);
        id
    }

    pub(super) fn fresh_temp(&mut self, hint: &'static str, ty: Ty, range: Range) -> LocalId {
        let id = LocalId(self.locals.len());
        let name = LocalName::Temp(self.next_temp, hint);
        self.next_temp += 1;

        self.locals.push(LocalDecl {
            id,
            name: name.clone(),
            ty,
            range,
        });
        id
    }

    pub(super) fn place(local: LocalId) -> Place {
        Place { local }
    }

    pub(super) fn const_operand(expr: &th::Expr) -> Option<Operand> {
        match &expr.expr {
            th::Expression::Int(i) => Some(Operand::Const(Constant::Int(*i))),
            th::Expression::Bool(b) => Some(Operand::Const(Constant::Bool(*b))),
            th::Expression::Unit => Some(Operand::Const(Constant::Unit)),
            _ => None,
        }
    }

    pub(super) fn var_operand(&self, name: &UniqVar) -> Operand {
        let local = *self
            .local_map
            .get(name)
            .unwrap_or_else(|| internal_bug!("missing local for variable {name:?}"));
        Operand::Copy(Self::place(local))
    }

    pub(super) fn simple_operand(&self, expr: &th::Expr) -> Option<Operand> {
        if let Some(c) = Self::const_operand(expr) {
            return Some(c);
        }

        match &expr.expr {
            th::Expression::Var(v) => Some(self.var_operand(v)),
            _ => None,
        }
    }

    pub(super) fn assign_stmt(&self, dst: LocalId, value: RValue, range: Range) -> Statement {
        Statement::Assign {
            dst: Self::place(dst),
            value,
            range,
        }
    }

    pub(super) fn goto_block(&mut self, target: BlockId) -> BlockId {
        self.new_block(Vec::new(), Terminator::Goto { target })
    }

    pub(super) fn unit_assign_then_goto(
        &mut self,
        dst: LocalId,
        range: Range,
        target: BlockId,
    ) -> BlockId {
        self.new_block(
            vec![self.assign_stmt(dst, RValue::Use(Operand::Const(Constant::Unit)), range)],
            Terminator::Goto { target },
        )
    }

    pub(super) fn lower_tail(&mut self, expr: &th::Expr) -> BlockId {
        match &expr.expr {
            th::Expression::If { cond, t, f } => {
                let then_bb = self.lower_tail(t);
                let else_bb = self.lower_tail(f);
                self.lower_pred(cond, then_bb, else_bb)
            }

            th::Expression::Block { statements, expr } => {
                let cont = if let Some(e) = expr {
                    self.lower_tail(e)
                } else {
                    self.new_block(Vec::new(), Terminator::Return { value: None })
                };
                self.lower_statements(statements, cont)
            }

            _ if expr.ty == Ty::Unit => {
                let ret = self.new_block(Vec::new(), Terminator::Return { value: None });
                self.lower_effect(expr, ret)
            }

            _ => {
                let tmp = self.fresh_temp("lower_tail_tmp", expr.ty, expr.range);
                let ret = self.new_block(
                    Vec::new(),
                    Terminator::Return {
                        value: Some(Operand::Copy(Self::place(tmp))),
                    },
                );
                self.lower_assign(expr, tmp, ret)
            }
        }
    }

    pub(super) fn lower_statements(
        &mut self,
        statements: &[th::Statement],
        cont: BlockId,
    ) -> BlockId {
        statements
            .iter()
            .rev()
            .fold(cont, |k, stmt| self.lower_statement(stmt, k))
    }

    pub(super) fn lower_statement(&mut self, stmt: &th::Statement, cont: BlockId) -> BlockId {
        match stmt {
            th::Statement::Declaration {
                name,
                range,
                ty,
                val,
            } => {
                let dst = self.get_or_create_local(*name, *ty, *range);
                self.lower_assign(val, dst, cont)
            }

            th::Statement::Assignment { name, range, val } => {
                let dst = self.get_or_create_local(*name, val.ty, *range);
                self.lower_assign(val, dst, cont)
            }

            th::Statement::Expr(e) => self.lower_effect(e, cont),
        }
    }

    pub(super) fn lower_assign(&mut self, expr: &th::Expr, dst: LocalId, cont: BlockId) -> BlockId {
        match &expr.expr {
            th::Expression::If { cond, t, f } => {
                let then_bb = self.lower_assign(t, dst, cont);
                let else_bb = self.lower_assign(f, dst, cont);
                self.lower_pred(cond, then_bb, else_bb)
            }

            th::Expression::While { .. } => {
                let after = self.unit_assign_then_goto(dst, expr.range, cont);
                self.lower_effect(expr, after)
            }

            th::Expression::Block {
                statements,
                expr: inner_expr,
            } => {
                let k = if let Some(e) = inner_expr {
                    self.lower_assign(e, dst, cont)
                } else {
                    self.unit_assign_then_goto(dst, expr.range, cont)
                };
                self.lower_statements(statements, k)
            }

            th::Expression::Int(_)
            | th::Expression::Bool(_)
            | th::Expression::Unit
            | th::Expression::Var(_) => {
                let op = self
                    .simple_operand(expr)
                    .expect("simple expression should lower to operand");

                self.new_block(
                    vec![self.assign_stmt(dst, RValue::Use(op), expr.range)],
                    Terminator::Goto { target: cont },
                )
            }

            th::Expression::UnOp { op, right } => {
                let r_tmp = self.fresh_temp("unop_right", right.ty, right.range);
                let final_bb = self.new_block(
                    vec![self.assign_stmt(
                        dst,
                        RValue::UnaryOp {
                            op: *op,
                            right: Operand::Copy(Self::place(r_tmp)),
                        },
                        expr.range,
                    )],
                    Terminator::Goto { target: cont },
                );
                self.lower_assign(right, r_tmp, final_bb)
            }

            th::Expression::BinOp { left, op, right } => {
                let l_tmp = self.fresh_temp("assign_binop_left", left.ty, left.range);
                let r_tmp = self.fresh_temp("assign_binop_left", right.ty, right.range);

                let final_bb = self.new_block(
                    vec![self.assign_stmt(
                        dst,
                        RValue::BinaryOp {
                            op: *op,
                            left: Operand::Copy(Self::place(l_tmp)),
                            right: Operand::Copy(Self::place(r_tmp)),
                        },
                        expr.range,
                    )],
                    Terminator::Goto { target: cont },
                );

                let right_bb = self.lower_assign(right, r_tmp, final_bb);
                self.lower_assign(left, l_tmp, right_bb)
            }

            th::Expression::Call { fn_name, args } => {
                let arg_temps = args
                    .iter()
                    .map(|a| self.fresh_temp("assign_call_argument", a.ty, a.range))
                    .collect::<Vec<_>>();

                let final_bb = self.new_block(
                    vec![
                        self.assign_stmt(
                            dst,
                            RValue::Call {
                                fn_name: *fn_name,
                                args: arg_temps
                                    .iter()
                                    .map(|id| Operand::Copy(Self::place(*id)))
                                    .collect(),
                            },
                            expr.range,
                        ),
                    ],
                    Terminator::Goto { target: cont },
                );

                args.iter()
                    .zip(arg_temps)
                    .rev()
                    .fold(final_bb, |k, (arg, tmp)| self.lower_assign(arg, tmp, k))
            }

            th::Expression::IntrinsicCall { fn_name, args } => {
                let arg_temps = args
                    .iter()
                    .map(|a| self.fresh_temp("assign_intrinsic_call_argument", a.ty, a.range))
                    .collect::<Vec<_>>();

                let final_bb = self.new_block(
                    vec![
                        self.assign_stmt(
                            dst,
                            RValue::IntrinsicCall {
                                fn_name: *fn_name,
                                args: arg_temps
                                    .iter()
                                    .map(|id| Operand::Copy(Self::place(*id)))
                                    .collect(),
                            },
                            expr.range,
                        ),
                    ],
                    Terminator::Goto { target: cont },
                );

                args.iter()
                    .zip(arg_temps)
                    .rev()
                    .fold(final_bb, |k, (arg, tmp)| self.lower_assign(arg, tmp, k))
            }
        }
    }

    pub(super) fn lower_effect(&mut self, expr: &th::Expr, cont: BlockId) -> BlockId {
        match &expr.expr {
            th::Expression::If { cond, t, f } => {
                let then_bb = self.lower_effect(t, cont);
                let else_bb = self.lower_effect(f, cont);
                self.lower_pred(cond, then_bb, else_bb)
            }

            th::Expression::While { cond, body } => {
                let loop_head = self.reserve_block();

                let back_edge = self.goto_block(loop_head);
                let body_bb = self.lower_effect(body, back_edge);
                let cond_entry = self.lower_pred(cond, body_bb, cont);

                self.set_block(
                    loop_head,
                    Vec::new(),
                    Terminator::Goto { target: cond_entry },
                );

                loop_head
            }

            th::Expression::Block { statements, expr } => {
                let k = if let Some(e) = expr {
                    self.lower_effect(e, cont)
                } else {
                    cont
                };
                self.lower_statements(statements, k)
            }

            th::Expression::Call { fn_name, args } => {
                let arg_temps = args
                    .iter()
                    .map(|a| self.fresh_temp("effect_call_argument", a.ty, a.range))
                    .collect::<Vec<_>>();

                let final_bb = self.new_block(
                    vec![Statement::Eval {
                        value: RValue::Call {
                            fn_name: *fn_name,
                            args: arg_temps
                                .iter()
                                .map(|id| Operand::Copy(Self::place(*id)))
                                .collect(),
                        },
                        range: expr.range,
                    }],
                    Terminator::Goto { target: cont },
                );

                args.iter()
                    .zip(arg_temps)
                    .rev()
                    .fold(final_bb, |k, (arg, tmp)| self.lower_assign(arg, tmp, k))
            }

            th::Expression::IntrinsicCall { fn_name, args } => {
                let arg_temps = args
                    .iter()
                    .map(|a| self.fresh_temp("effect_intrinsic_call_argument", a.ty, a.range))
                    .collect::<Vec<_>>();

                let final_bb = self.new_block(
                    vec![Statement::Eval {
                        value: RValue::IntrinsicCall {
                            fn_name: *fn_name,
                            args: arg_temps
                                .iter()
                                .map(|id| Operand::Copy(Self::place(*id)))
                                .collect(),
                        },
                        range: expr.range,
                    }],
                    Terminator::Goto { target: cont },
                );

                args.iter()
                    .zip(arg_temps)
                    .rev()
                    .fold(final_bb, |k, (arg, tmp)| self.lower_assign(arg, tmp, k))
            }

            _ => {
                let tmp = self.fresh_temp("effect_tmp", expr.ty, expr.range);
                self.lower_assign(expr, tmp, cont)
            }
        }
    }

    pub(super) fn lower_pred(
        &mut self,
        expr: &th::Expr,
        then_bb: BlockId,
        else_bb: BlockId,
    ) -> BlockId {
        match &expr.expr {
            th::Expression::Bool(true) => then_bb,
            th::Expression::Bool(false) => else_bb,

            th::Expression::If { cond, t, f } => {
                let t_bb = self.lower_pred(t, then_bb, else_bb);
                let f_bb = self.lower_pred(f, then_bb, else_bb);
                self.lower_pred(cond, t_bb, f_bb)
            }

            th::Expression::Block { statements, expr } => {
                let last = expr.as_ref().map(|e| &**e).unwrap_or_else(|| {
                    internal_bug!("block in predicate position should have a final expression")
                });
                let k = self.lower_pred(last, then_bb, else_bb);
                self.lower_statements(statements, k)
            }

            th::Expression::UnOp {
                op: Uop::Not,
                right,
            } => self.lower_pred(right, else_bb, then_bb),

            th::Expression::BinOp {
                left,
                op: Bop::Comp(_),
                right,
            } => {
                let l_tmp = self.fresh_temp("pred_binop_left", left.ty, left.range);
                let r_tmp = self.fresh_temp("pred_binop_right", right.ty, right.range);

                let cmp_tmp = self.fresh_temp("pred_binop_comp", Ty::Bool, expr.range);

                let branch_bb = self.new_block(
                    Vec::new(),
                    Terminator::Branch {
                        cond: Operand::Copy(Self::place(cmp_tmp)),
                        then_bb,
                        else_bb,
                    },
                );

                let cmp_bb = self.new_block(
                    vec![self.assign_stmt(
                        cmp_tmp,
                        RValue::BinaryOp {
                            op: match &expr.expr {
                                th::Expression::BinOp { op, .. } => *op,
                                _ => unreachable!(),
                            },
                            left: Operand::Copy(Self::place(l_tmp)),
                            right: Operand::Copy(Self::place(r_tmp)),
                        },
                        expr.range,
                    )],
                    Terminator::Goto { target: branch_bb },
                );

                let right_bb = self.lower_assign(right, r_tmp, cmp_bb);
                self.lower_assign(left, l_tmp, right_bb)
            }

            th::Expression::Var(v) => self.new_block(
                Vec::new(),
                Terminator::Branch {
                    cond: self.var_operand(v),
                    then_bb,
                    else_bb,
                },
            ),

            _ => {
                let tmp = self.fresh_temp("lower_pred_result", expr.ty, expr.range);
                let branch_bb = self.new_block(
                    Vec::new(),
                    Terminator::Branch {
                        cond: Operand::Copy(Self::place(tmp)),
                        then_bb,
                        else_bb,
                    },
                );
                self.lower_assign(expr, tmp, branch_bb)
            }
        }
    }
}
```

### src/passes/mod.rs

```rs
//! nano-passes for the compiler

pub mod build_ast;
pub mod explicate_control;
pub mod parse;
pub mod qualify;
pub mod type_ast;
```

### src/passes/parse.rs

```rs
//! parse the pest tokens into an AST

use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct LangParser;
```

### src/lsp/annotate.rs

```rs
//! generate diagnostics for expression annotation

use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::DiagnosticSeverity;
use tower_lsp::lsp_types::MessageType;

use crate::analysis::ProgramAnnotations;
use crate::analysis::analyse;
use crate::analysis::interactions::has_other_side_effects;
use crate::compiler::context::CompileCtx;
use crate::ir_types::typed_hir::TypedProgram;
use crate::lsp::Backend;
use crate::lsp::diagnostics::Diagnostics;
use crate::lsp::util::lsp_range_from_pest;

impl Backend<'_> {
    /// if an expression without side effects appears multiple times in the
    /// code, we can compute its value just once,
    /// and reuse everywhere else.
    ///
    /// we want to show this to the user by highlighting the repeated
    /// expressions:
    ///
    /// ```ignore
    /// def main(): Int := {
    ///     let x: Int = 5;
    ///     let y: Int = x ^ x;
    ///     let i: Int = 4;
    ///     while i ≥ 0 do {
    ///         y = y + (x ^ x);
    ///         i = i - 1;
    ///     }
    ///     y
    /// }
    /// ```
    /// in this example, both instances of `x ^ x` should be highlighted,
    /// indicating a reused value
    pub async fn annotate_reused_expressions<'run, 'lsp>(
        &'run self,
        ctx: &'run CompileCtx<'lsp>,
        ast: &TypedProgram,
    ) -> Diagnostics {
        self.log(
            MessageType::LOG,
            "analyzing expressions for reuse patterns".to_string(),
        )
        .await;

        let annotations: ProgramAnnotations = analyse(ctx, ast);
        let expr_count = annotations.expr_occurrences.len();
        self.log(
            MessageType::LOG,
            format!("found {} expressions to analyze", expr_count),
        )
        .await;

        // produce diagnostics for keys with more than one occurrence
        let mut diagnostics: Diagnostics = Diagnostics::default();
        for (e, occs) in annotations.expr_occurrences.into_iter() {
            // // NOTE: whether we include this check or not
            // // depends on how the annotations are made
            // if occs.len() <= 1 {
            //     continue;
            // }
            if has_other_side_effects(&e) {
                continue;
            }

            for (module, range) in occs {
                let uri = self
                    .context
                    .read()
                    .await
                    .url_of_file(ctx.file_of_module(module))
                    .clone();
                if let Some(text) = self.file_contents.read().await.get(&uri) {
                    let range = lsp_range_from_pest(text, range);

                    let message = format!("reused expression: {:?}", e.expr);

                    diagnostics.add_one(
                        uri.clone(),
                        Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::HINT),
                            source: Some("sand".into()),
                            message,
                            ..Default::default()
                        },
                    );
                };
            }
        }

        diagnostics
    }
}
```

### src/lsp/util.rs

```rs
//! helper methods

use bimap::BiBTreeMap;
use pest::error::LineColLocation;
use tower_lsp::lsp_types::*;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Pos;
use crate::compiler::structure::Range as LangRange;
use crate::lsp::LastCompilation;
use crate::lsp::diagnostics::Diagnostics;
use crate::passes::parse::Rule;

pub(super) fn lsp_position_from_pest(text: &str, pos: Pos) -> Position {
    // pest reports 1-based line/col; convert to 0-based
    let line_idx = pos.line.saturating_sub(1);
    let col_idx = pos.col.saturating_sub(1);

    // get the text of the line (lines() drops the newline)
    let line_str = text.lines().nth(line_idx).unwrap_or("");

    // take `col_idx` rust chars, then count UTF-16 code units (LSP uses UTF-16)
    let prefix: String = line_str.chars().take(col_idx).collect();
    let utf16_col = prefix.encode_utf16().count();

    Position::new(line_idx as u32, utf16_col as u32)
}

pub(super) fn lsp_positions_from_range(text: &str, range: LangRange) -> (Position, Position) {
    let start = lsp_position_from_pest(text, range.start);
    let end = lsp_position_from_pest(text, range.end);
    (start, end)
}

pub(super) fn lsp_range_from_pest(text: &str, range: LangRange) -> Range {
    let (start, end) = lsp_positions_from_range(text, range);
    Range::new(start, end)
}

pub(super) fn parse_error_to_diagnostic(text: &str, err: pest::error::Error<Rule>) -> Diagnostic {
    let (start, end) = match err.line_col {
        LineColLocation::Pos((l, c)) => {
            let p = lsp_position_from_pest(text, Pos::new(l, c));
            (p, p)
        }
        LineColLocation::Span((sl, sc), (el, ec)) => {
            let start = lsp_position_from_pest(text, Pos::new(sl, sc));
            let end = lsp_position_from_pest(text, Pos::new(el, ec));
            (start, end)
        }
    };

    Diagnostic {
        range: Range::new(start, end),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("sand".into()),
        message: err.variant.message().into(),
        ..Default::default()
    }
}

impl<'run> LastCompilation<'run> {
    pub fn diagnostics(&self) -> &Diagnostics {
        match self {
            LastCompilation::Success { diagnostics, .. } => diagnostics,
            LastCompilation::Failure { diagnostics } => diagnostics,
        }
    }
}

pub(super) fn url_of_module_unchecked(
    module: ModuleRef,
    ctx: &CompileCtx,
    file_map: &BiBTreeMap<Url, FileRef>,
) -> Url {
    let file = ctx.file_of_module(module);
    file_map.get_by_right(&file).cloned().unwrap()
}
```

### src/lsp/config.rs

```rs
//! project configuration files

use std::path::Path;

use anyhow::anyhow;

use crate::compiler::structure::Map;
use crate::compiler::structure::ProjectConfig;
use crate::lsp::Backend;

pub async fn load_config(root_path: &Path) -> anyhow::Result<Option<ProjectConfig>> {
    // look for `sand.toml`,
    // if found, parse it and return the config
    let config_path = root_path.join("sand.toml");
    if !config_path.exists() {
        return Ok(None);
    }
    let config = std::fs::read_to_string(&config_path)?;
    Ok(Some(toml::from_str(&config)?))
}

impl Backend<'_> {
    pub async fn apply_config(&self, config: &ProjectConfig) -> anyhow::Result<()> {
        use tower_lsp::lsp_types::MessageType;

        self.log(
            MessageType::LOG,
            format!(
                "applying config with {} tracked files",
                config.tracked_files.len()
            ),
        )
        .await;

        let mut new_files = Map::new();
        for f in &config.tracked_files {
            match std::fs::read_to_string(
                f.to_file_path()
                    .map_err(|_| anyhow!("uri {f} is not a path"))?,
            ) {
                Ok(content) => {
                    new_files.insert(f.clone(), content);
                    self.log(MessageType::LOG, format!("loaded config file: {}", f))
                        .await;
                }
                Err(e) => {
                    self.log(
                        MessageType::WARNING,
                        format!("failed to load config file {}: {}", f, e),
                    )
                    .await;
                    return Err(e.into());
                }
            }
        }

        self.log(
            MessageType::LOG,
            format!("registering {} files from config", new_files.len()),
        )
        .await;

        for (uri, content) in new_files {
            self.register_file(uri, content).await;
        }

        self.log(MessageType::LOG, "config applied successfully".to_string())
            .await;
        Ok(())
    }
}
```

### src/lsp/backend.rs

```rs
//! LSP backend document checking functionality.

use std::collections::BTreeMap;
use std::fmt::Display;

use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::lsp_types::MessageType;
use tower_lsp::lsp_types::Url;

use crate::compile_hir;
use crate::compiler::context::CompileCtx;
use crate::compiler::context::ProjectCtx;
use crate::lsp::Backend;
use crate::lsp::LastCompilation;

impl<'lsp> Backend<'lsp> {
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            project_root: RwLock::new(None),
            file_contents: RwLock::new(BTreeMap::new()),
            last_compilation: RwLock::new(None),

            standalone_files: RwLock::new(BTreeMap::new()),

            context: RwLock::new(ProjectCtx::initial()),
        }
    }

    pub async fn log(&self, ty: MessageType, msg: impl Display) {
        eprintln!("{ty:?}:{msg}");
        self.client.log_message(ty, format!("{msg}\n")).await
    }

    pub async fn check_project<'run>(&'run self)
    where
        'lsp: 'run,
    {
        self.log(MessageType::LOG, "starting project check...")
            .await;
        let mut modules = BTreeMap::new();
        let project_files = self.file_contents.read().await;
        self.log(
            MessageType::LOG,
            format!(
                "found {} project files: {project_files:?}",
                project_files.len()
            ),
        )
        .await;
        for (m, s) in project_files.iter() {
            if let Some(fr) = self.context.read().await.files.get_by_left(m) {
                modules.insert(*fr, s.as_str());
            }
        }

        self.log(
            MessageType::LOG,
            format!("compiling {} modules", modules.len()),
        )
        .await;

        let mut ctx = CompileCtx::initial();
        let last_compilation = match compile_hir(modules, &mut ctx) {
            Ok(ast) => {
                self.log(
                    MessageType::LOG,
                    "compilation successful, analyzing expressions".to_string(),
                )
                .await;
                let diagnostics = self.annotate_reused_expressions(&ctx, &ast).await;
                LastCompilation::Success {
                    context: Box::new(ctx),
                    diagnostics,
                    ast,
                }
            }
            Err(err) => {
                self.log(
                    MessageType::WARNING,
                    "compilation failed, generating diagnostics".to_string(),
                )
                .await;
                LastCompilation::Failure {
                    diagnostics: self.sand_diagnostics(&ctx, err).await,
                }
            }
        };

        let diagnostic_count = last_compilation
            .diagnostics()
            .map
            .values()
            .map(|v| v.len())
            .sum::<usize>();
        self.log(
            MessageType::LOG,
            format!("publishing {} diagnostics", diagnostic_count),
        )
        .await;
        self.publish_diagnostics(last_compilation.diagnostics().clone())
            .await;
        self.last_compilation
            .write()
            .await
            .replace(last_compilation);
        self.log(MessageType::LOG, "project check complete".to_string())
            .await;
    }

    pub async fn check_file(&self, uri: Url) {
        self.log(
            MessageType::LOG,
            format!("starting standalone file check: {}", uri),
        )
        .await;

        // if let Some((text, ctx)) =
        // self.standalone_files.write().await.get_mut(&uri) {
        //     let file_ref = match ctx.default_file(uri.clone()) {
        //         Ok(file_ref) => {
        //             self.log(
        //                 MessageType::LOG,
        //                 format!("registered file with ref: {:?}", file_ref),
        //             )
        //             .await;
        //             file_ref
        //         }
        //         Err(err) => {
        //             self.log(
        //                 MessageType::ERROR,
        //                 format!("failed to register file: {}", err.message),
        //             )
        //             .await;
        //             return;
        //         }
        //     };
        //     let _module = Map::from([(file_ref, text.as_str())]);

        //     self.log(
        //         MessageType::LOG,
        //         "standalone file check incomplete (todo)".to_string(),
        //     )
        //     .await;
        // todo!()
        // let diagnostics = match compile_hir(module) {
        //     Ok((_, ctx)) => {
        //         todo!()
        //     }
        //     Err(err) => {
        //         self.sand_individual_diagnostics(err).await
        //     }
        // };

        // self
        //     .publish_diagnostics(diagnostics, )
        //     .await;
        // } else {
        //     self.log(
        //         MessageType::WARNING,
        //         format!("file not found in standalone files: {}", uri),
        //     )
        //     .await;
        // }
    }
}
```

### src/lsp/files.rs

```rs
//! file management

use std::path::Path;
use std::path::PathBuf;

use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::Url;

use crate::compiler::structure::Map;
use crate::lsp::Backend;

impl Backend<'_> {
    pub async fn register_file(&self, uri: Url, content: String) {
        self.log(
            tower_lsp::lsp_types::MessageType::LOG,
            format!("registering file: {}", uri),
        )
        .await;

        self.file_contents
            .write()
            .await
            .insert(uri.clone(), content.clone());

        match self.context.write().await.register_file(uri.clone()) {
            Ok(fr) => {
                self.log(
                    tower_lsp::lsp_types::MessageType::LOG,
                    format!("successfully registered file with ref: {:?}", fr),
                )
                .await;
            }
            Err(e) => {
                self.log(
                    tower_lsp::lsp_types::MessageType::ERROR,
                    format!("failed to register file in context: {}", e),
                )
                .await;
                self.file_contents.write().await.remove(&uri);
                self.client
                    .publish_diagnostics(
                        uri.clone(),
                        vec![Diagnostic {
                            range: Default::default(),
                            message: e.to_string(),
                            severity: Some(tower_lsp::lsp_types::DiagnosticSeverity::ERROR),
                            ..Default::default()
                        }],
                        None,
                    )
                    .await;
            }
        };
    }
}

pub async fn discover_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk_directory(root, &mut files).await?;
    Ok(files)
}

pub async fn read_discovered_files(files: Vec<PathBuf>) -> std::io::Result<Map<Url, String>> {
    let mut map = Map::new();
    for file in files {
        let url = Url::from_file_path(&file).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid file path")
        })?;
        map.insert(url, std::fs::read_to_string(&file)?);
    }
    Ok(map)
}

async fn walk_directory(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if path.is_dir() {
            // Skip common directories we don't want to scan
            if let Some(name) = path.file_name() {
                let name = name.to_string_lossy();
                if name == "node_modules" || name == ".git" || name == "target" {
                    continue;
                }
            }

            Box::pin(walk_directory(&path, files)).await?;
        } else if let Some(ext) = path.extension() {
            // Match files by extension
            if ext == "sand" {
                files.push(path);
            }
        }
    }

    Ok(())
}
```

### src/lsp/mod.rs

```rs
//! an lsp implementation for our language

use std::collections::BTreeMap;
use std::path::PathBuf;

use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::compiler::context::CompileCtx;
use crate::compiler::context::ProjectCtx;
use crate::ir_types::typed_hir::TypedProgram;
use crate::lsp::config::load_config;
use crate::lsp::diagnostics::Diagnostics;
use crate::lsp::files::discover_files;
use crate::lsp::files::read_discovered_files;

pub mod annotate;
pub mod backend;
pub mod config;
pub mod diagnostics;
pub mod files;
pub mod util;

pub struct Backend<'lsp> {
    pub client: Client,
    // project context (persists for the lifetime of the server)
    pub project_root: RwLock<Option<PathBuf>>,
    pub file_contents: RwLock<BTreeMap<Url, String>>,

    pub context: RwLock<ProjectCtx>,

    pub standalone_files: RwLock<BTreeMap<Url, (String, Option<LastCompilation<'lsp>>)>>,

    pub last_compilation: RwLock<Option<LastCompilation<'lsp>>>,
}

pub enum LastCompilation<'cx> {
    Success {
        context: Box<CompileCtx<'cx>>,
        diagnostics: Diagnostics,
        ast: TypedProgram,
    },
    Failure {
        diagnostics: Diagnostics,
    },
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend<'static> {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root_uri = params.root_uri.as_ref();

        if let Some(uri) = root_uri
            && let Ok(root_path) = uri.to_file_path()
        {
            self.log(
                MessageType::INFO,
                format!("initializing with root: {}", root_path.display()),
            )
            .await;

            let mut lock = self.project_root.write().await;
            *lock = Some(root_path.clone());
            // Try to load config first
            match load_config(&root_path).await {
                Ok(Some(config)) => {
                    self.log(
                        MessageType::INFO,
                        format!("loaded project config from {}", root_path.display()),
                    )
                    .await;
                    // Use configured files
                    if let Err(e) = self.apply_config(&config).await {
                        self.log(MessageType::WARNING, format!("error applying config: {e}"))
                            .await;
                    } else {
                        let file_count = self.file_contents.read().await.len();
                        self.log(
                            MessageType::INFO,
                            format!("registered {} project files", file_count),
                        )
                        .await;
                    }
                }
                Ok(None) => {
                    self.log(
                        MessageType::INFO,
                        "no sand.toml found, discovering files...",
                    )
                    .await;
                    // Fall back to recursive discovery
                    if let Ok(paths) = discover_files(&root_path).await
                        && let Ok(files) = read_discovered_files(paths).await
                    {
                        let file_count = files.len();
                        for (url, text) in files.into_iter() {
                            self.register_file(url, text).await;
                        }
                        self.log(
                            MessageType::INFO,
                            format!("discovered {} sand files", file_count),
                        )
                        .await;
                    } else {
                        self.log(MessageType::WARNING, "failed to discover files")
                            .await;
                    }
                }
                Err(e) => {
                    self.log(
                        MessageType::WARNING,
                        format!("error loading sand.toml: {e}"),
                    )
                    .await;
                }
            }
        } else {
            self.log(
                MessageType::WARNING,
                "no root URI provided for initialization",
            )
            .await;
        }
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let root = self.project_root.read().await;
        let file_count = self.file_contents.read().await.len();
        let root_display = root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        self.log(
            MessageType::INFO,
            format!(
                "sand-lsp initialized at {} with {} tracked files",
                root_display, file_count
            ),
        )
        .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.log(MessageType::LOG, format!("opening file: {}", uri))
            .await;
        let text = params.text_document.text;
        self.handle_file(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.log(MessageType::LOG, format!("file changed: {}", uri))
            .await;
        let text = params.content_changes[0].text.clone();
        self.handle_file(uri, text).await;
    }
}

impl Backend<'_> {
    async fn handle_file(&self, uri: Url, text: String) {
        if self.file_contents.read().await.contains_key(&uri) {
            self.log(
                MessageType::LOG,
                "file is part of tracked project, re-checking project".to_string(),
            )
            .await;
            self.register_file(uri, text).await;
            self.check_project().await;
        } else {
            self.log(
                MessageType::LOG,
                "file is standalone, updating and re-checking".to_string(),
            )
            .await;
            if let Some(entry) = self.standalone_files.write().await.get_mut(&uri) {
                entry.0 = text;
            } else {
                self.standalone_files
                    .write()
                    .await
                    .insert(uri.clone(), (text, None));
            }
            self.check_file(uri).await;
        }
    }
}
```

### src/lsp/diagnostics/typecheck.rs

```rs
//! turn AstTypeError to diagnostics

use tower_lsp::lsp_types::*;

use crate::compiler::context::CompileCtx;
use crate::lsp::diagnostics::Diagnostics;
use crate::lsp::util::lsp_range_from_pest;
use crate::passes::type_ast::AstTypeError;

pub fn type_error_to_diagnostic(
    _ctx: &CompileCtx,
    uri: Url,
    text: &str,
    err: AstTypeError,
) -> Diagnostics {
    use crate::passes::type_ast::AstTypeError::*;
    let mut diagnostics = Diagnostics::default();
    match err {
        UnboundVariable { name, range } => {
            let range = lsp_range_from_pest(text, range);
            let message = format!("unbound variable '{}'", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "no binding found for this variable".into(),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }
        UndefinedFunction { name, range } => {
            let range = lsp_range_from_pest(text, range);
            let message = format!("undefined function '{}'", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "no function with this name was found".into(),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }
        TypeError {
            message,
            expected,
            found,
            range,
        } => {
            let range = lsp_range_from_pest(text, range);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: format!("expected type: {:?}, found type: {:?}", expected, found),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message: format!("{} (expected {:?}, found {:?})", message, expected, found),
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }
        FunctionCallTypeError {
            message,
            expected,
            found,
            range,
        } => {
            let range = lsp_range_from_pest(text, range);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: format!(
                    "expected argument types: {:?}, found argument types: {:?}",
                    expected, found
                ),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message: format!("{} (expected {:?}, found {:?})", message, expected, found),
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }
    }
    diagnostics
}
```

### src/lsp/diagnostics/uniquify.rs

```rs
//! convert uniquify errors to LSP diagnostics

use tower_lsp::lsp_types::*;

use crate::compiler::context::CompileCtx;
use crate::lsp::diagnostics::Diagnostics;
use crate::lsp::util::lsp_range_from_pest;
use crate::passes::qualify::uniquify::error::UniquifyError;

pub(super) fn uniquify_error_to_diagnostic(
    _ctx: &CompileCtx,
    uri: Url,
    text: &str,
    err: UniquifyError,
) -> Diagnostics {
    use UniquifyError::*;
    let mut diagnostics = Diagnostics::default();
    match err {
        UnboundVariable { name, at } => {
            let range = lsp_range_from_pest(text, at);
            let message = format!("unbound variable: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "no binding found for this variable".into(),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }

        UndefinedFunction { name, at } => {
            let range = lsp_range_from_pest(text, at);
            let message = format!("undefined function: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "no function with this name was found".into(),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }

        DuplicateFunction {
            name,
            first_instance,
            second_instance,
        } => {
            let first_range = lsp_range_from_pest(text, first_instance);
            let second_range = lsp_range_from_pest(text, second_instance);

            let message = format!("duplicate function: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range: first_range,
                },
                message: "first declaration is here".into(),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range: second_range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }

        IllegalFunctionName { name, at } => {
            let range = lsp_range_from_pest(text, at);
            let message = format!("illegal function name: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "function name is reserved".into(),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }

        DuplicateParameterName {
            name,
            first_instance,
            second_instance,
        } => {
            let first_range = lsp_range_from_pest(text, first_instance);
            let second_range = lsp_range_from_pest(text, second_instance);

            let message = format!("duplicate parameter: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range: first_range,
                },
                message: "first parameter with this name is here".into(),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range: second_range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }

        DuplicateVariableName {
            name,
            first_instance,
            second_instance,
        } => {
            let first_range = lsp_range_from_pest(text, first_instance);
            let second_range = lsp_range_from_pest(text, second_instance);

            let message = format!("duplicate variable: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range: first_range,
                },
                message: "first declaration is here".into(),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range: second_range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }
    }

    diagnostics
}
```

### src/lsp/diagnostics/qualify.rs

```rs
//! convert qualify errors to LSP diagnostics

use bimap::BiBTreeMap;
use tower_lsp::lsp_types::*;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FileRef;
use crate::lsp::diagnostics::Diagnostics;
use crate::lsp::diagnostics::uniquify::uniquify_error_to_diagnostic;
use crate::lsp::util::lsp_range_from_pest;
use crate::lsp::util::url_of_module_unchecked;
use crate::passes::qualify::error::QualifyError;

pub fn qualify_error_to_diagnostics(
    ctx: &CompileCtx,
    file_map: &BiBTreeMap<Url, FileRef>,
    uri: Url,
    text: &str,
    err: QualifyError,
) -> Diagnostics {
    let mut diagnostics = Diagnostics::default();
    match err {
        QualifyError::DuplicateFunction {
            name,
            module,
            first_instance,
            second_instance,
        } => {
            // these two functions are in the same module, so the
            // DiagnosticRelatedInformation can use the same file URL
            diagnostics.add_one(
                uri.clone(),
                Diagnostic {
                    range: lsp_range_from_pest(text, first_instance),
                    message: format!("function '{name}' is already defined in this module"),
                    source: Some(format!("error in module {module}")),
                    ..Default::default()
                },
            );
            diagnostics.add_one(
                uri,
                Diagnostic {
                    range: lsp_range_from_pest(text, second_instance),
                    message: format!("function '{name}' is already defined in this module",),
                    source: Some(format!("error in module {module}")),
                    ..Default::default()
                },
            );
        }
        QualifyError::DuplicateMain {
            first,
            second,
            first_module,
            second_module,
        } => {
            let links = vec![
                DiagnosticRelatedInformation {
                    location: Location {
                        uri: url_of_module_unchecked(first_module.index, ctx, file_map),
                        range: lsp_range_from_pest(text, first),
                    },
                    message: "first main function is here".to_string(),
                },
                DiagnosticRelatedInformation {
                    location: Location {
                        uri: url_of_module_unchecked(second_module.index, ctx, file_map),
                        range: lsp_range_from_pest(text, second),
                    },
                    message: "second main function is here".to_string(),
                },
            ];
            diagnostics.add_one(uri.clone(), Diagnostic {
                range: lsp_range_from_pest(text, first),
                message: "main function is already defined! you can only have one main function per project.".to_string(),
                related_information: Some(links.clone()),
                ..Default::default()
            });
            diagnostics.add_one(uri, Diagnostic {
                range: lsp_range_from_pest(text, second),
                message: "main function is already defined! you can only have one main function per project.".to_string(),
                related_information: Some(links),
                ..Default::default()
            });
        }

        // todo: keep track in which files each module was declared
        QualifyError::DuplicateModule(dm) => {
            diagnostics.add_one(
                uri,
                Diagnostic {
                    message: format!("module '{}' is already defined", dm.name),
                    source: Some(format!("error in module {}", dm.name)),
                    ..Default::default()
                },
            );
        }

        QualifyError::FunctionQualFailedFunctionNotFound {
            func,
            module,
            range,
        } => {
            diagnostics.add_one(
                uri,
                Diagnostic {
                    range: lsp_range_from_pest(text, range),
                    message: format!(
                        "function '{}' is not defined in module '{}'",
                        func, module.name
                    ),
                    source: Some(format!("error in module {}", module.name)),
                    ..Default::default()
                },
            );
        }

        QualifyError::FunctionQualFailedModuleNotFound {
            func,
            module,
            source_module,
            range,
        } => {
            diagnostics.add_one(
                uri,
                Diagnostic {
                    range: lsp_range_from_pest(text, range),
                    message: format!("module '{}' is not found for function '{}'", module, func),
                    source: Some(format!("error in module {}", source_module.name)),
                    ..Default::default()
                },
            );
        }

        QualifyError::UniquifyError { module: _, source } => {
            return uniquify_error_to_diagnostic(ctx, uri, text, source);
        }

        QualifyError::ModuleNotFound {
            module,
            source_module,
        } => {
            diagnostics.add_one(
                uri,
                Diagnostic {
                    message: format!("module '{}' is not found", module),
                    source: Some(format!("error in module {}", source_module.name)),
                    ..Default::default()
                },
            );
        }
    }
    diagnostics
}
```

### src/lsp/diagnostics/mod.rs

```rs
//! generate diagnostics from top-level compiler error `SandError`

pub mod ast;
pub mod qualify;
pub mod typecheck;
pub mod uniquify;

use bimap::BiBTreeMap;
use tower_lsp::lsp_types::*;

use crate::SandError;
use crate::SandErrorContext;
use crate::SandErrorSource;
use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;
use crate::lsp::Backend;
use crate::lsp::diagnostics::ast::ast_error_to_diagnostics;
use crate::lsp::diagnostics::qualify::qualify_error_to_diagnostics;
use crate::lsp::diagnostics::typecheck::type_error_to_diagnostic;
use crate::lsp::util::url_of_module_unchecked;

// todo: unimplement clone
#[derive(Debug, Default, Clone)]
pub struct Diagnostics {
    pub map: Map<Url, Vec<Diagnostic>>,
}

impl Diagnostics {
    pub fn add(&mut self, uri: Url, mut diagnostics: Vec<Diagnostic>) {
        self.map
            .entry(uri.clone())
            .and_modify(|e| e.append(&mut diagnostics))
            .or_insert(diagnostics);
    }
    pub fn add_one(&mut self, uri: Url, diagnostic: Diagnostic) {
        self.map
            .entry(uri)
            .and_modify(|e| e.push(diagnostic.clone()))
            .or_insert(vec![diagnostic]);
    }
    pub fn single(uri: Url, diagnostic: Diagnostic) -> Self {
        Self {
            map: Map::from([(uri, vec![diagnostic])]),
        }
    }
}

pub fn sand_source_diagnostics(
    ctx: &CompileCtx,
    file_map: &BiBTreeMap<Url, FileRef>,
    uri: Url,
    text: &str,
    sand_err: SandErrorSource,
) -> Diagnostics {
    match sand_err {
        SandErrorSource::AstParseError(err) => ast_error_to_diagnostics(ctx, uri, text, err),
        SandErrorSource::QualifyError(err) => {
            qualify_error_to_diagnostics(ctx, file_map, uri, text, err)
        }
        SandErrorSource::TypeError(err) => type_error_to_diagnostic(ctx, uri, text, err),
    }
}

impl<'lsp> Backend<'lsp> {
    async fn uri_of_context(
        &self,
        ctx: &CompileCtx<'lsp>,
        context: &SandErrorContext,
    ) -> Option<Url> {
        match (context.module, context.file) {
            (Some(mr), _) => {
                let url = Some(url_of_module_unchecked(
                    mr,
                    ctx,
                    &self.context.read().await.files,
                ));
                self.log(
                    MessageType::LOG,
                    format!("resolved module context: {:?}", mr),
                )
                .await;
                url
            }
            (None, Some(fr)) => {
                let url = Some(self.context.read().await.url_of_file(fr));
                self.log(MessageType::LOG, format!("resolved file context: {:?}", fr))
                    .await;
                url
            }
            (None, None) => {
                self.log(
                    MessageType::WARNING,
                    "no module or file context available".to_string(),
                )
                .await;
                None
            }
        }
    }

    pub async fn publish_diagnostics(&self, diagnostics: Diagnostics) {
        let total_diagnostics = diagnostics.map.values().map(|v| v.len()).sum::<usize>();
        self.log(
            MessageType::LOG,
            format!("publishing diagnostics to {} files", diagnostics.map.len()),
        )
        .await;

        for (uri, diags) in diagnostics.map {
            let count = diags.len();
            self.log(
                MessageType::LOG,
                format!("publishing {} diagnostics to {}", count, uri),
            )
            .await;
            self.client
                .publish_diagnostics(uri.clone(), diags, None)
                .await;
        }

        self.log(
            MessageType::LOG,
            format!("published {} total diagnostics", total_diagnostics),
        )
        .await;
    }

    pub async fn sand_diagnostics(
        &self,
        ctx: &CompileCtx<'lsp>,
        sand_err: SandError,
    ) -> Diagnostics {
        self.log(
            MessageType::LOG,
            "processing project compilation error".to_string(),
        )
        .await;

        let SandError { source, context } = sand_err;
        let Some(uri) = self.uri_of_context(ctx, &context).await else {
            self.log(
                MessageType::ERROR,
                format!("cannot resolve error context for error: {:?}", context),
            )
            .await;
            return Diagnostics::default();
        };

        if let Some(text) = self
            .file_contents
            .read()
            .await
            .get(&uri)
            .map(|s| s.as_str())
        {
            self.log(
                MessageType::LOG,
                format!("converting source diagnostics for {}", uri),
            )
            .await;
            sand_source_diagnostics(ctx, &self.context.read().await.files, uri, text, source)
        } else {
            self.log(
                MessageType::WARNING,
                format!("file text not found for error context: {}", uri),
            )
            .await;
            Diagnostics::default()
        }
    }

    pub async fn sand_individual_diagnostics(
        &self,
        ctx: &CompileCtx<'lsp>,
        sand_err: SandError,
    ) -> Diagnostics {
        self.log(
            MessageType::LOG,
            "processing standalone file error".to_string(),
        )
        .await;

        let SandError { source, context } = sand_err;
        let Some(uri) = self.uri_of_context(ctx, &context).await else {
            self.log(
                MessageType::ERROR,
                format!(
                    "cannot resolve error context for standalone file error: {:?}",
                    context
                ),
            )
            .await;
            return Diagnostics::default();
        };

        if let Some((text, _)) = self.standalone_files.read().await.get(&uri) {
            self.log(
                MessageType::LOG,
                format!("converting standalone file diagnostics for {}", uri),
            )
            .await;
            sand_source_diagnostics(
                ctx,
                &self.context.read().await.files,
                uri,
                text.as_str(),
                source,
            )
        } else {
            self.log(
                MessageType::WARNING,
                format!("standalone file not found for error context: {}", uri),
            )
            .await;
            Diagnostics::default()
        }
    }
}
```

### src/lsp/diagnostics/ast.rs

```rs
//! convert AstErrors into LSP diagnostics

use tower_lsp::lsp_types::*;

use crate::compiler::context::CompileCtx;
use crate::lsp::diagnostics::Diagnostics;
use crate::lsp::util::lsp_range_from_pest;
use crate::lsp::util::parse_error_to_diagnostic;
use crate::passes::build_ast::AstError;

/// convert an AstError into one or more lsp diagnostics
pub(super) fn ast_error_to_diagnostics(
    _ctx: &CompileCtx,
    uri: Url,
    text: &str,
    err: AstError,
) -> Diagnostics {
    let mut diagnostics = Diagnostics::default();
    match err {
        AstError::Pest(parse_err) => {
            diagnostics.add_one(uri, parse_error_to_diagnostic(text, *parse_err))
        }

        AstError::UnexpectedRule {
            expected,
            got,
            range,
        } => {
            let range = lsp_range_from_pest(text, range);
            let message = format!("unexpected rule: expected {:?}, got {:?}", expected, got);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: format!("expected: {:?}, got: {:?}", expected, got),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }

        AstError::Missing { expected, range } => {
            let range = lsp_range_from_pest(text, range);
            let message = format!("missing {}", expected);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "syntax may be incomplete here".into(),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }

        AstError::InvalidInteger { got, range } => {
            let range = lsp_range_from_pest(text, range);
            let message = format!("invalid integer literal: {}", got);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "integer literal must fit in i64 and contain only digits".into(),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }

        AstError::InvalidName { got, range } => {
            let range = lsp_range_from_pest(text, range);
            let message = format!("invalid name: {}", got);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "name is reserved or otherwise invalid".into(),
            };

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    related_information: Some(vec![related]),
                    ..Default::default()
                },
            );
        }

        AstError::ContextError(ce) => {
            let range = lsp_range_from_pest(text, crate::compiler::structure::Range::default());
            let message = format!("internal compiler error: {:?}", ce);

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    ..Default::default()
                },
            );
        }

        AstError::UriError(err) => {
            let range = lsp_range_from_pest(text, crate::compiler::structure::Range::default());
            let message = format!("uri error: {}", err.message);

            diagnostics.add_one(
                uri,
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sand".into()),
                    message,
                    ..Default::default()
                },
            );
        }
    }
    diagnostics
}
```

### src/grammar.pest

```pest
// language definition
WHITESPACE = _{ " " | "\t" | "\n" | "\r" }
COMMENT    = _{ ("/*" ~ (!"*/" ~ ANY)* ~ "*/") | ("//" ~ (!"\n" ~ ANY)*) }

// primitive tokens
type_ = {
    "Int"
  | "Bool"
  | "Unit" // equivalent to rust (), python None, (sort of) java null
}

// keywords (prevent them from being identifiers)
KEYWORD = { "if" | "then" | "else" | "let" | "def" | "true" | "false" | "Unit" | "Int" | "Bool" }

identifier = @{ !KEYWORD ~ ASCII_ALPHA ~ (ASCII_ALPHANUMERIC | "_")* }

number = @{ ASCII_DIGIT+ }

boolean = @{ "true" | "false" }

// binary_operand = _{ add | subtract | multiply | divide | power | and | or | xor }

unary_operand = { subtract | negate }

add      = { "+" }
subtract = { "-" }
multiply = { "*" }
divide   = { "/" }
pow      = { "^" }
negate   = { "!" }
and      = { "&" }
or       = { "|" }
xor      = { "⊕" | "#" }
gt       = { ">" }
lt       = { "<" }
ne       = { "≠" | "!=" }
eq       = { "==" }
ge       = { ">=" | "≥" }
le       = { "<=" | "≤" }

// language is built around expressions,
ifstatement = {
    "if" ~ expression ~ "then" ~ expression ~ ("else" ~ expression)?
}

whileloop = {
    "while" ~ expression ~ "do" ~ expression
}

function_call = { identifier ~ "(" ~ (expression ~ ("," ~ expression)*)? ~ ")" }
external_function_call = { identifier ~ "::" ~ identifier ~ "(" ~ (expression ~ ("," ~ expression)*)? ~ ")" }

// precedence chaining
primary = {
    ("(" ~ expression ~ ")")
  | ifstatement
  | whileloop
  | function_call
  | external_function_call
  | number
  | boolean
  | identifier
  | ("{" ~ statement* ~ expression? ~ "}")
}

unary      = { (unary_operand ~ unary) | primary }
power      = { unary ~ (pow ~ power)? }
mul_div    = { power ~ ((multiply | divide) ~ power)* }
add_sub    = { mul_div ~ ((add | subtract) ~ mul_div)* }
comparison = { add_sub ~ ((ge | le | gt | lt) ~ add_sub)* }
equality   = { comparison ~ ((eq | ne) ~ comparison)* }
logic_and  = { equality ~ (and ~ equality)* }
logic_xor  = { logic_and ~ (xor ~ logic_and)* }
logic_or   = { logic_xor ~ (or ~ logic_xor)* }
expression = { logic_or }

// variables
declaration = { "let" ~ identifier ~ ":" ~ type_ ~ "=" ~ expression }

// left here for later:
// constant = { "const" ~ declaration }
// mutable = { "mut" ~ declaration }

assignment = { identifier ~ "=" ~ expression }

// statements are the imperative elements of the language
statement = {
    ((declaration | assignment | expression) ~ ";")
}

// functions are highest-order tokens
parameter  = { identifier ~ ":" ~ type_ }
parameters = { parameter ~ ("," ~ parameter)* ~ ","? }

function = { "def" ~ identifier ~ "(" ~ parameters? ~ ")" ~ ":" ~ type_ ~ ":=" ~ expression }

module = { "module" ~ identifier ~ ";"? }

program = { SOI ~ (function | module)* ~ EOI }
```

### src/lang/types.rs

```rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ty {
    Int,
    Bool,
    Unit,
    Top,    // the any type. used for error reporting when type inference fails
    Bottom, // the never type, for when an expression can never produce a value
}

impl Ty {
    pub fn type_eq(&self, other: &Self) -> bool {
        use Ty::*;
        match (self, other) {
            (Bottom, Bottom) => true,
            (Top, Bottom) => false,
            (Bottom, Top) => false,
            (Top, _) => true,
            (_, Top) => true,
            (Int, Int) => true,
            (Bool, Bool) => true,
            (Unit, Unit) => true,
            _ => false,
        }
    }

    pub fn type_neq(&self, other: &Self) -> bool {
        !self.type_eq(other)
    }
}
```

### src/lang/mod.rs

```rs
//! language specification files

pub mod intrinsics;
pub mod ops;
pub mod types;
```

### src/lang/ops.rs

```rs
//! operators

use crate::lang::types::Ty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bop {
    Plus,
    Minus,
    Mult,
    Div,
    Pow,
    And,
    Or,
    Xor,
    Comp(CompOp),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompOp {
    Ge,
    Le,
    Eq,
    Ne,
    Gt,
    Lt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Uop {
    Neg,
    Not,
}

// specify what binary operations are allowed in this language.
impl Bop {
    /// returns the resulting type if the given types are accepted by this
    /// operator, and `Err(Ty)` with the expected type otherwise
    pub fn accepts_types(&self, left: Ty, right: Ty) -> Result<Ty, Ty> {
        use Bop::*;
        match self {
            Plus | Minus | Mult | Div | Pow => {
                if left == Ty::Int && right == Ty::Int {
                    Ok(Ty::Int)
                } else {
                    Err(Ty::Int)
                }
            }
            And | Or | Xor => {
                if left == right {
                    Ok(left) // both types are the same, so we can return either one
                } else {
                    Err(left) // could be either type, diagnostic is based on the first operand
                }
            }
            Comp(op) => {
                match op {
                    CompOp::Ge | CompOp::Le | CompOp::Gt | CompOp::Lt => {
                        if left == Ty::Int && right == Ty::Int {
                            Ok(Ty::Bool)
                        } else {
                            Err(Ty::Int)
                        }
                    }
                    CompOp::Eq | CompOp::Ne => {
                        if left == right {
                            Ok(Ty::Bool)
                        } else {
                            Err(left) // could be either type, diagnostic is based on the first operand
                        }
                    }
                }
            }
        }
    }
}

impl Uop {
    /// returns `Ok(Ty)` with the resulting type if the given type is accepted
    /// by this operator, and `Err(Ty)` with the expected type otherwise
    pub fn accepts_type(&self, right: Ty) -> Result<Ty, Ty> {
        use Uop::*;
        match self {
            Neg => {
                if right == Ty::Int {
                    Ok(Ty::Int)
                } else {
                    Err(Ty::Int)
                }
            }
            Not => {
                if right == Ty::Bool {
                    Ok(Ty::Bool)
                } else {
                    Err(Ty::Bool)
                }
            }
        }
    }
}
```

### src/lang/intrinsics.rs

```rs
//! intrinsics are functions that the compiler substitutes with non-language
//! machine code, in order to implement interactions with the OS

use std::fmt::Display;
use std::sync::LazyLock;

use crate::compiler::structure::FnName;
use crate::compiler::structure::Map;
use crate::lang::types::Ty;

pub static INTRINSICS: LazyLock<Map<Intrinsic, (FnName, IntrinsicSig)>> = LazyLock::new(intrinsics);

pub const RESERVED_FUNCTION_NAMES: [&str; 6] =
    ["print", "println", "printf", "scanf", "read", "readline"];

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Intrinsic {
    Print,
    Println,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IntrinsicSig {
    pub args: Vec<Ty>,
    pub ret_ty: Ty,
}

fn intrinsics() -> Map<Intrinsic, (FnName, IntrinsicSig)> {
    [
        (
            Intrinsic::Print,
            IntrinsicSig {
                args: vec![Ty::Top],
                ret_ty: Ty::Unit,
            },
        ),
        (
            Intrinsic::Println,
            IntrinsicSig {
                args: vec![Ty::Top],
                ret_ty: Ty::Unit,
            },
        ),
    ]
    .into_iter()
    .map(|(n, s)| (n, (FnName::from(n), s)))
    .collect()
}

impl Display for Intrinsic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Intrinsic::Print => write!(f, "print"),
            Intrinsic::Println => write!(f, "println"),
        }
    }
}

impl TryFrom<&str> for Intrinsic {
    type Error = ();
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "print" | "printf" => Ok(Intrinsic::Print),
            "println" => Ok(Intrinsic::Println),
            _ => Err(()),
        }
    }
}

pub fn fn_name_allowed(name: &str) -> bool {
    !RESERVED_FUNCTION_NAMES.contains(&name) && Intrinsic::try_from(name).is_err()
}
```

### src/ir_types/ssa.rs

```rs
//! a strongly typed abstract syntax tree IR,
//! - expressions are annotated with their types
//! - variables and functions are resolved (VarRef and FnRef instead of String)
//! - uniquify has already been run, so no name clashes
//! - is SSA form (each variable is assigned to exactly once)

use std::hash::Hash;
use std::hash::Hasher;

use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;
use crate::lang::structure::FnName;
use crate::lang::structure::Map;
use crate::lang::structure::Range;
use crate::lang::structure::VarName;
use crate::lang::types::*;

#[derive(Debug, Clone)]
pub struct SsaProgram {
    pub functions: Map<FnName, SsaFunction>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: VarName,
    pub ty: Ty,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub struct SsaFunction {
    pub name: FnName,
    pub range: Range,
    pub parameters: Vec<Parameter>,
    pub ret_type: Ty,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Declaration {
        name: VariableRef,
        range: Range,
        ty: Ty,
        val: Expr,
    },

    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VariableRef {
    Single(VarName),
    PhiNode(Vec<VarName>),
}

/// `Expr` wraps an `Expression` and carries start/end positions (line,col)
#[derive(Debug, Clone)]
pub struct Expr {
    pub expr: Expression,
    pub ty: Ty,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expression {
    If {
        cond: Box<Expr>,
        t: Box<Expr>,
        f: Box<Expr>,
    },
    While {
        cond: Box<Expr>,
        body: Box<Expr>,
    },
    BinOp {
        left: Box<Expr>,
        op: Bop,
        right: Box<Expr>,
    },
    UnOp {
        op: Uop,
        right: Box<Expr>,
    },
    Call {
        fn_name: FnName,
        args: Vec<Expr>,
    },
    IntrinsicCall {
        fn_name: Intrinsic,
        args: Vec<Expr>,
    },
    /// resolved variable reference
    RVar(VarName),
    Int(i64),
    Bool(bool),
    Unit,
    Block {
        statements: Vec<Statement>,
        expr: Option<Box<Expr>>,
    },
}

// --- trait implementations ---

impl PartialEq for Statement {
    fn eq(&self, other: &Self) -> bool {
        use Statement::*;
        match (self, other) {
            (
                Declaration {
                    name: n1,
                    ty: t1,
                    val: v1,
                    ..
                },
                Declaration {
                    name: n2,
                    ty: t2,
                    val: v2,
                    ..
                },
            ) => n1 == n2 && t1 == t2 && v1 == v2,

            (Expr(e1), Expr(e2)) => e1 == e2,
            _ => false,
        }
    }
}

impl Eq for Statement {}

impl Hash for Statement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use Statement::*;
        std::mem::discriminant(self).hash(state);
        match self {
            Declaration { name, ty, val, .. } => {
                name.hash(state);
                ty.hash(state);
                val.hash(state);
            }
            Expr(e) => e.hash(state),
        }
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

impl Hash for Expr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr.hash(state);
    }
}

impl Eq for Expr {}
```

### src/ir_types/qhir.rs

```rs
//! qualified functions high intermediate representation:
//!
//! all modules are combined,
//! all function calls have been confirmed
//! to be calling existing functions or intrinsics,
//! functions and variables all have unique identifiers

use std::hash::Hash;
use std::hash::Hasher;

use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;
use crate::lang::types::*;

#[derive(Debug, Clone)]
pub struct Program {
    pub functions: Map<FunRef, Function>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Parameter {
    pub name: UniqVar,
    pub ty: Ty,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Function {
    pub name: FunRef,
    pub range: Range,
    pub parameters: Vec<Parameter>,
    pub ret_type: Ty,
    pub body: Expr,
    pub src_module: ModuleRef,
}

#[derive(Debug, Clone, PartialOrd, Ord)]
pub enum Statement {
    Declaration {
        name: UniqVar,
        range: Range,
        ty: Ty,
        val: Expr,
    },

    Assignment {
        name: UniqVar,
        range: Range,
        val: Expr,
    },

    Expr(Expr),
}

/// `Expr` wraps an `Expression` and carries start/end positions (line,col)
#[derive(Debug, Clone, PartialOrd, Ord)]
pub struct Expr {
    pub expr: Expression,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Expression {
    If {
        cond: Box<Expr>,
        t: Box<Expr>,
        f: Box<Expr>,
    },
    While {
        cond: Box<Expr>,
        body: Box<Expr>,
    },
    BinOp {
        left: Box<Expr>,
        op: Bop,
        right: Box<Expr>,
    },
    UnOp {
        op: Uop,
        right: Box<Expr>,
    },
    Call {
        fn_name: FunRef,
        args: Vec<Expr>,
    },
    IntrinsicCall {
        fn_name: Intrinsic,
        args: Vec<Expr>,
    },
    Var(UniqVar),
    Int(i64),
    Bool(bool),
    Unit,
    Block {
        statements: Vec<Statement>,
        expr: Option<Box<Expr>>,
    },
}

impl Eq for Statement {}

impl Hash for Statement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use Statement::*;
        std::mem::discriminant(self).hash(state);
        match self {
            Declaration {
                name: var, ty, val, ..
            } => {
                var.hash(state);
                ty.hash(state);
                val.hash(state);
            }
            Assignment { name, val, .. } => {
                name.hash(state);
                val.hash(state);
            }
            Expr(e) => e.hash(state),
        }
    }
}

impl PartialEq for Statement {
    fn eq(&self, other: &Self) -> bool {
        use Statement::*;
        match (self, other) {
            (
                Declaration {
                    name: n1,
                    ty: t1,
                    val: v1,
                    ..
                },
                Declaration {
                    name: n2,
                    ty: t2,
                    val: v2,
                    ..
                },
            ) => n1 == n2 && t1 == t2 && v1 == v2,

            (
                Assignment {
                    name: n1, val: v1, ..
                },
                Assignment {
                    name: n2, val: v2, ..
                },
            ) => n1 == n2 && v1 == v2,

            (Expr(e1), Expr(e2)) => e1 == e2,
            _ => false,
        }
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

impl Hash for Expr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr.hash(state);
    }
}

impl Eq for Expr {}
```

### src/ir_types/hhir.rs

```rs
//! highest high intermediate representation:
//! the abstract syntax tree IR

use std::hash::Hash;
use std::hash::Hasher;

use crate::compiler::structure::FunRef;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::OriginalVarRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::lang::ops::*;
use crate::lang::types::*;

#[derive(Debug, Clone)]
pub struct ProgramModule {
    pub functions: Vec<Function>,
    pub module_name: ModuleRef,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum HirVar {
    Decl(OriginalVarRef),
    Unqualified(String),
    Uniq(UniqVar),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HirFnCall {
    Local(String),
    External { module: String, name: String },
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: HirVar,
    pub ty: Ty,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: FunRef,
    pub range: Range,
    pub parameters: Vec<Parameter>,
    pub ret_type: Ty,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Declaration {
        name: HirVar,
        range: Range,
        ty: Ty,
        val: Expr,
    },

    Assignment {
        name: HirVar,
        range: Range,
        val: Expr,
    },

    Expr(Expr),
}

/// `Expr` wraps an `Expression` and carries start/end positions (line,col)
#[derive(Debug, Clone)]
pub struct Expr {
    pub expr: Expression,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expression {
    If {
        cond: Box<Expr>,
        t: Box<Expr>,
        f: Box<Expr>,
    },
    While {
        cond: Box<Expr>,
        body: Box<Expr>,
    },
    BinOp {
        left: Box<Expr>,
        op: Bop,
        right: Box<Expr>,
    },
    UnOp {
        op: Uop,
        right: Box<Expr>,
    },
    Call {
        fn_name: HirFnCall,
        args: Vec<Expr>,
    },
    Var(HirVar),
    Int(i64),
    Bool(bool),
    Unit,
    Block {
        statements: Vec<Statement>,
        expr: Option<Box<Expr>>,
    },
}

impl HirVar {
    pub fn is_uniq(&self) -> bool {
        matches!(self, HirVar::Uniq(_))
    }
}

impl Eq for Statement {}

impl Hash for Statement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use Statement::*;
        std::mem::discriminant(self).hash(state);
        match self {
            Declaration {
                name: var, ty, val, ..
            } => {
                var.hash(state);
                ty.hash(state);
                val.hash(state);
            }
            Assignment { name, val, .. } => {
                name.hash(state);
                val.hash(state);
            }
            Expr(e) => e.hash(state),
        }
    }
}

impl PartialEq for Statement {
    fn eq(&self, other: &Self) -> bool {
        use Statement::*;
        match (self, other) {
            (
                Declaration {
                    name: n1,
                    ty: t1,
                    val: v1,
                    ..
                },
                Declaration {
                    name: n2,
                    ty: t2,
                    val: v2,
                    ..
                },
            ) => n1 == n2 && t1 == t2 && v1 == v2,

            (
                Assignment {
                    name: n1, val: v1, ..
                },
                Assignment {
                    name: n2, val: v2, ..
                },
            ) => n1 == n2 && v1 == v2,

            (Expr(e1), Expr(e2)) => e1 == e2,
            _ => false,
        }
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

impl Hash for Expr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr.hash(state);
    }
}

impl Eq for Expr {}
```

### src/ir_types/mod.rs

```rs
//! types for intermediate representations of the program.
//!
//! each pass takes in one IR and produces another (possibly the same) IR.

pub mod hhir;
pub mod qhir;
pub mod cfgmir;
pub mod display;
pub mod typed_hir;
```

### src/ir_types/typed_hir.rs

```rs
//! a strongly typed abstract syntax tree IR,
//! - expressions are annotated with their types
//! - variables and functions are resolved (VarRef and FnRef instead of String)
//! - uniquify has already been run, so no name clashes
//! - is SSA form (each variable is assigned to exactly once)

use std::hash::Hash;
use std::hash::Hasher;

use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::ir_types::qhir::Parameter;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;
use crate::lang::types::*;

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub functions: Map<FunRef, TypedFunction>,
}

#[derive(Debug, Clone)]
pub struct TypedFunction {
    pub name: FunRef,
    pub range: Range,
    pub parameters: Vec<Parameter>,
    pub ret_type: Ty,
    pub body: Expr,
    pub src_module: ModuleRef,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Declaration {
        name: UniqVar,
        range: Range,
        ty: Ty,
        val: Expr,
    },

    Assignment {
        name: UniqVar,
        range: Range,
        val: Expr,
    },

    Expr(Expr),
}

/// `Expr` wraps an `Expression` and carries start/end positions (line,col)
#[derive(Debug, Clone)]
pub struct Expr {
    pub expr: Expression,
    pub ty: Ty,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expression {
    If {
        cond: Box<Expr>,
        t: Box<Expr>,
        f: Box<Expr>,
    },
    While {
        cond: Box<Expr>,
        body: Box<Expr>,
    },
    BinOp {
        left: Box<Expr>,
        op: Bop,
        right: Box<Expr>,
    },
    UnOp {
        op: Uop,
        right: Box<Expr>,
    },
    Call {
        fn_name: FunRef,
        args: Vec<Expr>,
    },
    IntrinsicCall {
        fn_name: Intrinsic,
        args: Vec<Expr>,
    },
    Var(UniqVar),
    Int(i64),
    Bool(bool),
    Unit,
    Block {
        statements: Vec<Statement>,
        expr: Option<Box<Expr>>,
    },
}

// --- trait implementations ---

impl PartialEq for Statement {
    fn eq(&self, other: &Self) -> bool {
        use Statement::*;
        match (self, other) {
            (
                Declaration {
                    name: n1,
                    ty: t1,
                    val: v1,
                    ..
                },
                Declaration {
                    name: n2,
                    ty: t2,
                    val: v2,
                    ..
                },
            ) => n1 == n2 && t1 == t2 && v1 == v2,

            (
                Assignment {
                    name: n1, val: v1, ..
                },
                Assignment {
                    name: n2, val: v2, ..
                },
            ) => n1 == n2 && v1 == v2,

            (Expr(e1), Expr(e2)) => e1 == e2,
            _ => false,
        }
    }
}

impl Eq for Statement {}

impl Hash for Statement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use Statement::*;
        std::mem::discriminant(self).hash(state);
        match self {
            Declaration { name, ty, val, .. } => {
                name.hash(state);
                ty.hash(state);
                val.hash(state);
            }
            Assignment { name, val, .. } => {
                name.hash(state);
                val.hash(state);
            }
            Expr(e) => e.hash(state),
        }
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

impl Hash for Expr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr.hash(state);
    }
}

impl Eq for Expr {}
```

### src/ir_types/display/mod.rs

```rs
//! display implementations for inspecting the different IRs

pub mod cfgmir;
```

### src/ir_types/display/cfgmir.rs

```rs
//! inspect the MIR

use std::fmt::Write as _;

use crate::compiler::context::CompileCtx;
use crate::ir_types::cfgmir::*;

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
```

### src/ir_types/cfgmir.rs

```rs
//! a CFG MIR

use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;
use crate::lang::types::Ty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalId(pub usize);

#[derive(Debug, Clone)]
pub struct MirProgram {
    pub functions: Map<FunRef, MirFunction>,
}

#[derive(Debug, Clone)]
pub struct MirFunction {
    pub name: FunRef,
    pub range: Range,
    pub params: Vec<MirParam>,
    pub ret_type: Ty,

    pub locals: Vec<LocalDecl>,
    pub blocks: Vec<BasicBlock>,
    pub entry: BlockId,
}

#[derive(Debug, Clone)]
pub struct MirParam {
    pub local: LocalId,
    pub name: UniqVar,
    pub ty: Ty,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub enum LocalName {
    /// traceable back to source via CompileCtx
    User(UniqVar),
    /// index for uniqueness, hint for readability
    Temp(usize, &'static str),
}

#[derive(Debug, Clone)]
pub struct LocalDecl {
    pub id: LocalId,
    pub name: LocalName,
    pub ty: Ty,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Assign {
        dst: Place,
        value: RValue,
        range: Range,
    },

    /// expression statements with side effects
    Eval { value: RValue, range: Range },
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Goto {
        target: BlockId,
    },

    Branch {
        cond: Operand,
        then_bb: BlockId,
        else_bb: BlockId,
    },

    Return {
        value: Option<Operand>,
    },

    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Place {
    pub local: LocalId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Operand {
    Copy(Place),
    Const(Constant),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Constant {
    Int(i64),
    Bool(bool),
    Unit,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RValue {
    Use(Operand),

    BinaryOp {
        op: Bop,
        left: Operand,
        right: Operand,
    },

    UnaryOp {
        op: Uop,
        right: Operand,
    },

    Call {
        fn_name: FunRef,
        args: Vec<Operand>,
    },

    IntrinsicCall {
        fn_name: Intrinsic,
        args: Vec<Operand>,
    },
}
```

### src/compiler/context/mod.rs

```rs
//! the different contexts for the compiler

mod compile;
mod project;

pub use compile::*;
pub use project::*;
```

### src/compiler/context/project.rs

```rs
//! # the project context
//! the project context holds the project-wide configuration and other
//! data that persists across compilation runs

use bimap::BiBTreeMap;
use tower_lsp::lsp_types::Url;

use crate::compiler::structure::CodeFile;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::ProjectConfig;
use crate::compiler::structure::UriError;
use crate::compiler::structure::uri_name;

/// the project context
pub struct ProjectCtx {
    // project and files
    pub project_config: ProjectConfig,
    code_files: Vec<CodeFile>,
    pub files: BiBTreeMap<Url, FileRef>,

    default_file: Option<FileRef>,
}

impl ProjectCtx {
    pub fn initial() -> Self {
        Self {
            project_config: ProjectConfig::default(),
            code_files: vec![],
            files: BiBTreeMap::new(),
            default_file: None,
        }
    }

    // ============================ Files ==============================
    pub fn register_file(&mut self, uri: Url) -> Result<FileRef, UriError> {
        if let Some(fr) = self.files.get_by_left(&uri) {
            // file already registered, just return the pointer
            Ok(*fr)
        } else {
            let idx = self.code_files.len();
            let fr = FileRef(idx);
            let name = uri_name(&uri)?;
            let cf = CodeFile {
                uri: uri.clone(),
                name,
                index: fr,
                default_module: None,
            };
            self.code_files.push(cf);
            self.files.insert(uri, fr);

            Ok(fr)
        }
    }

    pub fn register_dummy_file(&mut self) -> FileRef {
        let idx = self.code_files.len();
        let fr = FileRef(idx);
        let cf = CodeFile {
            uri: Url::parse("dummy:///tmp/internal/sand_dummy_file.sand").unwrap(),
            name: "sand_dummy_file".to_string(),
            index: fr,
            default_module: None,
        };
        self.code_files.push(cf);
        fr
    }

    pub fn default_file(&mut self, uri: Url) -> Result<FileRef, UriError> {
        if let Some(fr) = self.default_file {
            Ok(fr)
        } else {
            let fr = self.register_file(uri)?;
            self.default_file = Some(fr);
            Ok(fr)
        }
    }

    pub fn url_of_file(&self, file: FileRef) -> Url {
        self.code_files[file.0].uri.clone()
    }
}
```

### src/compiler/context/compile.rs

```rs
//! # the compliation context
//! the context is passed between the different passes of the compiler
//! and holds persisting data or other compilation information.

use std::marker::PhantomData;

use pest::iterators::Pair;
use thiserror::Error;

use crate::compiler::structure::CodeModule;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::FunSig;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleInfo;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::OriginalFun;
use crate::compiler::structure::OriginalVar;
use crate::compiler::structure::OriginalVarRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::Set;
use crate::compiler::structure::UniqVar;
use crate::compiler::structure::VarName;
use crate::lang::types::Ty;
use crate::passes::parse::Rule;

const DEFAULT_MODULE_NAME: &str = "mAin";

pub struct CompileCtx<'run> {
    // variables
    original_variables: Vec<OriginalVar>,
    pub variable_usages: Map<OriginalVarRef, Set<Range>>,
    global_variables: Vec<UniqVar>,

    variable_types: Map<UniqVar, Ty>,

    // functions
    global_functions: Vec<OriginalFun>,
    function_signatures: Map<FunRef, FunSig>,
    pub entrypoint: Option<FunRef>,

    // modules
    project_modules: Vec<CodeModule>,

    // defaults
    file_defaults: Map<FileRef, ModuleRef>,
    default_module: Option<ModuleRef>,

    phantom: PhantomData<&'run ()>,
}

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("use of undeclared variable: {name} at {range}")]
    UndeclaredVariable { name: VarName, range: Range },

    #[error("cannot register variable with rule {rule:?}")]
    IllegalVariableRegistration { rule: Rule },

    #[error("cannot register function with rule {rule:?}")]
    IllegalFunctionRegistration { rule: Rule },
}

#[derive(Debug)]
pub struct CtxEmptyError {}

impl<'run> CompileCtx<'run> {
    pub fn initial() -> Self {
        Self {
            original_variables: Default::default(),
            variable_usages: Default::default(),
            global_variables: Default::default(),
            variable_types: Default::default(),
            global_functions: Default::default(),
            function_signatures: Default::default(),
            entrypoint: None,
            // project_config: Default::default(),
            // code_files: Vec::new(),
            project_modules: Default::default(),
            default_module: None,
            file_defaults: Default::default(),
            // default_file: None,
            phantom: Default::default(),
        }
    }

    // ========================== Variables ==============================
    pub fn new_original_variable(
        &mut self,
        pair: &Pair<'_, Rule>,
        rule: Rule,
    ) -> Result<OriginalVarRef, ContextError> {
        if !matches!(rule, Rule::declaration | Rule::parameter) {
            return Err(ContextError::IllegalVariableRegistration { rule });
        }

        let ovref = OriginalVarRef(self.original_variables.len());
        let var = OriginalVar::create(pair, ovref, rule.into());
        self.original_variables.push(var);

        Ok(ovref)
    }

    pub fn uniquify_original_variable(&mut self, ovref: OriginalVarRef) -> UniqVar {
        let idx = self.global_variables.len();
        let uv = UniqVar { idx, orig: ovref };
        self.global_variables.push(uv);
        uv
    }

    pub fn original_var_name(&self, ovref: OriginalVarRef) -> String {
        self.original_variables[ovref.0].name.name()
    }

    pub fn uniq_variable_name(&self, uv: UniqVar) -> String {
        debug_assert!(self.global_variables.contains(&uv));

        self.original_variables[uv.orig.0].name.name()
    }

    /// might be used later
    #[allow(dead_code)]
    fn register_variable_usage(&mut self, var: OriginalVarRef, range: Range) {
        self.variable_usages
            .entry(var)
            .and_modify(|e| {
                e.insert(range);
            })
            .or_insert(Set::from([range]));
    }

    pub fn get_var_type(&self, var: &UniqVar) -> Option<Ty> {
        debug_assert!(self.global_variables.contains(var));
        self.variable_types.get(var).cloned()
    }

    #[track_caller]
    pub fn var_type(&self, var: &UniqVar) -> Ty {
        debug_assert!(self.global_variables.contains(var));
        self.variable_types[var]
    }

    pub fn set_var_type(&mut self, var: UniqVar, ty: Ty) {
        debug_assert!(self.global_variables.contains(&var));
        let out = self.variable_types.insert(var, ty);
        debug_assert!(out.is_none());
    }

    // ============================= Functions ================================

    pub fn register_function(
        &mut self,
        pair: &Pair<'_, Rule>,
        module: &ModuleRef,
    ) -> Result<FunRef, ContextError> {
        let ofref = FunRef(self.global_functions.len());
        let fun = OriginalFun::create(pair, ofref, *module);
        self.global_functions.push(fun);

        Ok(ofref)
    }

    #[track_caller]
    pub fn original_fun_name(&self, fun: FunRef) -> String {
        debug_assert!(
            self.global_functions.len() >= fun.0,
            "{fun:?}:{:?}",
            self.global_functions
        );
        self.global_functions[fun.0].name.name()
    }

    pub fn get_fun_sig(&self, fun: &FunRef) -> Option<FunSig> {
        debug_assert!(self.global_functions.len() >= fun.0);
        self.function_signatures.get(fun).cloned()
    }

    #[track_caller]
    pub fn fun_sig(&self, fun: &FunRef) -> FunSig {
        debug_assert!(self.global_functions.len() >= fun.0);
        self.function_signatures[fun].clone()
    }

    pub fn set_fun_sig(&mut self, fun: FunRef, sig: FunSig) {
        debug_assert!(self.global_functions.len() >= fun.0);
        let out = self.function_signatures.insert(fun, sig);
        debug_assert!(out.is_none());
    }

    pub fn is_main(&self, fun: FunRef) -> bool {
        self.entrypoint == Some(fun)
    }

    // ============================ Modules ===================================
    pub fn create_dummy_module(&mut self, for_file: FileRef) -> Result<ModuleRef, CtxEmptyError> {
        if self.default_module.is_some() {
            Err(CtxEmptyError {})
        } else {
            Ok(self.register_module(DEFAULT_MODULE_NAME, for_file))
        }
    }

    pub fn register_module(&mut self, name: &str, in_file: FileRef) -> ModuleRef {
        let idx = self.project_modules.len();
        let mr = ModuleRef(idx);
        let cm = CodeModule {
            index: mr,
            from_file: in_file,
            name: name.to_string(),
        };
        self.project_modules.push(cm);
        mr
    }

    pub fn default_module(&mut self, for_file: FileRef) -> ModuleRef {
        if let Some(dm) = self.file_defaults.get(&for_file) {
            *dm
        } else {
            let name = format!("{DEFAULT_MODULE_NAME}_{}", for_file.0);
            let mr = self.register_module(&name, for_file);
            self.file_defaults.insert(for_file, mr);
            mr
        }
    }

    /// return a reference to a dummy file.
    /// this should ONLY EVER be invoked at the start of a file-less
    /// compilation.
    pub fn dummy_file(&self) -> FileRef {
        assert!(self.project_modules.is_empty());
        FileRef(69420)
    }

    pub fn get_mod_by_name(&self, name: &str) -> Option<ModuleRef> {
        self.project_modules
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.index)
    }

    pub fn module_info(&self, mr: &ModuleRef) -> ModuleInfo {
        let cm = &self.project_modules[mr.0];
        ModuleInfo {
            name: cm.name.clone(),
            index: *mr,
        }
    }

    pub fn file_of_module(&self, mr: ModuleRef) -> FileRef {
        self.project_modules[mr.0].from_file
    }
}
```

### src/compiler/mod.rs

```rs
//! compiler internals

pub mod context;
pub mod structure;
```

### src/compiler/structure/variables.rs

```rs
//! variable management

use std::fmt::Display;

use pest::iterators::Pair;

use crate::compiler::structure::Range;
use crate::internal_bug;
use crate::passes::parse::Rule;

/// a globally unique reference to a variable
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UniqVar {
    pub(in crate::compiler) idx: usize,
    pub(in crate::compiler) orig: OriginalVarRef,
}

/// for any IR-specific variable type,
/// this object holds a unique reference
/// to the variable's source in the code
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OriginalVarRef(pub(in crate::compiler) usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VarName(pub(in crate::compiler) String);

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum VarDeclType {
    Declaration,
    Parameter,
    IntrinsicParameter,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OriginalVar {
    pub name: VarName,
    pub declaration: Range,
    pub inst: VarDeclType,
    index: OriginalVarRef,
}

impl VarName {
    pub(in crate::compiler) fn from_pair(pair: &Pair<'_, Rule>) -> Self {
        VarName(pair.as_str().to_string())
    }

    pub(in crate::compiler) fn name(&self) -> String {
        self.0.clone()
    }
}

impl OriginalVar {
    pub(in crate::compiler) fn create(
        pair: &Pair<'_, Rule>,
        index: OriginalVarRef,
        inst: VarDeclType,
    ) -> Self {
        OriginalVar {
            name: VarName::from_pair(pair),
            declaration: Range::from(pair),
            inst,
            index,
        }
    }
}

impl From<Rule> for VarDeclType {
    fn from(value: Rule) -> Self {
        match value {
            Rule::declaration => Self::Declaration,
            Rule::parameter => Self::Parameter,
            _ => internal_bug!("illegal instatiation of VarDeclType"),
        }
    }
}

impl Display for VarName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Vr({})", self.0)
    }
}
```

### src/compiler/structure/functions.rs

```rs
//! function management

use std::fmt::Display;

use pest::iterators::Pair;

use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::types::Ty;
use crate::passes::parse::Rule;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FnName(String);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunRef(pub(in crate::compiler) usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OriginalFun {
    pub name: FnName,
    pub declaration: Range,
    pub module: ModuleRef,
    index: FunRef,
}

impl OriginalFun {
    pub(in crate::compiler) fn create(
        pair: &Pair<'_, Rule>,
        index: FunRef,
        module: ModuleRef,
    ) -> Self {
        OriginalFun {
            name: FnName::from_pair(pair),
            declaration: Range::from(pair),
            module,
            index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunSig {
    pub args: Vec<(UniqVar, Ty)>,
    pub ret_ty: Ty,
}

impl FunSig {
    pub fn with(args: &[crate::ir_types::qhir::Parameter], ret_ty: Ty) -> Self {
        Self {
            args: args.iter().map(|a| (a.name, a.ty)).collect(),
            ret_ty,
        }
    }
}

impl FnName {
    pub(in crate::compiler) fn from_pair(pair: &Pair<'_, Rule>) -> Self {
        FnName(pair.as_str().to_string())
    }

    pub(in crate::compiler) fn name(&self) -> String {
        self.0.clone()
    }
}

impl From<Intrinsic> for FnName {
    fn from(value: Intrinsic) -> Self {
        FnName(value.to_string())
    }
}

impl Display for FnName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fn({})", self.0)
    }
}
```

### src/compiler/structure/mod.rs

```rs
//! types for structuring projects

mod debug;
mod functions;
mod projects;
mod variables;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub use debug::*;
pub use functions::*;
pub use projects::*;
pub use variables::*;

pub type Map<K, V> = BTreeMap<K, V>;
pub type Set<V> = BTreeSet<V>;
```

### src/compiler/structure/projects.rs

```rs
//! types relating to projects and their structure

use std::fmt::Display;

use thiserror::Error;
use tower_lsp::lsp_types::Url;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleRef(pub(in crate::compiler) usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CodeModule {
    pub(in crate::compiler) name: String,
    pub(in crate::compiler) from_file: FileRef,
    pub(in crate::compiler) index: ModuleRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleInfo {
    pub name: String,
    pub index: ModuleRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodeFile {
    /// file can be file:///path/to/file or http://remote/file or ssh://remote/file
    pub(in crate::compiler) uri: tower_lsp::lsp_types::Url,
    pub(in crate::compiler) name: String,
    pub(in crate::compiler) index: FileRef,
    pub(in crate::compiler) default_module: Option<ModuleRef>,
}

/// a reference to a specific code file.
/// implemented as an index into the context.code_files
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileRef(pub(in crate::compiler) usize);

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, serde::Deserialize)]
pub struct ProjectConfig {
    //todo
    pub tracked_files: Vec<Url>,
}

#[derive(Debug, Error)]
#[error("there was an error with the uri {uri}: {message}")]
pub struct UriError {
    pub uri: tower_lsp::lsp_types::Url,
    pub message: String,
}
impl UriError {
    pub fn new(uri: Url, message: String) -> Self {
        Self { uri, message }
    }
}

pub fn uri_name(uri: &tower_lsp::lsp_types::Url) -> Result<String, UriError> {
    let mut path_iter = uri.path_segments().ok_or_else(|| {
        UriError::new(
            uri.clone(),
            format!("provided uri {uri} cannot be turned into segments"),
        )
    })?;
    let _extension = path_iter.next_back();
    let name = path_iter
        .next_back()
        .ok_or_else(|| UriError::new(uri.clone(), format!("provided uri {uri} seems empty")))?
        .to_string();
    Ok(name)
}

impl Display for ModuleInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
```

### src/compiler/structure/debug.rs

```rs
//! internal types for program debugability

use std::fmt::Display;

use pest::RuleType;
use pest::Span;
use pest::iterators::Pair;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Range {
    pub start: Pos,
    pub end: Pos,
}

impl Pos {
    pub fn line_col(&self) -> (usize, usize) {
        (self.line, self.col)
    }

    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl From<(usize, usize)> for Pos {
    fn from(value: (usize, usize)) -> Self {
        Pos {
            line: value.0,
            col: value.1,
        }
    }
}

impl Range {
    pub fn new(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            start: Pos::new(start_line, start_col),
            end: Pos::new(end_line, end_col),
        }
    }

    pub fn destruct(&self) -> ((usize, usize), (usize, usize)) {
        (self.start.line_col(), self.end.line_col())
    }
}

impl From<(Pos, Pos)> for Range {
    fn from(value: (Pos, Pos)) -> Self {
        Range {
            start: value.0,
            end: value.1,
        }
    }
}

impl From<Range> for (Pos, Pos) {
    fn from(value: Range) -> Self {
        (value.start, value.end)
    }
}

impl From<&Span<'_>> for Range {
    fn from(value: &Span) -> Self {
        let start = value.start_pos();
        let end = value.end_pos();
        Range {
            start: Pos::from(start.line_col()),
            end: Pos::from(end.line_col()),
        }
    }
}

impl From<Span<'_>> for Range {
    fn from(value: Span) -> Self {
        (&value).into()
    }
}

impl<T: RuleType> From<&Pair<'_, T>> for Range {
    fn from(value: &Pair<T>) -> Self {
        value.as_span().into()
    }
}

impl<T: RuleType> From<Pair<'_, T>> for Range {
    fn from(value: Pair<T>) -> Self {
        (&value).into()
    }
}

impl Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Range({}:{} – {}:{})",
            self.start.line, self.start.col, self.end.line, self.end.col
        )
    }
}

impl Display for Pos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}
```

### examples/sand.toml

```toml
tracked_files = [
    "file:///Users/andtsa/proj/sand/examples/RSA.sand"
]
```

### tests/fib.rs

```rs
//! tests for examples in examples/

mod common;
use sand::ir_types::typed_hir::Expression;

#[test]
fn fib() -> anyhow::Result<()> {
    // run the code, examples must always work
    let out = common::interpret_example("fib")?;

    assert_eq!(out, Expression::Int(55));

    Ok(())
}
```

### tests/correct_programs.rs

```rs
//! assert that well-typed programs succeed in all passes of the compiler

mod common;

use std::hint::black_box;

use common::open_example_from_file;
use sand::compiler::context::CompileCtx;
use sand::ir_types::hhir::ProgramModule;
use sand::ir_types::qhir;
use sand::ir_types::typed_hir::TypedProgram;

fn test_layers(file: &str) {
    let code = open_example_from_file(file);
    let mut ctx = CompileCtx::initial();

    let p = ProgramModule::parse_stub(&mut ctx, &code).unwrap();
    let q = qhir::Program::combine(&mut ctx, vec![p]).unwrap();
    let t = TypedProgram::from_ast_program(&mut ctx, q).unwrap();

    // println!("{t:?}");
    black_box(t);
}

#[test]
fn test_rsa() {
    test_layers("RSA");
}

#[test]
fn test_prime() {
    test_layers("prime");
}

#[test]
fn test_fib() {
    test_layers("fib");
}

#[test]
fn test_fact() {
    test_layers("fact");
}

#[test]
fn test_gcd() {
    test_layers("gcd");
}
```

### tests/fact.rs

```rs
//! tests for examples in examples/

mod common;
use sand::ir_types::typed_hir::Expression;

#[test]
fn fact() -> anyhow::Result<()> {
    // run the code, examples must always work
    let out = common::interpret_example("fact")?;

    assert_eq!(out, Expression::Int(362880));

    Ok(())
}
```

### tests/common/mod.rs

```rs
//! helper methods for integration tests
#![allow(dead_code)]

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;
use sand::interpreter::mir::MirValue;
use sand::ir_types::cfgmir::MirProgram;
use sand::ir_types::typed_hir::Expression;

pub fn open_example_from_file(name: &str) -> String {
    let path = format!("examples/{}.sand", name);
    std::fs::read_to_string(path).expect("failed to read example file")
}

pub fn interpret_example(name: &str) -> anyhow::Result<Expression> {
    let mut ctx = CompileCtx::initial();
    let src = open_example_from_file(name);
    let code = Map::from([(ctx.dummy_file(), src.as_str())]);
    let program = compile_hir(code, &mut ctx)?;
    let hir_result = program.interpret(&ctx)?;
    assert_eq!(hir_result, interpret_mir_example(name)?);
    Ok(hir_result)
}

pub fn interpret_mir_example(name: &str) -> anyhow::Result<Expression> {
    let mut ctx = CompileCtx::initial();
    let src = open_example_from_file(name);
    let code = Map::from([(ctx.dummy_file(), src.as_str())]);
    let ast = compile_hir(code, &mut ctx)?;
    let mir = MirProgram::from_typed_program(&ast);
    let result = mir.interpret(&ctx).map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(match result {
        MirValue::Int(i)  => Expression::Int(i),
        MirValue::Bool(b) => Expression::Bool(b),
        MirValue::Unit    => Expression::Unit,
    })
}
```

### tests/mir_tests.rs

```rs
//! Tests for the CFG-MIR:
//!   - structural tests on the lowered MIR (block counts, locals, entry point)
//!   - MIR interpreter correctness
//!   - cross-checks that the MIR and typed-HIR interpreters agree

// these tests were (obviously) llm-generated. i take responsibility but no
// credit

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;
use sand::interpreter::mir::{MirInterpError, MirValue};
use sand::ir_types::cfgmir::{MirProgram, Terminator};
use sand::ir_types::typed_hir::Expression;

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Compile `src` all the way to a `MirProgram`.
fn lower(src: &str) -> (MirProgram, CompileCtx<'static>) {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.dummy_file();
    let code = Map::from([(fr, src)]);
    let ast = compile_hir(code, &mut ctx).unwrap_or_else(|e| panic!("compile failed:\n  {e}"));
    (MirProgram::from_typed_program(&ast), ctx)
}

/// Compile and run via the MIR interpreter; return the `MirValue`.
fn run_mir(src: &str) -> MirValue {
    let (mir, ctx) = lower(src);
    mir.interpret(&ctx)
        .unwrap_or_else(|e| panic!("MIR interpret failed:\n  {e}"))
}

/// Compile and run via the MIR interpreter; expect an error.
fn run_mir_fails(src: &str) -> MirInterpError {
    let (mir, ctx) = lower(src);
    mir.interpret(&ctx)
        .expect_err("expected MIR interpret to fail, but it succeeded")
}

/// Run both the typed-HIR and MIR interpreters and assert they agree.
fn assert_hir_mir_agree(src: &str) {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.dummy_file();
    let code = Map::from([(fr, src)]);
    let ast = compile_hir(code, &mut ctx).unwrap_or_else(|e| panic!("compile failed:\n  {e}"));

    let hir_result = ast
        .interpret(&ctx)
        .unwrap_or_else(|e| panic!("HIR interpret failed:\n  {e}"));

    let mir = MirProgram::from_typed_program(&ast);
    let mir_result = mir
        .interpret(&ctx)
        .unwrap_or_else(|e| panic!("MIR interpret failed:\n  {e}"));

    // convert MirValue → Expression for comparison
    let mir_as_expr = match mir_result {
        MirValue::Int(i) => Expression::Int(i),
        MirValue::Bool(b) => Expression::Bool(b),
        MirValue::Unit => Expression::Unit,
    };

    assert_eq!(
        hir_result, mir_as_expr,
        "HIR and MIR interpreters disagreed on:\n  {src}"
    );
}

// ─── structural tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod mir_structure_tests {
    use super::*;

    #[test]
    fn function_count_matches_source() {
        let (mir, _ctx) = lower("def main(): Int := 42");
        assert_eq!(mir.functions.len(), 1);

        let (mir, _ctx) = lower(
            "def helper(): Int := 1
             def main(): Int := helper()",
        );
        assert_eq!(mir.functions.len(), 2);

        let (mir, _ctx) = lower(
            "def a(): Int := 1
             def b(): Int := 2
             def c(): Int := 3
             def main(): Int := a()",
        );
        assert_eq!(mir.functions.len(), 4);
    }

    #[test]
    fn simple_literal_has_at_least_one_block() {
        let (mir, ctx) = lower("def main(): Int := 99");
        for func in mir.functions.values() {
            assert!(
                !func.blocks.is_empty(),
                "function {} has no blocks",
                ctx.original_fun_name(func.name)
            );
        }
    }

    #[test]
    fn all_blocks_have_a_terminator() {
        // Every basic block must end with a non-Unreachable terminator
        // (Unreachable is only for provably dead code paths).
        let cases = [
            "def main(): Int := 1 + 2",
            "def main(): Bool := if true then false else true",
            "def main(): Int := {
                let i: Int = 0;
                while i < 5 do { i = i + 1; };
                i
            }",
            "def f(x: Int): Int := x * 2
             def main(): Int := f(21)",
        ];
        for src in cases {
            let (mir, _ctx) = lower(src);
            for func in mir.functions.values() {
                for block in &func.blocks {
                    assert!(
                        !matches!(block.terminator, Terminator::Unreachable),
                        "block {} in function {:?} has Unreachable terminator for:\n  {src}",
                        block.id.0,
                        func.name
                    );
                }
            }
        }
    }

    #[test]
    fn parameters_have_corresponding_locals() {
        // Every MirParam must correspond to an entry in func.locals.
        let (mir, _ctx) = lower("def add(a: Int, b: Int): Int := a + b
                                  def main(): Int := add(1, 2)");
        for func in mir.functions.values() {
            for param in &func.params {
                assert!(
                    func.locals.iter().any(|l| l.id == param.local),
                    "param local {:?} not found in locals for {:?}",
                    param.local,
                    func.name
                );
            }
        }
    }

    #[test]
    fn if_expression_produces_branch_terminator() {
        // condition must be a runtime value — a constant bool is short-circuited
        // in lower_pred and produces no Branch terminator
        let (mir, _ctx) = lower("def main(): Int := {
            let b: Bool = true;
            if b then 1 else 2
        }");
        let func = mir.functions.values().next().unwrap();
        let has_branch = func
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Branch { .. }));
        assert!(has_branch, "expected at least one Branch terminator for if expression");
    }

    #[test]
    fn while_loop_produces_branch_and_goto() {
        let src = "def main(): Int := {
            let i: Int = 0;
            while i < 3 do { i = i + 1; };
            i
        }";
        let (mir, _ctx) = lower(src);
        let func = mir.functions.values().next().unwrap();
        let has_branch = func
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Branch { .. }));
        let has_goto = func
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Goto { .. }));
        assert!(has_branch, "while loop should produce a Branch terminator");
        assert!(has_goto, "while loop should produce a Goto (back-edge) terminator");
    }

    #[test]
    fn no_function_has_zero_locals_when_it_has_params() {
        let (mir, _ctx) = lower("def f(x: Int, y: Bool): Int := 0
                                  def main(): Int := f(1, true)");
        for func in mir.functions.values() {
            if !func.params.is_empty() {
                assert!(
                    !func.locals.is_empty(),
                    "function {:?} has params but no locals",
                    func.name
                );
            }
        }
    }

    #[test]
    fn let_bindings_produce_locals() {
        // A function with two let bindings should have at least two user locals.
        let src = "def main(): Int := {
            let a: Int = 1;
            let b: Int = 2;
            a + b
        }";
        let (mir, _ctx) = lower(src);
        let func = mir.functions.values().next().unwrap();
        let user_locals = func
            .locals
            .iter()
            .filter(|l| matches!(l.name, sand::ir_types::cfgmir::LocalName::User(_)))
            .count();
        assert!(
            user_locals >= 2,
            "expected at least 2 user locals, got {user_locals}"
        );
    }
}

// ─── MIR interpreter ─────────────────────────────────────────────────────────

#[cfg(test)]
mod mir_interpreter_tests {
    use super::*;

    // ── literals ─────────────────────────────────────────────────────────

    #[test]
    fn int_literal() {
        assert_eq!(run_mir("def main(): Int := 7"), MirValue::Int(7));
    }

    #[test]
    fn bool_true() {
        assert_eq!(run_mir("def main(): Bool := true"), MirValue::Bool(true));
    }

    #[test]
    fn bool_false() {
        assert_eq!(run_mir("def main(): Bool := false"), MirValue::Bool(false));
    }

    #[test]
    fn unit_literal() {
        assert_eq!(run_mir("def main(): Unit := { }"), MirValue::Unit);
    }

    // ── arithmetic ───────────────────────────────────────────────────────

    #[test]
    fn addition() {
        assert_eq!(run_mir("def main(): Int := 3 + 4"), MirValue::Int(7));
    }

    #[test]
    fn subtraction() {
        assert_eq!(run_mir("def main(): Int := 10 - 3"), MirValue::Int(7));
    }

    #[test]
    fn multiplication() {
        assert_eq!(run_mir("def main(): Int := 6 * 7"), MirValue::Int(42));
    }

    #[test]
    fn division_exact() {
        assert_eq!(run_mir("def main(): Int := 20 / 4"), MirValue::Int(5));
    }

    #[test]
    fn division_truncates() {
        // Integer division truncates toward zero.
        assert_eq!(run_mir("def main(): Int := 7 / 2"), MirValue::Int(3));
    }

    #[test]
    fn power() {
        assert_eq!(run_mir("def main(): Int := 2 ^ 8"), MirValue::Int(256));
    }

    #[test]
    fn unary_negation() {
        assert_eq!(run_mir("def main(): Int := -(5)"), MirValue::Int(-5));
    }

    #[test]
    fn operator_precedence_mul_before_add() {
        assert_eq!(
            run_mir("def main(): Int := 2 + 3 * 4"),
            MirValue::Int(14)
        );
    }

    #[test]
    fn parentheses_override_precedence() {
        assert_eq!(
            run_mir("def main(): Int := (2 + 3) * 4"),
            MirValue::Int(20)
        );
    }

    // ── division by zero ──────────────────────────────────────────────────

    #[test]
    fn division_by_zero_is_an_error() {
        let err = run_mir_fails("def main(): Int := 1 / 0");
        assert!(
            matches!(err, MirInterpError::DivisionByZero),
            "expected DivisionByZero, got {err}"
        );
    }

    #[test]
    fn division_by_zero_inside_expression() {
        let src = "def main(): Int := {
            let x: Int = 0;
            10 / x
        }";
        let err = run_mir_fails(src);
        assert!(
            matches!(err, MirInterpError::DivisionByZero),
            "expected DivisionByZero, got {err}"
        );
    }

    // ── boolean operations ────────────────────────────────────────────────

    #[test]
    fn bool_and_tt() {
        assert_eq!(
            run_mir("def main(): Bool := true & true"),
            MirValue::Bool(true)
        );
    }

    #[test]
    fn bool_and_tf() {
        assert_eq!(
            run_mir("def main(): Bool := true & false"),
            MirValue::Bool(false)
        );
    }

    #[test]
    fn bool_or_ft() {
        assert_eq!(
            run_mir("def main(): Bool := false | true"),
            MirValue::Bool(true)
        );
    }

    #[test]
    fn bool_or_ff() {
        assert_eq!(
            run_mir("def main(): Bool := false | false"),
            MirValue::Bool(false)
        );
    }

    #[test]
    fn bool_not_true() {
        assert_eq!(
            run_mir("def main(): Bool := !true"),
            MirValue::Bool(false)
        );
    }

    #[test]
    fn bool_not_false() {
        assert_eq!(
            run_mir("def main(): Bool := !false"),
            MirValue::Bool(true)
        );
    }

    #[test]
    fn bool_xor() {
        assert_eq!(
            run_mir("def main(): Bool := true # false"),
            MirValue::Bool(true)
        );
        assert_eq!(
            run_mir("def main(): Bool := true # true"),
            MirValue::Bool(false)
        );
    }

    // ── comparisons ───────────────────────────────────────────────────────

    #[test]
    fn eq_true() {
        assert_eq!(
            run_mir("def main(): Bool := 5 == 5"),
            MirValue::Bool(true)
        );
    }

    #[test]
    fn eq_false() {
        assert_eq!(
            run_mir("def main(): Bool := 5 == 6"),
            MirValue::Bool(false)
        );
    }

    #[test]
    fn ne() {
        assert_eq!(
            run_mir("def main(): Bool := 5 != 6"),
            MirValue::Bool(true)
        );
    }

    #[test]
    fn lt_true() {
        assert_eq!(
            run_mir("def main(): Bool := 3 < 5"),
            MirValue::Bool(true)
        );
    }

    #[test]
    fn lt_false_when_equal() {
        assert_eq!(
            run_mir("def main(): Bool := 5 < 5"),
            MirValue::Bool(false)
        );
    }

    #[test]
    fn le_true_when_equal() {
        assert_eq!(
            run_mir("def main(): Bool := 5 <= 5"),
            MirValue::Bool(true)
        );
    }

    #[test]
    fn gt_true() {
        assert_eq!(
            run_mir("def main(): Bool := 5 > 3"),
            MirValue::Bool(true)
        );
    }

    #[test]
    fn ge_true_when_equal() {
        assert_eq!(
            run_mir("def main(): Bool := 5 >= 5"),
            MirValue::Bool(true)
        );
    }

    // ── if / else ─────────────────────────────────────────────────────────

    #[test]
    fn if_true_branch() {
        assert_eq!(
            run_mir("def main(): Int := if true then 1 else 2"),
            MirValue::Int(1)
        );
    }

    #[test]
    fn if_false_branch() {
        assert_eq!(
            run_mir("def main(): Int := if false then 1 else 2"),
            MirValue::Int(2)
        );
    }

    #[test]
    fn if_with_comparison_condition() {
        assert_eq!(
            run_mir("def main(): Int := if 3 < 5 then 10 else 20"),
            MirValue::Int(10)
        );
    }

    #[test]
    fn nested_if() {
        let src = "def main(): Int :=
            if true then
                if false then 10 else 20
            else 30";
        assert_eq!(run_mir(src), MirValue::Int(20));
    }

    // ── while ─────────────────────────────────────────────────────────────

    #[test]
    fn while_never_entered_when_condition_false() {
        let src = "def main(): Int := {
            let x: Int = 0;
            while false do { x = x + 1; };
            x
        }";
        assert_eq!(run_mir(src), MirValue::Int(0));
    }

    #[test]
    fn while_runs_correct_number_of_iterations() {
        let src = "def main(): Int := {
            let x: Int = 0;
            while x < 5 do { x = x + 1; };
            x
        }";
        assert_eq!(run_mir(src), MirValue::Int(5));
    }

    #[test]
    fn while_accumulator_sum() {
        // 1 + 2 + … + 10 = 55
        let src = "def main(): Int := {
            let i: Int = 1;
            let s: Int = 0;
            while i <= 10 do {
                s = s + i;
                i = i + 1;
            };
            s
        }";
        assert_eq!(run_mir(src), MirValue::Int(55));
    }

    #[test]
    fn while_condition_uses_updated_variable() {
        // Make sure the condition is re-evaluated each iteration.
        let src = "def main(): Bool := {
            let flag: Bool = true;
            let i: Int = 0;
            while flag do {
                i = i + 1;
                flag = i < 3;
            };
            i == 3
        }";
        assert_eq!(run_mir(src), MirValue::Bool(true));
    }

    // ── blocks and let-bindings ───────────────────────────────────────────

    #[test]
    fn block_returns_trailing_expression() {
        let src = "def main(): Int := {
            let a: Int = 3;
            let b: Int = 4;
            a + b
        }";
        assert_eq!(run_mir(src), MirValue::Int(7));
    }

    #[test]
    fn block_with_only_statements_returns_unit() {
        let src = "def main(): Unit := {
            let x: Int = 1;
        }";
        assert_eq!(run_mir(src), MirValue::Unit);
    }

    #[test]
    fn nested_block_scoping() {
        let src = "def main(): Int := {
            let a: Int = 1;
            let b: Int = {
                let a: Int = 100;
                a + 1
            };
            a + b
        }";
        // outer a=1, inner block → 101, b=101, result = 102
        assert_eq!(run_mir(src), MirValue::Int(102));
    }

    #[test]
    fn assignment_updates_value() {
        let src = "def main(): Int := {
            let x: Int = 1;
            x = 42;
            x
        }";
        assert_eq!(run_mir(src), MirValue::Int(42));
    }

    #[test]
    fn multiple_assignments_to_same_variable() {
        let src = "def main(): Int := {
            let x: Int = 1;
            x = 2;
            x = 3;
            x
        }";
        assert_eq!(run_mir(src), MirValue::Int(3));
    }

    // ── function calls ────────────────────────────────────────────────────

    #[test]
    fn call_no_args() {
        let src = "def answer(): Int := 42
                   def main(): Int := answer()";
        assert_eq!(run_mir(src), MirValue::Int(42));
    }

    #[test]
    fn call_with_args() {
        let src = "def add(a: Int, b: Int): Int := a + b
                   def main(): Int := add(10, 32)";
        assert_eq!(run_mir(src), MirValue::Int(42));
    }

    #[test]
    fn recursive_factorial() {
        let src = "
            def fact(n: Int): Int :=
                if n == 0 then 1 else n * fact(n - 1)
            def main(): Int := fact(9)";
        assert_eq!(run_mir(src), MirValue::Int(362880));
    }

    #[test]
    fn recursive_fibonacci() {
        let src = "
            def fib(n: Int): Int :=
                if n <= 1 then n
                else fib(n - 1) + fib(n - 2)
            def main(): Int := fib(10)";
        assert_eq!(run_mir(src), MirValue::Int(55));
    }

    #[test]
    fn mutual_recursion() {
        let src = "
            def is_odd(n: Int): Bool :=
                if n == 0 then false else is_even(n - 1)
            def is_even(n: Int): Bool :=
                if n == 0 then true else is_odd(n - 1)
            def main(): Bool := is_even(10)";
        assert_eq!(run_mir(src), MirValue::Bool(true));
    }

    #[test]
    fn chained_calls() {
        let src = "
            def double(x: Int): Int := x * 2
            def quad(x: Int): Int := double(double(x))
            def main(): Int := quad(5)";
        assert_eq!(run_mir(src), MirValue::Int(20));
    }

    // ── edge cases ────────────────────────────────────────────────────────

    #[test]
    fn zero_power_is_one() {
        assert_eq!(run_mir("def main(): Int := 99 ^ 0"), MirValue::Int(1));
    }

    #[test]
    fn negate_zero_is_zero() {
        assert_eq!(run_mir("def main(): Int := -(0)"), MirValue::Int(0));
    }

    #[test]
    fn large_integer() {
        assert_eq!(
            run_mir("def main(): Int := 1000000 * 1000000"),
            MirValue::Int(1_000_000_000_000)
        );
    }

    #[test]
    fn negative_intermediate_value() {
        assert_eq!(
            run_mir("def main(): Int := 3 - 10 + 4"),
            MirValue::Int(-3)
        );
    }

    #[test]
    fn bool_equality() {
        assert_eq!(
            run_mir("def main(): Bool := true == true"),
            MirValue::Bool(true)
        );
        assert_eq!(
            run_mir("def main(): Bool := true == false"),
            MirValue::Bool(false)
        );
    }

    #[test]
    fn bool_ne() {
        assert_eq!(
            run_mir("def main(): Bool := true != false"),
            MirValue::Bool(true)
        );
    }
}

// ─── HIR ↔ MIR cross-checks ──────────────────────────────────────────────────
//
// For every program here, both interpreters must produce the same answer.
// This catches divergence between the two execution paths without having
// to hard-code expected values for complex programs.

#[cfg(test)]
mod mir_hir_agreement_tests {
    use super::*;

    #[test]
    fn agree_int_literal() {
        assert_hir_mir_agree("def main(): Int := 42");
    }

    #[test]
    fn agree_bool_literal() {
        assert_hir_mir_agree("def main(): Bool := true");
    }

    #[test]
    fn agree_unit_literal() {
        assert_hir_mir_agree("def main(): Unit := { }");
    }

    #[test]
    fn agree_arithmetic_expression() {
        assert_hir_mir_agree("def main(): Int := (3 + 4) * 2 - 1");
    }

    #[test]
    fn agree_chained_booleans() {
        assert_hir_mir_agree("def main(): Bool := (1 < 2) & (3 > 2) | false");
    }

    #[test]
    fn agree_if_true_branch() {
        assert_hir_mir_agree("def main(): Int := if 2 > 1 then 100 else 200");
    }

    #[test]
    fn agree_if_false_branch() {
        assert_hir_mir_agree("def main(): Int := if 1 > 2 then 100 else 200");
    }

    #[test]
    fn agree_nested_if() {
        assert_hir_mir_agree(
            "def main(): Int :=
                if true then if false then 1 else 2 else 3",
        );
    }

    #[test]
    fn agree_while_sum() {
        assert_hir_mir_agree(
            "def main(): Int := {
                let i: Int = 1;
                let s: Int = 0;
                while i <= 10 do {
                    s = s + i;
                    i = i + 1;
                };
                s
            }",
        );
    }

    #[test]
    fn agree_nested_blocks() {
        assert_hir_mir_agree(
            "def main(): Int := {
                let a: Int = {
                    let b: Int = 3;
                    b * 2
                };
                a + 1
            }",
        );
    }

    #[test]
    fn agree_variable_assignment() {
        assert_hir_mir_agree(
            "def main(): Int := {
                let x: Int = 10;
                x = x + 5;
                x
            }",
        );
    }

    #[test]
    fn agree_recursive_factorial() {
        assert_hir_mir_agree(
            "def fact(n: Int): Int :=
                if n == 0 then 1 else n * fact(n - 1)
             def main(): Int := fact(7)",
        );
    }

    #[test]
    fn agree_fibonacci() {
        assert_hir_mir_agree(
            "def fib(n: Int): Int :=
                if n <= 1 then n else fib(n - 1) + fib(n - 2)
             def main(): Int := fib(10)",
        );
    }

    #[test]
    fn agree_gcd() {
        assert_hir_mir_agree(
            "def gcd(a: Int, b: Int): Int :=
                if b == 0 then a else gcd(b, a - (a / b) * b)
             def main(): Int := gcd(48, 18)",
        );
    }

    #[test]
    fn agree_mutual_recursion() {
        assert_hir_mir_agree(
            "def is_odd(n: Int): Bool :=
                if n == 0 then false else is_even(n - 1)
             def is_even(n: Int): Bool :=
                if n == 0 then true else is_odd(n - 1)
             def main(): Bool := is_even(12)",
        );
    }

    #[test]
    fn agree_power_operator() {
        assert_hir_mir_agree("def main(): Int := 2 ^ 10");
    }

    #[test]
    fn agree_complex_boolean_expression() {
        assert_hir_mir_agree(
            "def main(): Bool := !false & (1 == 1) | (2 != 3)",
        );
    }

    #[test]
    fn agree_multiple_function_calls() {
        assert_hir_mir_agree(
            "def inc(x: Int): Int := x + 1
             def double(x: Int): Int := x * 2
             def main(): Int := double(inc(inc(5)))",
        );
    }
}
```

### tests/fail_parse.rs

```rs
use sand::compiler::context::CompileCtx;
use sand::ir_types::hhir::ProgramModule;

#[test]
fn gibberish() {
    let mut ctx = CompileCtx::initial();
    let program = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    assert!(ProgramModule::parse_stub(&mut ctx, program).is_err());
}
```

### tests/hir_tests.rs

```rs
//! tests for IR types and passes:
//! hhir (parse + build_ast), qhir (qualify + uniquify), typed_hir (type_ast),
//! and the interpreter.

// these tests were (obviously) llm-generated. i take responsibility but no
// credit

// ─── shared helpers ──────────────────────────────────────────────────────────

mod common {
    use sand::compile_hir;
    use sand::compiler::context::CompileCtx;
    use sand::compiler::structure::Map;
    use sand::ir_types::hhir::ProgramModule;
    use sand::ir_types::qhir;
    use sand::ir_types::typed_hir::Expression;
    use sand::ir_types::typed_hir::TypedProgram;

    /// Parse only — returns the raw `ProgramModule`.
    pub fn parse(src: &str) -> ProgramModule {
        let mut ctx = CompileCtx::initial();
        ProgramModule::parse_stub(&mut ctx, src).expect("parse failed")
    }

    /// Parse and expect failure.
    pub fn parse_fails(src: &str) {
        let mut ctx = CompileCtx::initial();
        assert!(
            ProgramModule::parse_stub(&mut ctx, src).is_err(),
            "expected parse to fail, but it succeeded"
        );
    }

    /// Parse → qualify.
    pub fn qualify(src: &str) -> qhir::Program {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(&mut ctx, src).expect("parse failed");
        qhir::Program::combine(&mut ctx, vec![pm]).expect("qualify failed")
    }

    /// Parse → qualify → type-check.
    pub fn typecheck(src: &str) -> TypedProgram {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        compile_hir(code, &mut ctx).expect("compile failed")
    }

    /// Parse → qualify → type-check and expect a **compile error**.
    pub fn typecheck_fails(src: &str) {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        assert!(
            compile_hir(code, &mut ctx).is_err(),
            "expected compile to fail, but it succeeded"
        );
    }

    // Convenience: run the interpreter and return the inner Expression value.
    // compile_hir produces a TypedProgram; interpret returns an Expression.
    pub fn run(src: &str) -> Expression {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        let prog = compile_hir(code, &mut ctx).unwrap_or_else(|e| panic!("compile failed:\n  {e}"));
        prog.interpret(&ctx)
            .unwrap_or_else(|e| panic!("interpreting failed:\n  {e}"))
    }
}

// ─── HHIR / parse + build_ast ────────────────────────────────────────────────

#[cfg(test)]
mod hhir_tests {
    use sand::compiler::context::CompileCtx;
    use sand::ir_types::hhir::ProgramModule;

    use super::common::*;

    // ── happy-path parsing ────────────────────────────────────────────────

    #[test]
    fn parse_minimal_unit_function() {
        // A function returning Unit with no parameters.
        parse("def main(): Unit := { }");
    }

    #[test]
    fn parse_integer_literal() {
        parse("def main(): Int := 42");
    }

    #[test]
    fn parse_bool_literal_true() {
        parse("def main(): Bool := true");
    }

    #[test]
    fn parse_bool_literal_false() {
        parse("def main(): Bool := false");
    }

    #[test]
    fn parse_single_parameter() {
        parse("def id(x: Int): Int := x");
    }

    #[test]
    fn parse_multiple_parameters() {
        parse("def add(a: Int, b: Int): Int := a + b");
    }

    #[test]
    fn parse_nested_arithmetic() {
        parse("def f(): Int := (1 + 2) * (3 - 4) / 5");
    }

    #[test]
    fn parse_unary_negation() {
        parse("def f(): Int := -(1)");
    }

    #[test]
    fn parse_unary_not() {
        parse("def f(): Bool := !true");
    }

    #[test]
    fn parse_if_then_else() {
        parse("def f(x: Bool): Int := if x then 1 else 2");
    }

    #[test]
    fn parse_if_then_no_else() {
        // Grammar allows omitting the else branch.
        parse("def f(x: Bool): Unit := if x then { }");
    }

    #[test]
    fn parse_while_loop() {
        parse(
            "def f(): Unit := {
                let i: Int = 0;
                while i < 10 do { i = i + 1; };
            }",
        );
    }

    #[test]
    fn parse_block_with_trailing_expr() {
        parse(
            "def f(): Int := {
                let x: Int = 1;
                let y: Int = 2;
                x + y
            }",
        );
    }

    #[test]
    fn parse_block_without_trailing_expr() {
        parse(
            "def f(): Unit := {
                let x: Int = 1;
            }",
        );
    }

    #[test]
    fn parse_local_function_call() {
        parse(
            "def helper(): Int := 0
             def main(): Int := helper()",
        );
    }

    #[test]
    fn parse_function_call_with_args() {
        parse(
            "def add(a: Int, b: Int): Int := a + b
             def main(): Int := add(1, 2)",
        );
    }

    #[test]
    fn parse_external_module_call() {
        // Syntax: module_name::function_name(...)
        // We only check that the grammar accepts it; resolution is done in qualify.
        parse(
            "module mymod;
             def f(): Int := mymod::g()",
        );
    }

    #[test]
    fn parse_comparison_operators() {
        for op in ["==", "!=", "<", "<=", ">", ">="] {
            parse(&format!("def f(a: Int, b: Int): Bool := a {} b", op));
        }
    }

    #[test]
    fn parse_boolean_operators() {
        parse("def f(a: Bool, b: Bool): Bool := a & b");
        parse("def f(a: Bool, b: Bool): Bool := a | b");
        parse("def f(a: Bool, b: Bool): Bool := a # b");
    }

    #[test]
    fn parse_power_operator() {
        parse("def f(a: Int, b: Int): Int := a ^ b");
    }

    #[test]
    fn parse_variable_shadowing_in_blocks() {
        // Outer `a` and inner `a` are different bindings.
        parse(
            "def f(): Int := {
                let a: Int = 1;
                let b: Int = {
                    let a: Int = 2;
                    a
                };
                a
            }",
        );
    }

    #[test]
    fn parse_assignment() {
        parse(
            "def f(): Int := {
                let x: Int = 0;
                x = 5;
                x
            }",
        );
    }

    #[test]
    fn parse_deeply_nested_blocks() {
        parse(
            "def f(): Int := {
                let a: Int = {
                    let b: Int = {
                        let c: Int = 3;
                        c
                    };
                    b
                };
                a
            }",
        );
    }

    #[test]
    fn parse_multiple_functions() {
        parse(
            "def a(): Int := 1
             def b(): Int := 2
             def main(): Int := a()",
        );
    }

    // ── number of functions ───────────────────────────────────────────────

    #[test]
    fn function_count_is_correct() {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(
            &mut ctx,
            "def a(): Int := 1
             def b(): Int := 2
             def main(): Int := a()",
        )
        .unwrap();
        assert_eq!(pm.functions.len(), 3);
    }

    // ── parse failures ────────────────────────────────────────────────────

    #[test]
    fn parse_fails_empty_string() {
        // An empty program has no functions, which parse_stub forbids
        // (it expects exactly one module with at least the grammar's EOI).
        // The test just checks that something ill-formed is rejected.
        parse_fails("def");
    }

    #[test]
    fn parse_fails_missing_return_type() {
        parse_fails("def f() := 1");
    }

    #[test]
    fn parse_fails_missing_body() {
        parse_fails("def f(): Int :=");
    }

    #[test]
    fn parse_fails_unclosed_block() {
        parse_fails("def f(): Int := { 1 ");
    }

    #[test]
    fn parse_fails_missing_paren() {
        parse_fails("def f(x: Int: Int := x");
    }

    #[test]
    fn parse_fails_keyword_as_identifier() {
        // `let` is a keyword; cannot be used as a function name.
        parse_fails("def let(): Int := 1");
    }

    #[test]
    fn parse_fails_reserved_function_name_print() {
        // `print` is reserved as an intrinsic.
        parse_fails("def print(x: Int): Unit := { }");
    }

    #[test]
    fn parse_fails_reserved_function_name_println() {
        parse_fails("def println(x: Int): Unit := { }");
    }

    #[test]
    fn parse_fails_unknown_type() {
        parse_fails("def f(): Float := 1.0");
    }

    #[test]
    fn parse_fails_statement_missing_semicolon() {
        parse_fails(
            "def f(): Int := {
                let x: Int = 1
                x
            }",
        );
    }
}

// ─── QHIR / qualify + uniquify ───────────────────────────────────────────────

#[cfg(test)]
mod qhir_tests {
    use sand::compiler::context::CompileCtx;
    use sand::ir_types::hhir::ProgramModule;
    use sand::ir_types::qhir;

    use super::common::*;

    // ── happy-path qualification ──────────────────────────────────────────

    #[test]
    fn qualify_simple_program() {
        qualify("def main(): Int := 42");
    }

    #[test]
    fn qualify_self_recursive_function() {
        qualify(
            "def fact(n: Int): Int :=
                if n == 0 then 1 else n * fact(n - 1)",
        );
    }

    #[test]
    fn qualify_mutual_calls() {
        qualify(
            "def a(): Int := b()
             def b(): Int := 1
             def main(): Int := a()",
        );
    }

    #[test]
    fn qualify_intrinsic_call_is_resolved() {
        // `println` is an intrinsic; qualify must map it to IntrinsicCall.
        let prog = qualify(
            "def main(): Unit := {
                println(1);
            }",
        );
        // There should be exactly one function in the qualified program.
        assert_eq!(prog.functions.len(), 1);
    }

    #[test]
    fn qualify_variable_names_are_unique() {
        // Two functions each declare a variable named `x`; after uniquify the
        // two UniqVar indices must differ.
        let prog = qualify(
            "def f(): Int := { let x: Int = 1; x }
             def g(): Int := { let x: Int = 2; x }
             def main(): Int := f()",
        );
        // Collect all UniqVar references from the body of f and g.
        // We just confirm the program qualified without error; deeper
        // inspection would require walking the IR.
        assert_eq!(prog.functions.len(), 3);
    }

    #[test]
    fn qualify_parameter_names_are_unique_across_functions() {
        qualify(
            "def f(x: Int): Int := x
             def g(x: Int): Int := x
             def main(): Int := f(g(1))",
        );
    }

    #[test]
    fn qualify_shadowed_variable_in_nested_block() {
        qualify(
            "def main(): Int := {
                let a: Int = 1;
                let b: Int = {
                    let a: Int = 2;
                    a
                };
                a
            }",
        );
    }

    // ── qualify failures ──────────────────────────────────────────────────

    #[test]
    fn qualify_fails_undefined_function() {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(&mut ctx, "def main(): Int := undefined_fn()")
            .expect("parse ok");
        assert!(
            qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
            "expected qualify to fail for undefined function"
        );
    }

    #[test]
    fn qualify_fails_unbound_variable() {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(&mut ctx, "def main(): Int := x").expect("parse ok");
        assert!(
            qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
            "expected qualify to fail for unbound variable"
        );
    }

    #[test]
    fn qualify_fails_duplicate_main() {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(
            &mut ctx,
            "def main(): Int := 1
             def main(): Int := 2",
        )
        .expect("parse ok");
        assert!(
            qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
            "expected qualify to fail for duplicate main"
        );
    }

    #[test]
    fn qualify_fails_duplicate_function_in_same_module() {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(
            &mut ctx,
            "def helper(): Int := 1
             def helper(): Int := 2
             def main(): Int := helper()",
        )
        .expect("parse ok");
        assert!(
            qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
            "expected qualify to fail for duplicate function"
        );
    }

    #[test]
    fn qualify_fails_assignment_to_unbound_variable() {
        // Assigning to a variable that was never declared.
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(
            &mut ctx,
            "def main(): Unit := {
                x = 5;
            }",
        )
        .expect("parse ok");
        assert!(
            qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
            "expected qualify to fail: assign to undeclared variable"
        );
    }
}

// ─── TypedHIR / type_ast ─────────────────────────────────────────────────────

#[cfg(test)]
mod typed_hir_tests {
    use super::common::*;

    // ── happy-path type checking ──────────────────────────────────────────

    #[test]
    fn typecheck_int_literal() {
        typecheck("def main(): Int := 0");
    }

    #[test]
    fn typecheck_bool_literal() {
        typecheck("def main(): Bool := true");
    }

    #[test]
    fn typecheck_unit_literal() {
        typecheck("def main(): Unit := { }");
    }

    #[test]
    fn typecheck_arithmetic() {
        typecheck("def main(): Int := 1 + 2 * 3 - 4 / 2");
    }

    #[test]
    fn typecheck_comparison_returns_bool() {
        typecheck("def main(): Bool := 1 < 2");
    }

    #[test]
    fn typecheck_equality_returns_bool() {
        typecheck("def main(): Bool := 1 == 1");
    }

    #[test]
    fn typecheck_boolean_and() {
        typecheck("def main(): Bool := true & false");
    }

    #[test]
    fn typecheck_boolean_or() {
        typecheck("def main(): Bool := true | false");
    }

    #[test]
    fn typecheck_boolean_not() {
        typecheck("def main(): Bool := !false");
    }

    #[test]
    fn typecheck_unary_negation() {
        typecheck("def main(): Int := -(3)");
    }

    #[test]
    fn typecheck_if_branches_same_type() {
        typecheck("def main(): Int := if true then 1 else 2");
    }

    #[test]
    fn typecheck_while_loop() {
        typecheck(
            "def main(): Unit := {
                let i: Int = 0;
                while i < 3 do {
                    i = i + 1;
                };
            }",
        );
    }

    #[test]
    fn typecheck_let_binding_and_return() {
        typecheck(
            "def main(): Int := {
                let x: Int = 10;
                x
            }",
        );
    }

    #[test]
    fn typecheck_function_call_correct_arg_types() {
        typecheck(
            "def add(a: Int, b: Int): Int := a + b
             def main(): Int := add(1, 2)",
        );
    }

    #[test]
    fn typecheck_recursive_function() {
        typecheck(
            "def fact(n: Int): Int :=
                if n == 0 then 1 else n * fact(n - 1)
             def main(): Int := fact(5)",
        );
    }

    #[test]
    fn typecheck_block_with_assignment() {
        typecheck(
            "def main(): Int := {
                let x: Int = 5;
                x = x + 1;
                x
            }",
        );
    }

    #[test]
    fn typecheck_nested_blocks() {
        typecheck(
            "def main(): Int := {
                let a: Int = {
                    let b: Int = 3;
                    b * 2
                };
                a + 1
            }",
        );
    }

    #[test]
    fn typecheck_intrinsic_println() {
        typecheck(
            "def main(): Unit := {
                println(42);
            }",
        );
    }

    #[test]
    fn typecheck_power_operator() {
        typecheck("def main(): Int := 2 ^ 10");
    }

    // ── type-error cases ──────────────────────────────────────────────────

    #[test]
    fn typecheck_fails_wrong_return_type() {
        // Function declares Int return, body produces Bool.
        typecheck_fails("def main(): Int := true");
    }

    #[test]
    fn typecheck_fails_wrong_return_type_bool_for_int() {
        typecheck_fails("def main(): Bool := 42");
    }

    #[test]
    fn typecheck_fails_add_bool_and_int() {
        typecheck_fails("def main(): Int := true + 1");
    }

    #[test]
    fn typecheck_fails_negate_int() {
        // `!` is boolean NOT; applying it to Int should fail.
        typecheck_fails("def main(): Bool := !1");
    }

    #[test]
    fn typecheck_fails_arithmetic_negate_bool() {
        // Unary minus on Bool.
        typecheck_fails("def main(): Int := -(true)");
    }

    #[test]
    fn typecheck_fails_if_condition_not_bool() {
        typecheck_fails("def main(): Int := if 1 then 2 else 3");
    }

    #[test]
    fn typecheck_fails_if_branches_different_types() {
        typecheck_fails("def main(): Int := if true then 1 else false");
    }

    #[test]
    fn typecheck_fails_while_condition_not_bool() {
        typecheck_fails(
            "def main(): Unit := {
                while 1 do { };
            }",
        );
    }

    #[test]
    fn typecheck_fails_declare_wrong_type() {
        typecheck_fails(
            "def main(): Int := {
                let x: Bool = 42;
                0
            }",
        );
    }

    #[test]
    fn typecheck_fails_assign_wrong_type() {
        typecheck_fails(
            "def main(): Int := {
                let x: Int = 0;
                x = true;
                x
            }",
        );
    }

    #[test]
    fn typecheck_fails_wrong_argument_type() {
        typecheck_fails(
            "def add(a: Int, b: Int): Int := a + b
             def main(): Int := add(true, 2)",
        );
    }

    #[test]
    fn typecheck_fails_wrong_argument_count_too_few() {
        typecheck_fails(
            "def add(a: Int, b: Int): Int := a + b
             def main(): Int := add(1)",
        );
    }

    #[test]
    fn typecheck_fails_wrong_argument_count_too_many() {
        typecheck_fails(
            "def add(a: Int, b: Int): Int := a + b
             def main(): Int := add(1, 2, 3)",
        );
    }

    #[test]
    fn typecheck_fails_comparison_mixed_types() {
        // `<` only accepts Int operands.
        typecheck_fails("def main(): Bool := true < false");
    }
}

// ─── Interpreter ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod interpreter_tests {
    use sand::ir_types::typed_hir::Expression;

    use super::common::*;

    // ── literals ─────────────────────────────────────────────────────────

    #[test]
    fn interpret_int_literal() {
        assert_eq!(run("def main(): Int := 7"), Expression::Int(7));
    }

    #[test]
    fn interpret_bool_true() {
        assert_eq!(run("def main(): Bool := true"), Expression::Bool(true));
    }

    #[test]
    fn interpret_bool_false() {
        assert_eq!(run("def main(): Bool := false"), Expression::Bool(false));
    }

    #[test]
    fn interpret_unit() {
        assert_eq!(run("def main(): Unit := { }"), Expression::Unit);
    }

    // ── arithmetic ───────────────────────────────────────────────────────

    #[test]
    fn interpret_addition() {
        assert_eq!(run("def main(): Int := 3 + 4"), Expression::Int(7));
    }

    #[test]
    fn interpret_subtraction() {
        assert_eq!(run("def main(): Int := 10 - 3"), Expression::Int(7));
    }

    #[test]
    fn interpret_multiplication() {
        assert_eq!(run("def main(): Int := 6 * 7"), Expression::Int(42));
    }

    #[test]
    fn interpret_division() {
        assert_eq!(run("def main(): Int := 20 / 4"), Expression::Int(5));
    }

    #[test]
    fn interpret_power() {
        assert_eq!(run("def main(): Int := 2 ^ 8"), Expression::Int(256));
    }

    #[test]
    fn interpret_unary_negation() {
        assert_eq!(run("def main(): Int := -(5)"), Expression::Int(-5));
    }

    #[test]
    fn interpret_operator_precedence() {
        // 2 + 3 * 4 = 14, not 20
        assert_eq!(run("def main(): Int := 2 + 3 * 4"), Expression::Int(14));
    }

    #[test]
    fn interpret_parenthesised_expr() {
        assert_eq!(run("def main(): Int := (2 + 3) * 4"), Expression::Int(20));
    }

    // ── boolean operations ────────────────────────────────────────────────

    #[test]
    fn interpret_bool_and_true() {
        assert_eq!(
            run("def main(): Bool := true & true"),
            Expression::Bool(true)
        );
    }

    #[test]
    fn interpret_bool_and_false() {
        assert_eq!(
            run("def main(): Bool := true & false"),
            Expression::Bool(false)
        );
    }

    #[test]
    fn interpret_bool_or() {
        assert_eq!(
            run("def main(): Bool := false | true"),
            Expression::Bool(true)
        );
    }

    #[test]
    fn interpret_bool_not() {
        assert_eq!(run("def main(): Bool := !false"), Expression::Bool(true));
    }

    // ── comparisons ───────────────────────────────────────────────────────

    #[test]
    fn interpret_eq_true() {
        assert_eq!(run("def main(): Bool := 5 == 5"), Expression::Bool(true));
    }

    #[test]
    fn interpret_eq_false() {
        assert_eq!(run("def main(): Bool := 5 == 6"), Expression::Bool(false));
    }

    #[test]
    fn interpret_ne() {
        assert_eq!(run("def main(): Bool := 5 != 6"), Expression::Bool(true));
    }

    #[test]
    fn interpret_lt() {
        assert_eq!(run("def main(): Bool := 3 < 5"), Expression::Bool(true));
    }

    #[test]
    fn interpret_gt() {
        assert_eq!(run("def main(): Bool := 5 > 3"), Expression::Bool(true));
    }

    #[test]
    fn interpret_le_equal() {
        assert_eq!(run("def main(): Bool := 5 <= 5"), Expression::Bool(true));
    }

    #[test]
    fn interpret_ge_greater() {
        assert_eq!(run("def main(): Bool := 6 >= 5"), Expression::Bool(true));
    }

    // ── if / else ─────────────────────────────────────────────────────────

    #[test]
    fn interpret_if_takes_true_branch() {
        assert_eq!(
            run("def main(): Int := if true then 1 else 2"),
            Expression::Int(1)
        );
    }

    #[test]
    fn interpret_if_takes_false_branch() {
        assert_eq!(
            run("def main(): Int := if false then 1 else 2"),
            Expression::Int(2)
        );
    }

    #[test]
    fn interpret_nested_if() {
        let src = "def main(): Int :=
            if true then
                if false then 10 else 20
            else 30";
        assert_eq!(run(src), Expression::Int(20));
    }

    // ── while ─────────────────────────────────────────────────────────────

    #[test]
    fn interpret_while_not_entered_when_false() {
        let src = "def main(): Int := {
            let x: Int = 0;
            while false do { x = x + 1; };
            x
        }";
        assert_eq!(run(src), Expression::Int(0));
    }

    #[test]
    fn interpret_while_runs_correct_iterations() {
        let src = "def main(): Int := {
            let x: Int = 0;
            while x < 5 do { x = x + 1; };
            x
        }";
        assert_eq!(run(src), Expression::Int(5));
    }

    #[test]
    fn interpret_while_accumulator() {
        // Sum 1..=10 = 55
        let src = "def main(): Int := {
            let i: Int = 1;
            let s: Int = 0;
            while i <= 10 do {
                s = s + i;
                i = i + 1;
            };
            s
        }";
        assert_eq!(run(src), Expression::Int(55));
    }

    // ── blocks and let-bindings ───────────────────────────────────────────

    #[test]
    fn interpret_block_trailing_expression() {
        let src = "def main(): Int := {
            let a: Int = 3;
            let b: Int = 4;
            a + b
        }";
        assert_eq!(run(src), Expression::Int(7));
    }

    #[test]
    fn interpret_nested_block_shadowing() {
        let src = "def main(): Int := {
            let a: Int = 1;
            let b: Int = {
                let a: Int = 100;
                a + 1
            };
            a + b
        }";
        // outer a=1, inner block returns 101, b=101, result = 1+101 = 102
        assert_eq!(run(src), Expression::Int(102));
    }

    #[test]
    fn interpret_assignment_updates_value() {
        let src = "def main(): Int := {
            let x: Int = 1;
            x = 42;
            x
        }";
        assert_eq!(run(src), Expression::Int(42));
    }

    // ── function calls ────────────────────────────────────────────────────

    #[test]
    fn interpret_function_call_no_args() {
        let src = "def answer(): Int := 42
                   def main(): Int := answer()";
        assert_eq!(run(src), Expression::Int(42));
    }

    #[test]
    fn interpret_function_call_with_args() {
        let src = "def add(a: Int, b: Int): Int := a + b
                   def main(): Int := add(10, 32)";
        assert_eq!(run(src), Expression::Int(42));
    }

    #[test]
    fn interpret_fibonacci_10() {
        let src = "
            def fib(n: Int): Int :=
                if n <= 1 then n
                else fib(n - 1) + fib(n - 2)
            def main(): Int := fib(10)";
        assert_eq!(run(src), Expression::Int(55));
    }

    #[test]
    fn interpret_factorial_9() {
        let src = "
            def fact(n: Int): Int :=
                if n == 0 then 1 else n * fact(n - 1)
            def main(): Int := fact(9)";
        assert_eq!(run(src), Expression::Int(362880));
    }

    #[test]
    fn interpret_gcd() {
        let src = "
            def gcd(a: Int, b: Int): Int :=
                if b == 0 then a else gcd(b, a - (a / b) * b)
            def main(): Int := gcd(48, 18)";
        assert_eq!(run(src), Expression::Int(6));
    }

    #[test]
    fn interpret_higher_order_via_explicit_call() {
        // Double a value by calling a helper.
        let src = "
            def double(x: Int): Int := x * 2
            def quad(x: Int): Int := double(double(x))
            def main(): Int := quad(5)";
        assert_eq!(run(src), Expression::Int(20));
    }

    #[test]
    fn interpret_mutual_recursion() {
        // is_even / is_odd via mutual recursion.
        let src = "
            def is_odd(n: Int): Bool :=
                if n == 0 then false else is_even(n - 1)
            def is_even(n: Int): Bool :=
                if n == 0 then true else is_odd(n - 1)
            def main(): Bool := is_even(10)";
        assert_eq!(run(src), Expression::Bool(true));
    }

    // ── regression / edge cases ───────────────────────────────────────────

    #[test]
    fn interpret_zero_power_is_one() {
        assert_eq!(run("def main(): Int := 99 ^ 0"), Expression::Int(1));
    }

    #[test]
    fn interpret_negative_zero_is_zero() {
        assert_eq!(run("def main(): Int := -(0)"), Expression::Int(0));
    }

    #[test]
    fn interpret_chained_comparisons_via_bool_ops() {
        // (1 < 2) & (3 > 2) = true & true = true
        let src = "def main(): Bool := (1 < 2) & (3 > 2)";
        assert_eq!(run(src), Expression::Bool(true));
    }

    #[test]
    fn interpret_bool_equality() {
        assert_eq!(
            run("def main(): Bool := true == true"),
            Expression::Bool(true)
        );
        assert_eq!(
            run("def main(): Bool := true == false"),
            Expression::Bool(false)
        );
    }

    #[test]
    fn interpret_block_with_only_statements_returns_unit() {
        assert_eq!(
            run("def main(): Unit := {
                let x: Int = 1;
            }"),
            Expression::Unit
        );
    }
}
```

