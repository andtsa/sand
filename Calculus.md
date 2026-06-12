# Core Calculus

A formal description of the kind, type, region, and term systems for the sand language.

---

## Notation Conventions

```
Metavariables:
  k          kind
  'r, 's     region variables
  a, b       type variables
  T, U       types
  F          type constructor name
  x, y       term variables
  e          expression
  s          statement
  v          value
  Γ          typing context
  v̂          variance annotation

  ≥          outlives: 'r ≥ 's means region 'r outlives 's
  ε          empty (context, sequence, etc.)
  T̄, ē       sequences (T₁, T₂, ..., Tₙ)
```

---

## 1. Kinds

### 1.1 Grammar

```
Region        'r  ::=  'r              -- region variable
                    |  'static         -- permanent region (outlives everything)

Region context
               R  ::=  ε               -- empty
                    |  R, 'r           -- introduce region variable
                    |  R, 'r ≥ 's      -- outlives constraint ('r outlives 's)

Kind           k  ::=  Owned
                    |  Borrowed 'r
                    |  BorrowedMut 'r
                    |  InteriorMut
                    |  Never
```

### 1.2 Subkinding

The relation `k₁ <: k₂` reads "`k₁` is usable where `k₂` is expected."
`Owned` is at the top — it carries the most capability. The three borrow
modes are mutually incomparable. `Never` is the bottom — a subkind of
everything, corresponding to the uninhabited type.

```
──────────  (SK-Refl)
k <: k


────────────────────────  (SK-OwnedBorrowed)
Owned <: Borrowed 'r


──────────────────────────  (SK-OwnedBorrowedMut)
Owned <: BorrowedMut 'r


────────────────────────  (SK-OwnedInteriorMut)
Owned <: InteriorMut


────────────  (SK-Never)
Never <: k
```

There is intentionally no rule relating `Borrowed`, `BorrowedMut`, and
`InteriorMut` to each other — they are incomparable branches of the lattice.

### 1.3 Kind Lattice

```
                   Owned                        ← top (maximum capability)
                 /   |   \
               /     |     \
             /       |       \
           /         |         \
   Borrowed     BorrowedMut     InteriorMut    ← mutually incomparable
           \         |         /
             \       |       /
               \     |     /
                 \   |   /
                   Never                        ← bottom (uninhabited)
```

### 1.4 Kind Join

The least upper bound of two kinds, written `k₁ ∨ k₂`. Used during
inference to resolve kind variables when two branches must agree.

```
k ∨ k = k                               (join-refl)
Borrowed 'r ∨ BorrowedMut 'r = Owned   (join-borrow-modes)
Borrowed 'r ∨ InteriorMut   = Owned
BorrowedMut 'r ∨ InteriorMut = Owned
Never ∨ k = k                           (join-never)
k ∨ Never = k
```

The join of any two distinct borrow modes is `Owned`, reflecting that
the lattice has no intermediate kind between the borrow modes and the top.

---

## 2. Types

### 2.1 Variance

```
Variance   v̂  ::=  +     -- covariant
                |  -     -- contravariant
                |  ∅     -- invariant
```

Default variance is determined by the kind of the type parameter and
the positions in which it appears in the type constructor body:

```
Owned,       producer position only  →  +  (covariant)
Owned,       consumer position only  →  -  (contravariant)
Owned,       both positions          →  ∅  (invariant)
Borrowed 'r, any position            →  +  (read-only, always covariant)
BorrowedMut, any position            →  ∅  (read-write, always invariant)
InteriorMut, any position            →  ∅  (hidden mutation, always invariant)
```

Declaration-site annotations (`+`, `-`, `∅`) override these defaults.
The kind checker verifies that the declared variance is sound for the
given kind — for example, declaring `+a : BorrowedMut` is a kind error.

### 2.2 Type Constructor Parameters

```
Parameter   p  ::=  v̂ a : k
```

Each parameter carries a variance annotation and a kind. The variance
annotation may be omitted to accept the default. Examples:

```
-- Option holds an owned value, covariant (default for Owned producer)
type Option<+a : Owned> = #none | #some(a)

-- Either holds two owned values, both covariant
type Either<+a : Owned, +b : Owned> = #left(a) | #right(b)

-- Ref is a built-in: borrows a value from region 'r, covariant
type Ref<+a : Owned, 'r>

-- Cell is a built-in: wraps a value with interior mutability, invariant
-- The kind annotation on the type constructor itself is InteriorMut
type Cell<∅ a : Owned> : InteriorMut
```

### 2.3 Type Grammar

```
Type   T  ::=  a                        -- type variable (kind given by context)
            |  Int                      -- primitive integer        (Owned)
            |  Bool                     -- primitive boolean        (Owned)
            |  Unit                     -- unit type                (Owned)
            |  T @ 'r                   -- T in region 'r           (Owned)
            |  &'r T                    -- shared borrow            (Borrowed 'r)
            |                           --   sugar for Ref<T> @ 'r
            |  &'r mut T                -- mutable borrow           (BorrowedMut 'r)
            |  T₁ →[k] T₂              -- function type            (Owned)
            |  F<T̄>                     -- type constructor application
            |  (T₁, ..., Tₙ)           -- tuple  (n ≥ 2)           (Owned)
            |  #tag₁ | ... | #tagₙ     -- ad-hoc tag union          (Owned)
            |  mod::F                   -- qualified type constructor
            |  ∀(a : k). T             -- kind-polymorphic type
            |  ∀'r. T                  -- region-polymorphic type
```

The function arrow `→[k]` carries the *ownership mode of the function*:

```
T₁ →[Owned] T₂        -- consuming function: argument is moved in, single-use
T₁ →[Borrowed 'r] T₂  -- borrowing function: argument is borrowed, reusable
```

This is how the owned and borrowed variants of `fmap` differ at the
type level — the arrow kind, not just the argument kind:

```
-- owned map: consumes the container
fmap     : (a →[Owned] b)       →[Owned]       F<a> →[Owned] F<b>

-- borrowing map: borrows the container, produces a new one
fmap_ref : (a →[Borrowed 'r] b) →[Borrowed 'r] &'r F<a> →[Owned] F<b>
```

Region ascription `T @ 'r` is a type-level construct only. There is no
term-level `@` operator — regions are tracked through the type system,
not annotated on expressions directly.

---

## 3. Terms

### 3.1 Values

```
Value   v  ::=  x                           -- variable
             |  ()                          -- unit literal
             |  n                           -- integer literal
             |  true  |  false              -- boolean literals
             |  (v₁, ..., vₙ)              -- tuple  (n ≥ 2)
             |  F#Tag                       -- nullary enum constructor
             |  F#Tag(v)                    -- enum constructor with payload
             |  #Tag                        -- bare tag (check mode only)
             |  #Tag(v)                     -- bare tag with payload
             |  fn (x : T) -> e            -- consuming lambda   (Owned arg)
             |  fn &(x : T) -> e           -- borrowing lambda   (Borrowed arg)
             |  fn &mut (x : T) -> e       -- mut-borrowing lambda
```

Lambdas are not yet in the grammar (marked TODO). The calculus introduces
them here; the grammar will need extending before they can be used.

### 3.2 Expressions

```
Expr   e  ::=

  v                                -- value

  { s̄; e }                        -- block: sequence of statements closed
                                   --   by a final expression. Each block
                                   --   introduces an implicit fresh region.
                                   --   Corresponds to `{ statement* expression? }`

  e₁(e₂)                          -- function application

  let x : T = e₁; e₂              -- consuming let: x owns the result of e₁
  let &x : T = e₁; e₂             -- borrow let: x borrows from e₁
  let &mut x : T = e₁; e₂         -- mutable borrow let

  let (x₁, ..., xₙ) = e₁; e₂     -- tuple destructure (consuming)
  let F#Tag(x) = e₁ else e₂; e₃   -- constructor destructure with fallback

  x = e                            -- assignment (x must be BorrowedMut)

  box(e)                           -- heap allocation intrinsic;
                                   --   moves e into the heap ('static) region

  if e₁ then e₂ else e₃           -- conditional
  while e₁ do e₂                  -- loop
  match e { arm* }                 -- pattern match (scrutinee consumed)

  e : T                            -- type ascription; enters checking mode
```

