# The Sand Language Specification

> **Status:** working draft. This document specifies the _surface language_: its
> lexical structure, syntax, static rules, and dynamic semantics. It is written
> to be readable on its own, with worked examples, while remaining precise enough
> to serve as a reference for a second implementation.
>
> The formal type system (kinds, the region/outlives lattice, region-aware
> subtyping, the bidirectional typing judgements, and the ownership calculus) is
> developed rigorously in [`Calculus.md`](Calculus.md). Where this specification
> states a typing or region rule informally, it cites the corresponding section
> there by name (e.g. _[Calculus: Bidirectional Typing]_). The reference grammar
> is [`grammar.pest`](grammar.pest); §2-§7 describe the same language that grammar
> accepts, and Appendix A reproduces it.

---

## 1. Introduction

### 1.1 Overview

Sand is a small, statically typed, **expression-oriented** language. Its defining
characteristics are:

- **Everything is an expression.** Every construct has a type and a value;
  there are no statement-only forms that lack a value (a block with no trailing
  expression has value `Unit`).
- **Affine ownership.** Every value is used _at most once_. Passing a value
  moves it; using a moved value is a static error unless its type is `Copy`.
  Destruction is implicit and deterministic (RAII).
- **Lexical regions and borrows.** `&e` / `&mut e` borrow a value without
  consuming it. References carry a lexical region (`&'r T`); a borrow may not
  outlive its referent.
- **Generics, kinds, and typeclasses.** Type and region parameters are
  monomorphised away before code generation. Typeclass methods are ordinary
  functions resolved by argument type.

A first program:

```sand
def fib(n: Int): Int :=
  if n ≤ 1 then n else fib(n - 1) + fib(n - 2)

def main(): Int := {
    let mut x = 10;
    x = fib(x);
    println(x);
    0
}
```

### 1.2 Scope of this document

This specification covers the surface language a programmer writes. It does
**not** specify the compiler's internal intermediate representations (HHIR,
QHIR, Typed HIR, MIR) or its pass pipeline; those are described in
[`README.md`](README.md). Properties of _the compiler_ (diagnostics, CLI flags,
the LSP) are likewise out of scope except where they define observable program
behavior.

### 1.3 Conformance and terminology

A **conforming program** is one accepted by the static rules of §2-§10 and whose
execution is defined by §11-§13.

The key words are used as follows:

- **must** / **shall**: a requirement on conforming programs or implementations.
- **must not**: a prohibition; violating it is a static error unless stated
  otherwise.
- **may**: a genuinely optional behavior.
- **static error**: a condition an implementation is required to reject at
  compile time (parsing, type checking, region checking, or ownership analysis).

A program that contains a static error is **not** conforming and has no defined
behavior. This specification does not currently define any _undefined behavior_
for well-typed programs except where it explicitly says so (e.g. the behavior of
`extern` FFI calls, §6.5, and certain `Ptr` operations, §13).

### 1.4 Notational conventions

Syntax is presented in an EBNF-style notation derived from the reference PEG:

- `x?` is optional, `x*` zero or more, `x+` one or more.
- `a | b` are alternatives, `( … )` is grouping.
- Terminals are written in `"quotes"` or as named lexical productions from §2.
- Nonterminal names match the rules in [`grammar.pest`](grammar.pest) where
  practical, so the prose and the reference grammar can be read side by side.

Because the reference grammar is a Parsing Expression Grammar, alternatives are
**ordered**: the first matching alternative wins. This matters for a few rules
(e.g. patterns, §7.1) and is called out where it does.

---

## 2. Lexical structure

Lexical analysis turns a source file into a stream of tokens. Whitespace and
comments separate tokens but are otherwise insignificant.

### 2.1 Source text

A source file is a sequence of Unicode characters (typically a UTF-8 `.sand`
file). A small number of mathematical operators are spelled with non-ASCII
characters (§2.6); all other lexemes are ASCII.

### 2.2 Whitespace and layout

The whitespace characters are space (`U+0020`), tab (`U+0009`), carriage return
(`U+000D`), and line feed (`U+000A`). **Indentation and line breaks are not
significant**: any sequence of whitespace where a separator is permitted is
equivalent to a single space. Programs may be laid out freely.

### 2.3 Comments

Two comment forms, neither of which nests:

```
// line comment, runs to the end of the line
/* block comment, runs to the first */
```

A block comment ends at the first `*/`; it does **not** nest. Comments are
treated as whitespace.

### 2.4 Identifiers

```
identifier ::= "_"* ASCII_ALPHA (ASCII_ALPHANUMERIC | "_")*
```

An identifier may have leading underscores, must contain at least one alphabetic
character to start its "body," and continues with letters, digits, and
underscores. A lone underscore `_` is **not** an identifier; it is the wildcard
(`empty_identifier`), used in patterns and as a throwaway binding (§7.1).

A word that is a keyword (§2.5) is never an identifier.

### 2.5 Keywords

The following are reserved and may not be used as identifiers:

```
if    then   else   let    def    true   false  Unit
Int   Bool   while  do     module mut    type   match
and   or     xor    where  use    typeclass     impl
requires     for    extern deriving       fn
```

> Note: `Int`, `Bool`, and `Unit` are reserved as the primitive type names
> (§3.1). `Owned`, `Borrowed`, `BorrowedMut`, and `Never` are kind/arrow-mode
> names recognized in type and kind position (§9) but are not general keywords.

### 2.6 Operators and punctuation

Several operators accept both an ASCII spelling and a Unicode spelling; they are
exactly equivalent.

| Category   | Spelling(s)               | Meaning                                            |
| ---------- | ------------------------- | -------------------------------------------------- |
| Arithmetic | `+` `-` `*` `/` `^`       | add, subtract, multiply, divide, power             |
| Unary      | `-` `!`                   | numeric negate, boolean not                        |
| Comparison | `<` `>` `<=`/`≤` `>=`/`≥` | ordering                                           |
| Equality   | `==` `!=`/`≠`             | equal, not equal                                   |
| Boolean    | `and` `or` `xor`/`⊕`      | logical and / or / xor (on `Bool`)                 |
| Bitwise    | `&&` `or` `xor`/`⊕`       | bitwise and / or / xor (on `Int`; see §4.2)        |
| Reference  | `&` `&mut` `*`            | borrow, mutable borrow, dereference                |

Punctuation and multi-character tokens: `( ) { } < >` `, ; :` `:=` `=` `->`
`-[…]>` `<-` `=>` `::` `#` `@` `'` `|` `+` `-` (the last two also appear as
variance annotations in type-parameter position, §9).

Notes on potentially ambiguous lexemes:

- `*` is both prefix dereference and infix multiply; the two never collide
  because dereference binds to a primary and multiply only appears between
  operands (§4).
- `&` is prefix borrow; `&&` is **bitwise-AND on `Int`** (§4.2), not logical-and
  (which is `and`). `&mut` is the mutable-borrow prefix.
