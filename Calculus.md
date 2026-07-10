# Core Calculus

The kind, type, region, ownership, typeclass, and memory systems for the sand language. This is the formal counterpart to the [README](README.md): the README describes the compiler as a pipeline of passes; this document specifies *what* the two safety passes enforce.

Safety is split across two passes, and the document is organized around that split:

1. the **type checker** (`passes/type_ast/`) decides kinds, types, regions, and the region-escape check (the kinding rules through the escape check).
2. the **ownership pass** (`passes/ownership/`) is a move/borrow dataflow over the *already-typed* program. It enforces affinity (at-most-one use), `&mut` exclusivity, and RAII drop placement (the ownership section).

The typing judgment is deliberately **affine-agnostic**: it never splits the context, so on its own it does not reject use-after-move: that is the ownership pass's job.

> **Status tags.** Constructs are tagged where it matters:
> - **[live]**: type-checked *and* executed (both interpreters + LLVM)
> - **[planned]**: designed and described here as a target, not yet built
> - **[erased]**: present in the type system, removed by monomorphisation before
>   runtime (regions; references become plain pointers)

---

## 1. Notation

| symbol | meaning | symbol | meaning |
| --- | --- | --- | --- |
| $k$ | kind | $'r, 's$ | regions (`'static`, parameter, or scope) |
| $a, b$ | type variables | $T, U$ | types |
| $F$ | enum reference | $x, y$ | term variables (`UniqVar`) |
| $e$ | expression | $s$ | statement |
| $\Gamma$ | typing context (type checker) | $\Delta$ | ownership env (ownership pass) |
| $\ge$ | outlives | $\sqsubseteq$ | region-aware subtyping on types |
| $<:$ | subkinding | $\vee$ | kind join |
| $\bar T, \bar e$ | sequences | $\varepsilon$ | empty |

Judgments take the form $\Gamma \vdash e \Rightarrow T : k$ (synthesis) and $\Gamma \vdash e \Leftarrow T : k$ (checking): under context $\Gamma$, expression $e$ has type $T$ at kind $k$.

---

## 2. Kinds

A kind records *how a value may be used*: whether it is owned, borrowed, or diverging. Kinds are what the region-escape check reads (it never inspects the type), and they form a small lattice.

### 2.1 Grammar

```
Kind  k  ::=  Owned                 -- a normal owned value
           |  Borrowed              -- shared borrow (capability only)
           |  BorrowedMut           -- exclusive borrow
           |  InternalMut           -- unchecked shared borrow
           |  Never                 -- uninhabited / diverging
           |  k₁ → k₂               -- type-constructor kind (HKT)
```

### 2.2 Subkinding $<:$

$k <: k'$ reads "a value of kind $k$ is usable where kind $k'$ is expected".

```
k <: k                                 (refl)
Never <: k                             (Never is bottom)
Owned <: Borrowed                      (auto-reborrow capability)
Owned <: BorrowedMut
Owned <: InternalMut
```

`Borrowed`, `BorrowedMut`, and `InternalMut` are mutually **incomparable**: an owned value can be reborrowed in any mode, but one borrow mode never substitutes for another.

### 2.3 Join $\vee$

The join merges branch kinds at `if`/`match`:

```
k ∨ k = k
Never ∨ k = k         k ∨ Never = k
Borrowed ∨ BorrowedMut = Owned         (distinct borrow modes collapse to Owned)
```

`Owned` is the top of the lattice; `Never` is the bottom and drives divergence (a `while true` loop has kind `Never` and, in checking mode, coerces to any expected type via `coerce_never`).

---

## 3. Regions and the Outlives Lattice

Regions are **pure lexical lifetimes** governing reference validity only. They are wholly decoupled from allocation (allocation is the `Heaped` mechanism of the memory model) and are **[erased]** by monomorphisation.

