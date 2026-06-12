# Implementation Plan

Incremental implementation of the kind, region, ownership, and typeclass
systems on top of the existing Sand compiler. Each step is scoped to
produce a working, tested compiler at its conclusion — no step leaves
the compiler in a broken intermediate state.

Steps are ordered by dependency. Within each step, Claude Code should
treat the listed scope as a hard boundary — do not implement anything
from a later step, even if it seems natural to do so.

---

## Guiding Principles

- **No step breaks existing tests.** Every step ends with `cargo test
  --workspace` passing. Regressions must be fixed before moving on.
- **Keep code cleanly documented.** Documentation should be there to
  add something only when the code does not explain itself. Keep the
  style consistent, and avoid implementation comments and remarks
  relating to the implementation process itself, unless they will be
  explicitly useful later.
- **Monomorphisation sits between TypedHIR and MIR.** MIR and LLVM
  codegen always see fully concrete types. No generics leak past the
  mono pass.
- **Borrow checking is phased.** `Borrowed` (shared, immutable) comes
  first and is fully validated before `BorrowedMut` is added.
- **InteriorMut is reserved.** The kind lattice is structured to
  accommodate it later, but no `InteriorMut` logic is implemented in
  this plan.
- **Lifetimes are explicit.** The programmer annotates all region
  variables. Elision rules are scaffolded (data structures in place)
  but not active.
- **Orphan rules are strict.** A typeclass `impl` is only legal if the
  crate owns the typeclass or the type being implemented for.
- **One concern per step.** Claude Code should not anticipate later
  steps or add "useful scaffolding" beyond the stated scope.

---

## Step 0 — Harden the Index Types ✅

**Status**: Complete. `TyKind`/`Ty(usize)` split and all typed newtypes
were already in place. `TyKind::Bottom`/`Ty::BOTTOM` removed as dead
code; `TyKind::Top`/`Ty::TOP` retained for polymorphic intrinsics.
All 391 tests pass.

**Follow-up (arena migration), complete**: the index newtypes are now
*arena references* rather than `usize` indices, matching the rustc
"`'tcx` everywhere" model:
- `Ty<'tcx>` = `&'tcx TyKind<'tcx>` (bumpalo arena; `Copy`, equality by
  pointer identity via structural interning).
- `FunRef<'tcx>`, `ModuleRef<'tcx>`, `EnumRef<'tcx>`,
  `OriginalVarRef<'tcx>` are `Copy` newtypes over `&'tcx` data allocated
  in per-type `typed_arena::Arena`s (destructor-running, for the
  `String`/`Vec`-owning structs). `UniqVar<'tcx>` carries an
  `OriginalVarRef<'tcx>`. Equality/hash by pointer identity; ordering by
  a stored monotonic registration `id` for deterministic `BTreeMap`
  iteration.
- `EnumDef` payloads use `Cell<Option<Ty>>` for the two-phase
  (register → resolve) registration of recursive enums, with a
  justified `unsafe impl Sync`.
- The arena lives in `Arenas`, owned by `CompileCtx` and abstracted away
  from every other module. `CompileCtx::initial()` owns the arena and
  frees it on `Drop`, fixing the LSP's unbounded per-keystroke arena
  leak; terminal compile-error paths and test helpers `mem::forget` the
  ctx to keep `'static`-typed borrowed results valid.

**Semantic note for later steps**: arena handles have *pointer
identity*, so a given enum/function compiled in two *separate*
`CompileCtx`s yields *different* handles. Comparisons that span
compilations (some tests did) must share one compilation.

**Goal**: Replace `usize` phantom indices with typed newtypes so that
mixing `EnumRef`, `FunRef`, `UniqVar`, and `Ty` indices is a compile
error at the Rust level.

**Why first**: Every subsequent step adds new index types (RegionVar,
TypeParam, TypeclassRef). Doing this on a clean foundation prevents an
entire class of bugs.

**Scope**:
- Introduce `Interned<T>` or typed newtype wrappers for `EnumRef`,
  `FunRef`, `UniqVar`, and `TyId` (the index into the type intern
  table)
