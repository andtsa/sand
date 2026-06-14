# Core Calculus (current)

The kind, type, region, ownership, and typeclass systems for the sand language as
they stand **now plus the immediate roadmap** — i.e. everything implemented today
*and* everything in the next planned steps ([Step 11, 13, 15, Memory D, E per
`TypeSystemLedger.md` §2](TypeSystemLedger.md)). It excludes only what is
genuinely deferred (Ledger §3) or obsolete (`box`, subsumed by `Heaped`).

This is the *intersection* of three sources, with the live code as the tiebreaker:
- [`Calculus.md`](Calculus.md) — the original design.
- [`TypeSystemLedger.md`](TypeSystemLedger.md) / [`TypeSystemPlan.md`](TypeSystemPlan.md) — durable verdicts + roadmap + deviations.
- the live code — `lang/src/{passes,ir_types,interpreter,lang}`, `grammar.pest`.

Soundness + backend audits: [`Calculus-soundness.md`](Calculus-soundness.md).

> **Status tags.** Each construct is tagged:
> - **[live]** — type-checked *and* executed (both interpreters + LLVM) today.
> - **[planned: N]** — designed and on the immediate roadmap (Ledger §2), not yet
>   built. Included here "like everything else," per the calculus's role as the
>   target.
> - **[erased]** — present in the type system, removed by monomorphisation before
>   runtime (regions; references become plain pointers).

> **Two enforcement layers (read first).** Safety is split across two passes, and
> this document is organized around that split:
> 1. the **type checker** (`passes/type_ast/`) — kinds, types, regions, the
>    region-escape check (§7–10).
> 2. the **ownership pass** (`passes/ownership/`) — a move/borrow **dataflow over
>    the already-typed program** enforcing affinity (at-most-one use), `&mut`
>    exclusivity, and RAII drop placement (§11).
>
> The typing judgment is **deliberately affine-agnostic**: it does not split the
> context, so it alone does not reject use-after-move. That is the ownership pass's
> job. This is the chief way the live system departs from a textbook substructural
> calculus.

---

## 1. Notation

```
k          kind            'r,'s   regions ('static | param | scope)
a,b        type variables  T,U     types
F          enum reference  x,y     term variables (UniqVar)
e          expression      s       statement
Γ          typing context (type checker)     Δ   ownership env (ownership pass)
≥          outlives        ⊑       region-aware subtyping on types
T̄,ē        sequences       ε       empty
```

---

## 2. Kinds

### 2.1 Grammar

```
Kind  k  ::=  Owned                 -- a normal owned value             
           |  Borrowed              -- shared borrow (capability only)  
           |  BorrowedMut           -- exclusive borrow
           |  InternalMut           -- unchecked shared borrow
           |  Never                 -- uninhabited / diverging          
           |  k₁ → k₂               -- type-constructor kind (HKT)
```

### 2.2 Subkinding `<:`  

```
k <: k                                 (refl)
Never <: k                             (Never is bottom)
Owned <: Borrowed                      (auto-reborrow capability)
Owned <: BorrowedMut
Owned <: InternalMut
```

`Borrowed`, `BorrowedMut`, and `InternalMut` are **incomparable**.

### 2.3 Join `∨`

Merges branch kinds at `if`/`match`:

```
k ∨ k = k
Never ∨ k = k         k ∨ Never = k
Borrowed ∨ BorrowedMut = Owned         (distinct borrow modes → Owned)
```

`Owned` is the top of the implemented lattice; `Never` is bottom and drives divergence (a `while true` loop has kind `Never` and coerces to any expected type in checking mode, via `coerce_never`).

---

## 3. Regions and the Outlives Lattice

Regions are **pure lexical lifetimes** governing *reference validity only*, wholly decoupled from allocation (allocation is `Heaped`, §12.3). Regions are erased by monomorphisation.

```
Region  'r  ::=  'static             -- outlives everything
              |  'a                  -- a declared lifetime parameter
              |  scope region        -- the function frame F, or a block Bᵢ
```

Scope regions carry a **depth** (outer is smaller). `'static` and lifetime parameters sit below the frame (depth 0); each nested block is one deeper.

