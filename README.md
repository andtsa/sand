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
Map<FileName, CodeFile>,
Config
```

status: not implemented

### parse files

input: previous pass

- read files to string
- parse each one

output:
```
Map<FileName, Result<Pairs<'i, Rule>, Error<Rule>>>
```

status: parsing implemented with pest, no file handling exists

### build ASTs

input: previous pass

- build untyped ast for each file

output:
```
Map<FileName, Result<Map<FnName, Function>, AstError>>
Map<FnName, FnSig>

```

status: implemented for single file

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

status: not implemented as a pass, but function name uniquifying exists in the uniquify step

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

status: not implemented

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

status: done but needs refactoring to separate out uniquifying function names

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

status: done but needs cleaning up

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

- SSA pass