- `=` is assignment/binding; `==` is equality; `:=` introduces a definition
  body; `=>` separates a match arm's pattern from its result.
- `'` begins a region (lifetime) name and binds tightly to the following
  identifier with no intervening whitespace: `'r`, `'static`, `'heap`.

### 2.7 Literals

```
number  ::= ASCII_DIGIT+
boolean ::= "true" | "false"
```

- **Integer literals** are unsigned sequences of decimal digits and have type
  `Int`. There is no floating-point, hexadecimal, or underscore-separated
  literal syntax. A negative value is written with the unary `-` operator
  (`-5`), which is an expression, not a literal, except inside an integer
  _pattern_, where a leading `-` is part of the literal (§7.1).
- **Boolean literals** are `true` and `false`, of type `Bool`.
- There is no character or string literal in the surface language at this time.
- There is **no `()` literal**. A value of type `Unit` (§3.1) is produced by an
  empty block `{ }`, or by an `if` with no `else` (§4.5). The word `Unit` is only
  the *type*, not a value expression.

### 2.8 Regions (lifetimes)

A region token is a tick followed by an identifier:

```
lifetime ::= "'" identifier
```

Examples: `'a`, `'r`, `'static`, `'heap`. Regions appear in reference types
(`&'r T`), as region arguments in type applications (`Holder<'a>`), as region
parameters on generic items (§9), and in `where` outlives constraints
(`'a >= 'b`). Their meaning is given by the outlives lattice in
_[Calculus: Regions and the Outlives Lattice]_.

---

## 3. Types

This section describes the **surface syntax** of types and what each form
denotes. The formal grammar, the kinds each form carries, region-aware
subtyping, and variance are given in *[Calculus: Types]*; this section is the
programmer-facing companion to it.

A type expression is parsed by the `type_` production. The top level tries a
function type first (it is the only form containing an arrow), then falls back
to a "core" type optionally ascribed to a region:

```
type_     ::= fn_type | core_type ( "@" lifetime )?
core_type ::= "Int" | "Bool" | "Unit"
            | qualified_type      // mod::TypeName
            | tag_type            // #ok | #err
            | tuple_type          // (A, B, …)
            | type_application    // F<…>
            | borrow_type         // &T, &'r T, &'r mut T
            | identifier          // a named enum, or a type parameter in scope
```

### 3.1 Primitive types

The three built-in primitives are spelled with reserved words:

| Type   | Values                  | Notes                              |
|--------|-------------------------|------------------------------------|
| `Int`  | integer literals (§2.7) | machine integer; arithmetic in §4  |
| `Bool` | `true`, `false`         | result of comparisons, logic       |
| `Unit` | `{ }` (empty block)     | the single-valued type; the value of a statement-only block; no `()` literal (§2.7) |

All three are **`Copy`** (§8.2): using a value of a `Copy` type does not move it.
They are the only primitive types; there is no floating-point, character, or
string type in the surface language.

> There is also an internal `Top` type used as the parameter type of the
> polymorphic `println`/`print` intrinsics (§14). It is not writable in surface
> syntax and exists only as an intrinsic escape hatch until a `Display`
> typeclass replaces it.

### 3.2 Named and qualified enum types

A bare `identifier` in type position names an enum (algebraic data type)
declared with `type` (§6.2), e.g. `Ordering`, `Expr`. The same identifier
production also names a **type parameter** that is in scope (the `a` in
`def id<a>(x: a): a`); the two are disambiguated by what is in scope at that
point.

To name an enum defined in another module, qualify it with `module::Type`:

```
qualified_type ::= identifier "::" identifier
```

```sand
def f(o: math::Ordering): Int := …
```

### 3.3 Anonymous tag unions

An anonymous, structural union of nullary tags is written as `#`-prefixed names
separated by `|`:

```
tag_type ::= "#" identifier ( "|" "#" identifier )*
```

```sand
def check(x: Int): #one | #two | #other :=
    if x < 0 then #one else if x > 0 then #two else #other
```

These are OCaml-style polymorphic variants **without subtyping**: a value of
type `#one | #two | #other` is not a subtype of a wider union. They are
otherwise enums and follow the same matching rules (§7).

### 3.4 Tuple types

A parenthesised, comma-separated list of two or more types is a product type:

```
tuple_type ::= "(" type_ ( "," type_ )+ ")"
```

Arity is at least 2. A *one*-element parenthesisation `(T)` is just grouping and
denotes `T`; the empty product is `Unit`, not `()` in type position.

```sand
let pair: (Int, Bool) = (3, true);
```

### 3.5 Generic instantiation

A type constructor applied to arguments:

```
type_application ::= identifier "<" type_app_arg ( "," type_app_arg )* ">"
type_app_arg     ::= lifetime | type_
```

```sand
Option<Int>        Either<Int, Bool>        Holder<'a>        Both<'a, Int>
```

**Region arguments come before type arguments** (the *lifetimes-first*
convention), and this order is enforced. The number of arguments must match the
constructor's declared parameters (§9.1).

The raw pointer type `Ptr<T>` (§3.9) is written with this same syntax (`Ptr` is
an ordinary identifier, not a keyword).

### 3.6 Reference types

A reference borrows a value of the pointee type:

```
borrow_type ::= "&" lifetime? mut_kw? core_type
```

- `&T` / `&'r T` is a **shared** (immutable) reference, kind `Borrowed`. Shared
  references are themselves `Copy` (§8.2).
- `&mut T` / `&'r mut T` is an **exclusive** (mutable) reference, kind
  `BorrowedMut`. While a `&mut` is live, no other borrow of the same place may
  exist.

The optional `'r` names the reference's region. A borrow may not outlive its
referent; the escape check (§8.4, *[Calculus: The Escape Check]*) rejects a
reference whose region does not outlive the boundary it crosses. Region-aware
subtyping makes `&` covariant in its region and `&mut` invariant
*[Calculus: Types]*.

```sand
def max<'a>(x: &'a Int, y: &'a Int): Int := if *x > *y then *x else *y
def incr(r: &mut Int): Unit := *r = *r + 1
```

### 3.7 Region ascription

Any core type may be ascribed to a region with the `@ 'r` suffix:

```
core_type "@" lifetime          // e.g. Int @ 'r
```

This records that values of the type are associated with region `'r`; it carries
the same kind as the inner type. Regions have no runtime representation
*[Calculus: Types]*.

### 3.8 Function types

A function type is one or more `core_type` domains joined to a codomain by an
arrow:

```
fn_type    ::= core_type fn_arrow type_
fn_arrow   ::= "->" | "-[" arrow_kind "]>"
arrow_kind ::= "Owned" | "BorrowedMut" | "Borrowed"
```

- Arrows are **right-associative**: `A -> B -> C` is `A -> (B -> C)`.
- The codomain is a full `type_` (so it may itself be a function type); the
  domain is a `core_type`, so a function type *in the domain* must be
  parenthesised: `(A -> B) -> C`.