**Outlives `≥`**:

```
'static ≥ 'r                          'a ≥ F          F ≥ B₀ ≥ B₁ ≥ …
'r ≥ 'r                               where 'a >= 'b  (assumed edges)
```

plus transitive closure. Two distinct lifetime parameters (or a parameter vs. the frame) are incomparable without an explicit `where` (such constraints are conservatively rejected — sound, more conservative than Rust).

---

## 4. Types

### 4.1 Grammar

```
Type  T  ::=  a                       -- type variable        Ty::Param   (Owned)
           |  Int | Bool | Unit       -- primitives                       (Owned,Copy)
           |  &'r T                   -- shared reference     Ty::Ref     (Borrowed)
           |  &'r mut T               -- exclusive ref        Ty::RefMut  (BorrowedMut)
           |  T @ 'r                  -- region ascription    Ty::Region  (Owned)
           |  (T₁,…,Tₙ)               -- tuple (n ≥ 2)        Ty::Tuple    (Owned)
           |  F                       -- non-parametric enum  Ty::Enum     (Owned)
           |  F<T̄ ; 'r̄>               -- applied enum/ADT     Ty::App     (Owned)
           |  #tag₁ | … | #tagₙ       -- anonymous tag union (an Enum)  
           |  Ptr<T>                  -- raw pointer          Ty::Ptr     (Owned,Copy)
           |  Slot<L>                 -- reuse husk, layout-indexed       (Owned) 
           |  T₁ →[k] T₂              -- function type                    (Owned)       
           |  Top                     -- println/print arg only (intrinsic escape hatch)
```

- **`&'r T` is a dedicated `Ty::Ref(Region, Ty)`** 
- **`F<T̄ ; 'r̄>` is `Ty::App(EnumRef, &[Ty], &[Region])`** containing type *and* region arguments. Region args make a borrow stored in a payload part of the type, so `freeRegions` sees it (closes escape-via-data, §10). `Ty::Enum` is used only for
  fully non-parametric enums.
- **`Ptr<T>`**: raw, `Copy`, region-free substrate pointer (§12.3); element type erased to an opaque `ptr` at runtime.
- **`T₁ →[k] T₂`**: the function arrow carries the *ownership mode of the function* — `→[Owned]` consumes its argument (single-use), `→[Borrowed]` borrows it (reusable). Arrives with lambdas (§5.4); always `Owned` itself.
- References are erased to plain pointers at runtime

### 4.2 Generic parameters

Polymorphism is parameter *lists* on `def`s and `type`s, fully removed by monomorphisation before MIR:

```
type Holder<'a, +a : Owned> = H(&'a a)
def  longest<'a, 'b>(x: &'a Int, y: &'b Int): &'a Int  where 'a >= 'b := …
```

Lifetimes come **before** type parameters (declaration and use). Recursive types additionally require `deriving Heaped` (§12.2 / K-HeapedRec).

### 4.3 Region-aware subtyping `⊑`  

```
T ⊑ T                                           (identity, by interning)
Never inhabits any T                            (coerce_never, checking mode)
&'r T ⊑ &'s T'         iff  'r ≥ 's ∧ T ⊑ T'    (& covariant in region)
&'r mut T ⊑ &'s mut T' iff  'r = 's ∧ T = T'    (&mut invariant)
```

### 4.4 Variance

Parameters carry optional variance (`+`/`-`) and a kind. Default variance is determined by position

| Position | Variance |
| --- | ---|
| producer position only | + |
| consumer position only | - |
| both | ∅ |
| Borrowed param | + (always) |
| BorrowedMut/InteriorMut param | ∅ (always) |

Because mono erases generics and there is no concrete-type subtyping, variance is a **declaration/use-site soundness check**, not a coercion. `+a : BorrowedMut` is a kind error.

---

## 5. Terms

### 5.1 Expressions (live `Expression`, plus planned)

