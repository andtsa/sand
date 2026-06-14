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

## Implementation Roadmap (remaining work)

The concrete, ordered sequence to implement from here. Full scope for each item is
in its section further down; this is the execution order + dependencies.

**Foundation — ✅ DONE:** Steps 0–9 (generics → monomorphisation → kinds → regions
→ shared & mutable borrows) and the Usability pass (deref, call-site region
inference).

Legend: dependency in (parens); `path` = primary files to touch.

### Phase 1 — Reference soundness & real pointers
*Independent of the typeclass/memory tracks; do first — it closes the known
escape-check soundness gaps and unblocks the real pointers that closures and
write-through need. R1 → R2 → R3.*

1. **Ref-Rep R1 — sound region checker** (type-level; runtime still erases).
   Re-annotate reference types with real regions; `freeRegions(type)` escape check
   at block close + function return; per-call `meet` replacing `region_erase`.
   *Done when:* the eight `typecheck_passes`/`fails` cases in the Ref-Rep step pass.
   `passes/type_ast/`, `compiler/context/`.
2. **Ref-Rep R2 — pointer representation in the back-end** (R1). Stop erasing
   `Ref`/`RefMut` in mono; `Place` projections + `RValue::Ref` in MIR; explicate +
   LLVM `place_address`; store-based interpreters. Behaviour-preserving.
   `passes/mono.rs`, `ir_types/mir.rs`, `passes/explicate_control`, `llvm_codegen.rs`.
3. **Ref-Rep R3 — write-through** `*r = e` (R2). Place-LHS assignment; `&mut`-deref
   is a writable place, `&`-deref read-only (error); lower to a store.

### Phase 2 — Typeclasses
*Foundation for the whole `Heaped` memory system. 4 → 5.*

4. **Step 10 — typeclass declarations + impl + resolution.** `typeclass`/`impl`
   grammar + IR; instance table + coherence + strict orphan rules; method-call
   resolution; `where` parsing/storage (checked in Step 15).
5. **Step 11 — higher-kinded type params** (Step 10). `Kind::Arrow`;
   `F : Owned → Owned`; `Functor`/`Applicative`/`Monad` in `core.sand`; **define the
   `Clone`/`Copy` (and `Display`) typeclasses** (integration is Step 14; lets
   `Ty::TOP` be retired).

### Phase 3 — Memory / allocation (the `Heaped` system)
*Depends on Phase 2 for the typeclass machinery. A → B → C → D → E. A and B touch no
typeclasses and may start before Step 10 if convenient.*

6. **Memory A — substrate.** `Ptr<T>` primitive; `drop_in_place` intrinsic; `Heaped`
   lang-item registration. (`malloc`/`free` = library over `Ptr` + FFI.)
7. **Memory B — drop / RAII** (A). Formal drop insertion (reverse-decl); the
   conditional-move **completing drop** (refine `OwnershipEnv::merge`); wire
   `drop_in_place`. `passes/ownership/`.
8. **Memory C — `Heaped` (Unique)** (Step 10 + A + B). The `Heaped`/`HeapedUnique`
   typeclasses; recursive-type legality; rep `T` as `Unique<NodeOf<T>>`
   (nullable-pointer niche); lower `#C`→`alloc`, `match`→`borrow`, `&mut`→
   `borrow_mut`, drop→`release`; core-lib Box/Unique strategy. **Replaces Step 12.**
9. **Memory D — `reuse`** (C). `take`/`reuse`/`Slot<L>` + husk as-pattern + layout
   check; consuming-match drain. Lexical-only.
10. **Memory E — `Heaped` (Shared)** (C + A). `HeapedShared` + `.share()` + refcount
    strategy (counter via a raw `Ptr` write). Affine handles.

### Phase 4 — Higher-level features
*Depend on Phases 2–3.*

11. **Step 13 — lambdas / first-class functions** (Memory C, for closure
    environments). Lambda grammar/IR; capture analysis; fat-pointer codegen; the
    deferred **variance follow-up** (contravariant function-argument positions).
12. **Step 14 — `Clone`/`Copy` integration** (Step 11). Wire the typeclasses into
    the ownership pass (implicit copy for `Copy`; explicit `.clone()` otherwise).
13. **Step 15 — `where`-clause checking at call sites** (Step 11). Verify typeclass
    constraints + superclass requirements per call. ~~revive `where 'r >= 's`~~ —
    **the region half is done** (call-site region inference + `where 'r >= 's`
    checking, pulled forward; see the Step 8b note). Only the typeclass-constraint
    half remains here.

### Deferred (not in this roadmap)
The `fip`/`fbip` grade system + grade-polymorphism, first-class threadable `Slot`,
the safe `InteriorMut` kind, user-authored custom `Heaped` strategies, the general
`unsafe` model, reference cycles, and OOM/fallible allocation. See the Appendix and
the memory-model design log.

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

> **Status**: ✅ Complete. The `{Owned, Never}` lattice is in place and
> full divergence is implemented: a `while true` loop has kind `Never`
> and inhabits any type, compiling (via `Terminator::Unreachable`) and
> running end-to-end through LLVM. 468 tests pass (12 new in
> `kind_tests.rs` + `examples/divergence.sand`).
>
> **Calculus alignment**: implements the `{Owned, Never}` fragment of
> §1.2 (`Kind::is_subkind`: `a==b ‖ a==Never`), §1.4 (`Kind::join`),
> §5 (`CompileCtx::kind_of` → `Owned` for every current type), and §6.1
> subsumption (the only non-trivial subkind case, `Never <: k`, is
> realised as the `coerce_never` coercion in `check`; `if`/`match` merge
> branch kinds with `join`). Borrow modes, regions, and kind annotations
> are deferred to Steps 5–9 exactly as the lattice anticipates.
>
> **Implementation notes / deviations:**
> - `Kind { Owned, Never }` lives in `lang/types.rs` with `is_subkind` and
>   `join`. `typed_hir::Expr` carries `kind` (excluded from `Eq`/`Hash`
>   like `ty`); `TypeEnv` entries carry it too (inert `Owned` for now —
>   scaffolding for Step 6 borrowed bindings).
> - **Full divergence** (chosen over the lattice-only option): `while true`
>   → `Never`; in checking mode a `Never` expression coerces to any
>   expected type (re-typed via `coerce_never`); `if`/`match` take the
>   result type from non-diverging branches.
> - **Codegen**: this required touching `explicate_control` (the nearby
>   plan steps kept it untouched): `lower_tail`/`lower_assign` route a
>   `Never` expression through `lower_effect` with an `Unreachable`
>   continuation, so no value is fabricated for a path that never returns.
>   A reachable-but-unexecuted divergence (`if true then 7 else loop`)
>   therefore compiles and runs.
> - **Ownership pass** left on `is_copy()` per the pre-step note; kinds add
>   no constraints yet, so they stay consistent.

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

## Step 5 — Kind Annotations on Type Parameters ✅

> **Status**: Complete. Type parameters carry optional variance (`+`/`-`)
> and kind (`: Owned`/`: Never`) annotations; the kind-application rule
> and a variance-soundness check are enforced at the declaration/use
> sites. 477 tests pass (9 new in `generics_tests.rs`).
>
> **Calculus alignment**: implements §2.1 variance, §2.2 parameterised
> type-constructor parameters (`v̂ a : k`), grammar §8.4 (`variance_ann`,
> `kind_ann`, `type_param`), and the §5 `K-App` kind-argument check.
> `kind_ann` is `Owned`/`Never` only — the borrow kinds (which need a
> region) arrive in Steps 7–9.
>
> **Implementation notes / scope reality:**
> - Because monomorphisation erases all generics and the system has no
>   subtyping between concrete types, variance/kind annotations are
>   **declaration- and use-site checks**, not use-site coercions. They
>   have real teeth only where a concrete error can be produced:
>   - **`K-App`** (`build_type`, `type_application`): each argument's kind
>     must satisfy the parameter's declared kind → `KindArgMismatch`. With
>     only `Owned` types this fires for a `: Never` parameter.
>   - **Variance soundness** (`check_variance`, after enum payloads
>     resolve): every position in the current grammar is a *producer*
>     (covariant) position, so the only unsound declaration is
>     `Contravariant` on a *used* parameter → `UnsoundVariance`. Covariant
>     and invariant are always sound; a phantom (unused) parameter accepts
>     any variance.
> - **Defaults** (§2.1): unannotated variance → `Covariant` (the §2.1
>   result for producer-only positions), unannotated kind → `Owned`.
> - `TypeParam` gained `variance: Variance` and `kind: Kind`; a new
>   `TypeParamSpec` carries the parsed-and-defaulted annotation into
>   `begin_type_params`. `Variance` lives in `lang/types.rs`.
> - The full producer/consumer polarity engine (with variance
>   *composition* through nested applications) is deferred until function
>   types (Step 13) introduce the first consumer positions; today the
>   analysis is exact because contravariant positions cannot occur.

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

## Step 6 — Region Variables and the Region Context ✅

> **Status**: Complete. Lifetime syntax (`'r`, `'static`) parses, `T @ 'r`
> is a distinct interned type that round-trips, and monomorphisation
> erases regions so MIR/LLVM are unchanged. 484 tests pass (7 new in
> `region_tests.rs`).
>
> **Calculus alignment**: implements §1.1 regions (`Region::Var | Static`,
> `RegionConstraint` as the outlives relation) and §2.3 region ascription
> `T @ 'r : Owned`. Borrow types/expressions and actual constraint
> solving are deferred to Steps 7–8 as scoped.
>
> **Implementation notes / deviations:**
> - **`TyKind::Region(Ty, Region)`** (ascription carries its inner type)
>   rather than the plan's bare `Ty::Region(Region)`, which would lose
>   `T`. `is_copy`/`has_param`/`compatible`/`Display` recurse through it.
> - **Grammar**: `lifetime = @{ "'" ~ identifier }`; the `@ 'r` suffix is
>   parsed on a renamed `core_type` (`type_ = { core_type ~ ("@" ~
>   lifetime)? }`) to keep the PEG free of the left recursion that a
>   literal `region_type = type_ ~ "@" ~ lifetime` would introduce.
> - **Implicit region binding** (per "elision scaffolded, not active"):
>   regions bind on first use in `T @ 'r`; explicit `<'r>` region
>   parameters are deferred. Regions are currently **interned globally by
>   name** (sound because they have no semantics yet); per-declaration
>   region scoping arrives with the constraint solver in Step 8.
> - **Region ascription is a distinct type** from the bare type (faithful
>   to §2.3): `Int @ 'r ≠ Int`. Since there is no ascription *expression*
>   yet, region-ascribed values cannot be created from literals — regions
>   appear only in signatures and are erased by mono.
> - **Deferred to Step 8** (noted): the `Vec<RegionConstraint>` region
>   context in `TypeEnv`. Adding it now would convert `TypeEnv` from a map
>   alias to a struct and ripple through ~10 sites for an always-empty
>   field; its first real use (constraint generation + solving) is Step 8.
>   `RegionConstraint` and the `solve_outlives` stub (trivially satisfied)
>   are in place now per the "data structures in place" requirement.

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

