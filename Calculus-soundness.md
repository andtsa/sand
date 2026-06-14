# Calculus Soundness & Backend Audit

Companion to [`Calculus.md`](Calculus.md). Two things live here:

1. **Part I — Intuitive soundness audit.** Is the calculus, as written, *plausibly*
   type-safe? What are the load-bearing obligations, and where are the genuine
   gaps that stop "is it sound?" from even being a well-formed question yet.
2. **Part II — Backend code-read audit.** Do the two interpreters
   ([`mir.rs`](lang/src/interpreter/mir.rs),
   [`typed_hir.rs`](lang/src/interpreter/typed_hir.rs)) and the LLVM binary
   ([`llvm_codegen.rs`](lang/src/passes/llvm_codegen.rs),
   driven from [`compile.rs`](sand-cli/src/compile.rs)) faithfully realise the
   *sound* version of the calculus?

Divergences between the calculus-as-written and the implementation are recorded
*inline* in `Calculus.md` as `> **Divergence:**` callouts; this file is the
reasoning behind them plus the parts that don't attach to a single rule.

The audit bar (per the request) is **intuitive**, not mechanized: prose
arguments and gap-finding, not Coq/Redex proofs.

---

## Part I — Intuitive Soundness Audit

### I.0 The central caveat: there is no operational semantics

"Sound" for a type system means **progress** (a well-typed closed term is a value
or steps) and **preservation** (a step keeps the type). Both quantify over a
reduction relation `e → e'`. **`Calculus.md` defines no such relation.** It gives
syntax (§3), static typing (§5–6), and a kind lattice (§1), but nothing dynamic.

So, strictly, soundness is **not yet a well-formed question** about `Calculus.md`
alone. The closest thing to an operational semantics that exists is the two
*interpreters* — which means today the interpreters are the de-facto dynamics, and
"is the calculus sound" really means "do the static rules reject every program the
interpreters would get stuck on (or miscompile in LLVM)."

Everything below is therefore an argument about the static rules *relative to the
intended dynamics the interpreters encode*. If you ever want real metatheory, the
first task is to write the small-step relation; until then this is the honest
ceiling on rigor.

### I.1 What the type system must guarantee (the obligations)

Beyond ordinary progress/preservation, this language's static rules are
load-bearing for four runtime properties:

| # | Property | Enforced by | Rule(s) |
|---|----------|-------------|---------|
| O1 | **No use-after-move** (affine ownership) | ownership pass, *not* the type rules | §6.2 Var-Owned (in spirit) |
| O2 | **No dangling reference** (region/escape safety) | type checker, `freeRegions` on the type | §6.3 Block, §6.4 Let-Borrow |
| O3 | **No aliasing violation** (`&mut` exclusivity) | ownership pass | §9 (impl), absent from `Calculus.md` |
| O4 | **No leak / double-free** (RAII drop) | drop elaboration | §6.11 |

The honest status of each:

- **O2 is the one the calculus actually pins down well**, and the implementation
  matches it *literally* post-R1: the escape check is `'r ∉ freeRegions(T)` on the
  **type** at every scope boundary (block close + function return). This is the
  soundest part of the whole stack and the rules read correctly. See I.3.
- **O1 and O3 are real, but the calculus underspecifies them** — see I.2, the
  biggest soundness-presentation gap.
- **O4 is described (§6.11) but observationally inert** everywhere today (drop is
  a no-op in all three backends), so it is *unverified at runtime*. See II.3.

### I.2 The biggest gap: affinity is not expressed in the typing rules

§6.2 (Var-Owned) removes `x` from `Γ` on use, which is the *standard* way to write
a linear/affine discipline. But the surrounding rules **never split the context**:

- §6.5 App-Owned checks `e₁ ⇒ T→U` and `e₂ ⇐ T` against the *same* `Γ`.
- §6.8 If checks `e₂` and `e₃` against the same `Γ`.
- §6.4 Let threads `Γ` linearly but App/If/Match do not.

In a genuinely affine calculus you need `Γ = Γ₁ ⊎ Γ₂` (disjoint context splitting)
or a usage/multiplicity annotation, otherwise the rules as written let the *same*
owned variable be consumed in both `e₁` and `e₂` of an application — a
use-after-move the rules don't reject.