```
Expr e ::=
         |  n | true | false | ()                                 -- literals                         
         |  x                                                     -- variable                         
         |  &e | &mut e                                           -- borrow (Borrow, is_mutable)      
         |  *e                                                    -- deref / read-through (Deref)     
         |  e₁ ⊕ e₂ | ⊖ e                                         -- bin / un ops                     
         |  { s̄; e? }                                             -- block (carries drop metadata)    
         |  if e then e else e   |   while e do e                 -- control flow                     
         |  match e { arm* }                                      -- pattern match (scrutinee consumed)
         |  F#Tag | F#Tag(e) | #Tag | #Tag(e)                     -- constructors                     
         |  (e₁,…,eₙ)                                             -- tuple                            
         |  f(ē)                                                  -- call to a def / extern           
         |  m(ē)                                                  -- typeclass method call (MethodCall)
         |  __intrinsic(ē) | size_of::<T>()                       -- intrinsic / turbofish            
         |  e₁(e₂)                                                -- application of a value           
         |  fn (x:T) -> e | fn &(x:T) -> e | fn &mut (x:T) -> e   -- lambdas             
         |  reuse cell as #C(ē)                                   -- in-place reuse                   
         |  e.share()                                             -- duplicate a Shared handle        
```


### 5.2 Statements

```
Stmt s ::=
         | let x : T = e                             -- consuming declaration   (Declaration)
         | let &x = e   |   let &mut x = e           -- borrow declaration (desugared)        
         | let (x̄) = e                               -- tuple destructure       (LetTuple)     
         | let F#Tag(x) = e else e                   -- constructor destructure (LetPattern)   
         | x = e                                     -- variable assignment     (Assignment)   
         | *r = e                                    -- write-through           (DerefAssign)  
         | e                                         -- expression statement    (Expr)         
```


### 5.3 Patterns (live `MatchPattern`)

```
pat ::= 
      | _ 
      | x 
      | (pat̄) 
      | #Tag 
      | #Tag(pat) 
      | F#Tag(pat) 
      | n 
      | true 
      | false     
      | cell @ pat
```

A match **always consumes** the scrutinee; bindings are owned. Only `Variant` patterns are refutable. For a **heaped** scrutinee, the consuming match is lowered (by `heap_lower`, §12.2) to `unique_take` + an ordinary node match. A consuming match binds *every* payload position, including wildcards (which bind to generated temporary values), so all un-moved fields are `drop`ped on scope exit.

### 5.4 Lambdas and application 

```
Γ, x :_Owned T ⊢ e ⇒ U : k
──────────────────────────────────────────────  (Lam-Owned)
Γ ⊢ fn (x:T) -> e ⇒ T →[Owned] U : Owned

'r fresh   Γ, x :_(Borrowed) T ⊢ e ⇒ U : k   'r ∉ freeRegions(U)
──────────────────────────────────────────────────────────────  (Lam-Borrow)
Γ ⊢ fn &(x:T) -> e ⇒ T →[Borrowed] U : Owned

Γ ⊢ e₁ ⇒ T →[m] U : Owned    Γ ⊢ e₂ ⇐ T : (Owned if m=Owned else Borrowed)
──────────────────────────────────────────────────────────────────────────  (App)
Γ ⊢ e₁(e₂) ⇒ U : Owned
```

Closures capture by move or borrow (inferred from use); in codegen that is a function-pointer + captured-environment fat pointer. Function-argument positions are the first *contravariant* positions, as previously seen in §4.4 (variance).

---

## 6. Typing Context (type checker)

```
Γ ::= ε
    | Γ, x : T @ k @ home('r)      -- term var: type, kind, home scope region
    | Γ, a : k                     -- type variable
    | Γ, 'r                        -- region variable / parameter (with a depth)
    | Γ, 'a ≥ 'b                   -- assumed outlives edge (from a where-clause)
```

Each term binding records its **home region** (parameters to their frame, locals to their block) and the escape check (§10) reads it.

> In practice, the context `Γ` does not remove consumed term bindings.
> Affinity is enforced in the ownership environment `Δ` (§11), not here.

---

## 7. Kinding Rules

`Γ ⊢ T : k`, a pre-pass over type expressions.

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
─────────────────  (K-Borrow)
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
──────────────────────────  (K-Arrow) 
Γ ⊢ T₁ →[k] T₂ : Owned 

