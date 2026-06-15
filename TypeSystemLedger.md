# Type-System Ledger

A distilled companion to `TypeSystemPlan.md` (which is the full, append-only
design log). This file keeps only the durable essentials: the design decisions
worth remembering (with their *final* verdict), what is still unbuilt, what is
deliberately deferred, and the known bugs / limitations. When the plan and this
ledger disagree, the plan's most recent dated entry wins — fold the correction
back into here.

---

## 1. Design decisions (final verdicts)

### Pipeline & generics
- **Monomorphisation sits between TypedHIR and MIR.** MIR and LLVM codegen only
  ever see fully concrete types; no generics leak past mono.
- **Generic instantiation is on-demand and reachability-driven.** Non-generic
  functions are roots; generic functions/enums are specialised per distinct
  instantiation. No dead-code elimination of non-generic fns (keeps entry point
  + core lib).
- **`subst`/`unify` recurse through every type former** (tuple, app, ref/refmut,
  region, **ptr**). A missing former is a bug (the `Ptr` arm was such a bug,
  now fixed).

### Kinds, regions, borrows
- **Kind lattice is `{Owned, Never}` + `Borrowed`/`BorrowedMut`; `InteriorMut`
  is reserved** (lattice accommodates it, no logic implemented).
- **Borrow checking is phased:** shared `Borrowed` fully validated before
  `BorrowedMut`.
- **References are real pointers** — *both* `&T` and `&mut T`. Reads (`*r`) are
  loads regardless of mutability; only writes (`*r = e`) require `&mut`. One
  uniform runtime representation (keeps generic-over-borrow simple).
- **The escape check is load-bearing and sound**, applied to the **type**
  (`'r ∉ freeRegions(T)`, Calculus §6.3/§6.4) — *not* to the kind. Reference
  point: Rust's pre-NLL **lexical** region discipline, only as far as it agrees
  with `Calculus.md`.
- **Regions are pure lexical lifetimes, fully decoupled from allocation.**
  Allocation is the `Heaped` system's job. Consequence: **NLL is unblocked but
  deferred.**