> **Pre-step note — `&` operator conflict: ✅ DONE.**
> The old `&` operator was *overloaded* — bitwise AND on `Int` and logical
> AND on `Bool` (it accepted any `left == right`). It has been **split** to
> free single `&` for borrow syntax:
> - **`&&`** = bitwise AND on `Int` (`Bop::BitAnd`);
> - **`and`** = logical AND on `Bool` (`Bop::And`, now a keyword).
>
> The two are type-restricted (`&&` rejects `Bool`, `and` rejects `Int`);
> both lower to LLVM `and`. `|` (OR) and `xor` are left overloaded — they
> don't conflict with borrows, and `|` is also the tag-union / match-arm
> separator. Single `&` is now a parse error and is reserved for `&'r T`.
> Updated: `core.sand` (`n & 1` → `n && 1`, genuinely bitwise) and all
> existing tests/examples (`&` on `Bool` → `and`). New `operator_tests.rs`
> (9 tests) locks in the split. Borrow syntax (`&` / `&mut`) is added below.

> **Status**: ✅ Complete. The `Borrowed` kind, `&'r T` reference types,
> `&e` borrow expressions, and `let &x = e` borrow bindings all compile
> and run end-to-end; borrows don't consume their referent. 507 tests
> pass (14 new in `borrow_tests.rs`). The block-region escape check is
> deferred to Step 8 (per the confirmed scope decision).
>
> **Calculus alignment**: §1.1/§1.2 (`Kind::Borrowed(Region)`,
> `Owned <: Borrowed`), §2.3 (`TyKind::Ref(Region, Ty)`, `K-Borrow`),
> §3.2/§8.7 (`borrow_expr`, `borrow_type`), §6.2 (`Var-Borrow`:
> borrowing does not consume).
>
> **Implementation notes / deviations:**
> - **Affine tracking stays in the ownership pass** (where move-tracking
>   already lives), not the type checker: `&e` and `let &x = e` borrow
>   without consuming, so a non-`Copy` referent stays usable. `&T` is
>   `Copy` (immutable, freely shareable).
> - **Monomorphisation erases borrows**: `&'r T → T`; `&e` and `let &x`
>   lower transparently to the inner value. Sound because shared borrows
>   are immutable and nothing is freed, so codegen is unchanged.
> - **`let &x : T = e` desugars to `let x : &T = &e`** at build time
>   (reusing the borrow-expression machinery, no new IR fields). This
>   gives `x` a reference type `&T` rather than the calculus's "T at
>   kind `Borrowed`"; without a deref operator the distinction is not yet
>   observable. The `x : T @ Borrowed` refinement is deferred.
> - **Elided borrows share one anonymous region** (`anon_region`), so an
>   elided `&T` type and an elided `&e` value compare equal. Per-borrow
>   fresh regions and the **escape check** arrive with the region solver
>   in Step 8 (region scoping is its prerequisite, deferred in Step 6).
> - **`Owned <: Borrowed`** is in `Kind::is_subkind` but is mostly
>   scaffolding — there is no auto-ref coercion yet, so it fires only
>   where a value already has the right reference type.

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

> **✅ Step split into 8a (plumbing) + 8b (enforcement).**
> Step 8 entangles the region *machinery* (declared region parameters,
> `where` clauses threaded through every signature) with the *rules* that
> reason over it (escape check, outlives solver). Mirroring the Step 1→2
> (plumbing→enforcement) pattern, it is split so 8b's borrow checker reasons
> over real declared regions and stored constraints rather than inventing
> them.
>
> **✅ Step 8a — region-parameter plumbing (DONE).**
> Structural only; nothing is enforced and all 518 tests pass, clippy clean.
> - **Grammar**: `region_param = { lifetime }` mixed into `type_params`
>   (`def f<'r, T>(...)`, `type Ref<'r, a>`; Calculus §8.4). `where_clause` /
>   `where_constraint` for `where 'r >= 's` on functions (§8.10), plus the
>   `where` keyword. `region_param` is tried before `type_param` in the
>   choice (a `'`-prefixed token is unambiguously a region).
> - **Scoped region resolution**: `CompileCtx::resolve_region` now consults a
>   per-declaration `cur_regions` scope (replacing the global region-name
>   interner) and returns `Option` — a named lifetime must be a declared
>   region parameter or `'static`, else `AstError::UnknownRegion`. Elided
>   borrows (`&T`/`&e`) still use the shared `anon_region` (now a dedicated
>   lazily-allocated var, not `resolve_region("_")`).
> - **`begin_region_params` / `enter_region_param_scope`** mirror the
>   type-param scope methods; `end_type_params` clears both scopes. Region
>   params are allocated from the same `type_params` pair via
>   `collect_region_params` (type params via the now-filtered
>   `collect_type_params`).
> - **Threaded through the IR**: `region_params: Vec<RegionParam>` and
>   `where_constraints: Vec<RegionConstraint>` on hhir/qhir/typed `Function`;
>   `region_params` on `EnumDef` (re-entered when resolving variant payloads).
>   Mono erases all of it (empty vecs on specialised functions). `where`
>   constraints are resolved while the function's region params are in scope
>   and stored, but **not yet checked**.
> - `ty_mentions_param` now recurses through `Ref`/`Region` so a type
>   parameter used only under a borrow (`type Ref<'r, a> = Mk(&'r a)`) is not
>   mis-flagged as phantom by the variance check.
> - Tests: `region_param_tests.rs` (declaration/resolution, mixed
>   type+region lists, undeclared-lifetime rejection, `where` parse+storage,
>   enum region params). Step 6/7 tests that used bare `'r` updated to
>   declare `<'r>`.
>
> **Deferred to Step 8b — enforcement (the rules):**
> - Per-block scope regions + variable→scope tracking (replacing the single
>   shared elision region) — the prerequisite for the escape check.
> - The outlives solver (scope-nesting + the stored `where` constraints);
>   `solve_outlives` is still the `true` stub.
> - The block/return escape check (`'r ∉ freeRegions(T)` → `RegionEscape`).
> - Checking `where` constraints **at call sites**, and region instantiation
>   so an explicit-region function can be *called* with an elided borrow
>   (currently only its definition type-checks — see
>   `region_parametric_function_definition_type_checks`).
> - `ElisionRule` scaffolding enum.
>
> **✅ Step 8b — enforcement (DONE).** 528 tests pass, clippy clean.
> - **Lexical region scopes**: `CompileCtx` now keeps a `region_scope_stack` +
>   `region_depths`. `enter_region_scope`/`exit_region_scope` open/close a
>   scope (depth 0 = the function, deeper = each nested block);
>   `current_scope_region` / `region_depth` query it. `infer_function` opens
>   the function scope (parameters live at depth 0); the `Block` arm of both
>   `infer` and `check` opens a block scope, runs the body, then closes it.
> - **Variable home regions**: `TypeEnv` entries gained a 4th field — the
>   scope a binding was made in. Parameters get the function region; locals get
>   their block's region. The `Var` arm now returns the binding's stored
>   `Kind` (not always `Owned`), so a `let r = &local` remembers it is a borrow.
> - **The escape check** (`escape_check`, Calculus §6.3): a borrow's *type*
>   keeps the shared elided region (so `&e` still matches a `&T` annotation),
>   while its *kind* `Borrowed(region)` carries the referent's real scope. When
>   a block yields a value whose kind borrows a region at depth ≥ the block's,
>   that borrow would dangle → `RegionEscape`. Catches returning `&local`,
>   `let r = &local; r`, and nested-block escapes; a borrow of a parameter
>   (depth 0) is always fine.
> - **The outlives solver**: `outlives(longer, shorter, assumptions)` —
>   reflexive, `'static` greatest, shallower-scope-outlives-deeper, and
>   transitive closure over assumed `where` edges; `satisfies_outlives` checks
>   a constraint set. (Replaces the old `solve_outlives` stub.)
> - **`ElisionRule`** scaffolding enum added to `types.rs` (inert).
> - Tests: `region_escape_tests.rs` — escape fires on local borrows (direct,
>   let-laundered, nested-block) and is accepted for parameter borrows; the
>   outlives relation accept/reject (reflexive, static, assumptions,
>   transitivity).
>
> **Known limitations (acceptable for this step; full borrow-checking is later):**
> - ~~A borrow flowing through an `if`/`match` join loses its region, so an escape
>   *through* a branch is not caught.~~ **✅ Closed (post-R1).** A branch join now
>   stamps its result type with the **meet** (shortest-lived GLB) of the branches'
>   regions (`infer::join_region_ty`, wired into both modes of `if` and `match`),
>   mirroring the call path's per-argument meet. A borrow escaping through *any*
>   branch (not just the first/chosen one) therefore surfaces in the result type
>   and is caught by the enclosing escape check. The join meets under the
>   function's `where` assumptions, so a multi-lifetime branch return justified by
>   `'a >= 'b` is now admitted (see the next bullet).
> - ~~`where 'r >= 's` is parsed/stored/solvable but **not yet checked at call
>   sites**.~~ **✅ Closed (call-site region inference).** A call now infers a
>   per-parameter region substitution by unifying the callee's declared parameter
>   types against the actual argument types (`CompileCtx::infer_region_subst`),
>   stamps the return type with it (`region_subst_ty`, replacing the global-meet
>   `region_fill`), and checks each callee `where 'a >= 's` under that
>   substitution (`infer::instantiate_call_regions`), using the *enclosing*
>   function's own clauses as assumptions. The `outlives` solver was generalised
>   to depth comparison so a region parameter / the frame (depth 0) outlives every
>   inner block. This is the **region half of Step 15** pulled forward (the
>   typeclass-constraint half remains in Step 15). Explicit-region functions are
>   callable with ordinary borrows (already covered by `region_inference_tests`).
>   *Residual incompleteness (sound):* two depth-0 regions (e.g. a region
>   parameter vs the frame, or two unrelated parameters) are incomparable without
>   an explicit `where`, so such constraints are conservatively rejected.

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