L a layout class
─────────────────  (K-Slot)
Γ ⊢ Slot<L> : Owned 
```

---

## 8. Bidirectional Typing

`Γ ⊢ e ⇒ T : k` (synthesis), `Γ ⊢ e ⇐ T : k` (checking). **Context threaded structurally, never split**; affinity conditions are *not* stated here (§11).

### 8.1 Subsumption 

```
Γ ⊢ e ⇒ T : k    k <: k'    T ⊑ T'
──────────────────────────────────────  (Sub)
Γ ⊢ e ⇐ T' : k'
```

`<:` is the subkinding of §2.2; `⊑` is the region-aware type relation of §4.3 (not
a general subtype relation).

### 8.2 Variables, literals, borrow, deref  [live]

```
(x : T @ k) ∈ Γ                       Γ ⊢ e ⇒ T : k
─────────────────  (Var)              'r = home-region of e
Γ ⊢ x ⇒ T : k                         ──────────────────────────  (Borrow)
                                      Γ ⊢ &e ⇒ &'r T : Borrowed
──────────────────  (Lit)               ( &mut e ⇒ &'r mut T : BorrowedMut )
Γ ⊢ n|b|() ⇒ Prim : Owned

Γ ⊢ e ⇒ &'r T : Borrowed  (or &'r mut)
──────────────────────────────────────  (Deref)
Γ ⊢ *e ⇒ T : kind_of(T)
```

`Var` returns the binding's stored kind and **never consumes** `x`. `&mut e`
additionally requires `e` to name a `mut` place (`MutBorrowOfImmutable`). A
borrow's type region is the referent's *home scope* (the per-scope, not
per-binding, deviation from `Calculus.md` §6.4).

### 8.3 Blocks & the escape check  [live]

```
'B fresh block region (depth = enclosing + 1)
Γ,'B ⊢ s̄ ⊣ Γ'    Γ' ⊢ e ⇒ T : k    freeRegions(T) names no region of depth ≥ 'B
──────────────────────────────────────────────────────────────────────────────  (Block)
Γ ⊢ { s̄; e } ⇒ T : k
```

### 8.4 Let, control flow, calls  [live]

```
Γ ⊢ e₁ ⇒ T : Owned    Γ, x : T @ Owned @ home(cur) ⊢ e₂ ⇒ U : k
────────────────────────────────────────────────────────────────  (Let-Owned)
Γ ⊢ (let x : T = e₁; e₂) ⇒ U : k

Γ ⊢ e₁ ⇐ Bool : Owned   Γ ⊢ e₂ ⇒ T₂ : k₂   Γ ⊢ e₃ ⇒ T₃ : k₃
T = join_region_ty(T₂,T₃)   k = k₂ ∨ k₃
────────────────────────────────────────────────────────────────  (If)
Γ ⊢ if e₁ then e₂ else e₃ ⇒ T : k

Γ ⊢ e ⇒ T : Owned   ∀ armᵢ: Γ, bindings(patᵢ,T) ⊢ eᵢ ⇒ Uᵢ : kᵢ
U = join over Uᵢ (region meet)   k = ∨ kᵢ
────────────────────────────────────────────────────────────────  (Match)
Γ ⊢ match e { arm* } ⇒ U : k

f : (T̄ ; 'ᾱ) → U declared   Γ ⊢ eᵢ ⇐ Tᵢ   σ = infer_region_subst(declared, actual)
each callee `where 'a >= 'b` holds under σ + Γ's assumptions
────────────────────────────────────────────────────────────────────  (Call)
Γ ⊢ f(ē) ⇒ region_subst(U, σ) : Owned
```

`let &x`/`let &mut x` desugar to a borrow-typed `let`. Branches agree on type
**modulo the region meet** `join_region_ty` (so an escape *through any branch* is
caught) and join their kinds. A call infers a per-parameter region substitution,
stamps the result with it (a GLB bounded below by the call-site scope (total)
§10), and checks the callee's region `where` clauses under it. Typeclass `where`
checking at call sites: see §12.1.

---

rest still todo