```
Region  'r  ::=  'static             -- outlives everything
              |  'a                  -- a declared lifetime parameter
              |  scope region        -- the function frame F, or a block Bᵢ
```

Scope regions carry a **depth** (outer is smaller). `'static` and lifetime parameters sit below the frame (depth 0); each nested block is one deeper.

**Outlives $\ge$:**

```
'static ≥ 'r              'a ≥ F          F ≥ B₀ ≥ B₁ ≥ …
'r ≥ 'r                   'a ≥ 'b         (assumed edges, from where-clauses)
```

Two distinct lifetime parameters (or a parameter versus the frame) are incomparable without an explicit `where` edge. Such constraints are conservatively rejected when unprovable: sound, and more conservative than Rust.

---

## 4. Types

### 4.1 Grammar

```
Type  T  ::=  a                       -- type variable        Ty::Param   (Owned)
           |  Int | Bool | Unit       -- primitives                       (Owned, Copy)
           |  &'r T                   -- shared reference     Ty::Ref     (Borrowed)
           |  &'r mut T               -- exclusive reference  Ty::RefMut  (BorrowedMut)
           |  T @ 'r                  -- region ascription    Ty::Region  (Owned)
           |  (T₁,…,Tₙ)               -- tuple (n ≥ 2)        Ty::Tuple   (Owned)
           |  F                       -- non-parametric enum  Ty::Enum    (Owned)
           |  F<T̄ ; 'r̄>               -- applied enum/ADT     Ty::App     (Owned if saturated)
           |  a<T̄>                     -- applied type var     Ty::ParamApp (HKT)
           |  _                        -- constructor hole     Ty::Hole    (partial application only)
           |  #tag₁ | … | #tagₙ       -- anonymous tag union (an Enum)
           |  Ptr<T>                  -- raw pointer          Ty::Ptr     (Owned, Copy)
           |  Slot<L>                 -- reuse husk, layout-indexed        (Owned) [planned]
           |  T₁ →[k] T₂              -- function type                    (Owned)
           |  Top                     -- println/print arg only (intrinsic escape hatch)
```