- Update all construction and pattern-match sites across all passes
- All existing tests must pass unchanged
- **Remove `Ty::BOTTOM` / `TyKind::Bottom`**: it is dead code — nothing
  in the compiler ever produces it. Its semantic role (diverging
  expressions) will be filled by the `Never` kind in Step 4. Remove it
  now to avoid carrying dead weight.
- **Retain `Ty::TOP` / `TyKind::Top`**: actively used as the argument
  type for `println`/`print` intrinsics (the `type_eq (Top, _) → true`
  rule makes them accept any type). Replacement comes in Step 10 when
  typeclasses supply a proper `Display` constraint; see note there.

**Out of scope**: Any semantic change. This is purely a type-safety
refactor.

**Pre-existing detailed plan**: `.claude/plans/i-want-to-refactor-rippling-kitten.md`
contains the full file-by-file sub-steps for this refactor. Use it as
the concrete checklist for Step 0 rather than re-deriving the work.

---

## Step 1 — Type Parameters on Functions and Type Definitions ✅

**Status**: Complete. Generic functions and enums can be *declared*;
`T` in a signature/body resolves to an opaque `Ty::Param`. 420 tests
pass (16 new in `generics_tests.rs`). Instantiation/calls remain Step 2.

**Implementation notes / deviations from the original sketch:**
- **Syntax**: angle brackets — `def f<T, U>(...)`, `type Option<T> = ...`.
  Only the *declaration* takes parameters; uses are bare identifiers
  (`Foo<Int>` use-site syntax arrives with instantiation in Step 2).
  New grammar rule `type_params`.
- **`TypeParamId`** (`lang/types.rs`): a `usize` newtype, globally unique
  (not arena-backed — it has no associated heap data and must be `Copy`
  inside the arena-interned `TyKind::Param(TypeParamId)`).
- **One `TypeParam { id, name, range }` struct** is shared across HHIR /
  QHIR / TypedHIR rather than the planned HHIR-`{name,range}` →
  QHIR-`{id,name}` split. Reason: this codebase resolves type
  annotations to `Ty` during `build_ast` (in `build_type`), so the id
  must exist then — ids are assigned at build time, not in a later
  uniquify step.
- **Scope threading**: `CompileCtx` holds a `cur_type_params` name→id map
  (mirroring `set_build_module`). `begin_type_params` allocates ids and
  sets the scope; `build_type` resolves an in-scope name to
  `ctx.param_ty(id)` *before* falling back to enum lookup (so a parameter
  shadows a same-named enum). Enums re-enter their stored scope in the
  payload-resolution phase via `enter_type_param_scope`.
- **`Ty::Param` is opaque to every pass**: type-checking treats distinct
  ids as distinct types (pointer identity via interning); `is_copy` is
  `false`; `llvm_type` panics loudly if it is ever reached (it should
  not be until Step 3 mono erases all params before codegen).

**Goal**: Allow functions and type definitions to declare type
parameters. No constraints, no kinds, no instantiation checking yet —
just the syntactic and structural plumbing.

**Scope**:
- Grammar: add `type_params` to `function` and `type_alias` rules
- HHIR: extend `FunDef` and `EnumDef` to carry
  `Vec<TypeParam>` where `TypeParam = { name: String, range: Range }`
- QHIR: `TypeParam` becomes `{ id: TypeParamId, name: String }` after
  uniquification; `TypeParamId` is a new typed newtype
- TypedHIR: type parameters are present but unerased (mono pass will
  erase them)
- `Ty` gets a new variant `Ty::Param(TypeParamId)` for use sites
- `CompileCtx`: function and enum tables gain their type parameter lists
- All existing passes extended to thread type params through without
  inspecting them (they are opaque at this step)
- Existing tests still pass; new parse tests for generic syntax

**Out of scope**: Instantiation, substitution, kind annotations,
constraints, monomorphisation.

---

## Step 2 — Generic Instantiation and Type Substitution ✅

**Status**: Complete. Generic functions can be *called* (type arguments
inferred by unifying parameters against arguments), and generic enums
can be *used* (`Option<Int>` types, constructors, `match`, and
let-patterns). 463 tests pass (40 in `generics_tests.rs`). Generic
*bodies* still contain `Ty::Param`/`Ty::App` — Step 3 (mono) erases them
before codegen.