`move`, `copy`, and `drop` are not term-level syntax. Move semantics are
implicit in assignment and application. `drop` is a library function
`fn drop<T>(x : T) -> Unit`. `Clone` and `Copy` are typeclasses (see §7).

### 3.3 Statements

Statements appear only inside blocks. They are not expressions and do
not produce values on their own. The block's value comes from its final
expression.

```
Statement   s  ::=
  let x : T = e                   -- consuming declaration
  let &x : T = e                  -- borrow declaration
  let &mut x : T = e              -- mutable borrow declaration
  let (x̄) = e                     -- tuple destructure
  let F#Tag(x) = e else e         -- constructor destructure with fallback
  x = e                           -- assignment (x must be BorrowedMut)
  e                               -- expression statement (result dropped)
```

### 3.4 Patterns and Match Arms

```
Arm       arm  ::=  pat => e

Pattern   pat  ::=
  _                               -- wildcard (discard, no binding)
  x                               -- binding  (consuming)
  (pat₁, ..., patₙ)              -- tuple pattern
  #Tag                            -- nullary tag
  #Tag(pat)                       -- tag with payload pattern
  F#Tag(pat)                      -- qualified constructor pattern
  n                               -- integer literal (refutable)
  true  |  false                  -- boolean literal (refutable)
```

Match always consumes the scrutinee. Each arm receives owned bindings
for the variables it introduces.

---

## 4. Typing Contexts

```
Context   Γ  ::=
  ε                               -- empty context
  Γ, x :ₖ T                      -- term variable x of type T at kind k
  Γ, a : k                        -- type variable a of kind k
  Γ, 'r                           -- region variable
  Γ, 'r ≥ 's                      -- outlives constraint
```

The subscript on `:ₖ` is the ownership mode of the binding:

- `x :_Owned T`          — x is consumed on use; removed from Γ afterward
- `x :_(Borrowed 'r) T`  — x may be used multiple times within 'r; stays in Γ
- `x :_(BorrowedMut 'r) T` — same, but exclusive write access within 'r
- `x :_InteriorMut T`    — x may be used multiple times; mutation is internal

---

## 5. Kinding Rules

Kinding judgments assign a kind to a type expression: `Γ ⊢ T : k`.
These run as a pre-pass over type expressions before type inference.

```
─────────────────  (K-Int)        ─────────────────  (K-Bool)
Γ ⊢ Int : Owned                   Γ ⊢ Bool : Owned


──────────────────  (K-Unit)
Γ ⊢ Unit : Owned


(a : k) ∈ Γ
────────────────  (K-Var)
Γ ⊢ a : k


Γ ⊢ T : Owned    'r ∈ Γ
─────────────────────────  (K-Region)
Γ ⊢ T @ 'r : Owned


Γ ⊢ T : Owned    'r ∈ Γ
─────────────────────────  (K-Borrow)
Γ ⊢ &'r T : Borrowed 'r


Γ ⊢ T : Owned    'r ∈ Γ
──────────────────────────  (K-BorrowMut)
Γ ⊢ &'r mut T : BorrowedMut 'r


Γ ⊢ T₁ : k₁    Γ ⊢ T₂ : k₂
──────────────────────────────  (K-Arrow)
Γ ⊢ T₁ →[k] T₂ : Owned
-- function types are always Owned; k describes how the argument is used


F declared with parameter kinds k̄, result kind kF    Γ ⊢ T̄ : k̄
──────────────────────────────────────────────────────────────────  (K-App)
Γ ⊢ F<T̄> : kF


Γ ⊢ T₁ : Owned  ...  Γ ⊢ Tₙ : Owned
───────────────────────────────────────  (K-Tuple)
Γ ⊢ (T₁, ..., Tₙ) : Owned


Γ, a : k ⊢ T : k'
────────────────────────  (K-ForallKind)
Γ ⊢ ∀(a : k). T : k'


Γ, 'r ⊢ T : k
─────────────────────  (K-ForallRegion)
Γ ⊢ ∀'r. T : k
```

---

## 6. Bidirectional Typing Rules

Two judgments, extending Pierce & Turner with kinds:

```
Γ ⊢ e ⇒ T : k      synthesis: infer both the type and kind of e
Γ ⊢ e ⇐ T : k      checking:  verify e has type T at kind k
```