- The annotated form `-[k]>` records the **ownership mode of the function value
  itself** (how a call uses the closure's captured environment):

  | Arrow            | Mode          | Analogy   | Meaning                                  |
  |------------------|---------------|-----------|------------------------------------------|
  | `->`             | `Reusable`    | Rust `Fn` | reads its environment; callable repeatedly |
  | `-[BorrowedMut]>`| `ReusableMut` | `FnMut`   | may mutate its environment               |
  | `-[Owned]>`      | `Consuming`   | `FnOnce`  | may consume its environment; callable once |

  The bare `->` is the default. The annotated arrows are also accepted in
  surface syntax; in particular `-[Owned]>` produces a single-use (`FnOnce`)
  function, and using such a value twice is a static error. A more-permissive
  function value may stand in where a less-permissive arrow is expected
  (`Fn ⊆ FnMut ⊆ FnOnce`) *[Calculus: Types]*.

```sand
def apply(f: Int -> Int, x: Int): Int := f(x)
let g: Int -> Int = fn (n: Int) -> n + 1;
```

### 3.9 The raw pointer type `Ptr<T>`

`Ptr<T>` is the raw, address-sized, `Copy` pointer that underlies the memory
model (§13). Unlike a reference it carries **no region** and sits **outside** the
affine/borrow discipline, and it has a real runtime representation that survives
monomorphisation. Dereferencing a `Ptr` is the unsafe operation; its use is
confined to the core library (`core.sand`). Ordinary code uses references
(§3.6), not pointers.

### 3.10 Copy types

A value whose type is `Copy` is duplicated rather than moved when used (§8.2).
The `Copy` types are exactly:

- the primitives `Int`, `Bool`, `Unit`;
- any shared reference `&'r T`;
- any raw pointer `Ptr<T>`;
- a region-ascribed `T @ 'r` when `T` is `Copy`.

Enums, tuples, and `&mut` references are **not** `Copy` and are subject to move
semantics (§8).

---

## 4. Expressions

Sand is expression-oriented: every form in this section has a type (§3) and,
when run, a value (§12). The expression grammar is a precedence ladder
(§4.2) bottoming out at *primary* expressions (§4.1).

### 4.1 Primary expressions

A `primary` is the tightest-binding expression form:

```
primary ::= borrow_expr | deref_expr | tuple_expr | "(" expression ")"
          | ifstatement | whileloop | match_expr
          | function_call | external_function_call
          | external_constructor_expr | constructor_expr | tag_expr
          | number | boolean | identifier
          | "{" ( monadic_bind | statement )* expression? "}"
```

- `number`, `boolean`: literals (§2.7).
- `identifier`: a variable use, or a nullary use that resolves elsewhere.
- `( expression )`: grouping.
- the block form `{ … }` is described in §5.1 (and its do-notation variant in
  §5.5); the others follow below.

### 4.2 Operators and precedence

Binary operators are organized into the following precedence levels, **loosest
first**. Within a level, operators are **left-associative**, except `^` (power)
which is right-associative. Prefix operators (`-`, `!`, `&`, `&mut`, `*`) bind
tighter than any binary operator. A lambda (§4.9) binds looser than everything.

| Level (loose → tight) | Operators                | Assoc.        |
|-----------------------|--------------------------|---------------|
| lambda                | `fn (x: T) -> e`         | n/a           |
| logical or            | `or`                     | left          |
| logical xor           | `xor` / `⊕`              | left          |
| logical and           | `and`, `&&`              | left          |
| equality              | `==`, `!=` / `≠`         | left          |
| comparison            | `<`, `>`, `<=`/`≤`, `>=`/`≥` | left      |
| additive              | `+`, `-`                 | left          |
| multiplicative        | `*`, `/`                 | left          |
| power                 | `^`                      | **right**     |
| unary prefix          | `-e`, `!e`               | n/a           |
| primary               | (§4.1)                   | n/a           |

So `a + b * c ^ d` parses as `a + (b * (c ^ d))`, and `&x + 1` is `(&x) + 1`.

> **Note (`&&` vs `and`).** Despite sitting at the same precedence level, the two
> are *different operators*: `and` is logical conjunction on `Bool`, while `&&`
> is **bitwise AND on `Int`** (e.g. `(n && 1)` tests the low bit). `or` and
> `xor`/`⊕` are overloaded: on `Bool` they are logical, on `Int` they are
> bitwise.

### 4.3 Operator typing and semantics

| Operator(s)          | Operand types        | Result | Notes |
|----------------------|----------------------|--------|-------|
| `+` `-` `*` `/` `^`  | `Int, Int`           | `Int`  | wrapping arithmetic; `/` by zero is a runtime error (§12.2) |
| `&&` (bitwise)       | `Int, Int`           | `Int`  | bitwise AND |
| `or` `xor`/`⊕`       | `Int, Int`           | `Int`  | bitwise OR / XOR |
| `and`                | `Bool, Bool`         | `Bool` | logical AND |
| `or` `xor`/`⊕`       | `Bool, Bool`         | `Bool` | logical OR / XOR |
| `<` `>` `<=` `>=`    | `Int, Int`           | `Bool` | ordering |
| `==` `!=`            | `T, T`               | `Bool` | `T` ∈ {`Int`, `Bool`, enums, tuples}; structural |
| `-e` (unary)         | `Int`                | `Int`  | negation |
| `!e` (unary)         | `Bool` \| `Int`      | same   | logical NOT on `Bool`, bitwise NOT on `Int` |

`or` and `xor` require both operands to have the *same* type and yield that
type. Equality is structural: two enum values are equal when they have the same
variant and structurally equal payloads; comparing values of two different enum
types is a static type error (the operands must have equal type).

Both operands of every binary operator are evaluated (there is **no
short-circuit evaluation**; §12.1).

### 4.4 Borrow and dereference expressions

```
borrow_expr ::= "&" mut_kw? primary
deref_expr  ::= "*" primary
```

- `&e` produces a shared reference `&'r T` (kind `Borrowed`); `&mut e` produces
  an exclusive reference `&'r mut T` (kind `BorrowedMut`). `&mut e` requires `e`
  to name a `mut` place. The reference's region is the referent's home scope
  *[Calculus: Bidirectional Typing]*.
- `*r` reads through a reference (`&T`/`&mut T → T`). As a primary it binds
  tightly, so `*x + 1` is `(*x) + 1`. Writing through a `&mut` is the assignment
  form `*r = e` (§5.3), not an expression.

Borrowing does not consume its operand; ownership rules are in §8.

### 4.5 Conditionals and loops

```
ifstatement ::= "if" expression "then" expression ( "else" expression )?
whileloop   ::= "while" expression "do" expression
```

- **`if`**: the condition must be `Bool`. With both branches, the two branch
  types must agree (modulo region meet) and the result kind is the join of the
  branch kinds *[Calculus: Bidirectional Typing]*. An `if` **without** `else` is
  sugar for `if c then e else {}` (§11), where the `else` is the unit value;
  this requires the `then` branch to have type `Unit`.
- **`while`**: the condition must be `Bool`; the loop's value is always `Unit`.
  There is no `break`/`continue`. As a special case, `while true do …` can never
  exit, so it has kind `Never` (it diverges) rather than `Owned`.

```sand
if n ≤ 1 then n else fib(n - 1) + fib(n - 2)
while i < n do { sum = sum + i; i = i + 1; }
```

### 4.6 Calls

```
function_call          ::= identifier turbofish? "(" ( expression ( "," expression )* )? ")"
external_function_call ::= identifier "::" identifier "(" ( expression ( "," expression )* )? ")"
turbofish              ::= "::" "<" type_ ( "," type_ )* ">"
```

- `f(ē)` calls a function or a typeclass method named `f` in scope. Typeclass
  methods are called with this same ordinary call syntax and are **resolved by
  argument type** (§9.4); there is no `x.method()` dot syntax.
- `module::f(ē)` calls a function `f` from another module (§10.3).
- A `turbofish` supplies explicit type arguments, e.g. `size_of::<Int>()`. It is
  required where a type argument cannot be inferred from the value arguments
  (notably `size_of`, which has none).
- Intrinsics (`println`, `print`, `abs` via `__abs`, etc.) are called like
  ordinary functions; see §14.

Each argument is checked against the corresponding parameter type; generic
parameters are solved by unifying parameters against argument types, and region
arguments are inferred per call site *[Calculus: Region Substitution at Call Sites]*.

### 4.7 Constructors and tags

```
constructor_expr          ::= identifier "#" identifier ( "(" expression ( "," expression )* ")" )?
external_constructor_expr  ::= identifier "::" identifier "#" identifier ( "(" expression … ")" )?
tag_expr                  ::= "#" identifier ( "(" expression ( "," expression )* ")" )?
```

- `Enum#Variant` / `Enum#Variant(payload…)` constructs a value of the named
  enum. A comma-separated payload list is sugar for a single tuple payload:
  `Cons(x, rest)` ≡ `Cons((x, rest))` (§11).
- `mod::Enum#Variant(…)` constructs a value of an enum from another module.
- `#tag` / `#tag(payload)` is a bare tag whose enum is resolved from the
  expected type (checking mode). A bare `#gt` in a context expecting `Ordering`
  becomes that enum's corresponding variant.

```sand
Option#Some(21)        Ordering#Lt        #ok        #err(42)
```

### 4.8 Tuples

```
tuple_expr ::= "(" expression ( "," expression )+ ")"
```

A parenthesised list of two or more expressions builds a tuple (§3.4). `(e)` is
grouping, not a 1-tuple. There is no `()` literal; the unit value is an empty
block `{ }` (§2.7, §3.1).

### 4.9 Lambdas

```
lambda_expr  ::= "fn" lambda_param fn_arrow expression
lambda_param ::= "(" mut_kw? identifier ":" type_ ")"
```

A lambda `fn (x: T) -> e` is an anonymous function of type `T -> U` (§3.8). The
body is a full `expression`, so it extends as far right as possible until a
terminator (`,`, `)`, `}`, …); this is why a lambda binds looser than any
operator.

```sand
fmap(Option#Some(21), fn (n: Int) -> n * 2)
let g: Int -> Int = fn (n: Int) -> n + 1;
```

> A lambda takes a single parameter; multi-argument functions are expressed by
> tupling or currying. A lambda may **capture** locals from the enclosing scope
> (e.g. `fn (x: Int) -> x + bonus`); when the resulting closure outlives the
> frame it captured from, its environment is heap-allocated. The default arrow is
> `Reusable` (`->`); annotating it `-[Owned]>` makes the closure single-use
> (§3.8) *[Calculus: Terms]*.

---

## 5. Statements and blocks

### 5.1 Blocks

A block is a sequence of statements followed by an optional trailing expression:

```
block ::= "{" ( monadic_bind | statement )* expression? "}"
```

Statements execute in order (§12.1). The block's value is its trailing
expression; if the trailing expression is omitted, the block's value is the unit
value, of type `Unit`. A block introduces a new scope: bindings made inside it are not
visible outside, owned locals are dropped at block exit in reverse declaration
order (§8.5), and the block's result may not name a region introduced inside it
(the escape check, §8.4).

```sand
{
    let mut x = 10;
    x = fib(x);
    println(x);
    0          // block value
}
```

A block containing one or more top-level `<-` binds is a *do-block* (§5.5).

### 5.2 Declarations

```
declaration ::= "let" ( let_constructor | let_tuple | borrow_binding
                      | mut_kw? (identifier | "_") )
                ( ":" type_ )? "=" expression ( "else" expression )?
```

A `let` binds the value of its right-hand side. Bindings are **immutable by
default**; `let mut x = …` makes `x` reassignable (§5.3). The type annotation is
optional and inferred when omitted. `_` discards the value (still evaluating the
right-hand side, and still subject to ownership: an owned `_` binding is dropped
at scope exit).

Binding forms:

- **Simple**: `let x = e`, `let mut x = e`, `let x: T = e`.
- **Tuple destructure**: `let (a, mut b) = e` binds each element of a tuple,
  with optional per-element `mut`.
- **Borrow binding**: `let &x = e` / `let &mut x = e` desugars to a
  borrow-typed `let` (§11), binding `x` to a reference.
- **Constructor destructure** (*refutable*): `let E#V(payload) = e else fallback`
  matches a single variant and binds its payload. Because this pattern can
  fail, an `else` clause is **required**; the `else` expression runs when the
  value is not that variant and must diverge or yield the binding's type. The
  sub-pattern elements are immutable bindings (or `_`).

```sand
let x = 10;
let y: Int = 20;
let mut z = x;
let (q, r) = divmod(a, b);
let Option#Some(v) = lookup(k) else return_default();
```

### 5.3 Assignment

```
assignment ::= ( deref_expr | identifier ) "=" expression
```

- **Reseat**: `x = e` reassigns a `mut` variable. Reassigning a non-`mut`
  binding is a static error. The new value's type must match the binding's type.
- **Write-through**: `*r = e` writes through a `&mut` reference, mutating the
  referent in place.

The `=` distinguishes assignment from a bare `*r` expression statement.

```sand
x = fib(x);
*r = *r + 1;
```

### 5.4 Expression statements

```
statement ::= ( declaration | assignment | expression ) ";"
```

Any expression followed by `;` is a statement: it is evaluated for its effects
(and any owned value it produces that is not bound is dropped). Statements are
terminated by `;`; the final expression of a block (§5.1) has **no** trailing
`;`.

### 5.5 Do-notation (monadic bind)

```
monadic_bind ::= identifier ":" type_ "<-" expression ";"
```

A block that contains at least one top-level `<-` is a **do-block** over a monad
`F` (§14). Each `x: T <- e;` is a monadic bind: `e` must have type `F<T>`, and
the rest of the block is the continuation. The block desugars to nested `bind`
calls (§11):

```sand
def compute(divisor: Int): Option<Int> := {
    x: Int <- safe_div(100, 5);   // bind
    let doubled = x * 2;          // ordinary pure let, stays a let
    y: Int <- safe_div(doubled, divisor);
    Option#Some(x + y)            // monadic result, type F<_>
}
```

Ordinary `let`/statements between binds remain pure; the trailing expression is
the block's monadic result and must have type `F<_>`. The desugaring uses the
`bind` of whichever `Monad` instance `F` has, so the short-circuiting behavior
(e.g. `Option#None` ending the chain) comes from that instance, not from a
language primitive.

---

## 6. Declarations (top-level items)

A program is a sequence of top-level items:

```
program ::= ( function | extern_decl | type_alias
            | typeclass_decl | impl_decl | module | use_decl )*
```

### 6.1 Function definitions

```
function ::= "def" identifier type_params? "(" parameters? ")" ":" type_
             where_clause? ":=" expression
parameter  ::= mut_kw? (identifier | "_") ":" type_
```

A `def` introduces a function with an explicit return type and a body
expression after `:=`. Parameters are typed; a parameter may be `mut` (locally
reassignable in the body) and may be `_` (unused). Type and region parameters
(`type_params`) and `where` constraints are covered in §9.

```sand
def fib(n: Int): Int := if n ≤ 1 then n else fib(n - 1) + fib(n - 2)
def max<'a>(x: &'a Int, y: &'a Int): Int := if *x > *y then *x else *y
```

The function `main` with type `(): Int` is the program entry point; its `Int`
result is the process exit code (§12.6).

### 6.2 Type (enum) declarations

```
type_alias   ::= "type" identifier type_params? "=" enum_variant ( "|" enum_variant )*
                 deriving_clause? ";"?
enum_variant ::= identifier ( "(" type_ ( "," type_ )* ")" )?
deriving_clause ::= "deriving" identifier ( "," identifier )*
```

A `type` declares an enum (algebraic data type) as a `|`-separated list of
variants. A variant is either nullary (`Red`) or carries a payload (`Ok(Int)`);
a multi-type payload is sugar for a single tuple payload (§11). Type and region
parameters may be declared (§9.1), with optional variance and kind annotations.

```sand
type Ordering = Lt | Eq | Gt
type Option<+a> = #none | #some(a)
type Expr = Lit(Int) | Add(Expr, Expr) | Neg(Expr) deriving Heaped
```

A `deriving` clause requests compiler-derived instances. The one
generally-significant derivable is **`Heaped`** (§13): a (mutually) recursive
type *must* derive `Heaped`, since a non-heap recursive type would have infinite
size *[Calculus: Kinding Rules, K-HeapedRec]*.

### 6.3 Typeclass declarations

```
typeclass_decl   ::= "typeclass" identifier type_params requires_clause? "{" typeclass_method* "}"
typeclass_method ::= "def" identifier type_params? "(" parameters? ")" ":" type_
                     where_clause? ( ":=" expression )?
requires_clause  ::= "requires" identifier ( "," identifier )*
```

A `typeclass` declares methods over one or more type parameters. A method with a
body (`:= …`) is a **default**; an `impl` inherits it unless it overrides it. A
`requires` clause names superclasses that any implementer must also implement.

```sand
typeclass ToInt<T> {
  def to_int(x: T): Int
  def is_zero(x: T): Bool := if to_int(x) == 0 then true else false  // default
}
```

### 6.4 Implementations

```
impl_decl ::= "impl" identifier "for" type_ "{" function* "}"
```

An `impl C for T { … }` supplies a class `C`'s methods for head type `T`.
Instances are keyed globally by `(class, head(T))` and must be coherent:
overlapping or orphan instances are rejected (§9.4).

```sand
impl ToInt for Bool { def to_int(x: Bool): Int := if x then 1 else 0 }
impl Functor for Option {
    def fmap<A, B>(x: Option<A>, f: A -> B): Option<B> := match x { … }
}
```

### 6.5 External (FFI) declarations

```
extern_decl ::= "extern" "def" identifier "(" parameters? ")" ":" type_ ";"
```

An `extern def` is a bodyless declaration bound to a C symbol of the same name.
It is the FFI boundary; calls across it are outside the language's safety
guarantees. The core library uses it for `malloc`/`free` (§13).

```sand
extern def malloc(size: Int): Ptr<Unit>;
extern def free(p: Ptr<Unit>): Unit;
```

### 6.6 Modules and imports

`module` and `use` declarations are part of the module system; see §10.

---

## 7. Patterns and matching

### 7.1 Patterns

```
pattern ::= constructor_pattern | tag_pattern | tuple_pattern
          | wildcard_pattern | bool_literal_pattern | int_literal_pattern
          | binding_pattern
constructor_pattern ::= identifier "#" identifier ( "(" pattern ( "," pattern )* ")" )?
tag_pattern         ::= "#" identifier ( "(" pattern ( "," pattern )* ")" )?
tuple_pattern       ::= "(" pattern ( "," pattern )+ ")"
wildcard_pattern    ::= "_"
binding_pattern     ::= identifier
int_literal_pattern  ::= "-"? number
bool_literal_pattern ::= "true" | "false"
```

- **`_`** matches anything and binds nothing.
- **`x`** (binding) matches anything and binds it to `x` (an owned binding).
- **`(p₁, …, pₙ)`** destructures a tuple, recursively.
- **`Enum#Variant(p…)`** / **`#tag(p…)`** match a specific variant and
  destructure its payload (a comma list is a tuple sub-pattern, §11). Only
  variant patterns are *refutable*.
- **integer/boolean literal** patterns match that exact value (refutable). A
  leading `-` is part of an integer pattern literal.

The alternatives are tried in the listed (grammar) order, so a literal pattern
like `true`/`false` or an integer is recognized before it could be mistaken for
a binding.

### 7.2 The `match` expression

```
match_expr ::= "match" expression "{" match_arm+ "}"
match_arm  ::= pattern "=>" expression ","?
```

`match` evaluates the scrutinee and selects the first arm whose pattern matches.
All arms must yield a common type (modulo region meet) and the result kind is the
join of the arm kinds *[Calculus: Bidirectional Typing]*.

A `match` on an **owned** scrutinee **consumes** it, and its payload bindings are
owned. A consuming match binds *every* payload position (including wildcards,
which bind generated temporaries), so any field not moved out of an arm is
dropped at scope exit (§8.5). For a heaped scrutinee (§13), the consuming match
is lowered to `unique_take` followed by an ordinary node match.

```sand
match x {
    Option#None => fallback,
    Option#Some(v) => v,
}
```

**Borrowing match.** A `match` on a **reference** `&'r T` / `&'r mut T`
destructures *through* the borrow: it matches the pointee's variant/tuple and
binds each payload position as a borrow of that field (`a : &'r Field` for a
shared scrutinee, `a : &'r mut Field` for a mutable one) rather than moving it.
It therefore does **not** consume the scrutinee and inserts no drops, and the
field borrows share the scrutinee's region `'r` and capability.

The **shared** form is what makes `Clone` (`def clone(x: &T): T`) implementable
for non-`Copy` aggregates: the impl reads the borrowed fields and recurses:

```sand
impl Clone for Pair {
    def clone(x: &Pair): Pair := match x {       // x : &Pair
        Pair#P(a, b) => Pair#P(clone(a), clone(b))   // a, b : &Int
    }
}
```

The **mutable** form binds each field as an exclusive `&'r mut` borrow, enabling
in-place mutation of disjoint fields (the field borrows are disjoint by
construction, and the scrutinee's single exclusive borrow guarantees no other
access to the referent while they are live):

```sand
def scale(p: &mut Pair, k: Int): Unit := match p {   // p : &mut Pair
    Pair#P(a, b) => { *a = *a * k; *b = *b * k; }     // a, b : &mut Int
}
```

A `&mut` field binding is affine (a borrow, not `Copy`): use it once by value or
reborrow `&mut *a` to use it again; `*a` reads/writes are unrestricted.
Destructuring through a reference to a **heaped** (§13) type is not yet supported
(it needs a `unique_borrow` indirection).

### 7.3 Exhaustiveness

A `match` must be **exhaustive**: the set of variant indices covered by its arms
must equal the enum's full variant set (a wildcard or binding arm covers the
rest). A non-exhaustive match is a static error. Exhaustiveness for nested
patterns is checked structurally.