- **Sound Region Model is NORMATIVE** (`TypeSystemPlan.md` §"The Sound Region
  Model"): regions = `'static` | lifetime param | scope (frame ⊃ blocks);
  outlives lattice `'static ≥ 'a ≥ F ≥ B₀ ≥ …`; reference types **carry** their
  region (not region-blind); per-call `meet` (not erasure). Do not diverge.
- **Lifetimes are explicit;** elision is scaffolded but inactive.

### Drop / RAII (Memory B)
- **Drop placement: HIR scope-metadata → first-class MIR `Statement::Drop`.**
  `Expression::Block` carries `drops: Vec<UniqVar>` (computed on HIR where move
  analysis lives); explicate lowers it to `Statement::Drop(Place)` (the node MIR
  passes reorder/elide and where a future `Drop` typeclass slots in).
- **The ownership pass is a transformer** (`TypedProgram → TypedProgram`), not a
  pure checker — single source of truth for move state + drop insertion.
- **Declaration order via `UniqVar.idx`** (ordered `im::OrdMap`); reverse-iterate
  for reverse-declaration drop order.
- **No runtime drop flags** — all drop points are static.
- **Completing drops at `if`/`match` merges**: a value owned on one branch but
  moved on another is dropped on the owning branch (falls out of
  `OwnershipEnv::merge`).

### Memory model (the `Heaped` system)
- **Three orthogonal axes:** ownership+RAII = *when* freed; `Heaped` = *how*
  allocated/reclaimed; regions = *whether a reference is valid*. The compiler is
  allocation-agnostic — it knows only `Ptr`, `__drop_in_place`, and the `Heaped`
  hook; `Box`/`Rc`/arenas are library.
- **`malloc`/`free` are library functions over `Ptr` + FFI, not intrinsics.** The
  only alloc/dealloc sites live in `core.sand` (`unique_alloc`/`unique_take`/
  `unique_release`).
- **Capability vs strategy (the `Heaped(Unique)` ambiguity, resolved):**
  `deriving Heaped` is the **unique floor** and is all Step C needs. **Uniqueness
  is the ambient property of every owned affine value — not opted into** — so
  `borrow_mut`/`reuse` are capabilities a `Heaped` value *automatically gains*
  (Step D), gated by the borrow checker. **`HeapedUnique` is redundant as a
  user-facing concept**; it survives only as the internal
  `HeapedStrategy::Unique`. `HeapedShared` (Step E) is the one real alternative
  strategy (a different representation — refcount cell — that *loses* `&mut`/
  `reuse`).
- **`deriving` is general and extensible** (`enum Derivable`, today only
  `Heaped(HeapedStrategy)`), not heap-specific. The source token is `Heaped`.
- **K-HeapedRec:** a (mutually) recursive type **must** `deriving Heaped`
  (else infinite-sized / leaking); a non-recursive type **may** (to opt a large
  value onto the heap).
- **`Heaped` ships as a compiler lang-item recognised by name, NOT a real
  typeclass.** A meaningful `typeclass Heaped` needs associated types (to relate
  node type ↔ handle type) and a second strategy to dispatch to — both absent in
  C. `Unique<T>` is concrete `core.sand` code. The typeclass form is a post-E
  milestone (then `deriving Heaped` becomes sugar for a generated `impl` and
  user-authored strategies fall out).
- **`Heaped` lowering = a full IR rewrite to `Unique<Node>`** (chosen over a
  codegen-local fix or a shared-recipe middle ground). `passes::heap_lower`
  (run before ownership + mono) synthesises a node enum `E$Node`, rewrites
  `E<a> → Unique<E$Node<a>>`, `E#C(p) → unique_alloc(E$Node#C(p))`, and a
  consuming `match`/`let`-pattern → `unique_take` + an ordinary node match.
  **No heaped enum survives into ownership/mono/codegen** — every backend sees
  only ordinary enums, `Unique` handles, and `Ptr` ops.
- **Consuming match binds *every* payload position** (wildcards → fresh
  bindings), so ownership's scope-exit drops reclaim any field the arm doesn't
  move out. This is *why* the original wildcard-leak concern is closed.
- **Multi-payload constructors are tuple-payload sugar.** `Cons(Int, List)`,
  `C#Mk(a, b)`, the pattern `#Cons(x, rest)`, and `let C#Mk(a, b) = …` all desugar
  in `build_ast` to the single tuple payload `Cons((Int, List))` / `((a, b))` —
  so the representation stays one-payload-per-variant and the old double-bracket
  spelling keeps working unchanged. A single argument is *not* tuple-wrapped.
- **Match lowers to a Maranget decision tree** (`explicate_control`,
  `compile_match_matrix`): a pattern matrix is specialised one column at a time,
  switching on each occurrence's discriminant/literal *once* and sharing common
  sub-trees, instead of the old per-arm backtracking check-chain. Variable
  bindings are *not* part of the tree — they are extracted at the matched arm by
  walking the original pattern from the scrutinee, so the tree only decides
  *which* arm wins. MIR has only a binary `Branch`, so a multi-constructor switch
  is a chain of `disc == k` tests off a single discriminant read.
- **Exhaustiveness + reachability run one usefulness algorithm** (Maranget's
  `U(P,q)`, `useful` in `type_ast/check.rs`) over the same pattern matrix the
  lowering uses — replacing the old ad-hoc coverage sets + one-level nested map +
  `irrefutable_seen` flag. An arm is **reachable** iff its row is useful against
  the arms above it (subsumes exact-duplicate and after-catch-all detection); the
  match is **exhaustive** iff the wildcard row is *not* useful (an uncovered value
  is the witness, rendered into the `NonExhaustiveMatch` error, e.g. `(N, _)`).
  This fixed a **soundness hole**: refutable patterns nested in a tuple were
  treated as irrefutable, so a non-exhaustive product match compiled and hit the
  `Unreachable` MIR block at runtime. Now handles tuples, nested variants, and
  nested literals (`#S(5)`) at arbitrary depth precisely; the checker also flags a
  redundant catch-all after exhaustive nested arms (matching Rust). A nested
  literal binds nothing — its equality test is emitted by the decision tree, and
  binding extraction skips it.
- **Structural `__drop_in_place` is codegen-generated per-type glue**, memoised
  and recursion-safe (a `Unique<T>` frees its cell after recursing the node;
  aggregates recurse fields). It is *not* routed through `unique_release`,
  because the drop recursion isn't visible to mono. `Unique` instances are
  recognised post-mono via a `ctx.unique_instances` registry.
- **No niche optimisation** — a straightforward tagged-union layout; nullary
  heaped variants still get a (disc-only) allocation. (User: "no added
  complexity.")
- **Non-heaped enum layout = stack tagged-union** `{ i64 disc, [P x i8] }`
  (P = max variant payload store size; bare `i64` when all-nullary); payload
  stored/loaded at field-1 address by its own type (opaque pointers, no bitcast).
- **`size_of::<T>()` via turbofish** → `RValue::SizeOf(Ty)`; turbofish is wired
  **only** for `size_of` in Step C (any other turbofish is a clear error).

### Higher-kinded types (Step 11)
- **Arrow kinds are interned** — `Kind::Arrow(KindId)` + a ctx kind interner,
  *not* `Arrow(Box<Kind>,Box<Kind>)`. Keeps `Kind: Copy`/`Ord`/`Hash` while
  staying fully general (nesting/currying). `is_subkind`/eq use canonical
  `KindId` equality (arrows are matched exactly, never coerced). Surface syntax:
  `Owned`/`Never` atoms, `->` (right-assoc), parens.
- **`F<A>` is `TyKind::ParamApp(TypeParamId, &[Ty])`** — distinct from `App`
  (whose head is an `EnumRef`). A higher-kinded param's `Subst` entry is the
  **bare `Enum(er)` constructor**; `unify(ParamApp, App)` binds `F := Enum(er)`
  with a **head-only** conflict check (so `F<A>`, `F<B>` agree on `F`), and
  `subst`/`mono_ty` reconstruct `App(er, args)`. Never survives mono (like
  `Param`).
- **Typeclass methods may declare their own generics** (`def fmap<A,B>`); they
  scope alongside the class parameter (`extend`/`retract_type_params`).
- **A higher-kinded class's impl head is a bare constructor** (`impl C for Opt`),
  resolved directly to `TypeHead::Enum(er)` (not via `build_type`, which rejects
  a bare generic enum as under-applied).
- **`Functor`/`Applicative`/`Monad` wait for Step 13** (their methods need
  function types `A -> B`). Step 11 ships the machinery; arrow-free classes
  (constructor param in *argument* position) are the demonstrable clients now.

### Functions / lambdas (Step 13, in progress)
- **One kind-annotated arrow, not Rust's four.** Sand has a single function type
  `TyKind::Fn(arg, ret, FnMode)` (`A →[K] B`, Calculus §3.1) instead of Rust's
  thin `fn` pointer + the `Fn`/`FnMut`/`FnOnce` traits. `FnMode` =
  `Reusable` (`Fn`/`→[Borrowed]`), `ReusableMut` (`FnMut`), `Consuming`
  (`FnOnce`/`→[Owned]`); subsumption reuses kind subtyping; a capture-free
  function is the empty-env case of the fat pointer. No separate `fn` type, no
  closure traits.
- **Bare `->` is the reusable arrow** (the common case; what `fmap`/`bind` need,
  since `Consuming`/`FnOnce` is once-callable). Unary + right-associative;
  multi-arg via tuples/currying.
- **Runtime rep = fat pointer `{ fn_ptr, env_ptr }`** (env null for capture-free).
- **Indirect calls vs. function names:** `g(arg)` is an indirect call (apply a
  function value) only when `g` is a bound local **and not a function**; a
  function of the same name wins in call position (preserves the existing
  "no collision between variable and function names" rule). Uniquify makes this
  call (it has both the var scope and the function table).
- **Lambdas are lifted to top-level functions during monomorphisation.** Mono
  hoists each `Lambda` into a fresh top-level function and replaces it with
  `typed_hir::Expression::Closure { func, captures }`, so both interpreters and
  codegen see the lifted form. A closure value is a fat pointer
  `{ fn_ptr, env_ptr }` (MIR `RValue::Closure`); an indirect call extracts the
  fn pointer (`RValue::CallIndirect`). Done through codegen for capturing and
  non-capturing lambdas.
- **Captures are by move.** `infer` collects the body's referenced outer-env vars
  (`collect_dependencies`, filtered to the enclosing scope) into
  `Lambda.captures`; mono `malloc`s the environment (`Unit`/single/tuple) and the
  lifted fn unpacks it from its leading `env_ptr` via `__ptr_read`; ownership
  moves each non-`Copy` capture in and drops it from the body. **The env is
  leaked** (never freed) — documented limitation below. Escaping closures work.
- **Calling modes via `-[k]>`.** `arrow_kind = Owned | BorrowedMut | Borrowed`
  picks the `FnMode` (`Owned`→`Consuming`, `BorrowedMut`→`ReusableMut`, else
  `Reusable`); bare `->` = `Reusable`. Ownership consumes the callee on `Apply`
  **only** when its mode is `Consuming` — so reusable/mutating closures are
  callable repeatedly, a consuming one is once-only (second call = use-after-move).
- **Subsumption** (`FnMode::usable_as`, `Reusable <: ReusableMut <: Consuming` ≈
  `Fn ⊆ FnMut ⊆ FnOnce`) is directional: applied in `eq_modulo_regions` (actual
  `<:` expected) and `unify` (supplied `<:` declared), so a reusable lambda passes
  where a consuming arrow is expected. Domains/codomains compared by equality
  (no use-site subtyping between concrete types yet, so this is sound).
- **Variance follow-up done (Step 5's deferred half).** Function arrows are the
  first *consumer* positions, so `check_variance` now does full polarity analysis
  (`param_polarity` in `build_ast.rs`): a function *argument* flips polarity
  (contravariant), its *result* keeps it; generic applications `F<..>` compose the
  current polarity with `F`'s declared per-param variance; a param at both
  polarities is invariant. An **explicit** `+a`/`-a` is checked against the
  inferred polarity (`+` rejects contravariant occurrences, `-` rejects covariant
  ones); an **absent** annotation is inferred and never rejected (`TypeParam`
  gains `explicit_variance`). Variance still has no *use-site* effect (no concrete
  subtyping) — it is a declaration-site soundness check only.
  - **Known imprecision (on-paper unsound, runtime-harmless):** the `App(er,..)`
    composition in `param_polarity` reads `er`'s *declared/defaulted* parameter
    variance, and an unannotated param defaults to `Covariant`. So a param that is
    contravariant only because it flows through an *unannotated* intermediate is
    mis-scored — e.g. `type Sink<a> = MkSink(a -> Int)` (no annotation) then
    `type Relay<+a> = MkRelay(Sink<a>)` is wrongly **accepted** (`a` is really
    contravariant in `Sink`). Harmless because variance has no use-site effect.
    A fully sound fix is a **fixpoint** (mutual recursion makes a single ordered
    pass impossible): add a `Bivariant` (∅, top) lattice element with join
    `Cov ⊔ Con = Inv`; iterate `inferred(p) = ⊔ over occurrences of
    (position_sign ∘ inferred_variance_of_constructor_passed_through)` to a fixed
    point; validate explicit `+a`/`-a` against the result and **write the inferred
    variance back into the `EnumDef`** (today it is computed for validation only
    and never stored, which is the root of the imprecision). Gate this on the
    language gaining subtyping — until then it only changes which *declarations*
    are accepted, never any checking/runtime behaviour.
- **`Functor`/`Applicative`/`Monad` live in `core.sand`** (Step 13's capstone):
  HKT classes (`F : Owned -> Owned`) whose methods take/return lambdas
  (`fmap`, `ap`, `bind`). `examples/monad.sand` instantiates them for `Option`
  through codegen; `hkt_tests.rs` asserts HIR/MIR agreement.
- **Return-type-driven dispatch** for `pure<A>(x: A): F<A>` (no `F<_>` argument to
  resolve the instance from): the bidirectional `check` path threads the expected
  type into `infer_method_call`, which seeds the receiver by unifying the method's
  return type against it (so `let x: Option<Int> = pure(5)` picks the `Option`
  instance). Falls back to argument-driven resolution when there is no expected.
- **Deferred:** by-borrow *capture* (vs. move-only) + the §3.1 region stay gated
  on stack closures.

### Typeclasses & misc
- **Orphan rules are strict** — an `impl` is legal only if the crate owns the
  class or the type.
- **`Ty::TOP` retirement is deferred** until a `Display` typeclass is wired in.
- **Module system principles are locked** (`TypeSystemPlan.md` §"Principles
  (locked)"): every item has an owning module (so `pub` privacy is designed-for);
  ambiguity errors only at an ambiguous *reference*.

---

## 2. Steps not yet completed

Done so far: **Steps 0–9, M, 10, 11, 14, 15**, the **Usability pass**,
**Ref-Rep R1–R5**, and **Memory A, B, C** (C.1–C.6). (Step 15 — `where`-clause
checking — was already complete: call-site typeclass constraints, superclass-at-impl,
and region outlives are all implemented + tested, built across Steps 8b/10b/14c.)
Remaining, in roadmap order:

- **Step 13 — Lambdas / first-class functions.** Lambda grammar/IR, capture
  analysis, fat-pointer codegen. Includes the **variance follow-up** Step 5
  deferred (contravariant function-argument positions). Depends on Memory C
  (closure environments).
- **Step 15 — `where`-clause checking at call sites — typeclass half only.** The
  **region half is already done** (call-site region inference + `where 'r >= 's`).
  Remaining: verify typeclass constraints + superclass requirements per call.
- **Memory D — `reuse`.** `take`/`reuse`/`Slot<L>`, husk as-pattern, layout
  check, consuming-match drain. **Lexical-only** (threadable `Slot` deferred).
- **Memory E — `Heaped` (Shared).** `HeapedShared` + `.share()` + a refcount
  strategy (counter via raw `Ptr` write); affine handles.

> **Step 12 (`box`) is obsolete** — subsumed by Memory C.

---

## 3. Deferred features (explicitly out of scope, no current step)

- The **`fip`/`fbip` allocation-grade guarantee** layer (grade lattice on the
  arrow, grade polymorphism / effect inference). `reuse` ships without it.
- **First-class / threadable `Slot`** reuse tokens (Step D is lexical-only);
  **fat (offset/generational) handles** (pointer-sized ships first).
- The safe **`InteriorMut` kind** (trusted impls use a raw `Ptr` write meanwhile).
- **User-authored custom `Heaped` strategies** + the **associated-types `Heaped`
  typeclass** (the user-facing alloc/`Ptr` surface). The closed `Heaped` set
  (C/E) ships first.
- The general **`unsafe` model**.
- **Reference cycles** without a tracing collector (impossible to build without
  interior mutability) and the **OOM / fallible-allocation** model.
- **NLL** (non-lexical lifetimes) — unblocked by the pure-lifetime region model,
  still deferred.
- **Region variance on ADT params** (Step 13 follow-up); **two-phase borrows**;
  **borrow splitting** (independent borrows of two fields).
- **Multi-parameter typeclasses**, **functional dependencies**, **`derive`/
  auto-derive macros**, **`x.clone()` dot sugar**.
- **Async / `Future`**, **recursive lambdas / `fix`**, **pattern matching in
  lambda params**, **string/array primitives**.
- Module **privacy (`pub`)**, **`{a,b}` import grouping**, **`as` aliases**.

---

## 4. Known bugs

- **Keyword-prefix identifiers mis-lex.** A type/identifier whose name begins
  with a primitive type keyword splits at the keyword — e.g. `IntList` lexes as
  `Int` + `List`, so `type IntList = …` fails to parse ("expected core_type").
  Same class for any `Int*`/`Bool*`/`Unit*` name. Fix: require a word boundary
  after the primitive keywords in the grammar. (Flagged as a background task.)

*(Note: the `unify`/`subst` missing-`Ptr` arm and mono not monomorphising the
`App` payloads of a non-generic enum were latent bugs found and **fixed** during
Memory C — they are no longer open.)*

---

## 5. Known limitations (correct but restricted)

### Memory / `Heaped`
- **Consuming match only.** Match-by-reference / `unique_borrow` is not wired up;
  a heaped scrutinee is always taken (read node + free husk). (Borrowing match is
  Step D/E territory.)
- **Nested variant pattern on a heaped field is unsupported** — e.g. matching
  `Cons((Cons(...), t))` where the first field is itself a heaped value matched by
  a variant pattern. It would need a recursive `unique_take`; currently an
  internal-error guard.
- **A named binding arm that rebinds a whole heaped scrutinee** in a
  variant-inspecting match is unsupported (the value has been taken to a node, not
  a handle). Exhaustive variant arms + a wildcard catch-all are fine.
- **Refutable heaped `let`-pattern can leak the non-matching variant's heaped
  fields.** `let E#V(..) = val else fb` takes `val`; if `val` is a *different*
  variant carrying heaped fields, those are discarded without release. (Same
  shallow-free class; rare. The common all-bound consuming `match` is leak-free.)
- **Nullary heaped variants each get a heap allocation** (no niche optimisation),
  so e.g. every `Empty`/`Leaf` is a separate `malloc`. Performance, not
  correctness.
- **The interpreters do not actually `free`** — they model the heap as an `Rc`
  cell graph and reclaim automatically; only codegen runs the real allocator.
  Observable values agree across all three backends (this is by design, not a
  defect).

### Higher-kinded types (Step 11)
- **Instance resolution needs the constructor parameter in *argument* position.**
  A method that mentions `F` only in its return (`pure : A -> F<A>`) can't be
  resolved — that needs expected-type-driven resolution, which the checker
  doesn't do. Arrow-free classes used now keep `F` in an argument.
- **`Functor`/`Applicative`/`Monad` are not yet expressible** — their methods
  need function types (`A -> B`), pending Step 13.
- **A bare nullary generic constructor can't infer its element type** —
  `Opt#Nothing` needs an annotation (`let x: Opt<Int> = Opt#Nothing`). This is a
  pre-existing generic-enum-inference gap, surfaced by HKT demos but not specific
  to them.

### Functions / lambdas (Step 13)
- **Closure environments are never freed (leak).** A capturing lambda heap-
  allocates its env (a tuple of the captures) and the env is not freed on closure
  drop — `type_needs_drop(Fn)` is false. Same interim-leak class as pre-C.5c
  boxing; a future `Fn` drop-glue (free env) closes it. Captures are **by move**
  only (Copy captures are copied); by-borrow capture comes with the borrowing
  arrows.
- **No stack-allocated closures.** `A -> B` is one concrete type with a uniform,
  type-erased fat pointer `{ fn_ptr, env_ptr }`, so a capturing closure's env
  can't be stored inline at the use site, and bare `->` is first-class with no
  region bound, so an escaping closure's env must outlive its frame → heap. Not a
  hard prohibition: the deferred borrowing arrow `A →[Borrowed 'r] B` (§3.1
  region) would let a provably-non-escaping closure keep its env on the stack.
  (Moot today — the non-capturing milestone has an empty env.)
- **No recursive lambdas.** `let` is non-recursive (the bound name isn't in scope
  in its own initializer) and there's no `letrec`/`fix`; a lambda can't refer to
  itself by name, and self-capture would be a construction-order cycle needing
  indirection we don't have. Recursion is available via top-level `def`
  (mutually-recursively scoped). Deferred (`fix` is on the out-of-scope list).

### Borrows / regions
- **Generic deref `*r : T`** for an un-monomorphised type parameter `T` is
  rejected (the ownership pass can't see `T`'s `Copy`-ness). Works once
  specialised; revisit with `Copy`-bound generics.

---

## 6. Pointers

- Full design log + per-step detail + rationale: `TypeSystemPlan.md`.
- The normative calculus: `Calculus.md` (and `Calculus-soundness.md`).
- Heap-lowering pass: `lang/src/passes/heap_lower.rs`; drop glue:
  `lang/src/passes/llvm_codegen.rs`; core-lib strategy fns: `lang/src/core.sand`.
- Showcase example for the memory model: `examples/expr.sand`.