### 6.1 Subsumption

The bridge between synthesis and checking. Applies subkinding and
subtyping simultaneously, enabling implicit coercions (e.g. passing
an `Owned` value where `Borrowed` is expected).

```
Γ ⊢ e ⇒ T : k    k <: k'    T <: T'
──────────────────────────────────────  (Sub)
Γ ⊢ e ⇐ T' : k'
```

### 6.2 Variables

```
(x :_Owned T) ∈ Γ    Γ' = Γ \ {x}
────────────────────────────────────  (Var-Owned)
Γ' ⊢ x ⇒ T : Owned
-- x is consumed: it is removed from the context on use


(x :_k T) ∈ Γ    k ≠ Owned
────────────────────────────  (Var-Borrow)
Γ ⊢ x ⇒ T : k
-- borrowed/interior variables remain in context; can be used multiple times
```

### 6.3 Blocks

Each block introduces a fresh implicit region `'r`. The final expression
is the block's result. The result type must not mention `'r` — this is
the formal statement of lifetime safety: values cannot outlive their region.

```
Γ, 'r ⊢ s̄ ⊣ Γ'    Γ' ⊢ e ⇒ T : k    'r ∉ freeRegions(T)
────────────────────────────────────────────────────────────  (Block)
Γ ⊢ { s̄; e } ⇒ T : k
```

### 6.4 Let Bindings

```
Γ ⊢ e₁ ⇒ T : Owned    Γ, x :_Owned T ⊢ e₂ ⇒ U : k
──────────────────────────────────────────────────────  (Let-Owned)
Γ ⊢ (let x : T = e₁; e₂) ⇒ U : k


Γ ⊢ e₁ ⇒ T : Owned
'r fresh    Γ, x :_(Borrowed 'r) T ⊢ e₂ ⇒ U : k    'r ∉ freeRegions(U)
────────────────────────────────────────────────────────────────────────  (Let-Borrow)
Γ ⊢ (let &x : T = e₁; e₂) ⇒ U : k


Γ ⊢ e₁ ⇒ T : Owned
'r fresh    Γ, x :_(BorrowedMut 'r) T ⊢ e₂ ⇒ U : k    'r ∉ freeRegions(U)
──────────────────────────────────────────────────────────────────────────────  (Let-BorrowMut)
Γ ⊢ (let &mut x : T = e₁; e₂) ⇒ U : k
```

The freshness condition `'r ∉ freeRegions(U)` ensures borrowed bindings
cannot escape the scope in which they are introduced.

### 6.5 Functions and Application

```
Γ, x :_Owned T ⊢ e ⇒ U : k
──────────────────────────────────────────────  (Lam-Owned)
Γ ⊢ fn (x : T) -> e  ⇒  T →[Owned] U : Owned


'r fresh
Γ, x :_(Borrowed 'r) T ⊢ e ⇒ U : k    'r ∉ freeRegions(U)
──────────────────────────────────────────────────────────────  (Lam-Borrow)
Γ ⊢ fn &(x : T) -> e  ⇒  T →[Borrowed 'r] U : Owned


Γ ⊢ e₁ ⇒ T →[Owned] U : Owned    Γ ⊢ e₂ ⇐ T : Owned
────────────────────────────────────────────────────────  (App-Owned)
Γ ⊢ e₁(e₂) ⇒ U : Owned


Γ ⊢ e₁ ⇒ T →[Borrowed 'r] U : Owned    Γ ⊢ e₂ ⇐ T : Borrowed 'r
────────────────────────────────────────────────────────────────────  (App-Borrow)
Γ ⊢ e₁(e₂) ⇒ U : Owned
```

### 6.6 Heap Allocation

`box` is the single allocation intrinsic. It moves a value into the
heap region, which is `'static` — it outlives every other region.

```
Γ ⊢ e ⇒ T : Owned
──────────────────────────────────────  (Box)
Γ ⊢ box(e) ⇒ Box<T> @ 'static : Owned
```

### 6.7 Ascription

Type ascription is the explicit entry point into checking mode.
It also serves as the place where kind annotations are verified.