---

## 8. Ownership, borrows, and regions

This section states the surface rules; the formal development is in *[Calculus:
The Escape Check]* and *[Calculus: Ownership and Drop]*. The static rules here are enforced in two
layers: the type/region checker (kinds, escape) and a separate ownership
dataflow pass (affinity, exclusivity, drop insertion).

### 8.1 Affinity and moves

Every value is **affine**: it may be used at most once. Passing a value by value,
returning it, or binding it elsewhere **moves** it; the source is thereafter
*moved* and a second use is a static error. (When the moved type is `Clone`, the
diagnostic suggests `clone(&x)`.)

```sand
let xs = make_list();
consume(xs);
consume(xs);   // error: `xs` was already moved
```

### 8.2 Copy types

If a value's type is `Copy` (§3.10: `Int`, `Bool`, `Unit`, `&T`, `Ptr<T>`, and
`Copy`-inner `T @ 'r`), using it copies rather than moves it, so it remains
usable. `Copy` and `Clone` are ordinary typeclasses from `core.sand`, not
language primitives (§9.4, §14); the primitives implement `Copy` with a `clone`
that is just a dereference.

### 8.3 Borrows and exclusivity

`&e` / `&mut e` borrow without consuming (§4.4). The ownership pass enforces
**`&mut` exclusivity**: while a mutable borrow of a place is live, no other
borrow of that place may exist. Borrows are released non-lexically: a loan is
pruned once the holder's last use has passed, with a lexical block-exit restore
kept as a backstop for temporaries and untracked loans. At an `if`/`match`
merge, the surviving borrows of the branches are unioned.