> **✅ Step split into 9a (structural) + 9b (exclusivity)**, mirroring 8a→8b.
>
> **✅ Step 9a — structural mutable borrows (DONE).** 546 tests pass, clippy
> clean. No aliasing enforcement yet.
> - **Kind**: `Kind::BorrowedMut(Region)` added; `is_subkind` adds
>   `Owned <: BorrowedMut` (`SK-OwnedBorrowedMut`); `Borrowed`/`BorrowedMut`
>   stay incomparable; `join` already yields `Borrowed ∨ BorrowedMut = Owned`.
> - **Type**: `TyKind::RefMut(Region, Ty)` + `ref_mut_ty`; `kind_of(&'r mut T)
>   = BorrowedMut 'r`; `&'r mut T` is **not** `Copy`; `has_param`/`compatible`/
>   `Display` arms; mono erases `&'r mut T → T` (like `Ref`).
> - **Grammar**: `mut_kw?` threaded into `borrow_type` (`&'r mut T`),
>   `borrow_expr` (`&mut e`), and `borrow_binding` (`let &mut x`).
> - **IR**: rather than a second variant, the existing `Expression::Borrow`
>   gained an `is_mutable: bool` (`Borrow(Box<Expr>, bool)`) — every pass that
>   treats `&e`/`&mut e` identically (mono, qualify, interpreters, explicate,
>   annotate, lsp, traversal) just forwards the flag. Only `build_ast`, the
>   type checker, and the displays branch on it.
> - **Type checker**: the `Borrow` arm produces `RefMut`/`BorrowedMut` for a
>   mutable borrow; the Step 8b escape check now fires for `BorrowedMut` too
>   (returning `&mut local` is a `RegionEscape`). `let &mut x = e` desugars to
>   `let x : &mut T = &mut e` and is marked mutable (so 9b's assignment-through
>   has a binding to write to).
> - Tests: `mut_borrow_tests.rs` (kind lattice incl. incomparability + join;
>   `&mut e`/`&'r mut T`/`let &mut x` parse + type-check; non-consuming; erased
>   run; escape check fires for `&mut local`, accepts `&mut param`).
>
> **Deferred to Step 9b — exclusivity (the rule):**
> - The **exclusivity invariant** in the ownership pass (where move-tracking
>   lives): `&mut x` requires `x` not otherwise borrowed; `&x` conflicts with a
>   live `&mut x`; borrows released lexically at block exit. Scoped to mut-borrow
>   exclusivity so it does not forbid the move-while-shared-borrowed that Step 7
>   allows.
> - A mutability requirement on `&mut x` (the place must be a `mut` binding).
> - Assignment **through** a `&mut` binding (currently a `&mut` binding is just
>   marked mutable; 9b gives it the intended write-through semantics/checks).
> - Variance: reject a non-invariant annotation on a `BorrowedMut` kind
>   parameter (needs `BorrowedMut` in the `kind_ann` grammar first).
>
> **✅ Step 9b — exclusivity (DONE).** 553 tests pass, clippy clean.
> - **Exclusivity invariant** in the ownership pass (where move-tracking lives,
>   so it gets block-scoping + if/match env-merge for free): `OwnershipEnv`
>   gained a `borrows: Map<UniqVar, BorrowState{Shared,Mut}>`. The `Borrow` arm
>   records a borrow of a *variable* and rejects a conflict — `&mut x` conflicts
>   with **any** live borrow of `x`; `&x` conflicts with a live `&mut x`
>   (`OwnershipError::ConflictingBorrow`). Borrows of *temporaries* are
>   untracked (the borrower owns them). The check is borrow-vs-borrow only, so
>   it does not forbid the move/use-while-shared-borrowed that Step 7 allows.
> - **Lexical release**: the `Block` arm snapshots borrows on entry and restores
>   them on exit, releasing borrows created inside the block; `merge` unions
>   branch borrows (`Mut` dominates) so an if/match cannot smuggle a conflict
>   past the merge.
> - **Mutability requirement** in the type checker: `&mut x` of a variable
>   requires `x` to be a `mut` binding/parameter, else
>   `AstTypeError::MutBorrowOfImmutable` (borrowing a temporary is always fine).
> - Tests (in `mut_borrow_tests.rs`): single `&mut` accepted; two `&mut` of the
>   same place conflict; `&mut` after `&` and `&` after `&mut` conflict; two `&`
>   coexist; `&mut` of an immutable variable rejected; a borrow released at block
>   exit lets a later `&mut` succeed.
>
> **Deferred (documented, minor/representation-bound):**
> - **Assignment through a `&mut` binding.** `let &mut x : T = e` follows the
>   Step 7 precedent and binds `x : &mut T` (a reference), not the calculus's
>   `x : T` kind `BorrowedMut`. So `x = v` (writing through) doesn't type-check
>   (`v : T` vs `x : &mut T`) without a deref operator / the calculus binding
>   representation. Revisit alongside deref (tracked in the Appendix's "Deferred
>   test coverage").
> - **Variance-invariance for `BorrowedMut` kind params** (`reject +a :
>   BorrowedMut`). Needs `Borrowed`/`BorrowedMut` added to the `kind_ann`
>   grammar (currently only `Owned`/`Never`) plus the polarity check; folds
>   naturally into the Step 13 variance follow-up, which is already reworking
>   `check_variance`.

---

## Step M — Module System Redesign (prerequisite for Step 10)

The module system grew incrementally (multi-file support → the `module` keyword →
a global core library → enums bolted on) and has accreted historical cruft:
unqualified names resolve by **scanning every module** (an unprincipled fallback
introduced to reach `core` without `core::`); **types/enums live outside the
module system entirely** (shoved straight into `CompileCtx`); and `build_ast`
interleaves declaration-collection with name-resolution in one stateful,
two-passes-in-one-function blob. Typeclasses make this untenable (orphan rules and
"where do classes/impls live" need a coherent namespace). Redesign it cleanly
*before* Step 10b's resolution work.

### Principles (locked)

1. **Modules are namespaces, not compilation units.** Sand compiles the whole
   program at once — no separate compilation, ABI, or interface stability. A
   module is *only* a namespace; this deletes most of the complexity other
   languages carry.
2. **A file is a module named after the file; internals stay tree-shaped
   (path-based).** One module per file *for now*, but do **not** hardwire
   `file == module` — model modules as nodes addressable by a multi-segment path,
   leaving room for nested `mod { … }` later (the user's stated direction).
3. **Uniform items.** Functions, types, and typeclasses are all named items owned
   by a module, registered and resolved *identically*. Types stop being global.
4. **Lexical resolution — no global fallback.** An unqualified name in module `M`
   resolves in order and stops: **own items → explicit `use` → glob `use` →
   prelude**. A qualified `path::name` resolves directly. (Kills
   `find_function_globally`.)
5. **Prelude = all of `core`, auto-seeded** into every *non-core* module's scope
   (the lowest layer). `core` is compiled as an ordinary module and does **not**
   receive the prelude itself (no self-reference). Primitives (`Int`/`Bool`/`Unit`,
   `__`-intrinsics) sit *below* modules, always in scope.
6. **Instances are one global coherent set** — never `use`d, separate from name
   resolution (lands with Step 10; the redesign reserves the seam).
7. **`use` imports** (§8 of the design): `use path::name;` and `use path::*;`;
   paths parsed as general multi-segment; module-top-level only. Precedence: own >
   explicit-use > glob-use > prelude. Clashes: an explicit `use` colliding with an
   own item / another explicit `use` is an eager error; two globs offering one name
   error only at an ambiguous *reference*. **Deferred:** `{a,b}` grouping, `as`
   aliases, block-level `use`, `super`/`crate` roots.
8. **Two-phase front end: Collect → Resolve.** *Collect* walks every parsed file
   and registers each module + every item it declares (functions, types; later
   typeclasses/impls) into module-scoped symbol tables — **no bodies, no
   resolution**. *Resolve* resolves names against the now-complete tables (by the
   scope rules) and builds the qualified/typed AST. This replaces the
   two-passes-in-`build_program` blob and folds types into modules for free; the
   prelude is "just another collected module," so it needs no special ordering.
9. **Privacy: designed-for, deferred.** Every item has an owning module so `pub`
   is *expressible* later; everything is public for now (no keyword yet).
10. **One path mechanism.** Qualified references to any item kind go through a
    single path-resolution, replacing the per-construct `external_function_call` /
    `qualified_type` special cases.

### Representation

- **Grammar**: a general `path = { identifier ~ ("::" ~ identifier)* }`; a
  top-level `use_decl = { "use" ~ path ~ ("::" ~ "*")? ~ ";" }`; add `use_decl` to
  `program`.
- **Declaration model — NOT yet changed (M1–M4 were resolution-only).** The
  imperative `module Foo;` *cursor* (every item after it belongs to `Foo`, order-
  dependent) is **still supported and unchanged**, and the default module is still
  synthetically named (`mAin_<fileN>`), not the file name. The principle-2 target
  (a file is a module named after the file; cursor replaced by nested `mod { }`)
  is **remaining work** — the cursor currently *is* the only way to get multiple
  modules per file (the "room" the design wants to keep), so it stays until nested
  `mod { }` blocks land. See "Remaining declaration-model work" below.
- **HHIR**: `ProgramModule` gains `uses: Vec<UseDecl>` (and, with Step 10,
  `typeclasses`/`impls`). A file's items default into the module named after the
  file.
- **`CompileCtx`**: module-scoped symbol tables for functions *and* types
  (replacing the flat global maps and the per-module `available_functions` set in
  qualify); a scope-builder that layers own-items + imports + prelude; `core`
  becomes a normal module named `core` (drop the `__core` sentinel naming).

### Phasing (each ends green; cleans its area as it goes)

- **M1 — Lexical function resolution + prelude. ✅ DONE.** `find_function_globally`
  replaced with own-module → prelude(`core`) resolution (`qualify::find_in_prelude`);
  qualified `mod::f` stays; no test relied on the user↔user fallback.
- **M2 — Types into the module system. ✅ DONE.** Bare type/constructor names
  resolve module-scoped (`CompileCtx::lookup_enum_scoped`/`lookup_enum_current`);
  phase 1.5 sets the enum's module for payload resolution. One test migrated to
  idiomatic bare `#tag` patterns.
- **M3 — `use` imports. ✅ DONE.** `use path::name;` / `use path::*;` grammar +
  per-module import store in `CompileCtx`; resolution layers own → explicit `use`
  → glob `use` → prelude for both functions (`qualify`) and types
  (`lookup_enum_scoped`); glob ambiguity resolves to nothing (must qualify);
  validate-on-use for unresolved imports. Tests in `module_tests.rs`. *(Path-form
  unification — converging `external_function_call`/`qualified_type` — deferred to
  M4's structural pass.)*
- **M4 — Finalize the Collect/Resolve seam. ✅ DONE.** `__core` renamed to a
  normal `core` module (so `core::f` works). `build_program`'s
  two-passes-in-one-function is extracted into a thin Collect → Resolve
  orchestrator over named phases: `collect_declarations` (with
  `collect_enum_skeleton` + `collect_use`) → `resolve_enum_payloads` →
  variance → `build_functions`. This is the seam Step 10's typeclass/impl
  registration plugs into (a new `collect_*` per item kind). *Optional remaining
  tidy (not blocking):* converge the `external_function_call`/`qualified_type`
  grammar forms into one path-resolution rule.

### Remaining declaration-model work (not done by M1–M4)

M1–M4 rebuilt name *resolution*; module *declaration* is untouched. Still to do,
to realise principle 2:
- **Name the default module after its file** (replace the synthetic `mAin_<fileN>`).
- **Decide the cursor's fate.** `module Foo;` is currently the only way to get
  multiple modules per file (capability we want to keep), so it stays until
  **nested `mod { }` blocks** provide the clean, non-stateful replacement; at that
  point the cursor can be deprecated/removed.
This is independent of Step 10 (which depends only on resolution + the Collect
seam, both done).

### Out of scope (deferred)

Nested `mod { }` blocks, `pub`/privacy, `{}`-grouping / `as` / block-level `use`,
`super`/`crate` path roots, separate compilation, and instance namespacing
(instances are global by construction).

---

## Step 10 — Typeclass Declarations and Instance Resolution

> **Pre-step note — retire `Ty::TOP`:**
> `println`/`print` currently use `Ty::TOP` as their argument type (an
> escape hatch since they accept any printable value). Once a `Display`
> typeclass exists, replace these intrinsic signatures with a proper
> `T: Display` constraint and remove `Ty::TOP` / `TyKind::Top` entirely.
> If `Display` is not defined in this step, carry the note forward to
> whichever step defines it.
>
> **Decision (deferred):** retiring `Ty::TOP` is explicitly deferred until we
> implement the **core-library typeclasses** (i.e. after the rest of the
> feature work is complete). `Ty::TOP` stays as-is until then.

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

### Detailed implementation plan

**Design decisions (the forks, resolved):**

1. **Method calls are free-function calls resolved by argument type — no dot
   syntax (for now).** A method `eq(a, b)` is an ordinary `function_call` whose
   name happens to be a typeclass method; the *instance* is selected from the
   argument types. This matches how the Calculus writes method bodies
   (`ap(pure(f), x)`) and adds **zero** new expression grammar. (`x.method()`
   sugar is a deferred, purely-syntactic add-on.) **One owning class per method
   name** — a method name belongs to exactly one typeclass (no cross-class
   overloading yet); a clash is a declaration error.
2. **Dispatch is monomorphization-based — no dictionaries, no `dyn`.** A method
   call on a *concrete* receiver type is resolved to the impl's concrete function
   at type-check time (rewritten to an ordinary `Call`). A method call on a
   *type parameter* constrained by `where T : C` stays a `MethodCall` node and is
   resolved by **mono**, once `T` is concrete — exactly mirroring how mono already
   specialises generic functions. (This is Rust-without-`dyn`, and fits the
   existing "no generics past mono" invariant.)
3. **Orphan rule:** `impl C for T` is legal iff `C` *or* `T` is **user-defined**
   (declared in a non-core project module) — so you cannot `impl CoreClass for
   Int` where both are foreign. **Coherence:** at most one impl per `(C, head(T))`,
   where `head(T)` is the type constructor (so `impl C for Option<_>` is one
   instance, keyed by the base enum, not per type argument — Step 10 keys on the
   head; per-argument specialisation is mono's job).

**Representation:**

- **Grammar** (`grammar.pest`): `typeclass_decl`, `impl_decl` added to the
  top-level `program` alternation (§8.8–8.9). `where_constraint` gains a typeclass
  form: `where_constraint = { (lifetime ~ ">=" ~ lifetime) | (identifier ~ ":" ~
  typeclass_constraint) }` (§8.10). Method signatures inside a `typeclass` body
  reuse the function-signature grammar (no body) + optional default (`def … :=`).
- **HHIR** (`ir_types/hhir.rs`): `ProgramModule` gains `typeclasses:
  Vec<TypeclassDecl>` and `impls: Vec<ImplDecl>`. `TypeclassDecl { name, param,
  superclasses, methods: Vec<MethodSig | DefaultMethod>, range }`; `ImplDecl {
  class_name, for_type, methods: Vec<Function>, range }`.
- **QHIR + ctx**: new typed newtypes `TypeclassRef` / `ImplRef`. `CompileCtx`
  gains:
  - **typeclass table** `TypeclassRef → TypeclassDef { param: TypeParamId,
    superclasses: Vec<TypeclassRef>, methods: Map<String, MethodDef>,
    src_module }` where `MethodDef` holds the method's signature (over the class's
    param) and whether it has a default body.
  - **instance table** `(TypeclassRef, EnumRef|prim-head) → ImplDef { for_ty,
    methods: Map<String, FunRef>, src_module }`.
  - **method index** `String → TypeclassRef` (resolve a call name to its class).
- **Impl methods are functions.** Each `impl C for T { def m(…) := body }`
  registers a `FunRef` with a mangled name (`C$T$m`) and an ordinary `FunSig`;
  its body type-checks like any function (with the class param bound to `T`). A
  method the impl omits but that has a **default** is synthesised from the default
  body with the class param set to `T`.
- **typed_hir**: a new `Expression::MethodCall { class: TypeclassRef, method:
  String, self_ty: Ty, args }` for *deferred* (type-parameter-receiver) dispatch;
  concrete dispatch is rewritten to `Expression::Call` during type-checking, so
  most of the pipeline never sees `MethodCall`.
- **`where` constraints**: functions/typeclasses store `Vec<TypeConstraint {
  param: TypeParamId, class: TypeclassRef }>` alongside the existing region
  `where_constraints`.

**Phasing (each ends green; mirrors 8a/8b, 9a/9b):**

- **10a — declarations, tables, coherence + orphan (structural; no dispatch).**
  Grammar + HHIR/QHIR items; register typeclasses (methods, superclasses, the
  class param) and impls (methods as `FunRef`s) into the ctx tables; build the
  method index; **coherence** (reject duplicate `(C, head)` impls), **orphan**
  (reject all-foreign impls), **superclass presence** (an `impl C for T` requires
  `impl A for T` for every `A` in `C`'s `requires`), and **method-set
  completeness** (every non-default method is provided; no unknown methods).
  Method *calls* are not yet dispatched — a program that only *declares*
  typeclasses/impls type-checks; calling a method is still "unknown function".
  Existing tests untouched. Tests: declaration/registration, duplicate-impl
  rejection, orphan rejection, missing-superclass rejection, missing-method
  rejection.
- **10b — resolution + `where` + defaults + mono dispatch.**
  - **qualify**: a `function_call` whose name is in the method index becomes
    `qhir::MethodCall { class, method, args }` instead of a `FunRef` call (so it
    no longer errors as "unknown function").
  - **type-check** (`type_ast`): infer the args; unify the method's declared
    signature (over the class param) against the actual arg types to solve the
    class param → the receiver type. If **concrete**: look up `(class, head)` in
    the instance table → rewrite to `Call(impl_method_fr, args)` with the
    substituted result type. If a **type parameter** `U`: require `where U :
    class` to be in scope (else error), and emit `MethodCall { self_ty: U, … }`.
  - **`where T : C` checking at call sites**: when a generic function with
    `where T : C` is called with `T = Concrete`, verify `(C, head(Concrete))` (and
    its superclasses) resolve — reusing the call-site machinery built for region
    `where` clauses. Inside the body, the constraint is assumed.
  - **default methods**: derivation at impl-registration (10a stored the default
    bodies; 10b synthesises the missing impl methods).
  - **mono**: a `MethodCall { self_ty, … }` is resolved once `self_ty` is
    concrete (after the enclosing generic function is specialised) — look up the
    instance, rewrite to a concrete `Call`. Output is free of `MethodCall` (white-
    box assert, like `Ty::App`/`Ty::Param`).
  - Tests: method call on a concrete type runs end-to-end (interp + LLVM); a
    generic function dispatching on `where T : C` runs for two instantiations;
    superclass-method use; default-method use + override; missing-instance
    rejection at the call site.

**Edge cases (no open holes):**
- *Method name shadowing a real function* → declaration error (the one-class-per-
  name rule; the method index and function table must be disjoint).
- *Superclass method called via a subclass constraint* (`where T : Monad` lets you
  call `fmap`) — resolved through the `requires` chain; **deferred to Step 11**
  (needs HKT to even express Functor/Monad); for Step 10, superclass dispatch is
  exercised only with first-order classes.
- *Impl for a generic enum* (`impl C for Option`) — keyed by the **head** enum;
  the impl method is itself generic over the enum's type params (specialised by
  mono per instantiation), so the instance table holds one entry per `(C, base
  enum)`.
- *Constraint not satisfiable at a call site* → a clear "no impl `C` for `T`"
  error (mirrors the region `where` failure path).
- *Recursion through default methods* (a default calls another method of the same
  class) — resolves against the same instance; terminates because methods are
  finite.

**Test plan:** see the per-phase lists above; plus a `core.sand`-style user
typeclass (`Eq`/`ToInt`) exercised through the interpreters *and* a compiled
`examples/typeclass_*.sand`.

**Out of scope (Step 10, restated):** higher-kinded classes (`Functor`/`Monad`,
Step 11), multi-parameter classes, functional dependencies, associated types/
consts, `dyn`/dictionaries, `x.method()` dot sugar, blanket impls, and retiring
`Ty::TOP` (deferred until the core-library typeclasses land).

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

> **⚠ SUPERSEDED — folded into the Memory Model Steps (Step C, `Heaped`
> Unique).** `box` is now "construct a `Heaped(Unique)` value"; heap allocation
> goes through the `Heaped` hook protocol (`alloc`/`release`), not ad-hoc
> malloc-in-codegen. `Box<T>` = `Unique<T>`, a library `Heaped` strategy. The
> scope below is retained for reference; implement it as part of Step C, not as
> a standalone step.
>
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
- **Variance follow-up (carried over from Step 5)**: function types are
  the first *consumer* (contravariant) positions in the grammar, so this
  step must finish the variance story that Step 5 could only scaffold:
  - extend the polarity analysis (`check_variance` in `build_ast.rs`) to
    flip polarity across a function arrow's argument position and compose
    variance through nested type applications (a parameter under a
    contravariant constructor argument is itself contravariant);
  - recompute the §2.1 *default* variance from actual positions
    (producer → `+`, consumer → `-`, both → `∅`) instead of the current
    always-`Covariant` default;
  - **add the variance tests that Step 5 deferred**: a parameter used
    only in argument position accepts `-a` and rejects `+a`; a parameter
    in both positions must be `∅`; declaring `+a`/`-a` against the wrong
    polarity is an `UnsoundVariance` error; nested composition
    (`F<G<a>>` where `G`'s parameter is contravariant) resolves correctly.
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
- Lifetime elision / region inference — **partially done** (see the "Usability
  pass" above): call-site region inference via type-level region erasure is
  implemented, so borrows flow through calls without annotation. Still out of
  scope: region-aware unification (needed to check `where` clauses at call
  sites) and the `ElisionRule` activation.
- Two-phase borrows
- Borrow splitting (borrowing two fields of a struct independently)
- `derive` macro system
- Async / `Future`
- Multi-parameter typeclasses
- Recursive lambdas / `fix`
- Pattern matching in lambda parameters
- String and array primitives
- Foreign function interface
- **(Memory model)** General `unsafe` model + user-authored custom `Heaped`
  strategies (the user-facing `Ptr`/alloc surface) — the closed `Heaped` set
  (Steps C/E) ships first; extensibility is a deliberate later phase.
- **(Memory model)** The `fip`/`fbip` allocation-grade *guarantee* layer (grade
  lattice on the function arrow, grade polymorphism / effect inference) — the
  `reuse` mechanism (Step D) ships without it; grades only document an existing
  property.
- **(Memory model)** First-class / threadable `Slot` reuse tokens (Step D ships
  lexical-only); fat (offset / generational) `Heaped` handles (pointer-sized
  ships first); the safe `InteriorMut` *kind* surfacing (trusted impls use a raw
  `Ptr` write meanwhile).
- **(Memory model)** Reference cycles without a tracing collector (impossible to
  construct without interior mutability; revisited only with the `InteriorMut`
  kind) and the out-of-memory / fallible-allocation model.

## Usability pass — make regions & borrows usable ✅ (after Step 9)

Steps 7–9 built borrows but left them barely usable: a `&T` value could be
passed around but never *read*, and a `f<'r>(x: &'r T)` function could not be
*called* with a borrow. This pass closed those gaps so complex programs can be
written on top of the borrow foundation. 576 tests pass, clippy clean.

- **✅ Generic-over-borrow.** `subst`/`unify` (`type_ast/generics.rs`) now
  recurse through `Ref`/`RefMut`/`Region`, so `f<T>(x: &T)` infers `T`, enum
  payloads can be `&T`, and these monomorphise and erase normally. Tests in
  `generics_tests.rs`.
- **✅ Deref (`*e`).** New `deref_expr` grammar + `Expression::Deref` threaded
  through every IR; type rule `&T`/`&mut T → T`; transparent at runtime (lowers
  like `Borrow`, erased by mono). The ownership pass rejects moving a non-`Copy`
  value out of a borrow (`MoveOutOfBorrow`), so `*r` reads only `Copy` pointees.
  This is what lets a function *use* its borrowed parameters
  (`def add(a: &Int, b: &Int): Int := *a + *b`). Tests in `deref_tests.rs`.
- **✅ Call-site region inference.** `CompileCtx::region_erase` canonicalises a
  callee's reference regions at the call boundary, so `f<'r>(x: &'r T)` accepts
  any `&_ T` argument (`longest<'a>(...)` is now callable + readable). Reference
  regions never carried a type-level constraint — region *safety* is the lexical
  escape check (which reads the borrow's `Kind`, not its type) — so erasing them
  for matching is sound. Tests in `region_inference_tests.rs`;
  `examples/references.sand` exercises the whole thing through LLVM.

Still deferred from this pass (genuinely advanced / lower-value):
- **`where 'r >= 's` checked *at call sites*.** Since reference types are
  region-blind for matching, the call site can't map arguments back to a
  callee's region parameters, so the stored `where` constraints aren't verified
  against caller regions (they are still parsed, stored, and solvable). **Note:**
  the Reference-Representation Step re-annotates reference types with real
  regions and replaces `region_erase` with the per-call `meet`, at which point
  call-site `where` checking falls out (NORMATIVE items 8–9) — this is the
  designated way to revive it, *not* a separate region-unification effort.
- **Deref of a generic `*r : T`** where `T` is an un-monomorphised parameter is
  rejected (the ownership pass can't see `T`'s `Copy`-ness), so generic deref
  only works once specialised. Acceptable; revisit with `Copy`-bound generics.

---

## Reference-Representation Step — references become real pointers + write-through

**Goal**: give every reference (`&T` *and* `&mut T`) a real runtime
representation — a pointer to the referent's memory — so that `*r` is a genuine
load and `*r = e` is a genuine store that mutates the original. This replaces
the current model where mono *erases* references to their pointee (pass-by-value
copy), which makes write-through impossible. Consistent with every mainstream
language: a reference is an address.

> **Relation to the Memory Model Steps (A–E).** This step owns *reference
> validity* only: regions stay **pure lexical lifetimes** (escape safety), fully
> decoupled from *allocation* — which is the `Heaped` system's job (Steps C–E).
> The two are orthogonal and may land in parallel. Because allocation no longer
> rides on regions, **NLL becomes an unblocked (but still deferred) enhancement**
> of this step's region model.

**Design decision (set):** *both* shared and mutable references are pointers.
Reads (`*r`) become loads regardless of mutability; only writes (`*r = e`) require
`&mut`. Uniform representation keeps generic-over-borrow (`f<T>(x: &T)` vs
`&mut`) simple and avoids two runtime reps.

### ⚠ The critical consequence: the escape check becomes load-bearing

Today references are erased to values, so `&x` yields a *copy* — returning it can
never dangle, and Step 8b could approximate the escape check on the *kind*. Once
references are real pointers, **a returned reference is a real address**, the
approximation is unsound, and the borrow checker must become genuinely sound.
This step is therefore two things bolted together: **(1) references-as-pointers**
and **(2) a sound region/escape check**.

The reference point is **Rust's lexical (pre-NLL) region discipline**, taken only
as far as it agrees with `Calculus.md`. The escape rule is the *literal* Calculus
rule (§6.3 Block, §6.4 Let-Borrow), `'r ∉ freeRegions(T)`, applied to the
**type** — not, as Step 8b did, to the kind. Everything below follows from taking
that literally.

### The Sound Region Model (NORMATIVE — do not diverge)

This subsection is the authoritative model. Any implementation choice that
contradicts it is a bug, not a deviation.

1. **Regions.** A region is one of: `'static`; a **lifetime parameter** `'a`
   (signature-bound, via `<'a>` / Calculus `∀'a`); or a **scope region** — the
   function **frame** `F` (where by-value parameters and the body live) and the
   nested **blocks** `B₀ ⊃ B₁ ⊃ …` it contains.

2. **Outlives lattice (`≥`).** `'static ≥` everything. Each lifetime parameter
   `'a ≥ F` (a borrow handed in by the caller outlives the whole call). `F ≥ B₀
   ≥ B₁ ≥ …` (an outer scope outlives an inner one). Lifetime parameters are
   mutually incomparable unless a `where 'a >= 'b` clause says otherwise; `where`
   clauses are extra assumed edges. "Outer" = `'static` and lifetime parameters
   (they outlive the frame); "local" = the frame and all blocks.

3. **Reference types carry their region** (`&'r T`, `&'r mut T`) — *not*
   region-blind. (This **reverts** the usability-pass region erasure on types;
   see "Reconciliation" below for how region inference survives.)

4. **A borrow takes its referent's storage region.** `&v` where `v` is a local
   in block `B` → `'B`; where `v` is a by-value parameter → `'F`; the pointee of
   a reference parameter `r : &'a T` (i.e. `*r`) lives in `'a`.

5. **Variance in the region.** Shared references are **covariant** in their
   region: `&'r T <: &'s T` iff `'r ≥ 's` (a longer-lived borrow is usable where
   a shorter one is wanted). Mutable references are **invariant** in their region
   (`&'r mut T <: &'s mut T` iff `'r = 's`) — Calculus §2.1, `BorrowedMut`
   invariant.

6. **Escape check = `'r ∉ freeRegions(T)` on the *type*.** At any scope boundary
   carrying region `'r` — a block close *or* the function return — the crossing
   value's **type** must not name `'r` or any region inside it. Operationally,
   give each scope region a depth (outer = smaller; `'static`/params are below
   the frame). A boundary at depth `d` rejects a result whose type's free regions
   include any region of depth `≥ d`; the **function return** is the frame
   boundary, so it rejects a result whose type names **any** local (frame/block)
   region, admitting only outer (`'static` / lifetime-parameter) regions.
   Composite types propagate for free: `freeRegions((&'B T, Int)) = {'B}`,
   `freeRegions(Option<&'B T>) = {'B}` — so **escape via data** (a returned
   tuple/enum holding a reference) is caught with no extra machinery.

7. **Consequences (these must all hold).**
   - **Owned / primitive values are always returnable** — their types contain no
     regions, so `freeRegions = ∅` and the check fires vacuously. The escape
     check *only ever* rejects a type that names a *local* region.
   - **Reference-parameter borrows are returnable** — `r : &'a T`'s region `'a`
     is a parameter (outer), so `def f<'a>(r: &'a T): &'a T := r` is accepted.
   - **Borrows of locals / by-value parameters are NOT returnable** — `&local`
     and `&by_value_param` name a local region. `def f(x: Int): &Int := &x` is
     rejected (it would dangle). This *tightens* the Step 8b rule, which wrongly
     accepted `&by_value_param`.
   - **`if`/`match` joins are caught** — the result *type* is `&'meet(rA,rB) T`,
     so `freeRegions` sees the region even when the kind lattice would have
     collapsed it to `Owned`. The kind lattice keeps doing only its real job
     (capability: `Owned`/`Borrowed`/`Never` for subsumption + divergence);
     region escape no longer piggy-backs on it.

8. **Calls compute the result region as a `meet` (this replaces `region_erase`).**
   For `f<'a>(x: &'a T): &'a T` applied to an argument of region `'r`, the inflow
   constraint is `'r ≥ 'a` (the argument must outlive the parameter's lifetime),
   so the result region for `'a` is the **meet** (shortest, by the lattice) of
   the argument regions tied to `'a`. Thus `longest(&a, &b)` over two locals
   yields a *local* result (correctly not returnable), while
   `wrapper<'a>(r: &'a Int): &'a Int := get_ref(r)` yields `'a` (correctly
   returnable). This is a per-call computation over the lattice, **not** an
   iterative constraint solver.

   *Why the result is always well-defined* (no undefined `meet`): the result is
   the greatest lower bound of the tied argument regions **bounded below by the
   call-site scope region**, which always exists because the call site is in
   scope of every argument. For scope-region arguments (nested blocks are totally
   ordered) it is the innermost; for two *incomparable* lifetime parameters it
   bottoms out at the call-site scope (→ a local result, correctly
   non-returnable). It is therefore *not* a naïve `meet` of two incomparable
   lattice points — it is a GLB with a guaranteed lower bound, hence total.

9. **Reconciliation — region inference survives.** Region-aware types (3) plus
   the per-call `meet` (8) together give *both* soundness *and* the call-site
   region inference the usability pass delivered: `f<'a>(x: &'a T)` stays callable
   with any borrow (the `meet` instantiates `'a`), but the result is the real
   meet region rather than an erased blank, so the escape check on the caller
   side stays sound. `where 'a >= 'b` checking at call sites also falls out — the
   argument regions must satisfy the `where` edges.

10. **Accepted limitations (sound, more conservative than Rust).** Borrows are
    **lexical to their block** (no NLL — a borrow lives to the end of its block).
    No **reborrow-through-deref return**: `&*r` is a fresh frame borrow and is not
    returnable; return `r` itself.

11. **Region-aware subtyping is used at *every* boundary, replacing `type_eq` on
    reference types (Calculus §6.1, Sub).** Once item 3 puts real regions on
    types, comparing reference types by interned pointer identity is wrong in
    *both* directions: it rejects valid covariant flows and accepts dangling
    ones. So every subsumption point — function arguments, the function return
    vs. its declared type, `if`/`match` branch agreement, and **assignment**
    (`x = e` reseating *and* `*r = e` write-through) — must check `Tsrc <: Tdst`
    with the item-5 region rule (`&` covariant, `&mut` invariant), not equality.
    Assignment is the case that is otherwise outright unsound:

    ```
    let mut o : &Int = &a;            -- o : &'a Int
    { let i = mk(); o = &i }          -- &i : &'B Int;  needs 'B ≥ 'a  → REJECT
    ```

    Without this, `o` dangles after the block (no scope-*result* crosses, so the
    item-6 escape check never fires); with it, `'B ≥ 'a` is false and the
    reseat is rejected. This is the single load-bearing addition for `&mut`/outer
    references — closes "escape via reseating an outer location."

12. **A value may not be moved while a borrow of it is live.** With real
    pointers, `let r = &x; … move(x); *r` is a use-after-free that no scope
    boundary catches. The Step 9b `borrows` map already tracks live lexical
    borrows per place, so the ownership pass's consuming-move arm must reject
    moving a variable that has a live `borrow_state` (shared *or* mut). Note `&T`
    is `Copy`, so `r`'s own affinity cannot carry this — the *referent* `x` must
    be the thing tracked, which the `borrows` map already does.

13. **`box` preserves pointee regions in its result type.** When Step 12's
    `box(e)` lands, `box(&local) : Box<&'B T> @ 'static` must keep `'B` in the
    type (not canonicalise it to `'static`); then item-6 `freeRegions` catches
    escape-via-box for free. A `box` that erases the inner region is a back-door
    to the same dangling pointer.

**Two intentional, sound divergences from the literal Calculus** (footnotes, not
problems): (a) Calculus §6.4 gives each `let &x` *binding* its own fresh region;
this model ties a borrow to its *referent's storage* region (item 4) and checks
at scope boundaries — coarser (per-scope, not per-binding) but strictly more
conservative, consistent with the "lexical, no NLL" limitation. (b) The Calculus
is representation-agnostic (`x :_(Borrowed 'r) T`, regions erased at runtime);
representing a borrowed binding as a value of pointer type `&T` is an
implementation choice the Calculus permits — and it is what makes `*r`
reads/writes exist operationally at all. The deref stays purely operational.

### Phasing

Three phases, mirroring 8a/8b. **Ordering is itself a soundness constraint:** the
sound checker must land *before* references become observable pointers — the
moment R2 turns references into real addresses, any program the *lenient* checker
accepted (return `&local`, move-while-borrowed) miscompiles to UAF. So R1 makes
the **checker** sound *over the still-erased runtime* (conservative but safe — it
only rejects programs that *would* dangle), R2 swaps in the **pointer runtime**
for the now-sound program set, and R3 adds **write-through**. There is no
intermediate window in which the compiler both accepts and miscompiles a dangling
program.

#### Phase R1 — sound region checker (type-level only; runtime still erases)

Pure type-checker tightening: no runtime change, no new surface syntax. The
runtime still erases references to values, so accepted programs run exactly as
before; the only observable change is that some previously-accepted *unsound*
programs are now rejected (their tests flip). Implements NORMATIVE items 1–12:

- **Re-annotate reference types with real regions** — undo the usability-pass
  region-blindness (item 3); a borrow `&v` is typed `&'scope(v) T` (item 4).
  Feed the per-borrow scope region — *already computed for the kind in 8b*
  (`infer.rs`, the `Borrow` arm) — into the **type**, replacing the elided
  borrow's `anon_region` (`compile.rs`) *and* `region_erase` (which becomes the
  per-call `meet`).
- **Region lattice with proper outlives** — `'static`/params outer, frame/blocks
  by depth, `where` clauses as assumed edges (item 2); covariant `&`, invariant
  `&mut` (item 5).
- **Region-aware subtyping replaces `type_eq` on references at every boundary**
  (item 11) — arguments, return-vs-declared, `if`/`match` agreement, and
  assignment.
- **Escape check on `freeRegions(type)`** at block boundaries and the function
  return (item 6) — *replacing* the Step 8b kind-based `escape_check`.
- **Per-call `meet`** for call result regions, *replacing* `region_erase`
  (items 8–9); `where`-clause checking at call sites falls out.
- **Ownership pass: forbid moving a borrowed value** (item 12) — the consuming
  Var-move arm rejects a variable with a live `borrow_state` (the Step 9b
  `borrows` map already has the data).
- **Tests / flips**: all of item 7's consequences (owned/primitive return
  accepted, `&'a`-parameter return accepted, `&local`/`&by_value_param` return
  rejected, `if c then &x else &y` return rejected, returned *tuple* holding a
  local borrow rejected — *enum/ADT* payloads are deferred to R5, not R1);
  `longest`/`wrapper` `meet` behaviour; **move-while-
  borrowed rejected** (item 12); **outer-reference reseating rejected** (item 11
  example) and the symmetric `*pmut = &local` write-through-into-outer case.
  Step 8b/9 tests on the *old lenient* rule flip:
  `returning_a_borrow_of_a_parameter_is_accepted` (+ `&mut` analogue) →
  **rejected**; `regions.sand`'s `keep(x: Int): &Int := &x` removed/replaced; a
  new accepted case is "return a reference *parameter*".

#### Phase R2 — pointer representation in the back-end (no new surface syntax)

Stop erasing references; thread real addresses through MIR, codegen, and the
interpreters. The checker is already sound (R1), so flipping the runtime to real
pointers is safe; valid programs behave identically (a load through `&x` yields
the same value a copy did), so the suite stays green.

- **Mono** (`passes/mono.rs`): stop erasing `Ref`/`RefMut` to the pointee.
  Canonicalise the *region* only (it is compile-time) and keep the reference
  constructor, so MIR types include `Ref(_, T)` / `RefMut(_, T)`, both denoting
  "pointer to T".
- **MIR** (`ir_types/mir.rs`): give `Place` projections —
  `Place { local: LocalId, projection: Vec<ProjElem> }` with `ProjElem::Deref`
  (currently `Place` is a bare `LocalId`). Add `RValue::Ref(Place)` (address-of).
  A read of `*r` is `Operand::Copy(Place{ r, [Deref] })`; a write (R3) is
  `Assign { dst: Place{ r, [Deref] }, .. }`.
- **Explicate** (`passes/explicate_control`): `Borrow(inner)` stops being
  transparent. `&place` → `RValue::Ref` of that place; `&temporary` →
  materialise the temporary into a fresh local (explicate already mints temps)
  and take *its* address. `Deref(inner)` in value position → a `[Deref]`-place
  operand (a load).
- **LLVM codegen** (`passes/llvm_codegen.rs`): the heavy lifting is *already
  done* — every local is an `alloca` (a pointer). Add `llvm_type(Ref/RefMut) =
  ptr`; a `place_address(place)` helper that starts at `locals[local]` and, per
  `Deref` projection, loads to get the next address; reads = `load
  place_address`; `RValue::Ref(place)` = `place_address(place)`.
- **Interpreters** (`interpreter/typed_hir.rs` + the MIR interpreter): replace
  the value-keyed environment with a *store* — `env: var → LocId`, a backing
  `Vec<Value>`/slotmap, and a `Value::Ptr(LocId)` variant. `&x` → `Ptr(env[x])`;
  `*r` → `store[loc]`; `let`/declaration allocates a slot; existing `x = e`
  writes `store[env[x]]`. The most invasive R2 change (both interpreters' value
  model), but mechanical.

#### Phase R3 — write-through

Builds on R1's sound checker (assignment region-subtyping already there) and R2's
pointer runtime.

- **Grammar**: `assignment = { (deref_expr | identifier) ~ "=" ~ expression }`
  (PEG tries `assignment` before `expression`, so `*r = e` vs. the `*r`
  expression-statement disambiguates by the `=`).
- **IR**: generalise `Statement::Assignment`'s LHS from a bare variable to a
  *place expression* (reuse `Expr`, constrained to `Var` / `Deref` chains;
  validated by the type checker) across hhir/qhir/typed_hir.
- **Type checker — write-through**: assignment to a place —
  - `Var(x)` → `x` must be a `mut` binding (existing rule);
  - `Deref(inner)` → `inner : &mut T` (a `&T` deref is a read-only place →
    error); the place type is `T`; the RHS is checked against `T` **using region
    subtyping** (item 11), not equality.
  The "is this mutable" check moves from *variable* to *place*.
- **Explicate/codegen**: `*r = e` lowers to `Assign { dst: Place{ r, [Deref] },
  value }` → a store through `place_address`.
- **Interpreters**: `*r = e` writes `store[r.as_loc()]`.
- **Tests**: write-through actually mutates (`def incr(r: &mut Int): Unit := *r =
  *r + 1`, observed through the caller's variable); a `&T` write is rejected; an
  end-to-end `examples/*.sand` that mutates through a reference and exits with the
  mutated value.

#### Phase R4 — interpreter store model ✅ (done)

Both interpreters model storage as a graph of mutable cells (`Rc<RefCell<…>>`); a
reference value is a shared handle to a cell, so `*r = e` is observable across
aliases and across calls (faithful to §3.2/§6.4). TypedHIR gained a dedicated
runtime `Value` type with a boundary `Value → Expression` conversion. See
`interpreter/{mir,typed_hir}.rs`, `mut_borrow_tests` R4 cases.

**Soundness follow-ons ✅ (done, post-R1):** the `if`/`match`-join escape gap
(`join_region_ty` region meet), call-site region inference + `where 'a >= 's`
checking (`infer_region_subst` / `region_subst_ty` / `instantiate_call_regions`,
`outlives` depth generalisation), and the *tuple* case of escape-via-data
(check-mode `Tuple` now carries real element regions).

#### Phase R5 — region-parameterized ADTs (close escape-via-data for ADTs) ✅ DONE

> **Status: complete.** All sub-phases (R5a representation → R5b use-site syntax →
> R5c declaration check + constructor inference → R5d match substitution → R5e
> call-site inference) landed; 626 tests pass, clippy clean. The escape-via-data
> hole is closed for enums/ADTs. Coverage in `tests/layer_tests/region_adt_tests.rs`.
> Deferred as planned: true region variance on ADT params (Step 13), region
> elision at ADT use sites, recursive ADT *values* (Memory C).


**The hole.** A reference stored in an ADT payload is *region-opaque*:
`TyKind::Enum`/`App` carry no region for a borrow inside the payload (`App` holds
only type arguments; the `&` lives in the `EnumDef`), so `freeRegions` finds
nothing and `def f(): Holder := { let y = 5; Holder#H(&y) }` is wrongly accepted —
it returns a dangling reference. (R1's test list over-claimed this as closed; it
was only ever closed for *tuples*, and only after the check-mode `Tuple` fix.)

**The fix (approach A, Rust's model).** A borrow inside an ADT must be tied to a
**region parameter of that type**, and the parameter is threaded into the type at
instantiation (`Holder<'a>`), so the lifetime is part of the type and therefore
part of every *signature* — the contract a caller reasons from without seeing the
body (the modularity property our call-site region inference already relies on).
Rust requires this (`struct Holder<'a> { x: &'a i32 }`) for exactly these reasons:
complete/modular signatures, multiple independent lifetimes, composition through
generics, and variance. A value-level "meet region" (approach B) is unsound across
a function boundary unless the region is in the signature anyway, at which point it
is a strictly weaker (one-lifetime, non-composing) A — so we do A.

**Soundness invariant.** After R5, `freeRegions(T)` for any ADT type `T` exposes
every region a value of `T` may borrow from; the existing block + function-return
escape checks then catch an ADT-of-local-borrow exactly as they do a bare `&local`.

**Representation.** Extend the instantiation node with a region-argument slice:
`TyKind::App(EnumRef, &'tcx [Ty], &'tcx [Region])`. A type constructor with *any*
parameter (type or region) is represented as `App` when used (either slice may be
empty); `TyKind::Enum` remains only for fully non-parametric enums. The
`app_interner` key gains the region args, so `Holder<'a>` and `Holder<'b>` are
distinct interned types (distinct `freeRegions`).

**Use-site syntax + the lifetimes-first convention.** Extend `type_application` to
accept lifetime arguments interleaved with type arguments, **lifetimes first**
(as in Rust): `Holder<'a>`, `Pair<'a, 'b>`, `Both<'a, Int>`. Declaration parameter
lists already allow `<'a, T>`; enforce lifetimes-before-types at *both* declaration
and use so the positional mapping of args→params is unambiguous. `build_type`
routes each arg to the region- or type-arg list and arity-checks both against the
enum's `region_params` / `type_params`.

**Declaration-time lifetime requirement.** During payload resolution, a reference
in a payload must name a declared region parameter of the enum (or `'static`); an
elided/`anon` region is an error (new `AstError::PayloadBorrowNeedsLifetime`).
Check by walking the resolved payload's `freeRegions`: each must be one of the
enum's region-param vars or `Static`. (Breaks the lone existing case
`type Holder<T> = H(&T)` → update to `type Holder<'r, T> = H(&'r T)`.)

**Constructor region inference.** Typing `Holder#H(e)` infers the enum's region
params by unifying the *declared* payload type (`&'r Int`, `'r` = the enum's region
param) against the *actual* payload type (`&'actual Int`) — reusing
`collect_region_bindings` with `solve` = the enum's region params — and builds
`App(er, type_args, [region_args])`. Unbound (phantom) region params default to the
call-site scope (as `infer_region_subst` already does). Type-arg inference is
unchanged.

**`match` region substitution.** For a scrutinee `App(er, tyargs, [r…])`,
`enum_instantiation` additionally builds a region map (enum region-param var →
region arg, positionally) and applies it (via `region_subst_ty`) to each variant
payload type, so a binding `H(x)` gets `x : &'r_actual Int`. This closes the
*extraction* sub-hole (match a borrow out of an ADT and return it).

**Region-machinery extension (the mechanical ripple — every `TyKind::App` site).**
- `free_regions(App)`: recurse type args (existing) **and** push/recurse region
  args → ADT regions become visible to the escape check.
- `region_erase` / `region_fill` / `region_subst_ty` (`compile.rs`): also map the
  region args (canonicalise / fill / substitute), mirroring their `Ref` arms.
- `collect_region_bindings`: pair the region args of a declared `Holder<'a>`
  parameter with the actual call argument's region args → call-site region
  inference works for ADT-typed parameters (modular cross-function use).
- `eq_modulo_regions(App)`: compare type args modulo regions and **ignore** the
  region args (region-blind at check boundaries, like `Ref`), so regions are
  inferred at call sites, not matched structurally.
- `mono_ty(App)` (`passes/mono.rs`): **drop** the region args (specialise on type
  args only; regions are compile-time, already erased by mono).
- `subst` / `unify` (`generics.rs`), `has_param`, `compatible`, `Display`, and the
  `App` arms in `ir_types/display/*` — thread the third field (Display shows
  `Holder<'a, Int>`).

**Edge cases (no open holes):**
- *Multiple independent lifetimes* (`Pair<'a,'b>`): a list of region args, tracked
  separately — A's key advantage over the meet collapse.
- *Mixed type+region params* (`Both<'a, T>`): lifetimes-first positional mapping.
- *Nested ADTs* (`Outer<'a> = O(Holder<'a>)`): region flows through the inner
  `App`'s region args; `freeRegions` recurses.
- *`'static` payload* (`H(&'static Int)`): allowed; region arg `Static` (depth 0,
  never escapes).
- *Phantom region param*: unbound at construction → call-site default (conservative).
- *`&'a mut` payload*: handled like `&'a`; **true region variance** (covariant
  `&`, invariant `&mut` on ADT region args) is **deferred** with the Step 13
  variance follow-up — sound today because there is no region-subtyping coercion
  (boundaries are region-blind + inferred), so no unsound widening exists.
- *Bare use of a region-parametric enum* (`Holder` with no args): arity error —
  region args are required wherever type args are (preserves signature modularity);
  no silent elision into an untracked region.
- *Recursive ADTs holding borrows* (`List<'a> = Cons(&'a Int, List<'a>)`): the
  type-level region threading works here, but **constructing/representing**
  recursive values is gated by the separate `Heaped` work (recursive types need a
  `Heaped` impl, Memory Step C) — so this composes later, no new hole.
- *Anonymous tag unions* (`#a | #b`): no region params; unaffected.
- *`T @ 'r` ascription on an ADT*: orthogonal; the `Region` wrapper around an `App`
  still works and `freeRegions` sees both.

**Sub-phasing (each ends green):**
- **R5a — representation.** Add the (initially always-empty) region-arg slice to
  `App`; update every match site + interner + region machinery to thread it.
  Behaviour-preserving (all slices empty). Isolates the big mechanical ripple.
- **R5b — use-site syntax.** Grammar + `build_type` for `Holder<'a>`
  (lifetimes-first), producing `App` with region args; arity-check against
  `region_params`. Region-parametric ADTs become expressible in signatures.
- **R5c — declaration check + constructor inference.** Require payload borrows to
  name a region param (reject elided); infer region args at constructor sites. **The
  hole closes here** — `freeRegions` now exposes ADT regions, so returning an
  ADT-of-local-borrow is rejected. Update the `Holder<T>` test.
- **R5d — `match` region substitution.** Payload bindings get the instantiated
  region; closes the extraction sub-hole.
- **R5e — call-site inference for ADT params + verification.** Confirm
  `collect_region_bindings` over `App` region args gives modular cross-function
  behaviour; full test sweep.

**Test plan.** Reject: return an ADT (enum *and* nested) holding a borrow of a
local; match a local borrow out and return it. Accept: an ADT holding a *parameter*
borrow returned (`def make<'a>(x: &'a Int): Holder<'a> := Holder#H(x)`); an
ADT-of-`'static`-borrow; an ADT-of-local-borrow used only in-scope; `Pair<'a,'b>`
keeping two lifetimes distinct (extract the `'a` field, use past `'b`). Plus a
`where`-clause-on-ADT-region case for the call-site path.

**Out of scope (R5):** true region variance on ADT params (Step 13 follow-up);
region elision at ADT use sites (always explicit for now); recursive ADT *values*
(gated by `Heaped`, Memory C).

**Out of scope**: NLL (non-lexical lifetimes), reborrow-through-deref return,
field places (`s.f = e`), `Drop` on overwrite (no `Drop` yet), two-phase borrows.

### Deferred test coverage (add when the enabling feature lands)

The borrow/region examples (`borrowing`, `regions`, `variance`, `operators`)
predate the usability pass; `references.sand` is the post-pass showcase. The
remaining gaps:

- **Escape check through `if`/`match` joins** — ✅ **closed** (`join_region_ty`
  region meet; `region_escape_tests::escape_through_an_if_branch_is_rejected` /
  `escape_through_a_match_arm_is_rejected`).
- **Escape via data** (a returned aggregate holding a local borrow) — **partially
  closed.** *Tuples* are now caught: the `freeRegions` check recurses tuple types
  and the check-mode `Tuple` arm carries the elements' real regions
  (`region_escape_tests::returning_a_tuple_holding_a_local_borrow_is_rejected`).
  ✅ **`enum`/ADT payloads — closed by Phase R5 (region-parameterized ADTs).** A
  payload borrow must name a region parameter (`type Holder<'a> = H(&'a T)`);
  `App` carries region arguments, `freeRegions` exposes them, constructors infer
  them, and `match` substitutes them into payload bindings. Returning an ADT (or
  nested ADT) that holds a borrow of a local — or extracting one and returning it
  — is now rejected; param/`'static` borrows and in-scope use are accepted. See
  `tests/layer_tests/region_adt_tests.rs`.
- **Variance in consumer positions** — already tracked in Step 13's
  "Variance follow-up": function-argument (contravariant) positions enable the
  `-a` accept / `+a` reject and nested-composition tests Step 5 could only
  scaffold.
- **Write-through (`*r = e`) / assignment through a `&mut`** — ✅ **closed** by R3
  (typecheck) + R4 (observable in both interpreters): `mut_borrow_tests`
  write-through cases and `examples/write_through.sand`.

---

## Memory Model Steps (A–E) — the `Heaped` allocation system

These steps implement the memory/allocation model from the memory-model
decision log (`~/.claude/plans/can-you-evaluate-the-magical-dusk.md`). They slot
into the dependency order **after Step 11** (typeclasses + HKTs), since `Heaped`
is a lang-item typeclass. The Reference-Representation Step (pure-lifetime
regions + escape check) is independent and may land in parallel.

**Organizing principle — three orthogonal axes:** ownership + RAII drop decides
*when* memory is freed; the `Heaped` typeclass decides *how* a type is
allocated/reclaimed; regions (pure lifetimes) decide *whether a reference is
still valid*. The compiler stays allocation-agnostic — it knows only `Ptr`,
`drop_in_place`, and the `Heaped` hook protocol; `Box`/`Rc`/arenas are library.

### Step A — Memory substrate

**Goal**: The minimal compiler/runtime primitives every `Heaped` strategy is
built on. No allocation policy lives in the compiler.

**Scope**:
- `Ptr<T>` primitive type: raw, `Copy`, address-sized; load / store / offset;
  *outside* the affine/region/borrow discipline. Deref is the `unsafe`
  operation (general `unsafe` gating deferred; confine `Ptr` use to core-lib).
- `drop_in_place(x)` intrinsic: compiler-generated structural field-drop glue,
  callable by library code at a point it chooses (for conditional / manual
  free). Policy (when) = library; mechanism (recurse fields) = compiler.
- `Heaped` lang-item registration: the compiler holds a reference to the
  `Heaped` typeclass (declared in core lib, Step C) so it can emit calls.
- `malloc`/`free` are **not** intrinsics — a core-lib `alloc` over `Ptr` + FFI.

**Out of scope**: the `Heaped` typeclass itself (Step C); any allocation
strategy; the user-facing `unsafe` model.

### Step B — Drop / RAII

**Goal**: Make deterministic destruction first-class and correct under
conditional moves.

**Scope**:
- Formalize drop insertion at scope exit, reverse declaration order.
- **Conditional-move completing drop**: refine the ownership pass's branch
  merge (`OwnershipEnv::merge` in `passes/ownership/`) so a value moved on some
  branches is *dropped* on the others (as if `else drop(x)`) — replacing
  today's silent leak. No runtime drop flags; drop is placed statically at the
  end of the innermost block where the value is last owned.
- Wire `drop_in_place` (Step A) as the structural-drop mechanism.
- `drop` remains a library fn; `Copy` types are exempt from move/drop.

**Out of scope**: dynamic drop flags (rejected by design); `Drop` overriding
(folds into Step C/E's `release`).

### Step C — `Heaped` (Unique)

**Goal**: Recursive types become legal and heap-allocated via the unique
(Box-like) `Heaped` strategy. **Subsumes Step 12 (`box`).**

**Scope**:
- Core-lib lang-item typeclasses:
  `Heaped<T> { type Handle; alloc; borrow; release }` and
  `HeapedUnique<T> requires Heaped<T> { borrow_mut; take; reuse }`.
- **Recursive-type legality**: a (mutually) recursive `type` is well-formed
  only with a `Heaped` impl; its kind is `Owned`. Grammar: a `: Heaped(Strategy)`
  annotation on `type` (e.g. `type List<+a> : Heaped(Unique)`).
- **Representation**: a `Heaped` type `T` is represented as `S<NodeOf<T>>`
  (`S` = the strategy's generic handle type, pointer-sized; `NodeOf<T>` = T's
  constructors with recursive occurrences = the handle). Nullary variants use a
  nullable/tagged pointer (niche); only payload variants get heap nodes.
- **Root-owns-tree** ownership: the root owns all nodes transitively; interior
  nodes are accessed as borrows into the root.
- **Lowering** (compiler → impl methods): `#C(ē)` → `alloc`; `match` (borrow) →
  `borrow`; `&mut` → `borrow_mut`; end-of-life → `release`. (`take` in Step D.)
- Core-lib `Unique` (Box-like) strategy over `Ptr` + `alloc`/`free`.
- Move-stability invariant: handles survive a move of the root (pointer-sized
  → heap-stable addresses; fat/offset handles deferred).

**Out of scope**: `reuse`/`take` (Step D); the Shared family (Step E); custom
user strategies; fat handles.

### Step D — `reuse` (in-place reuse mechanism)

**Goal**: Explicit, checked in-place reuse — functional code with zero net
allocation — with no runtime mechanism (uniqueness is static via affine
ownership).

**Scope**:
- `HeapedUnique`'s `take : T → (Node, Slot<L>)` (owning destructure → fields +
  reuse husk) and `reuse : (Slot<L>, node:L) → T` (rebuild, no alloc).
- Grammar: `reuse_expr` (`reuse cell as #C(ē)`); a husk-capturing as-pattern
  `cell @ pat` bound in a *consuming* match. `Slot<L>` is a layout-indexed,
  `Owned`, affine type; an unused `Slot` drops = frees.
- Typing: the `(Reuse)` rule + the `layout(#C) = L` side-condition.
- Match refinement: a *consuming* match on a Unique scrutinee drains via `take`
  (owned fields + `Slot`); a *borrowing* match uses `borrow`.
- **Lexical only**: the husk is consumed by a `reuse`/`release` in the same
  match arm; threadable first-class `Slot` is deferred.

**Out of scope**: the `fip`/`fbip` *guarantee* layer (grade lattice on the
arrow, grade polymorphism / effect inference) — deferred. `reuse` ships
standalone; the grades only document a property the code already has.

### Step E — `Heaped` (Shared)

**Goal**: Opt-in shared ownership (`Rc`-like) for unordered-lifetime
co-ownership, as a library `Heaped` strategy.

**Scope**:
- `HeapedShared<T> requires Heaped<T> { share }`; the core-lib `Shared`
  (refcounted-node) strategy.
- `.share()` method → `share(&t)` (explicit increment, syntactically present).
  Shared handles stay **affine**; the decrement is ordinary RAII `release`,
  only newly *conditional* (free at dec-to-zero). No compiler dup-insertion.
- The counter uses interior mutation of aliased memory — a raw `Ptr` write
  inside the trusted impl now; the safe `InteriorMut` kind surfacing lands
  later. Leak-free today (cycles need interior mutability).
- `take`/`reuse`/`borrow_mut` are **not** available on Shared (moving out of /
  mutating shared storage needs a runtime uniqueness check — excluded).

**Out of scope**: `Arc` (atomic counter) — an additive strategy; cycles / weak
refs; custom user strategies; the safe `InteriorMut` kind.

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
                           └─ Step A  (memory substrate: Ptr, drop_in_place)
                                └─ Step B  (drop / RAII; completing-drop)
                                     └─ Step C  (Heaped Unique; subsumes Step 12 box)
                                          ├─ Step D  (reuse / Slot)
                                          │    └─ Step E  (Heaped Shared; .share())
                                          └─ Step 13 (lambdas)
                                               └─ Step 14 (Clone/Copy integration)
                                                    └─ Step 15 (where clause checking)
```

Steps 4–9 (ownership/region track) and Steps 10–11 (typeclass track)
both depend on Step 3 but are independent of each other. They can be
developed in parallel on separate branches and merged before the memory
steps (A–E). The Reference-Representation Step (pure-lifetime regions +
escape check) is independent of A–E and may land in parallel.