**Why this isn't a bug in the compiler:** the implementation does *not* enforce
affinity in the type checker at all. It runs a separate `passes/ownership/` pass
(an `OwnershipEnv` move/borrow dataflow) that catches double-use, move-while-
borrowed, and `&mut` exclusivity. So O1 and O3 hold *operationally* — they're just
enforced somewhere the calculus doesn't describe.

**Recommendation (intuitive-soundness level):** either (a) add a one-paragraph §4
note that the affine discipline is discharged by a separate ownership analysis and
the typing judgment is deliberately affine-agnostic, or (b) make the rules
context-splitting if you want the calculus to be self-contained. Today a reader
who trusts §6 alone would wrongly conclude the type system catches
use-after-move. This is recorded as the §4 divergence.

### I.3 Region safety (O2) — reads correctly

The model in the Reference-Representation step's "Sound Region Model (NORMATIVE)"
is the authoritative version and it is internally consistent:

- Regions form an outlives lattice (`'static` ≥ params ≥ frame ≥ blocks by depth).
- References carry their region; `&` covariant in region, `&mut` invariant.
- Escape = `freeRegions(T)` at boundaries, which propagates through tuples and
  (post-R5) ADTs for free, so escape-via-data is caught structurally.
- Calls compute the result region as a GLB bounded below by the call-site scope,
  which is *total* (the argument about a guaranteed lower bound is correct — two
  incomparable lifetime params bottom out at the local call-site region, yielding
  a correctly-non-returnable local result).

Two intentional, *sound* divergences from the literal calculus are noted in the
plan and are genuinely conservative (strictly fewer programs accepted):
borrows are tied to their *referent's storage* region (per-scope) rather than each
`let &x` getting a fresh region (§6.4 literal), and a borrowed binding is
represented as a value of pointer type `&T` rather than "`T` at kind Borrowed". I
concur these are safe — coarser-but-conservative, and the second is just a
representation choice the calculus permits.