```sand
def incr(r: &mut Int): Unit := *r = *r + 1
```

### 8.4 Regions and the escape check

Each reference carries a lexical **region** (`&'r T`); regions form an outlives
lattice with a top `'static` *[Calculus: Regions and the Outlives Lattice]*. A
borrow may not outlive its referent. The **escape check** fires at every scope
boundary (block result, function return) and rejects any result type that names
a region introduced at or inside that boundary; equivalently, a value of type
`T` is rejected when
`'r ∈ freeRegions(T)` for a local region `'r`. Crucially, `freeRegions` also
looks inside an applied enum's region arguments, so a borrow hidden in a payload
cannot escape unnoticed.

```sand
// rejected: returns a reference to a local
def dangling(): &Int := { let x = 5; &x }
```

Region parameters and outlives `where` constraints let a function return a
borrow tied to one of its inputs (§9.2) *[Calculus: Region Substitution at Call Sites]*.

### 8.5 Drops (RAII)

Destruction is implicit and deterministic. At scope exit, every owned, non-`Copy`
local is dropped in **reverse declaration order**. At a branch merge, a value
owned on one branch but moved on another receives a **completing drop** on the
branch where it survived, so the value is uniformly consumed at the merge with no
runtime drop flags and no leak. Drops recurse structurally and, for heaped values
(§13), free the backing allocation. The drop of a value runs the structural
destructor for its type.