- A reference $\&'r T$ is a dedicated `Ty::Ref(Region, Ty)`.
- An applied enum $F\langle\bar T ; \bar{'r}\rangle$ is `Ty::App(EnumRef, &[Ty], &[Region])`, carrying type *and* region arguments. Region arguments record a borrow that lives inside a payload, so `freeRegions` sees it (this closes escape-via-data; see the escape check). `Ty::Enum` is used only for fully non-parametric enums.
- An **under-saturated** `Ty::App` — fewer type arguments than `F`'s arity, or an argument list containing holes `_` (`Ty::Hole`) — is a *partial application*: a type-constructor abstraction (§4.5). `a<T̄>` (`Ty::ParamApp`) is the application of a higher-kinded type *variable*. Both reduce away before monomorphisation, so a value's type is always saturated and hole-free.
- `Ptr<T>` is the raw, `Copy`, region-free substrate pointer of the memory model; its element type erases to an opaque `ptr` at runtime.
- The arrow $T_1 \to_{[k]} T_2$ carries the *ownership mode of the function itself*: $\to_{[\mathsf{Owned}]}$ consumes its argument (single-use), $\to_{[\mathsf{Borrowed}]}$ borrows it (reusable). It arrives with lambdas and
  is always `Owned` as a value.
- References are **[erased]** to plain pointers at runtime.

### 4.2 Generic parameters

Polymorphism is parameter *lists* on `def`s and `type`s, fully removed by monomorphisation before MIR:

```
type Holder<'a, +a : Owned> = H(&'a a)
def  longest<'a, 'b>(x: &'a Int, y: &'b Int): &'a Int  where 'a >= 'b := …
```

Lifetimes come **before** type parameters, in both declaration and use. A (mutually) recursive type additionally requires `deriving Heaped` (see the memory model and the kinding rule for recursive enums).

### 4.3 Region-aware subtyping $\sqsubseteq$

```
T ⊑ T                                           (identity, by interning)
Never inhabits any T                            (coerce_never, checking mode)
&'r T ⊑ &'s T'         iff  'r ≥ 's ∧ T ⊑ T'    (& covariant in its region)
&'r mut T ⊑ &'s mut T' iff  'r = 's ∧ T = T'    (&mut invariant)
```

$\sqsubseteq$ is not a general subtype relation: there is no subtyping between concrete data types. It exists only to let a longer-lived borrow stand in for a shorter-lived one.

### 4.4 Variance

Parameters carry an optional variance (`+`/`-`) and a kind. The default variance follows the parameter's position:

| Position | Variance |
| --- | --- |
| producer position only | `+` |
| consumer position only | `-` |
| both | `∅` |
| `Borrowed` parameter | `+` (always) |
| `BorrowedMut`/`InternalMut` parameter | `∅` (always) |

Because monomorphisation erases generics and there is no concrete-type subtyping, variance is a **declaration/use-site soundness check**, not a coercion. Declaring `+a : BorrowedMut` is a kind error.

### 4.5 Partial application and constructor holes

A type constructor `F : k₁ → … → kₙ → Owned` need not be fully applied. Supplying fewer than `n` arguments, or putting **holes** `_` in argument positions, forms a *partial application* — a **constructor abstraction**

```
Λ (X₁:k_{j₁} … Xₘ:k_{jₘ}). F<…>
```

whose parameters `X̄` are the holes, taken left-to-right. Concretely `Ty::App(F, T̄)` *is* this abstraction: each `_` is a hole, and any arguments omitted from the right are implicit trailing holes. (Write `_` for a hole.)

```
Option<_>     ≡  Λ X. Option<X>        : Owned → Owned            (≡ bare `Option`)
Result<_, E>  ≡  Λ X. Result<X, E>     : Owned → Owned
Result<E, _>  ≡  Result<E>             : Owned → Owned            (trailing hole = currying)
Result<_, _>  ≡  Λ X Y. Result<X, Y>   : Owned → Owned → Owned
```

So *currying* (omit trailing arguments) and *arbitrary holes* (an interior `_`) are the **one** construct; the kind of a partial application is the kinds of its holes, in order, arrowed over `Owned` (rule **K-App**, generalised in §7).

**Reduction.** Applying an abstraction substitutes its holes positionally:

```
(Λ X̄. F<T̄>) <S̄>   ↝   F<T̄[S̄ / X̄]>                                (β)
```

`a<T̄>` (`Ty::ParamApp`) is the application of a higher-kinded type *variable*; once `a` is bound to an abstraction `Φ` — by monomorphisation, or when elaborating an `impl`'s methods (§12.1) — `a<T̄>` becomes `Φ<T̄>` and β-reduces. β is first-order, non-recursive substitution, hence **strongly normalising and confluent**; a *saturated* application (kind `Owned`) reduces to a unique hole-free `Ty::App`, which is exactly the normal form the later passes consume. **Holes never appear in a value's type** (values have kind `Owned`): they are confined to abstraction heads — the binding of a higher-kinded parameter, and the head of an `impl`.

---

## 5. Terms

### 5.1 Expressions

```
Expr e ::=
         |  n | true | false | ()                                 -- literals
         |  x                                                     -- variable
         |  &e | &mut e                                           -- borrow
         |  *e                                                    -- deref / read-through
         |  e₁ ⊕ e₂ | ⊖ e                                         -- binary / unary ops
         |  { s̄; e? }                                             -- block (carries drop metadata)
         |  if e then e else e   |   while e do e                 -- control flow
         |  match e { arm* }                                      -- pattern match (scrutinee consumed)
         |  F#Tag | F#Tag(e) | #Tag | #Tag(e)                     -- constructors
         |  (e₁,…,eₙ)                                             -- tuple
         |  f(ē)                                                  -- call to a def / extern
         |  m(ē)                                                  -- typeclass method call
         |  __intrinsic(ē) | size_of::<T>()                       -- intrinsic / turbofish
         |  e₁(e₂)                                                -- application of a value [planned]
         |  fn (x:T) -> e | fn &(x:T) -> e | fn &mut (x:T) -> e   -- lambdas [planned]
         |  reuse cell as #C(ē)                                   -- in-place reuse [planned]
         |  e.share()                                             -- duplicate a Shared handle [planned]
```

### 5.2 Statements

```
Stmt s ::=
         | let x : T = e                             -- consuming declaration
         | let &x = e   |   let &mut x = e           -- borrow declaration (desugared)
         | let (x̄) = e                               -- tuple destructure
         | let F#Tag(x) = e else e                   -- constructor destructure
         | x = e                                     -- variable assignment
         | *r = e                                    -- write-through
         | e                                         -- expression statement
```

### 5.3 Patterns

```
pat ::=  _ | x | (pat̄) | #Tag | #Tag(pat) | F#Tag(pat) | n | true | false | cell @ pat
```

A `match` **always consumes** the scrutinee, and its bindings are owned. Only `Variant` patterns are refutable. A consuming match binds *every* payload position, including wildcards (which bind generated temporaries), so all un-moved fields are dropped on scope exit. For a heaped scrutinee, the consuming match is lowered by heap lowering to `unique_take` followed by an ordinary node match.

### 5.4 Lambdas and application *[planned]*

```
Γ, x :Owned T ⊢ e ⇒ U : k
─────────────────────────────────────  (Lam-Owned)
Γ ⊢ fn (x:T) -> e ⇒ T →[Owned] U : Owned

'r fresh   Γ, x :Borrowed T ⊢ e ⇒ U : k   'r ∉ freeRegions(U)
──────────────────────────────────────────────────────────────  (Lam-Borrow)
Γ ⊢ fn &(x:T) -> e ⇒ T →[Borrowed] U : Owned

Γ ⊢ e₁ ⇒ T →[m] U : Owned    Γ ⊢ e₂ ⇐ T : (Owned if m=Owned else Borrowed)
──────────────────────────────────────────────────────────────────────────  (App)
Γ ⊢ e₁(e₂) ⇒ U : Owned
```

Closures capture by move or by borrow (inferred from use); in codegen that is a function pointer plus a captured-environment pointer. A function-argument position is the first contravariant position (see variance).

---

## 6. Typing Context

```
Γ ::= ε
    | Γ, x : T @ k @ home('r)      -- term var: type, kind, home scope region
    | Γ, a : k                     -- type variable
    | Γ, 'r                        -- region variable / parameter (with a depth)
    | Γ, 'a ≥ 'b                   -- assumed outlives edge (from a where-clause)
```

Each term binding records its **home region** (a parameter's frame, a local's block); the escape check reads it. In practice $\Gamma$ never removes a consumed term binding: affinity is enforced in the ownership environment $\Delta$, not here.

---

## 7. Kinding Rules

$\Gamma \vdash T : k$, a pre-pass over type expressions.

```
───────────────  (K-Prim)            
Γ ⊢ Int|Bool|Unit : Owned            

(a : k) ∈ Γ
───────────  (K-Var)									 
Γ ⊢ a : k

Γ ⊢ T : Owned                        
─────────────────  (K-Region)        
Γ ⊢ T @ 'r : Owned                   

Γ ⊢ T : Owned
─────────────────────  (K-Borrow)
Γ ⊢ &'r T : Borrowed

Γ ⊢ T : Owned                        
──────────────────────  (K-BorrowMut)
Γ ⊢ &'r mut T : BorrowedMut          

Γ ⊢ Tᵢ : Owned (each i)
 ───────────────────────  (K-Tuple)
Γ ⊢ (T₁,…,Tₙ) : Owned

F : (k̄ ; 'r̄ kinds) → kF   Γ ⊢ T̄ : k̄         
──────────────────────────────────  (K-App) 
Γ ⊢ F<T̄ ; 'r̄> : kF                          

Γ ⊢ T : Owned
 ───────────────  (K-Ptr)
Γ ⊢ Ptr<T> : Owned

F (mutually) recursive   F derives Heaped    
─────────────────────────────────────────  (K-HeapedRec) 
Γ ⊢ F<…> : Owned                             

Γ ⊢ T₁ : k₁   Γ ⊢ T₂ : k₂
──────────────────────    (K-Arrow)
Γ ⊢ T₁ →[k] T₂ : Owned
```

`K-HeapedRec` is the rule that forces recursive types through the heap: a (mutually) recursive enum is well-kinded *only* when it derives `Heaped`, since otherwise it would have infinite size.

**Higher kinds and partial application.** `K-App` above is the *saturated* case (`m = 0`); it generalises to partial applications and higher-kinded variables. `Δ` is a **hole context** recording the kinds of an abstraction's holes.

```
F : k₁→…→kₙ→Owned     Γ ⊢ Tᵢ : kᵢ  (each supplied/fixed slot i)
the holes of T̄, left-to-right, occupy slots j₁ < … < jₘ
─────────────────────────────────────────────────────────────  (K-App, general)
Γ; Δ ⊢ F<T̄ ; 'r̄> : k_{j₁} → … → k_{jₘ} → Owned

(a : k₁→…→kₘ→k) ∈ Γ      Γ ⊢ Tᵢ : kᵢ
────────────────────────────────────  (K-VApp)
Γ ⊢ a<T̄> : k

(Xᵢ : kᵢ) ∈ Δ
─────────────────  (K-Hole)
Γ; Δ ⊢ _ᵢ : kᵢ

Γ ⊢ Φ : k₁→…→kₘ→k      Γ ⊢ Sᵢ : kᵢ      Φ ↠ Λ X̄. F<T̄>
─────────────────────────────────────────────────────  (K-Beta)
Γ ⊢ Φ<S̄> : k      with      Φ<S̄> ≡ F<T̄[S̄/X̄]>
```

`K-Hole` is checked **only** under a hole context (an abstraction head), so no closed (value) type contains a hole. `K-Beta` is the kinding counterpart of the β-rule of §4.5: it is the sole constructor-level computation, it is confluent and strongly normalising, and on a saturated use (`k = Owned`) it yields a hole-free `Ty::App` — preserving the invariant that monomorphisation only ever sees ground applied enums.

---

## 8. Bidirectional Typing

$\Gamma \vdash e \Rightarrow T : k$ (synthesis) and $\Gamma \vdash e \Leftarrow T : k$ (checking). The context is threaded structurally and **never split**; affinity conditions are stated separately in the ownership section.

### 8.1 Subsumption

```
Γ ⊢ e ⇒ T : k    k <: k'    T ⊑ T'
──────────────────────────────────  (Sub)
Γ ⊢ e ⇐ T' : k'
```

$<:$ is the subkinding of the kinds section; $\sqsubseteq$ is the region-aware relation above, not a general subtype relation.

### 8.2 Variables, literals, borrow, deref *[live]*

```
(x : T @ k) ∈ Γ                       Γ ⊢ e ⇒ T : k   'r = home-region of e
─────────────────  (Var)              ──────────────────────────────────────  (Borrow)
Γ ⊢ x ⇒ T : k                         Γ ⊢ &e ⇒ &'r T : Borrowed
                                        ( &mut e ⇒ &'r mut T : BorrowedMut )
──────────────────  (Lit)
Γ ⊢ n|b|() ⇒ Prim : Owned             Γ ⊢ e ⇒ &'r T : Borrowed  (or &'r mut)
                                      ──────────────────────────────────────  (Deref)
                                      Γ ⊢ *e ⇒ T : kind_of(T)
```

`Var` returns the binding's stored kind and **never consumes** `x`. `&mut e` additionally requires `e` to name a `mut` place. A borrow's type region is the referent's *home scope* (per scope, not per binding).

### 8.3 Blocks *[live]*

```
'B fresh block region (depth = enclosing + 1)
Γ,'B ⊢ s̄ ⊣ Γ'    Γ' ⊢ e ⇒ T : k    freeRegions(T) names no region of depth ≥ 'B
──────────────────────────────────────────────────────────────────────────────  (Block)
Γ ⊢ { s̄; e } ⇒ T : k
```

The side condition is the escape check at a block boundary; see the regions and escape sections for `freeRegions`.

### 8.4 Let, control flow, calls *[live]*

```
Γ ⊢ e₁ ⇒ T : Owned    Γ, x : T @ Owned @ home(cur) ⊢ e₂ ⇒ U : k
────────────────────────────────────────────────────────────────  (Let-Owned)
Γ ⊢ (let x : T = e₁; e₂) ⇒ U : k

Γ ⊢ e₁ ⇐ Bool : Owned   Γ ⊢ e₂ ⇒ T₂ : k₂   Γ ⊢ e₃ ⇒ T₃ : k₃
T = join_region_ty(T₂, T₃)   k = k₂ ∨ k₃
────────────────────────────────────────────────────────────────  (If)
Γ ⊢ if e₁ then e₂ else e₃ ⇒ T : k

Γ ⊢ e ⇒ T : Owned   ∀ armᵢ: Γ, bindings(patᵢ, T) ⊢ eᵢ ⇒ Uᵢ : kᵢ
U = join over Uᵢ (region meet)   k = ∨ kᵢ
────────────────────────────────────────────────────────────────  (Match)
Γ ⊢ match e { arm* } ⇒ U : k

f : (T̄ ; 'ᾱ) → U declared   Γ ⊢ eᵢ ⇐ Tᵢ   σ = infer_region_subst(declared, actual)
each callee `where 'a >= 'b` holds under σ + Γ's assumptions
────────────────────────────────────────────────────────────────────  (Call)
Γ ⊢ f(ē) ⇒ region_subst(U, σ) : Owned
```

`let &x` / `let &mut x` desugar to a borrow-typed `let`. Branches agree on type *modulo the region meet* (`join_region_ty`), so an escape through any single branch is still caught, and their kinds join. A call infers a per-parameter region substitution, stamps the result with it, and checks the callee's region `where`-clauses under it.

---

## 9. Region Substitution at Call Sites

A call site computes fresh result regions.
Given a callee declared with regions $\bar{'\alpha}$ and a result type $U$, the checker:

1. matches each declared region position against the region actually supplied by the corresponding argument, producing a substitution $\sigma$ (`infer_region_subst`);
2. for any result region not pinned by an argument, picks the greatest lower bound of the argument regions, bounded below by the call-site scope, so the result is
   never claimed to outlive the call;
3. discharges each callee `where 'a >= 'b` under $\sigma$ together with the
   caller's own assumed outlives edges.

This is what lets `def longest<'a,'b>(...) where 'a >= 'b: &'a Int` return a borrow
whose region is equal to the caller's `'a`, no looser.

---

## 10. The Escape Check

The escape check is the region half of safety. It rejects any borrow that would outlive what it points into. It reads **kinds and free regions**, never affinity.

`freeRegions(T)` collects every region mentioned in `T`, including those carried in an applied enum's region arguments (`Ty::App`'s `&[Region]`). That is why a borrow hidden inside a payload cannot escape unnoticed.

The check fires at every boundary that ends a scope:
- **block result**: the block's value type may name no region of depth $\ge$ the block's own region (the side condition of the `Block` rule);
- **function return**: the return type may name no scope region of the frame.

Equivalently: a returned or block-yielded value of type $T$ is rejected when $'r \in \mathrm{freeRegions}(T)$ for some local region $'r$. The rule is purely structural and more conservative than a full borrow checker: it never reasons about *which* paths are live, only about whether a local region could appear in the result.

---

## 11. Ownership and Drop

The ownership pass is the **second enforcement layer**: a move/borrow dataflow over the already-typed program, and a *transformer* rather than a pure checker: it inserts drops. Its environment $\Delta$ is an `im::OrdMap` keyed by `UniqVar` (so key order is declaration order), tracking each variable as `Owned`/`Moved` together with its live borrows.

It enforces three things:
- **affinity**: using a non-`Copy` owned variable marks it `Moved`; a second use is an error. When the type is `Clone`, the error hints `clone(&x)`.
- **`&mut` exclusivity**: a mutable borrow conflicts with any other live borrow of the same place. Borrows are released lexically at block exit, and `if`/`match` merges union the surviving borrows.
- **drop placement**: at scope exit, every owned non-`Copy` local is dropped in reverse declaration order. At a branch merge, a value owned on one branch but moved on another gets a *completing drop* on the branch where it survives, so it is uniformly consumed at the merge with no runtime drop flags and no leak.

Drops are recorded as block metadata and lowered to a first-class MIR `Statement::Drop`, which codegen turns into the type's structural destructor (`__drop_in_place`) plus, for heaped values, the backing free.

Affinity lives *here* and not in the typing rules on purpose: the type checker's context is structural (it never removes a variable on use), so affinity is a separate analysis over the already-typed tree.

---

## 12. Memory Model and Typeclasses

### 12.1 Typeclasses

A `typeclass` declares a set of methods over one or more type parameters; an `impl` supplies them for a concrete head type. Methods are called as ordinary functions and **resolved by argument type**, then dispatched by monomorphisation, i.e. there is no `x.method()` dot sugar and no runtime dictionary. A `def` may carry `where T : C` constraints, discharged at each call site against the instance table.

Instances are keyed globally by `(class, head(T))` and form one coherent set: overlapping or orphan instances are rejected, so resolution is unambiguous program-wide. A class may give **default method** bodies that an `impl` inherits unless it overrides them.

`Clone` and `Copy` are ordinary typeclasses, not language primitives
([`core.sand`](lang/src/core.sand)):

```sand
typeclass Clone<T> { def clone(x: &T): T }   -- borrow in, fresh owned value out
typeclass Copy<T> requires Clone { }         -- marker: cloning is implicit & cheap
```

`Int`, `Bool`, and `Unit` implement `Copy` (their `clone` is just a deref). For a non-`Copy` type the affinity check demands an explicit `clone(&x)`.

Since sand has higher kinded types, we can easily express the familiar `Functor`, `Applicative`, and `Monad` type classes:
```sand
typeclass Functor<F : Owned -> Owned> {
    def fmap<A, B>(x: F<A>, f: A -> B): F<B>
}

typeclass Applicative<F : Owned -> Owned> requires Functor {
    def pure<A>(x: A): F<A>
    def ap<A, B>(f: F<A -> B>, x: F<A>): F<B>
}

typeclass Monad<F : Owned -> Owned> requires Applicative {
    def bind<A, B>(x: F<A>, f: A -> F<B>): F<B>
}
```

**Instances for higher-kinded classes.** An `impl` of a class whose parameter is higher-kinded supplies a *constructor of that kind* — a partial application (§4.5). It is written either as a bare constructor (sugar for the all-holes abstraction) or with explicit holes whose remaining slots are fixed by the impl's own parameters:

```sand
impl Functor for Option            -- ≡ Functor for Option<_>
impl<E> Functor for Result<_, E>   -- map over the first slot; E is carried
```

The head must have the class parameter's kind (`Result<_, E> : Owned → Owned`, one hole per arrow). Elaborating the methods binds the class parameter `F` to the head abstraction and brings the impl parameters (`E`) into scope, so each `F<A>` in a method signature β-reduces (§4.5): `fmap`'s type becomes `fmap<A,B>(x: Result<A,E>, f: A→B): Result<B,E>`, with `E` recovered per call.

**Coherence and resolution (generalised).** Instances are bucketed by their head constructor `head(T) = F`. Within a bucket they must be **non-overlapping**, where two heads overlap iff their abstractions *unify* (a bare/all-holes head overlaps every other, so there is still at most one `Functor` instance per type). Resolving a call at a ground type `F<Ū>` selects the unique instance whose head unifies with `F<Ū>`, recovering the impl parameters (`E ↦ Ū` at the fixed slots) and the class operand at the holes, then β-reduces the method signature for monomorphisation. Because the normal form is unique (§4.5), resolution stays deterministic — coherence holds exactly as in the first-order, ground-headed case.

**Region restriction.** Holes abstract *type* parameters only; a higher-kinded head supplies its region parameters in full (or the constructor has none). So `freeRegions` (the escape check, §10) always sees complete region arguments, and partial application adds nothing the region analysis must reason about.

### 12.2 Heap Lowering

Heap allocation is opt-in: a type derives `Heaped`, and the compiler rewrites it into an ordinary handle with a raw-pointer *before* the ownership and monomorphisation passes run. After heap lowering **no heaped enum survives**: the later passes see only ordinary enums, `Unique` handles, and `Ptr` operations.

For each heaped enum `E<T...>`, heap lowering ([`heap_lower.rs`](lang/src/passes/heap_lower.rs)):
- synthesises a non-recursive **node enum** `E$Node<T...>` with the same variants, every heaped field rewritten to a `Unique<...$Node>` handle, so the type is finite (recursion now goes through a pointer);
- rewrites construction `E#C(p)` to `unique_alloc(E$Node#C(p))`;
- rewrites a consuming `match` / `let`-pattern to `unique_take` plus an ordinary
  node match.

It runs before ownership (so drops land uniformly on the resulting handles) and before monomorphisation (so the injected `unique_*` calls specialise like any other generic call).

### 12.3 The `Ptr` Type and Allocation Strategies

The compiler itself stays allocation-agnostic. It knows only three things: the `Ptr<T>` primitive, the `extern` FFI boundary, and the `Heaped` lowering protocol above. Everything about *how* memory is obtained and released lives in [`core.sand`](lang/src/core.sand), over the pointer intrinsics:

```
__ptr_read(p: Ptr<T>): T            -- load through a raw pointer
__ptr_write(p: Ptr<T>, v: T): Unit  -- store through a raw pointer
__ptr_cast(p: Ptr<A>): Ptr<B>       -- reinterpret a raw pointer
__drop_in_place(x): Unit            -- compiler-generated structural destructor
size_of::<T>(): Int                 -- the byte size of T
```

The `Unique` strategy is the `Box`-equivalent: a `Unique<T> = U(Ptr<T>)` is a non-`Copy` handle owning one heap node. `unique_alloc` moves a value onto the heap (`malloc` + `__ptr_write`); `unique_release` is the *deep* drop (structurally drop the node, then `free`) used when an owned heaped value leaves scope unconsumed; `unique_take` is the *shallow* release backing a consuming match (read the node onto the stack, free only the backing cell, hand the fields to the arm bindings). Because drop placement is deterministic, every allocation has exactly one matching
release: heaped programs free deterministically, with no GC and no leak.

Swapping `malloc`/`free` or the whole strategy is swapping these library functions; the compiler does not change. Reference-counted (`Shared`) handles, in-place `reuse` of a drained node's husk (`Slot<L>`), and the richer `HeapedUnique` / `HeapedShared` strategy hierarchy are **[planned]** extensions of this same protocol.