**Implementation notes:**
- **Use-site syntax**: angle brackets — `Option<Int>`, `Either<A, B>`
  (new grammar rule `type_application`, resolved in `build_type` with an
  arity check → `AstError::TypeArgArityMismatch`).
- **`TyKind::App(EnumRef, &[Ty])`** represents a generic enum applied to
  type arguments; interned by `CompileCtx::intern_app` so `Option<Int>`
  and `Option<Bool>` are distinct types. The base enum keeps its
  parametric payloads (`Param`); use sites substitute on the fly. This is
  the Step-2/Step-3 boundary the plan describes (intern now, specialise
  bodies later).
- **`passes/type_ast/generics.rs`**: `subst(ty, mapping)` (recurses into
  `Tuple`/`App`), `unify(declared, actual, &mut mapping)` (binds params,
  rejects conflicts), and `Subst = Map<TypeParamId, Ty>`. `Ty::has_param`
  decides whether a signature is generic.
- **Generic calls** (`infer.rs`): a call is generic iff its signature
  still mentions a parameter. Parametric args are inferred then unified;
  concrete args keep the existing `check` path (so bare tags still
  resolve). The return type is `subst`ituted with the solved mapping. A
  parameter forced to two types is a `FunctionCallTypeError`.
- **Generic constructors** (`infer_constructor`, shared by infer/check):
  type arguments are seeded from the expected type (`let x: Option<Int>
  = Option#None`) and/or inferred from the payload (`Option#Some(5)`). An
  underdetermined constructor (bare nullary, no annotation) is
  `CannotInferTypeArguments`.
- **Generic `match`/let-patterns** (`check.rs`): an `App` scrutinee is
  matched like its base enum; the `enum_instantiation` helper builds the
  param→arg substitution, and variant payload types are `subst`ituted so
  bindings get concrete types (`Option<Int>#Some(x)` ⟹ `x : Int`).

**Goal**: Type-check calls to generic functions and uses of generic
types, performing substitution to produce fully typed expressions with
`Ty::Param` replaced by concrete types.

**Scope**:
- Implement `subst(ty: Ty, mapping: Map<TypeParamId, Ty>) -> Ty`
- In `type_ast/infer.rs`: when inferring a call to a generic function,
  unify the parameter types against argument types to produce the
  substitution mapping, then substitute into the return type
- In `type_ast/check.rs`: propagate the expected type to resolve
  ambiguous generic instantiations
- `CompileCtx`: intern generic enum instantiations
  (`Option<Int>`, `Option<Bool>` become separate interned types)
- Error cases: arity mismatch in type parameters, unsolvable
  unification
- New typecheck tests for generic functions and generic enum uses

**Out of scope**: Kind annotations on type parameters, monomorphisation,
constraints/where clauses.

---

## Step 3 — Monomorphisation Pass ✅

**Status**: Complete. `passes/mono.rs` lowers a generic `TypedProgram`
to a fully concrete one; generic functions and enums compile through MIR
and LLVM and execute correctly end-to-end. 455 tests pass (49 in
`generics_tests.rs`, incl. interpreter and real LLVM
`examples/generic_*.sand` runs).

**Implementation notes:**
- **`passes/mono.rs`** runs at the end of `compile_hir` (after ownership),
  so every later pass sees only concrete types. For a program with no
  generics it is behaviour-preserving (non-generic functions keep their
  `FunRef`, so the entry point and core library are untouched).
- **Worklist with memoisation**: every non-generic function is a root;
  generic functions/enums are specialised on demand per distinct
  instantiation. `fn_instances`/`enum_instances` memoise (and are inserted
  *before* the body is rewritten, so recursive generics terminate).
- **Specialisations**: generic functions get a fresh mangled `FunRef`
  (`id$Int`) via `CompileCtx::register_mono_function`; generic enums get a
  fresh `EnumRef` (`Option$Int`) via `register_enum` with substituted
  payloads. `mono_ty` substitutes `Param` and rewrites `App` → the
  specialised enum's `Enum`.