### 8.6 Thread-safety markers and `spawn`

Two **marker typeclasses** (declared in the core library, with no methods, like
`Copy`) classify thread safety:

- **`Send<T>`**: a value of `T` may be moved to another thread.
- **`Sync<T>`**: `&T` may be shared with another thread (equivalently, `&T :
  Send`).

The compiler satisfies them **structurally**: primitives (`Int`, `Bool`, `Unit`)
are `Send + Sync`; a shared `&T` is `Send`/`Sync` iff `T : Sync`; a `&mut T` is
`Send` iff `T : Send` and `Sync` iff `T : Sync`; a tuple iff every element is; a
raw `Ptr<T>` is **neither** (it escapes the ownership discipline). An aggregate
type opts in with an empty `impl Send for E { }` (and `impl Sync`).

The core library exposes a minimal threading interface whose safety rests on
these bounds:

```sand
def spawn<T, R>(f: T -> R, arg: T): Thread<R> where T : Send, R : Send
def join<R>(t: Thread<R>): R
```

`spawn` moves `arg` (which must be `Send`) into the computation `f` and yields a
`Thread<R>` handle; `join` waits for it and takes the `Send` result. There is no
shared mutable state in this interface, so a program using it is data-race free
by construction. *(The current implementation runs `spawn` synchronously; real
OS threads are a forthcoming change that preserves this interface and these
bounds.)*

---

## 9. Generics, kinds, and typeclasses

### 9.1 Type and region parameters

```
type_params  ::= "<" (region_param | type_param) ( "," (region_param | type_param) )* ">"
type_param   ::= variance_ann? identifier ( ":" kind_ann )?
region_param ::= lifetime
variance_ann ::= "+" | "-"
kind_ann     ::= kind_atom ( "->" kind_atom )*
kind_atom    ::= "Owned" | "Never" | "(" kind_ann ")"
```

`def`s and `type`s may declare type and region parameters. **Region parameters
come first** (lifetimes-first), in both declaration and use. Each type parameter
may carry:

- a **variance** annotation (`+` covariant, `-` contravariant; default depends
  on usage position), validated at the declaration site; since there is no
  subtyping between concrete types, variance is a soundness check, not a
  coercion *[Calculus: Types]*;