```
Γ ⊢ T : k    Γ ⊢ e ⇐ T : k
──────────────────────────────  (Ascribe)
Γ ⊢ (e : T) ⇒ T : k
```

### 6.8 Conditionals

Both branches must agree on type and kind. The condition is `Bool : Owned`.

```
Γ ⊢ e₁ ⇐ Bool : Owned
Γ ⊢ e₂ ⇒ T : k
Γ ⊢ e₃ ⇐ T : k
────────────────────────────────────────  (If)
Γ ⊢ if e₁ then e₂ else e₃ ⇒ T : k
```

### 6.9 Match

The scrutinee is always consumed. Each arm receives owned bindings for
the variables it introduces. All arms must agree on result type and kind.

```
Γ ⊢ e ⇒ T : Owned
∀ armᵢ = (patᵢ => eᵢ):   Γ, bindings(patᵢ, T) ⊢ eᵢ ⇒ U : k
all arms agree on U and k
──────────────────────────────────────────────────────────────  (Match)
Γ ⊢ match e { arm* } ⇒ U : k
```

---

## 7. Standard Typeclasses

### 7.1 Functor

Two variants, differing in the ownership mode of the container and
the mapping function.

```
-- Consumes the container. The natural case for owned containers.
typeclass OwnedFunctor<F : Owned → Owned> {
  fmap : ∀(a b : Owned).
         (a →[Owned] b) →[Owned] F<a> →[Owned] F<b>
}

-- Borrows the container without consuming it.
typeclass BorrowedFunctor<F : Owned → Owned> {
  fmap_ref : ∀(a b : Owned)('r).
             (a →[Borrowed 'r] b) →[Borrowed 'r] &'r F<a> →[Owned] F<b>
}
```

### 7.2 Applicative

Sits between Functor and Monad. Adds `pure` (wrapping) and `ap`
(applying a wrapped function to a wrapped value). The key distinction
from Monad: `ap` combines two *independent* effects, whereas `bind`
sequences *dependent* effects.

```
typeclass Applicative<F : Owned → Owned>
  requires OwnedFunctor<F>
{
  pure : ∀(a : Owned).
         a →[Owned] F<a>
         -- wraps a value; allocates only if F itself requires it

  ap : ∀(a b : Owned).
       F<(a →[Owned] b)> →[Owned] F<a> →[Owned] F<b>
       -- both F<a → b> and F<a> are independent; neither depends on
       -- the result of the other before being computed
}
```

### 7.3 Monad

Extends `Applicative` with `bind`, which sequences dependent effects:
the second computation `a →[Owned] M<b>` can observe the result of
the first `M<a>`.

```
typeclass Monad<M : Owned → Owned>
  requires Applicative<M>
{
  bind : ∀(a b : Owned).
         M<a> →[Owned] (a →[Owned] M<b>) →[Owned] M<b>
}
```

`bind` consumes `M<a>`, extracts the inner `a` (still owned), and
passes it to the continuation. Ownership transfers linearly through
the chain with no hidden allocation.

`Monad` is only well-kinded at `Owned → Owned` type constructors.
Attempting to instantiate it at a `Borrowed` constructor is a kind
error — the lattice enforces this structurally rather than by convention.

`fmap` and `ap` can both be derived from `pure` and `bind`, so a
`Monad` instance needs only provide those two. The `Functor` and
`Applicative` methods are then available for free.

### 7.4 Clone and Copy

`Clone` and `Copy` are typeclasses, not language primitives.

```
typeclass Clone<T : Owned> {
  clone : &'r T →[Borrowed 'r] T
  -- borrows, produces a new owned copy; implementation decides the cost
}

typeclass Copy<T : Owned> requires Clone<T> {}
-- Copy carries no new methods. It is a marker: "cloning this type is
-- cheap enough for the compiler to do implicitly."
-- For all other types, clone must be called explicitly: x.clone()
```

Primitive types (`Int`, `Bool`, `Unit`) implement `Copy` automatically.
Heap-allocated types (`Box<T>`) do not — they require explicit `.clone()`.

---

## 8. Grammar Changes Required

The following additions and changes to the existing `.pest` grammar
are needed to support the type system described above.

### 8.1 Updated Keyword List