- **Recovering call instantiations**: a call's callee instantiation is
  recovered by unifying the callee's parametric signature against the
  call's argument/result types in their *original*, `App`-preserving form
  (with the caller's own substitution applied), then fully monomorphising
  the recovered arguments. This correctly handles return-only type
  parameters and generic-returning-generic calls.
- **Output invariant**: no `TypedFunction` has `type_params`, and no
  `Ty::Param`/`Ty::App` survives (a white-box test asserts the former;
  MIR/LLVM execution would fail loudly on the latter).

**Goal**: Eliminate all `Ty::Param` from the program by specialising
every generic function and type for each concrete instantiation it is
used with, producing a `TypedProgram` with no generics.

**Scope**:
- New pass `passes/mono.rs`: `TypedProgram -> TypedProgram`
- Traverses the program starting from non-generic entry points,
  collecting all generic instantiations encountered
- For each unique instantiation, clones the generic function/type body
  and applies the substitution
- Renames monomorphised copies to avoid collisions
  (e.g. `map__Int__Bool` internally)
- Output: a `TypedProgram` where no `FunDef` or `EnumDef` has type
  parameters and no `Ty::Param` appears anywhere
- `explicate_control` and `llvm_codegen` are unchanged
- New tests: generic functions compile and execute correctly end-to-end

**Out of scope**: Kind-aware monomorphisation, lifetime parameters.

---

## Step 4 — Kind System (Owned and Never only)

> **Pre-step note — migration from `Top`/`Bottom`:**
> `Ty::TOP` was kept in Step 0 as a live intrinsic escape hatch;
> `Ty::BOTTOM` was removed as dead code. In this step, `Never` is the
> kind of uninhabited/diverging types. Do NOT conflate it with the old
> `Bottom` type — `Kind::Never` is a *kind* (classifies types), not a
> type itself. The kind of a diverging expression (e.g. an infinite
> `while true` loop) is `Never`; its *type* is still inferred
> contextually or left as a fresh type variable.
>
> **Pre-step note — ownership pass migration:**
> The existing `passes/ownership/mod.rs` uses `ty.is_copy()` (a boolean)
> to decide whether to consume or retain variables. This step introduces
> `Kind` as a first-class concept. The ownership pass must be updated to
> work in terms of kinds rather than the `is_copy()` shortcut — `is_copy`
> should become a derived query (`kind == Owned && impl Copy`) eventually,
> but for this step it suffices to confirm that `is_copy()` and `Owned`
> agree for all current types.

**Goal**: Introduce the kind system with the two uncontroversial kinds
first. Every existing type is `Owned`. `Never` is the uninhabited kind,
useful for diverging expressions.

**Scope**:
- New `Kind` enum in `lang/types.rs`:
  ```rust
  enum Kind { Owned, Never }
  // Borrowed and BorrowedMut added in Step 6
  // InteriorMut reserved for future
  ```
- Subkinding relation `fn is_subkind(k1: Kind, k2: Kind) -> bool`
- Kind join `fn join_kind(k1: Kind, k2: Kind) -> Kind`
- New kinding judgment `fn kind_of(ty: &Ty, ctx: &CompileCtx) -> Kind`
  as a pre-pass over type expressions; all current types return `Owned`
- Extend `TypedExpr` to carry `kind: Kind` alongside `ty: Ty`
- `TypeEnv` entries gain `kind: Kind`
- Subsumption rule in the type checker extended to check subkinding
- All existing types assigned `Owned`; `Never` assigned to the type of
  diverging expressions (infinite loops, future `panic`)
- All existing tests pass

**Out of scope**: `Borrowed`, `BorrowedMut`, `InteriorMut`, region
variables, kind annotations on type parameters.

---

## Step 5 — Kind Annotations on Type Parameters

**Goal**: Allow type parameters to carry kind annotations
(`+a : Owned`) and variance annotations (`+`, `-`, `∅`). Validate
that type constructor bodies are consistent with declared variance.

**Scope**:
- Grammar: extend `type_param` to carry optional `variance_ann` and
  `kind_ann`
- `TypeParam` struct gains `variance: Variance` and `kind: Kind`
- Kinding rule `K-App`: when a generic type is applied, check that
  argument kinds match declared parameter kinds
- Variance checker: for each type constructor, verify that the declared
  variance of each parameter is sound given the parameter's kind and
  the positions it appears in the body
- Default variance rules (from the calculus §2.1) applied when no
  annotation is given
- New error variants for kind mismatch and unsound variance declaration
- New tests for kind annotation acceptance and rejection

**Out of scope**: `Borrowed`/`BorrowedMut` kinds, region parameters.

---

## Step 6 — Region Variables and the Region Context

**Goal**: Introduce region variables as first-class entities. Add
lifetime syntax (`'r`) to the grammar and thread region variables
through the compiler infrastructure. No borrow types yet — this step
is purely structural plumbing for regions.

**Scope**:
- New `RegionVar` typed newtype (similar to `UniqVar`)
- New `Region` enum: `Region::Var(RegionVar) | Region::Static`
- Region context in `TypeEnv`: `Vec<RegionConstraint>` where
  `RegionConstraint = Outlives(RegionVar, RegionVar)`
- Grammar: `lifetime = @{ "'" ~ identifier }` and `region_type =
  { type_ ~ "@" ~ lifetime }`; `'static` as a reserved lifetime
- HHIR/QHIR: lifetime syntax parses and round-trips; no semantics yet
- `Ty` gets new variant `Ty::Region(Region)` (used in type position
  for region ascription `T @ 'r`)
- Uniquify: region variables get fresh `RegionVar` ids at their
  binding sites (function signatures, borrow bindings)
- `CompileCtx` gains a region intern table
- Outlives constraint solver stub: data structures in place, solver
  returns trivially-satisfied for now (used in Step 8)
- All existing tests pass

**Out of scope**: `Borrowed`/`BorrowedMut` types, borrow expressions,
actual constraint solving.

---

## Step 7 — Borrowed Kind and Shared Reference Types

> **Pre-step note — `&` operator conflict:**
> Sand currently uses `&` as the boolean AND operator. The calculus
> (§8.3) frees `&` for borrow syntax by renaming bitwise AND to `band`.
> Before introducing borrow expressions, this operator must be renamed.
> Proposed rename: boolean AND `&` → `&&` (conventional two-char form),
> keeping `|` as boolean OR unchanged. The grammar change is in this
> step; all existing tests using `&` for boolean AND must be updated.
> Decide the exact replacement symbol here before touching borrow syntax.

**Goal**: Add `Borrowed 'r` as a kind and `&'r T` as a type. Implement
shared (immutable) borrow expressions and borrow let-bindings. This is
the first step where the ownership semantics change meaningfully.

**Scope**:
- `Kind` gains `Borrowed(RegionVar)`
- `Ty` gains `Ty::Ref(Region, Box<Ty>)` for `&'r T`
- Kinding rules `K-Borrow`: `&'r T : Borrowed 'r`
- Grammar: `borrow_expr = { "&" ~ expression }`,
  `borrow_type = { "&" ~ lifetime? ~ type_ }`;
  free `&` from bitwise AND (rename to `band`)
- `let &x : T = e` binding form in grammar, HHIR, QHIR, TypedHIR
- Typing rules `Let-Borrow` and `Var-Borrow`: borrowed variables
  remain in context after use (not consumed)
- Block rule: each block introduces a fresh `RegionVar`; result type
  must not mention that region (escape check)
- Subkinding: `Owned <: Borrowed 'r` (implicit reborrow coercion)
- `TypeEnv` entries: `Owned` entries removed on use, `Borrowed`
  entries kept
- New borrow tests: basic borrow, multiple borrows of same value,
  reborrow coercion, escape check rejection
- Ownership pass extended to understand `Borrowed` entries

**Out of scope**: `BorrowedMut`, aliasing exclusivity, mutable borrow
expressions, region inference.

---

## Step 8 — Region Constraint Solving and Lifetime Safety

> **Pre-step note — `@` grammar conflict:**
> The calculus uses `T @ 'r` for region ascription. In pest, `@` is a
> reserved metacharacter for atomic rules. `T @ 'r` will need careful
> grammar engineering — likely wrapping it in a named rule with quoted
> `"@"` literal — to avoid silently misparse. Confirm the pest handling
> works before accepting region ascription syntax.

**Goal**: Activate the region constraint solver. Enforce that borrows
do not outlive their source region. Validate the outlives relation
(`'r ≥ 's`) in function signatures.

**Scope**:
- Implement the outlives constraint solver: given a set of
  `Outlives(r1, r2)` constraints and a set of region variables with
  known scopes, check satisfiability
- Block typing now actively checks `'r ∉ freeRegions(T)` on the
  result type, producing a `RegionEscape` error if violated
- Function signatures with explicit lifetime parameters (`∀'r.`)
  checked for well-formedness
- `where 'r >= 's` constraints in function signatures parsed, stored,
  and checked at call sites
- Lifetime elision scaffolding: `ElisionRule` enum with variants
  for the simple cases (single input region → output region), data
  structures in place but elision not yet active (explicit annotations
  still required everywhere)
- New tests: escape check fires correctly, outlives constraints
  accepted and rejected correctly

**Out of scope**: Region inference/elision activation, `BorrowedMut`.

---

## Step 9 — Mutable Borrow Kind and Exclusive Reference Types

**Goal**: Add `BorrowedMut 'r` as a kind and `&'r mut T` as a type.
Enforce the exclusivity invariant: while a `BorrowedMut` borrow is
live, no other borrow (`Borrowed` or `BorrowedMut`) of the same place
may exist.

**Scope**:
- `Kind` gains `BorrowedMut(RegionVar)`
- `Ty` gains `Ty::RefMut(Region, Box<Ty>)`
- Kinding rules `K-BorrowMut`
- Grammar: `&mut expression` and `&'r mut T`
- `let &mut x : T = e` binding form through all IR layers
- Typing rules `Let-BorrowMut`, `Var-BorrowMut`
- Subkinding: `Owned <: BorrowedMut 'r`
- **Exclusivity check**: when a `BorrowedMut 'r` borrow of place `p`
  is introduced, the context must contain no other live borrow of `p`
  (neither `Borrowed` nor `BorrowedMut`). This is a dataflow check
  over `TypeEnv`.
- Assignment through `BorrowedMut`: `x = e` where `x :_(BorrowedMut)
  T` is now legal (previously only `let mut` allowed assignment)
- Invariance enforcement: `BorrowedMut` type parameters are invariant;
  the kind checker rejects `+a : BorrowedMut`
- New tests: exclusive mutable borrow, rejection of aliased mutable
  borrow, mutable borrow and shared borrow cannot coexist

**Out of scope**: `InteriorMut`, two-phase borrows, borrow splitting.

---

## Step 10 — Typeclass Declarations and Instance Resolution

> **Pre-step note — retire `Ty::TOP`:**
> `println`/`print` currently use `Ty::TOP` as their argument type (an
> escape hatch since they accept any printable value). Once a `Display`
> typeclass exists, replace these intrinsic signatures with a proper
> `T: Display` constraint and remove `Ty::TOP` / `TyKind::Top` entirely.
> If `Display` is not defined in this step, carry the note forward to
> whichever step defines it.

**Goal**: Add `typeclass` and `impl` declarations. Implement instance
resolution during type checking. Enforce strict orphan rules.

**Scope**:
- Grammar: `typeclass_decl` and `impl_decl` as top-level items
  (§8.8–8.9 of the calculus document)
- HHIR: new top-level items `TypeclassDecl` and `ImplDecl`
- QHIR: typeclass and impl names resolved to `TypeclassRef` and
  `ImplRef` typed newtypes
- `CompileCtx` gains:
  - typeclass table: `TypeclassRef → TypeclassDef`
    (methods + required superclasses)
  - instance table: `(TypeclassRef, Ty) → ImplDef`
  - coherence checker: rejects duplicate instances
- Orphan rule checker: for each `impl TC for T`, either `TC` or `T`
  must be defined in the current compilation unit
- Instance resolution in `type_ast/infer.rs`: when a method is called
  on a value, look up the instance for that value's type and typeclass
- `where` clause parsing and storage on `FunDef` (checked at call
  sites, not yet used for resolution)
- Default method implementations in typeclass bodies
- Superclass requirements (`requires`): implementing a typeclass
  requires all `requires` constraints to also be satisfied
- Monomorphisation pass updated to specialise typeclass method calls
- New tests: basic typeclass declaration, impl, method call, orphan
  rule rejection, missing impl rejection, superclass requirement
  enforcement

**Out of scope**: Multi-parameter typeclasses, functional dependencies,
higher-kinded typeclasses (`Functor`, `Monad` — these require Step 11).

---

## Step 11 — Higher-Kinded Type Parameters

**Goal**: Allow type parameters of kind `Owned → Owned` (type
constructors), enabling `Functor`, `Applicative`, and `Monad` to be
expressed as typeclasses.

**Scope**:
- `Kind` gains `Arrow(Box<Kind>, Box<Kind>)` for type constructor kinds
  (e.g. `Owned → Owned`)
- Type parameter declarations can now specify constructor kinds:
  `F : Owned → Owned`
- Kinding rule `K-App` generalised to handle constructor application
- Kind inference for type constructor applications
- `Functor`, `Applicative`, and `Monad` typeclasses expressible in
  Sand source (in `core.sand` or standard library)
- Instance resolution extended to handle HKT constraints:
  `where F : OwnedFunctor` unifies `F` against a type constructor
- Monomorphisation extended: HKT instantiations are specialised like
  regular generic instantiations
- `Clone` and `Copy` typeclasses defined in `core.sand`; primitive
  types (`Int`, `Bool`, `Unit`) get auto-derived `Copy` instances
- New tests: `OwnedFunctor` instance for `Option`, `Monad` instance
  for `Option`, `fmap` and `bind` calls compile and execute

**Out of scope**: `BorrowedFunctor` (requires borrows to be stable,
can be added after this step as an additive extension), `InteriorMut`.

---

## Step 12 — `box` Intrinsic and Heap Allocation

> **Pre-step note — MIR borrow representation:**
> After Step 7, MIR locals can have type `&'r T` (a borrowed reference),
> which lowers to a pointer in LLVM rather than an owned value. The plan
> says MIR always sees concrete types (no generics), but it does not yet
> specify how borrow kinds flow through MIR locals and into codegen.
> Resolve before this step: either MIR gains a borrow annotation on
> locals, or the ownership pass strips borrows back to the underlying
> type before MIR lowering. This decision affects how `Box<T>` values and
> their drop points are represented in MIR.
>
> **Pre-step note — existing heap allocation:**
> The current LLVM codegen unconditionally heap-allocates enum payloads
> (via `malloc`, no `free`). Once `box` is the explicit allocation
> primitive, the implicit heap allocation in `emit_aggregate` should be
> replaced: non-recursive enums get a stack-allocated tagged union;
> recursive enums require `Box` (and are now an error without it).
> This is the "phase 2" from the earlier stack-vs-heap discussion.

**Goal**: Add `box(e)` as the explicit heap allocation primitive,
producing `Box<T> @ 'static`.

**Scope**:
- Grammar: `box_expr = { "box" ~ "(" ~ expression ~ ")" }`; `"box"`
  added to keyword list
- `Ty` gains `Ty::Box(Box<Ty>)` or `Box` as a built-in generic type
  with kind `Owned → Owned`
- Typing rule `Box`: `Γ ⊢ e ⇒ T : Owned` implies
  `Γ ⊢ box(e) ⇒ Box<T> @ 'static : Owned`
- LLVM codegen: `box(e)` lowers to `malloc` + store (replacing the
  existing ad-hoc heap allocation for enum payloads where applicable)
- `Drop` trait scaffolded: `Box<T>` has a compiler-generated `drop`
  that calls `free`; called at end of owning scope
- Ownership pass: `Box<T>` values are owned and consumed on move;
  free is inserted at the drop point in MIR
- New tests: `box` expression typechecks, heap-allocated value
  accessible, value freed at end of scope (no leak under valgrind/
  address sanitizer)

**Out of scope**: Custom `Drop` implementations, `Rc`, `Arc`.

---

## Step 13 — Lambda Expressions and First-Class Functions

**Goal**: Add lambda expressions as values. Functions become first-class
— they can be passed as arguments, stored in data structures, and
returned.

**Scope**:
- Grammar: `lambda_expr = { "fn" ~ lambda_param ~ "->" ~ expression }`
  (consuming and borrowing variants per calculus §3.1)
- `Ty` gains `Ty::Fn(Box<Ty>, Box<Ty>, FnKind)` where `FnKind`
  distinguishes consuming (`→[Owned]`) from borrowing (`→[Borrowed 'r]`)
- HHIR/QHIR: `Expression::Lambda { param, body, kind }`
- Closure capture analysis: determine which variables from the enclosing
  scope are captured, and whether each is captured by move or borrow
- TypedHIR: `Lambda` carries its capture list with kinds
- Monomorphisation: lambdas are monomorphised at their capture sites
- LLVM codegen: lambdas lower to a function pointer + captured
  environment struct (fat pointer representation)
- Typing rules `Lam-Owned` and `Lam-Borrow` from the calculus
- New tests: lambda passed as argument, lambda returned from function,
  closure over owned value (moves it), closure over borrowed value

**Out of scope**: Recursive lambdas, `move` closures as a keyword
(capture mode is inferred from usage), async.

---

## Step 14 — `Clone` and `Copy` Typeclass Integration

**Goal**: Connect the `Clone`/`Copy` typeclasses (defined in Step 11)
to the ownership pass so that `Copy` types are implicitly duplicated
and non-`Copy` types require explicit `.clone()`.

**Scope**:
- Ownership pass: when a variable of `Copy` kind is used, it is not
  consumed from the context (implicit copy)
- When a variable of non-`Copy`, non-`Clone` type is used twice, a
  clear error is produced suggesting `.clone()`
- When `.clone()` is called, the ownership pass treats the result as a
  fresh owned value
- Primitive types (`Int`, `Bool`, `Unit`) are confirmed as `Copy`
- User-defined types are `Copy` only if all fields are `Copy` (checked
  by the kind checker when deriving `Copy`)
- Enum types are `Copy` only if all variants' payloads are `Copy`
- New tests: `Copy` type used twice without clone, non-`Copy` type
  used twice produces error, `.clone()` permits second use

**Out of scope**: `derive` macros, auto-derive syntax.

---

## Step 15 — `where` Clause Constraint Checking

**Goal**: Activate `where` clause checking at call sites. Functions
with typeclass constraints can only be called with types that satisfy
those constraints.

**Scope**:
- At each call site to a function with `where T : TC` constraints,
  verify that the concrete type substituted for `T` has an `impl TC`
  in scope
- At each `impl TC for T` that has `requires` superclasses, verify
  all required instances exist
- Error messages name the missing instance and the constraint that
  required it
- `where 'r >= 's` outlives constraints activated at call sites
  (previously stored but not checked)
- New tests: constraint satisfaction accepted, missing instance
  rejected, superclass missing rejected, outlives constraint checked

**Out of scope**: Constraint inference, implicit instance search beyond
direct lookup.

---

## Appendix: Deferred Features

The following are explicitly out of scope for this plan and should not
be anticipated in any step's implementation:

- `InteriorMut` kind and `Cell`-like types
- Lifetime elision / region inference (scaffolded in Step 8, not
  activated)
- Two-phase borrows
- Borrow splitting (borrowing two fields of a struct independently)
- `derive` macro system
- Async / `Future`
- Multi-parameter typeclasses
- Recursive lambdas / `fix`
- Pattern matching in lambda parameters
- String and array primitives
- Foreign function interface

---

## Dependency Graph

```
Step 0  (index types)
  └─ Step 1  (type param syntax)
       └─ Step 2  (generic instantiation)
            └─ Step 3  (monomorphisation)
                 ├─ Step 4  (kind system: Owned + Never)
                 │    └─ Step 5  (kind annotations + variance)
                 │         └─ Step 6  (region variables)
                 │              └─ Step 7  (Borrowed kind + &T)
                 │                   └─ Step 8  (constraint solving)
                 │                        └─ Step 9  (BorrowedMut + &mut T)
                 └─ Step 10 (typeclass decl + impl)
                      └─ Step 11 (HKTs: Functor, Monad)
                           └─ Step 12 (box + heap)
                                └─ Step 13 (lambdas)
                                     └─ Step 14 (Clone/Copy integration)
                                          └─ Step 15 (where clause checking)
```

Steps 4–9 (ownership/region track) and Steps 10–11 (typeclass track)
both depend on Step 3 but are independent of each other. They can be
developed in parallel on separate branches and merged before Step 12.