**One thing to verify if you ever formalize:** the `meet`/GLB result-region
computation (Reference-Rep item 8) is asserted total but the argument leans on
"the call site is in scope of every argument." That holds for the surface
language (you can't name an out-of-scope region), but it is exactly the kind of
side-condition a mechanized proof would force you to state as an invariant of
well-formed call contexts. Flagging it as the one non-obvious step.

### I.4 The kind lattice (§1) — sound, but the "region-free" claim is now false

The `{Owned, Borrowed, BorrowedMut, InteriorMut, Never}` lattice with `Owned` top,
`Never` bottom, three incomparable borrow modes is a clean lattice; join (§1.4) is
the correct LUB. Subkinding (§1.2) as subsumption is fine. No soundness concern in
the lattice itself.

**But** §1.1 spends a paragraph asserting "**Kinds are region-free** … a borrow's
region is part of its type, not its kind." The implementation contradicts this:
`Kind::Borrowed(Region)` and `Kind::BorrowedMut(Region)` *carry* the region
(introduced in Step 7/9a, originally so the Step 8b escape check could read the
region off the kind). Post-R1 the escape check moved onto the type's
`freeRegions`, so the region-on-the-kind is now *vestigial* for safety but still
present in the data type. This is a documentation/representation divergence, not
an unsoundness — but the calculus's stated rationale no longer matches the code.
Recorded as the §1 divergence.

### I.5 Subtyping (§6.1 Sub) is used but never defined

Rule (Sub) invokes `T <: T'`, but `Calculus.md` gives **no rules for `<:` on
types** — only subkinding `k <: k'` is defined (§1.2). So the one judgment that
"enables implicit coercions" rests on an undefined relation.

In the implementation, `<:` on types is exactly two things: (1) region-aware
reference subtyping (`&` covariant, `&mut` invariant in the region) and (2) the
`Never`-coercion (a diverging expression inhabits any type). There is **no**
general structural/width/depth subtyping — concrete non-reference types are
compared by interned pointer identity (equality). That's a perfectly sound choice,
but the calculus should *say* it: define `<:` as "equality, refined by region
variance on references, plus `Never <: anything`." As written, §6.1 overstates the
generality. Recorded as the §6.1 divergence.

### I.6 Per-rule intuitive verdicts (the rest)

- **§5 Kinding** — sound and matches impl for the implemented fragment
  (`Int/Bool/Unit/Var/Region/Borrow/BorrowMut/App/Tuple`). `K-Arrow`, `K-Slot` are
  for unimplemented features (function types, `Slot`). `K-HeapedRec` is realised
  as the `deriving Heaped` requirement — same intent, different surface.
- **§6.3 Block / §6.4 Let** — sound; the implemented escape check is the literal
  rule. ✓
- **§6.8 If / §6.9 Match** — "both branches agree on type and kind" is right; the
  impl additionally meets the branch *regions* so escape-through-a-branch is
  caught (a refinement of, not a contradiction to, the rule). Match "always
  consumes the scrutinee" holds.
- **§6.11 Drop** — the *placement* logic (reverse-decl order, completing drops at
  merges, no runtime flags) is sound and implemented (Step B). The *effect* (free)
  is inert until Step C lands for all types; see O4 / II.3.
- **§6.5 App / §6.6 Box / §6.7 Ascribe / §6.10 Reuse / §7.1–7.3 HKT classes** —
  describe **unimplemented** features (lambdas, `box`, ascription expressions,
  `reuse`, Functor/Monad). They can't be unsound (nothing realises them) but they
  are aspirational, not descriptive. Each is flagged inline so a reader doesn't
  assume they exist.

### I.7 Soundness audit — bottom line

- The **region/escape** story (O2) is the strong part: well-specified, and the
  implementation matches it literally. Intuitively sound.
- The **affine** story (O1/O3) is *operationally* sound but **not expressed in the
  calculus's typing rules** — the rules don't split contexts and the real
  enforcement is a separate pass. This is the one place the calculus is
  *misleading* as a standalone artifact. Fix by documenting (or by adding
  context-splitting).
- **`<:` is undefined** though used; pin it down.
- **Drop/free** (O4) is described but unverified at runtime — soundness of the
  *memory* model is currently untested because nothing frees yet.
- Nothing here is a *latent unsoundness in the implementation* that I can see; the
  gaps are (a) presentation gaps in the calculus and (b) features described ahead
  of implementation.

---

## Part II — Backend Code-Read Audit

Three backends realise the dynamics:

- **typed-HIR interpreter** ([`typed_hir.rs`](lang/src/interpreter/typed_hir.rs)) —
  runs on the typed HIR, *before* monomorphisation, so it additionally does
  runtime typeclass dispatch (`eval_method_call`).
- **MIR interpreter** ([`mir.rs`](lang/src/interpreter/mir.rs)) — runs *after*
  mono on concrete MIR; no generics, no `MethodCall`.
- **LLVM codegen** ([`llvm_codegen.rs`](lang/src/passes/llvm_codegen.rs)) — the
  real binary, driven by [`compile.rs`](sand-cli/src/compile.rs)
  (`MirProgram::from_typed_program` → `LlvmCodegen::emit_program` → object → `cc`).

### II.1 What the three agree on (faithful to the calculus)

- **Reference / write-through semantics (§3.2, §6.4).** Both interpreters model
  storage as an `Rc<RefCell<…>>` cell graph; a reference is a *shared handle* to a
  cell, and `*r = e` stores into that cell — observable across aliases and across
  call frames (a `Ref` arg shares its `Rc` into the callee's param cell). This is
  the same observable behaviour LLVM gets from `alloca` slots +
  `load`/`store`/`place_address`. The MIR interpreter's `place_cell` (follow
  `Deref` via the held `Ref`) and codegen's `place_address` (follow `Deref` via a
  `load`) are structurally the same algorithm. **Good alignment.** ✓
- **Aggregates / enums / tuples.** Field 0 = discriminant, field 1 = payload
  convention is identical across the MIR interpreter (`MirValue::EnumVariant`) and
  codegen (`emit_field`/`emit_aggregate`). ✓
- **`size_of`** lowers to `RValue::SizeOf` in both, but with different values —
  see II.4.

### II.2 The interpreters cannot catch O1/O2/O3 violations — by construction

This is the most important audit finding for "do the backends reflect the *sound*
calculus."

