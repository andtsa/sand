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