- a **kind** annotation (default `Owned`). A base kind is `Owned` or `Never`; an
  arrow kind like `Owned -> Owned` marks a **higher-kinded** type-constructor
  parameter (the `F` in `Functor<F : Owned -> Owned>`). Arrow kinds are
  right-associative and may be parenthesised.

```sand
def id<a>(x: a): a := x
type Holder<'a, +a : Owned> = H(&'a a)
typeclass Functor<F : Owned -> Owned> { def fmap<A, B>(x: F<A>, f: A -> B): F<B> }
```

### 9.2 `where` clauses

```
where_clause     ::= "where" where_constraint ( "," where_constraint )*
where_constraint ::= (lifetime ">=" lifetime) | (identifier ":" identifier)
```

A `where` clause states either an **outlives** constraint between regions
(`'a >= 'b`) or a **typeclass** constraint on a type parameter (`T : C`). Region
constraints are discharged by the region solver at each call site; class
constraints are discharged against the instance table.

```sand
def longest<'a, 'b>(x: &'a Int, y: &'b Int): &'a Int where 'a >= 'b := …
def use_it<T>(x: T): Int where T : ToInt := to_int(x)
```

### 9.3 Monomorphisation

Generics are a purely compile-time mechanism: every generic function and enum is
**monomorphised**: specialised for each concrete instantiation it is used with,
reachability-driven from non-generic roots. No type parameter, region argument,
or higher-kinded application survives into the lower IRs; code generation only
ever sees fully concrete types. Region arguments are erased (regions have no
runtime representation).

### 9.4 Typeclass resolution

Typeclass methods are called as ordinary functions (§4.6) and **resolved by
argument type**, then dispatched by monomorphisation: there is **no `x.method()`
dot syntax and no runtime dictionary**. Resolution:

- Instances are keyed globally by `(class, head(T))` and must form one coherent
  set, so **overlapping or orphan instances are rejected** and resolution is
  unambiguous program-wide.
- A class may provide **default method bodies**, inherited by an `impl` unless
  overridden (§6.3).
- A method like `pure` that has no argument of the class's type parameter is
  resolved from the **expected type** at the call site (checking mode):
  `let lifted: Option<Int> = pure(42)` selects `Option`'s instance.
- `where T : C` constraints (§9.2) are discharged at each call site against the
  instance table.

`Clone`/`Copy` and the `Functor`/`Applicative`/`Monad` hierarchy are library
typeclasses defined this way (§14), not built-ins.

---

## 10. Modules and projects

### 10.1 Modules

```
module ::= "module" identifier ";"?
```

A module is a namespace of top-level items. By default a file forms one module
whose name is the file name; a `module x;` declaration names the module
explicitly, and a file may declare several modules.

### 10.2 Imports

```
use_decl ::= "use" use_path ";"
use_path ::= identifier ( "::" identifier )* ( "::" "*" )?
```

A `use` brings a name (or, with a trailing `::*` glob, all names) from another
module into unqualified scope.

```sand
use math::gcd;
use collections::*;
```

### 10.3 Qualified references

Without a `use`, items in other modules are referred to with `module::`
qualification: `module::function(…)` for calls (§4.6), `mod::Type` for types
(§3.2), and `mod::Enum#Variant(…)` for constructors (§4.7).

### 10.4 Projects (`sand.toml`)

A multi-file project is described by a `sand.toml`:

```toml
[project]
name = "my-project"
sources = [
    "src/",        # a directory: all .sand files under it
    "main.sand",   # or an individual file
]
```

Both fields are optional. `sources` lists files and/or directories; directories
are searched **recursively** for `.sand` files. The compiler merges all listed
sources into one program before type checking. A single file can also be
compiled directly without a project.

### 10.5 The core library

Every compilation implicitly includes the core library
([`core.sand`](lang/src/core.sand)), which supplies the `Clone`/`Copy`,
`Functor`/`Applicative`/`Monad` typeclasses, the `Unique` heap strategy, and
numeric helpers (§14). Its names are available as if part of the language.

---

## 11. Desugarings

The following surface forms are defined by translation to more primitive forms.
These are normative: a conforming implementation behaves as if the rewrite were
applied.

| Surface form | Desugars to |
|--------------|-------------|
| `if c then e` (no `else`) | `if c then e else {}` (empty-block unit value; requires `e : Unit`) |
| `let &x = e` / `let &mut x = e` | a borrow-typed `let` binding `x` to `&e` / `&mut e` |
| Comma payload `Cons(a, b)` | single tuple payload `Cons((a, b))` (in expressions, patterns, and `enum_variant` declarations) |
| Multi-type variant `V(A, B)` | single tuple-payload variant `V((A, B))` |
| do-block bind `x: T <- e; rest` | `bind(e, fn (x: T) -> «rest»)`, nested left-to-right |
| `Enum#Variant` payload sub-pattern list | single tuple sub-pattern |
| Heaped enum construction / match / drop | `unique_alloc` / `unique_take` / `unique_release` over node enums (§13) |

The do-block rewrite leaves ordinary `let`s and statements between binds
unchanged (they stay pure); only `<-` lines become `bind` calls, and the
trailing expression is the monadic result (§5.5).

---

## 12. Dynamic semantics

This section describes the runtime behavior of a well-typed program. (Sand has a
reference interpreter over the typed IR and a native LLVM backend; the observable
behavior described here is what both realize.)

### 12.1 Evaluation order

Evaluation is **eager** and **left-to-right**:

- In a block, statements run top to bottom; a `let` evaluates its right-hand side
  and binds the result; an expression statement is evaluated for effect.
- A call evaluates its callee's arguments left to right before entering the body.
- Both operands of a binary operator are evaluated before the operator is
  applied: there is **no short-circuit** evaluation of `and`/`or`/`xor`.
- A tuple/constructor evaluates its elements left to right.

### 12.2 Operator runtime behavior

- Integer arithmetic (`+ - * ^` and unary `-`) is **two's-complement wrapping**:
  overflow wraps around rather than trapping.
- Integer **division by zero is a runtime error** that aborts the program.
- `^` raises to a non-negative integer power.
- Bitwise `&&`, `or`, `xor`, and unary `!` on `Int` operate bit-for-bit;
  logical `and`/`or`/`xor`/`!` on `Bool` operate as expected.
- Structural equality compares values deeply (variant + payloads for enums,
  element-wise for tuples).

### 12.3 Control flow

- `if c then a else b` evaluates `c`, then exactly one branch.
- `while c do body` re-evaluates `c` before each iteration and runs `body` while
  `c` is `true`; its value has type `Unit`. There is no `break`/`continue`, so a loop
  whose condition is statically `true` diverges (§12.5).
- `match` evaluates the scrutinee once and runs the first matching arm,
  consuming the scrutinee (§7.2).

### 12.4 Drops