```
KEYWORD = {
  "if" | "then" | "else" | "let" | "def" | "true" | "false"
  | "Unit" | "Int" | "Bool" | "while" | "do" | "module"
  | "mut" | "type" | "match"
  | "fn" | "box"          -- new
}
```

`move`, `copy`, and `drop` are not keywords — they are library
identifiers. `clone` is a method name.

### 8.2 Lifetime / Region Syntax

```
lifetime    = @{ "'" ~ identifier }          -- e.g. 'r, 'heap, 'static
```

### 8.3 Type Extensions

```
-- borrow types
borrow_type = { "&" ~ lifetime? ~ "mut"? ~ type_ }
              -- &T, &'r T, &mut T, &'r mut T

-- region ascription (type-level only; no term-level @ operator)
region_type = { type_ ~ "@" ~ lifetime }
              -- T @ 'r
```

Extend `type_` to include `borrow_type` and `region_type`.

Free `&` from its current use as bitwise AND. Suggested replacement:
`bitand = { "band" }` (keyword operator), leaving `&` solely for borrows.
`|` remains as tag union separator and match arm separator; bitwise OR
can similarly become `bor` if needed.

### 8.4 Type Parameters (Generics)

```
kind_ann     = { "Owned" | "Borrowed" | "BorrowedMut" | "InteriorMut" | "Never" }
variance_ann = { "+" | "-" }
type_param   = { variance_ann? ~ identifier ~ (":" ~ kind_ann)? }
              -- e.g.  a,  +a,  +a : Owned,  ∅ a : BorrowedMut
region_param = { lifetime }
type_params  = { "<" ~ (type_param | region_param)
                     ~ ("," ~ (type_param | region_param))* ~ ">" }
```

Extend `type_alias` and `function` to optionally accept `type_params`:

```
type_alias = { "type" ~ identifier ~ type_params?
               ~ "=" ~ enum_variant ~ ("|" ~ enum_variant)* ~ ";"? }

function   = { "def" ~ identifier ~ type_params?
               ~ "(" ~ parameters? ~ ")" ~ ":" ~ type_ ~ ":=" ~ expression }
```

### 8.5 Lambda Expressions

```
lambda_param = { ("&" ~ "mut"?)? ~ "(" ~ identifier ~ ":" ~ type_ ~ ")" }
             -- ()        →  consuming lambda
             -- &()       →  borrowing lambda
             -- &mut ()   →  mutable-borrowing lambda

lambda_expr  = { "fn" ~ lambda_param ~ "->" ~ expression }
```

Add `lambda_expr` to `primary`.

### 8.6 Box Expression

```
box_expr = { "box" ~ "(" ~ expression ~ ")" }
```

Add `box_expr` to `primary`.

### 8.7 Borrow Expressions

```
borrow_expr = { "&" ~ "mut"? ~ expression }
```

Add `borrow_expr` to `primary`. Produces a `Borrowed` or `BorrowedMut`
reference to the sub-expression.

### 8.8 Typeclass Declaration

A typeclass declares an abstract interface over a type or type constructor.
It may require other typeclasses be satisfied first (`requires`), may
declare abstract methods, and may provide default implementations for
methods that can be derived from others.

```
typeclass_decl = {
  "typeclass" ~ identifier ~ type_params?
  ~ ("requires" ~ typeclass_constraint
      ~ ("," ~ typeclass_constraint)*)?
  ~ "{" ~ typeclass_member* ~ "}"
}

typeclass_constraint = {
  identifier ~ ("<" ~ (type_ | lifetime)
                    ~ ("," ~ (type_ | lifetime))* ~ ">")?
}
-- e.g.  OwnedFunctor<F>,  Clone<T>,  Eq<T>

typeclass_member = {
  typeclass_method_sig
  | typeclass_default_method
}

-- abstract method: signature only, no body
typeclass_method_sig = {
  identifier ~ type_params?
  ~ ":" ~ type_
  ~ where_clause?
}

-- default method: has a body; impls may override
typeclass_default_method = {
  "def" ~ identifier ~ type_params?
  ~ "(" ~ parameters? ~ ")" ~ ":" ~ type_
  ~ where_clause?
  ~ ":=" ~ expression
}
```

Examples:

```
typeclass OwnedFunctor<+F : Owned → Owned> {
  fmap : ∀(a b : Owned). (a →[Owned] b) →[Owned] F<a> →[Owned] F<b>
}

typeclass Applicative<+F : Owned → Owned>
  requires OwnedFunctor<F>
{
  pure : ∀(a : Owned). a →[Owned] F<a>
  ap   : ∀(a b : Owned). F<(a →[Owned] b)> →[Owned] F<a> →[Owned] F<b>

  -- default: fmap derived from pure and ap
  def fmap<a : Owned, b : Owned>(f : a →[Owned] b, x : F<a>) : F<b>
    := ap(pure(f), x)
}

typeclass Monad<+M : Owned → Owned>
  requires Applicative<M>
{
  bind : ∀(a b : Owned). M<a> →[Owned] (a →[Owned] M<b>) →[Owned] M<b>

  -- defaults: pure and ap derived from bind
  def pure<a : Owned>(x : a) : M<a>
    := bind(x, fn (a) -> a)   -- identity monad lift; impl may override

  def ap<a : Owned, b : Owned>(mf : M<(a →[Owned] b)>, mx : M<a>) : M<b>
    := bind(mf, fn (f) -> bind(mx, fn (x) -> pure(f(x))))
}
```

### 8.9 Typeclass Implementation

An `impl` block provides a concrete implementation of a typeclass for
a specific type or type constructor. All non-default methods must be
provided. Default methods may be overridden.

```
impl_decl = {
  "impl" ~ typeclass_constraint
  ~ "for" ~ type_
  ~ type_params?
  ~ where_clause?
  ~ "{" ~ impl_method* ~ "}"
}

impl_method = {
  "def" ~ identifier ~ type_params?
  ~ "(" ~ parameters? ~ ")" ~ ":" ~ type_
  ~ where_clause?
  ~ ":=" ~ expression
}
```

Examples:

```
-- Option is a Functor
impl OwnedFunctor<Option> {
  def fmap<a : Owned, b : Owned>(f : a →[Owned] b, x : Option<a>) : Option<b>
    := match x {
         #none    => #none
         #some(v) => #some(f(v))
       }
}

-- Option is a Monad (provides bind and pure; ap is derived)
impl Monad<Option> {
  def pure<a : Owned>(x : a) : Option<a>
    := #some(x)

  def bind<a : Owned, b : Owned>(x : Option<a>, f : a →[Owned] Option<b>) : Option<b>
    := match x {
         #none    => #none
         #some(v) => f(v)
       }
}
```

Note: because `Monad` requires `Applicative` which requires `OwnedFunctor`,
implementing `Monad<Option>` without also implementing `OwnedFunctor<Option>`
and `Applicative<Option>` is a compile error. The compiler checks that all
required typeclass constraints are satisfied, either by explicit `impl` blocks
or by default method derivation.

### 8.10 Where Clauses

Typeclass constraints on generic functions and type constructors are
expressed with `where` clauses. They appear on both `def` and `impl`.

```
where_clause = {
  "where" ~ where_constraint ~ ("," ~ where_constraint)*
}

where_constraint = {
  typeclass_constraint             -- e.g.  F : OwnedFunctor
  | lifetime ~ ">=" ~ lifetime    -- e.g.  'r >= 's  (region outlives)
}
```

Examples:

```
-- a function requiring any Monad
def sequence<M : Owned → Owned, a : Owned>(xs : List<M<a>>) : M<List<a>>
  where M : Monad
  := ...

-- a function requiring Clone and Eq
def deduplicate<+a : Owned>(xs : List<a>) : List<a>
  where a : Clone, a : Eq
  := ...

-- a region outlives constraint
def longest<'r, 's>(x : &'r Str, y : &'s Str) : &'r Str
  where 'r >= 's
  := ...
```

### 8.11 Updated Keyword List (Final)

```
KEYWORD = {
  "if" | "then" | "else" | "let" | "def" | "true" | "false"
  | "Unit" | "Int" | "Bool" | "while" | "do" | "module"
  | "mut" | "type" | "match"
  | "fn" | "box"               -- ownership
  | "typeclass" | "impl"       -- typeclass system
  | "requires" | "for"         -- typeclass relations
  | "where"                    -- constraints
}