- **Affinity (O1).** `MirValue`/`Value` are `.clone()`d on every read. A "move" in
  the interpreter is a deep copy; a moved-from variable's cell still holds its
  value, so **using a moved value still works** in the interpreter. The
  interpreters implement *value-copy* semantics, not affine semantics.
- **Regions (O2).** The cell graph **never deallocates** (`free`/`Drop` are
  no-ops; the `Rc` reclaims cells only when the meta-level `Rc` count hits zero).
  So a *missed* escape check (returning `&local`) would simply… keep working in
  the interpreter, because the cell outlives the frame via its `Rc`. No dangling,
  no crash, no observable error.
- **Exclusivity (O3).** Not modelled at all at runtime.

**Consequence:** all three of O1/O2/O3 are enforced *only* at compile time (type
checker + ownership pass). The runtime is deliberately permissive. This is sound
*given* the static passes run first and are correct — but it means **the
interpreters provide zero independent check on the static analyses.** A
region-soundness or affinity bug would be invisible to interpreter-based tests.
The comments in both files acknowledge this ("the static region/escape checker
already guarantees no dangling, so the interpreter never models deallocation").

If you want runtime to *witness* region/affine bugs, the LLVM path is the only
candidate — and even it can't today, because nothing frees (II.3). So right now
**no backend can falsify a soundness bug in O1–O3.** That's the single biggest
"backends vs. the sound calculus" gap. It's a known/accepted design point, not a
regression, but it's worth stating plainly: the soundness of O1–O3 rests entirely
on the front-end, untested by execution.

### II.3 Drop/free (O4) is inert everywhere — the memory model is unexecuted

- typed-HIR: `Expression::Block { drops, .. }` ignores `drops`; `DropInPlace` → `Unit`.
- MIR interp: `Statement::Drop { .. } => Ok(())`; `DropInPlace` → `Unit`.
- LLVM: `Statement::Drop { .. } => {}`; `DropInPlace` → unit const.

And `emit_aggregate` **unconditionally `malloc`s** every payload-carrying *heaped*
enum cell (`{i64, ptr}` + a separate payload malloc) **with no `free`** — the
interim Step-C boxing. So:

- Heaped/recursive programs **leak** under LLVM (acknowledged: "keep working and
  keep leaking, as today").
- §6.11's "uniformly consumed at the merge — no leak" is realised only as *drop
  placement*; the placement is correct (Step B has structural tests) but the
  *effect* is a no-op until Step C.5 wires `unique_release`.

**Faithful to the plan, divergent from the calculus.** §6.11 reads as if drops
free; today they don't. The whole O4 guarantee is therefore unverified by any
execution. When Step C.5 lands, this is the area to re-audit hardest (double-free
vs. leak on every branch).

### II.4 `size_of` diverges between interpreter and binary (intended)

`interp_size_of` is a **layout-free approximation** in both interpreters
(`RValue::SizeOf` → an int the interpreter makes up), while codegen emits the
*real* `TargetData` size. A program whose output depends on `size_of::<T>()` will
**print different numbers under the interpreter vs. the compiled binary.** This is
documented and deliberate (the interpreter heap is a cell graph with no byte
layout), but it means the interpreters are *not* a faithful oracle for any
size-dependent program. If you build differential tests later, exclude
size-dependent programs or special-case them.

### II.5 Smaller backend observations

1. **`malloc` ignores its size argument** in both interpreters (`malloc`/`calloc`
   → one fresh cell). Only single-value allocation works; an `alloc`+`offset`
   array pattern would silently misbehave under the interpreter (returns one cell,
   no offset arithmetic). Matches the Step-A "single Int" scope; a hazard if
   `Ptr`/array code grows before the interpreter heap does.
2. **Deref of a non-reference is silently identity** in typed-HIR
   (`Expression::Deref(inner) => match … { Value::Ref(c) => …, v => Ok(v) }`, and
   `eval_place`'s `_ => cell(eval(expr))`). The type checker should make this
   unreachable, but unlike most other "shouldn't happen" sites (which
   `internal_bug!`/`unreachable!`) this one **masks** the bug instead of surfacing
   it. Low risk, but inconsistent with the file's own defensive style; consider an
   `internal_bug!` on the non-`Ref` deref arm to catch lowering errors.
3. **Divergent failure modes between the two interpreters.** typed-HIR returns
   `Err(BinOpTypeError/…)` on a type mismatch; MIR `internal_bug!`-panics. Both are
   "type checker should have prevented this," but the asymmetry means a lowering
   bug shows up as a clean error in one and a panic in the other. Acceptable,
   noting it.
4. **Enum equality** is handled consistently: MIR compares payloads only when both
   sides are `Some` (with a justifying comment); typed-HIR compares `pl_l == pl_r`
   directly. Both sound given a variant's payload-presence is fixed by
   declaration. Ordering comparisons (`<`,`>`) on enums use `variant_idx` in both. ✓
5. **`println` of a payload enum prints only the variant name** in codegen
   (`emit_print_value` ignores the payload), whereas both interpreters'
   `fmt_value` print `tag(payload)`. **The compiled binary and the interpreters
   produce different output for `println` of a non-nullary enum.** Verify whether
   any test/example relies on this; it's a real interp-vs-binary observable
   divergence beyond `size_of`.

### II.6 Backend audit — bottom line

- Reference/write-through and aggregate semantics are **faithfully and
  consistently** realised across all three backends. ✓
- The backends are **permissive by design**: they cannot witness O1/O2/O3
  violations, and O4 (free) is inert. So execution validates *functional*
  behaviour, not *safety* — safety rests entirely on the front-end passes.
- Two real **interp-vs-binary output divergences** exist today: `size_of`
  (II.4) and `println` of a payload enum (II.5). Both matter if you ever treat the
  interpreter as an oracle for the binary.
- One small robustness nit: silent identity-deref of a non-reference (II.5.2).

---

## Appendix — Feature implementation status vs. `Calculus.md`

Quick map of what the calculus describes vs. what exists (drives the inline
divergence callouts).

| Calculus § | Feature | Status |
|---|---|---|
| §1 | `Owned/Borrowed/BorrowedMut/Never` kinds | ✅ (kind carries region; `InteriorMut` reserved) |
| §2.1 | Variance defaults (producer/consumer) | ⚠️ always-covariant default; full polarity deferred (Step 13) |
| §2.3 | `&'r T`, `&'r mut T`, `T @ 'r` | ✅ |
| §2.3 | `T₁ →[k] T₂` function types | ❌ deferred (Step 13) |
| §2.3 | `Box<T>`, `Slot<L>` | ❌ (`Box`→`Unique`/`Heaped`; `Slot` is Step D) |
| §3.1 | lambdas `fn (x:T)->e` | ❌ deferred (Step 13) |
| §3.2 | `box(e)` | ❌ superseded by `deriving Heaped` |
| §3.2 | deref `*r` | ✅ *(added by impl; absent from calculus grammar)* |
| §3.2 | write-through `*r = e` | ✅ *(added by impl R3; absent from calculus)* |
| §3.2 | `e : T` ascription expression | ❌ not a surface expression |
| §4 | affine `Γ` discipline | ⚠️ enforced in ownership pass, not the type rules |
| §6.1 | type subtyping `<:` | ⚠️ region-variance + `Never` only; undefined in calculus |
| §6.3/6.4 | escape check `'r ∉ freeRegions(T)` | ✅ literal match |
| §6.5 | App / lambda typing | ❌ deferred |
| §6.6 | `Box` typing | ❌ superseded |
| §6.9 | match Heaped drain / `Slot` | ❌ Step D |
| §6.10 | `reuse` | ❌ Step D |
| §6.11 | RAII drop placement | ✅ placement; ⚠️ free is inert until Step C.5 |
| §7.1–7.3 | Functor / Applicative / Monad | ❌ deferred (Step 11, needs HKT) |
| §7.4 | Clone / Copy | ✅ (Step 14) |
| §7.5 | Heaped / HeapedUnique / HeapedShared | ⚠️ base `Heaped` partial (Step C); Unique extras Step D; Shared Step E |
| §8.3 | `band` for bitwise AND | ⚠️ shipped as `&&` (bitwise) + `and` (logical) instead |
| §8.5/8.6 | lambda / box grammar | ❌ deferred / superseded |
| — | `deriving`, `extern` keywords | ✅ added, *not in calculus §8 keyword list* |