At each scope exit, owned non-`Copy` locals are destroyed in reverse declaration
order (§8.5). A drop runs the value's structural destructor and, for heaped
values, frees its backing allocation (§13). Because drop placement is
determined statically (including completing drops at branch merges), destruction
is deterministic: there is no garbage collector and every allocation has exactly
one matching free.

### 12.5 Divergence

An expression that never produces a value (an infinite loop `while true do …`,
or a call to `exit`) has kind `Never` and is usable where a value of any type is
expected (`Never` is the bottom kind) *[Calculus: Kinds]*.

### 12.6 Program entry

Execution begins at `def main(): Int`. The returned `Int` is the process exit
code. The `exit(code)` core function (intrinsic `__exit`) terminates the process
immediately with the given code.

---

## 13. Memory model

Most types are stack values: a non-recursive enum is a stack tagged-union, a
tuple is a flat product. Heap allocation is **opt-in** and the compiler itself
stays allocation-agnostic: it knows only the `Ptr<T>` primitive, the `extern`
FFI boundary, and the `Heaped` lowering protocol. The actual allocation policy
lives in the core library (`core.sand`), over a small set of pointer intrinsics.
The full treatment is *[Calculus: Memory Model and Typeclasses]*.

### 13.1 `deriving Heaped`

A type that derives `Heaped` is heap-allocated. A **(mutually) recursive type
must derive `Heaped`**; otherwise it would have infinite size and is ill-kinded
*[Calculus: Kinding Rules]*.

```sand
type Expr = Lit(Int) | Add(Expr, Expr) | Neg(Expr) deriving Heaped
```

### 13.2 Heap lowering

Before ownership and monomorphisation run, the compiler rewrites every heaped
enum `E<T…>` away (§11):

- it synthesises a **non-recursive node enum** `E$Node<T…>` with the same
  variants, each heaped field replaced by a `Unique<…$Node>` handle (so recursion
  now goes through a pointer and the type is finite);
- construction `E#C(p)` becomes `unique_alloc(E$Node#C(p))`;
- a consuming `match`/`let`-pattern becomes `unique_take` plus an ordinary node
  match.

After this pass, no heaped enum survives; later passes see only ordinary enums,
`Unique` handles, and `Ptr` operations.

### 13.3 `Ptr<T>`, intrinsics, and the `Unique` strategy

The raw substrate is `Ptr<T>` (§3.9) and these intrinsics:

```
__ptr_read(p: Ptr<T>): T            // load through a raw pointer
__ptr_write(p: Ptr<T>, v: T): Unit  // store through a raw pointer
__ptr_cast(p: Ptr<A>): Ptr<B>       // reinterpret a raw pointer (runtime no-op)
__drop_in_place(x): Unit            // structural destructor glue
size_of::<T>(): Int                 // byte size of T
```

The `Unique` strategy (the `Box` equivalent) is defined in `core.sand` as
`type Unique<T> = U(Ptr<T>)`, a non-`Copy` handle owning one heap node:

- `unique_alloc` moves a value onto the heap (`malloc` + `__ptr_write`);
- `unique_release` is the **deep** drop (structurally drop the node, then
  `free`), used when an owned heaped value leaves scope unconsumed;
- `unique_take` is the **shallow** release backing a consuming match (read the
  node onto the stack, free only the backing cell, hand the fields to the arm
  bindings).

Swapping the allocator means swapping these library functions; the compiler does
not change. Reference-counted handles and in-place node reuse are planned
extensions of this same protocol.

---

## 14. Core library and intrinsics

### 14.1 Intrinsics

Intrinsics are compiler-known functions that map to machine operations or OS
interactions rather than ordinary Sand code:

| Intrinsic            | Surface form                | Signature                  |
|----------------------|-----------------------------|----------------------------|
| `println` / `print`  | `println(x)` / `print(x)`   | `(Top) → Unit`; accepts any type; `printf` aliases `print` |
| `__abs`              | via `abs`                   | `(Int) → Int`              |
| `__min` / `__max`    | via `min` / `max`           | `(Int, Int) → Int`         |
| `__read_int`         | via `read_int`              | `() → Int`                 |
| `__exit`             | via `exit`                  | `(Int) → Unit`             |
| `size_of::<T>()`     | turbofish                   | `() → Int` (byte size of `T`) |
| `__ptr_read/write/cast` | core lib                 | raw pointer ops (§13.3)    |
| `__drop_in_place`    | compiler-inserted           | structural destructor      |

The names `print`, `println`, `printf`, `scanf`, `read`, and `readline` are
**reserved** and may not be used as user function names.

### 14.2 Core library surface (`core.sand`)

Included in every compilation:

- **Numeric helpers**: `abs`, `min`, `max`, `clamp`, `is_odd`, `is_even`, `pow`,
  `read_int`, `exit`.
- **`Clone` / `Copy`**: `typeclass Clone<T> { def clone(x: &T): T }` and the
  marker `typeclass Copy<T> requires Clone {}`. `Int`, `Bool`, `Unit` implement
  both (their `clone` is a deref). A non-`Copy` value must be duplicated with an
  explicit `clone(&x)`.
- **`Functor` / `Applicative` / `Monad`**: the standard hierarchy over a
  higher-kinded `F : Owned -> Owned`, providing `fmap`, `pure`/`ap`, and `bind`.
  `bind` is what do-notation (§5.5) desugars onto.
- **`Unique` heap strategy**: `Unique<T>`, `unique_alloc`, `unique_release`,
  `unique_take`, and the `extern` `malloc`/`free` it is built on (§13.3).

---

## Appendix A. Grammar

The authoritative grammar is the PEG in [`grammar.pest`](grammar.pest), from
which the parser is generated. The EBNF fragments throughout this document are
derived from it; where the prose and the grammar disagree, the grammar is
canonical for *what parses* and this document is canonical for *what it means*.

The operator precedence ladder (§4.2), loosest to tightest, corresponds to the
grammar's chained productions:

```
expression → lambda_expr | logic_or → logic_xor → logic_and
           → equality → comparison → add_sub → mul_div → power
           → unary → primary
```

## Appendix B. Example programs

The [`examples/`](examples) directory contains runnable `.sand` programs that
exercise the features in this specification, including:

- `fib.sand`, `fact.sand`, `ackermann.sand`, `gcd.sand`: recursion and
  arithmetic;
- `monad.sand`, `do_notation.sand`: typeclasses and do-notation (§5.5, §9.4);
- `borrowing.sand`, `references.sand`, `mut_references.sand`, `regions.sand`:
  borrows and regions (§8);
- `tree.sand`, `expr.sand`, `lists.sand`: heaped recursive types (§13);
- `exhaustiveness.sand`, `nested_exhaustiveness.sand`: match exhaustiveness
  (§7.3);
- `hkt.sand`, `typeclass_dispatch.sand`, `variance.sand`: generics and kinds
  (§9).
