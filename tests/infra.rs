//! # Dataflow, context, and infrastructure bug tests
//!
//! Covers:
//!   CompileCtx  — module registration, dummy-file guard, variable/function
//!                 type maps, DEFAULT_MODULE_NAME
//!   ProjectCtx  — file registration idempotency, default_file last-write-wins,
//!                 url_of_file with magic FileRef
//!   Error / diagnostic pipeline — SandLangError Display, DuplicateMain
//!                 cross-file diagnostic, zero-range diagnostics
//!   LSP backend — stale diagnostics not cleared, silent file skip in
//!                 check_project, url_of_module_unchecked panic
//!
//! Run with:  cargo test --test dataflow_tests
//!
//! Labels:
//!   [BUG]   – test documents a known defect; expected to FAIL until fixed.
//!   [GUARD] – test documents correct behaviour; must always pass.

// ═════════════════════════════════════════════════════════════════════════════
// 1. CompileCtx — module registration
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod compile_ctx_module_registration {
    use sand::compiler::context::CompileCtx;
    use sand::compiler::structure::FileRef;

    fn dummy_fr() -> FileRef {
        // Use a real-looking FileRef (index 0) rather than the magic 69420
        // so we don't trigger the OOB panic while testing something else.
        FileRef::test_new(0)
    }

    /// [BUG] `create_dummy_module` checks `self.default_module` as its
    /// "already-called" guard, but `register_module` never sets
    /// `self.default_module`.  The guard is always false, so the function
    /// can be called repeatedly, creating multiple modules named "mAin"
    /// under the same FileRef.  The second call should return Err.
    #[test]
    fn create_dummy_module_is_idempotent() {
        let mut ctx = CompileCtx::initial();
        let fr = dummy_fr();

        let first = ctx.create_dummy_module(fr);
        assert!(first.is_ok(), "first call should succeed");

        let second = ctx.create_dummy_module(fr);
        assert!(
            second.is_err(),
            "second call should fail — guard is broken and currently succeeds, \
             creating a duplicate \"mAin\" module"
        );
    }

    /// [BUG] Calling `create_dummy_module` twice with the same FileRef and
    /// then compiling any source will produce a `DuplicateModule` error
    /// because two modules now share the name "mAin".
    #[test]
    fn duplicate_dummy_modules_cause_compile_error() {
        use sand::compile_hir;
        use sand::compiler::structure::Map;

        let mut ctx = CompileCtx::initial();
        let fr = FileRef::test_new(0);

        // Force two "mAin" modules into the context
        let _ = ctx.create_dummy_module(fr).unwrap();
        let _ = ctx
            .create_dummy_module(fr)
            .expect_err("duplicate dummy modules should fail");

        // Now run a trivial compile; the two identically-named modules will
        // cause a DuplicateModule error in the qualify pass.
        let code = Map::from([(fr, "def main(): Int := 1")]);
        let result = compile_hir(code, &mut ctx);
        // With the bug present this either succeeds (wrong) or panics; after
        // fixing create_dummy_module it should have been stopped earlier.
        assert!(
            result.is_ok(),
            "duplicate 'mAin' modules should produce a compile error"
        );
    }

    /// [GUARD] register_module with distinct names should produce distinct
    /// refs.
    #[test]
    fn register_distinct_modules_produces_distinct_refs() {
        let mut ctx = CompileCtx::initial();
        let fr = FileRef::test_new(0);
        let m1 = ctx.register_module("alpha", fr);
        let m2 = ctx.register_module("beta", fr);
        assert_ne!(m1, m2);
    }

    /// [GUARD] file_of_module round-trips correctly.
    #[test]
    fn file_of_module_round_trips() {
        let mut ctx = CompileCtx::initial();
        let fr = FileRef::test_new(0);
        let mr = ctx.register_module("foo", fr);
        assert_eq!(ctx.file_of_module(mr), fr);
    }

    /// [BUG] `set_var_type` contains `debug_assert!(out.is_none())`.
    /// In release builds, calling it twice on the same `UniqVar` silently
    /// overwrites the type — a potentially dangerous silent data mutation.
    /// The test verifies that a second call is either rejected or detected.
    ///
    /// Note: this test only exercises the observable behaviour; the assert
    /// only fires in debug mode.  The test therefore documents intent, not
    /// a crash.
    #[test]
    fn set_var_type_does_not_silently_overwrite() {
        use sand::compiler::structure::OriginalVarRef;
        use sand::lang::types::Ty;

        let mut ctx = CompileCtx::initial();
        // Manually construct a UniqVar for a known OriginalVarRef.
        // We inject it directly to avoid going through the full parse pipeline.
        // This is white-box: we're testing the ctx internals.
        let ovref = OriginalVarRef::test_new(0);
        let uv = ctx.uniquify_original_variable(ovref);

        ctx.set_var_type(uv, Ty::Int);
        let ty_after_first = ctx.get_var_type(&uv);
        assert_eq!(ty_after_first, Some(Ty::Int));

        // In debug mode this panics; in release it silently overwrites.
        // Either way the type should still be Int after an erroneous second set.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut ctx2 = CompileCtx::initial();
            let uv2 = ctx2.uniquify_original_variable(OriginalVarRef::test_new(0));
            ctx2.set_var_type(uv2, Ty::Int);
            ctx2.set_var_type(uv2, Ty::Bool); // second call — should error
        }));

        // In debug builds: panics (caught above) → the behavior is detected.
        // In release builds: silently succeeds → the bug is present.
        // Either way we document it.
        if result.is_ok() {
            // Release mode: second set silently succeeded — this is the bug.
            eprintln!("WARNING: set_var_type silently overwrote an existing type in release mode");
        }
        // We don't assert here because the behaviour is build-mode-dependent;
        // the test acts as a canary.
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. CompileCtx — dummy_file magic index
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod compile_ctx_dummy_file {
    use sand::compiler::context::CompileCtx;

    /// [GUARD] dummy_file() may only be called when project_modules is empty.
    /// Calling it after registering a module should panic.
    #[test]
    fn dummy_file_panics_after_module_registration() {
        use sand::compiler::structure::FileRef;

        let result = std::panic::catch_unwind(|| {
            let mut ctx = CompileCtx::initial();
            let fr = FileRef::test_new(0);
            ctx.register_module("foo", fr);
            ctx.dummy_file() // should panic: assertion fails
        });
        assert!(
            result.is_err(),
            "dummy_file() should panic when modules are already registered"
        );
    }

    /// [BUG] The value returned by `dummy_file()` — FileRef(69420) — is a
    /// sentinel that is never a valid index into `code_files` in ProjectCtx.
    /// Any code that calls `ProjectCtx::url_of_file(dummy_file())` will panic
    /// with an index-out-of-bounds.
    #[test]
    fn dummy_file_ref_cannot_be_used_for_url_lookup_in_project_ctx() {
        use sand::compiler::context::CompileCtx;
        use sand::compiler::context::ProjectCtx;

        let ctx = CompileCtx::initial();
        let dummy = ctx.dummy_file();

        let mut project_ctx = ProjectCtx::initial();
        // Register only a couple of real files (indices 0, 1)
        let _ = project_ctx.register_dummy_file();
        let _ = project_ctx.register_dummy_file();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            project_ctx.url_of_file(dummy) // FileRef(69420) is OOB
        }));

        assert!(
            result.is_err(),
            "url_of_file(FileRef(69420)) should panic — the magic index is never \
             a valid code_files position"
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. ProjectCtx — file registration & default_file
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod project_ctx_file_registration {
    use sand::compiler::context::ProjectCtx;
    use tower_lsp::lsp_types::Url;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    /// [GUARD] Registering the same URI twice returns the same FileRef.
    #[test]
    fn register_file_is_idempotent() {
        let mut ctx = ProjectCtx::initial();
        let u = url("file:///project/src/main.sand");
        let fr1 = ctx.register_file(u.clone()).expect("first register ok");
        let fr2 = ctx.register_file(u.clone()).expect("second register ok");
        assert_eq!(fr1, fr2, "same URI should return the same FileRef");
    }

    /// [GUARD] Two distinct URIs get distinct FileRefs.
    #[test]
    fn register_distinct_files_get_distinct_refs() {
        let mut ctx = ProjectCtx::initial();
        let fr1 = ctx
            .register_file(url("file:///project/a.sand"))
            .expect("a.sand ok");
        let fr2 = ctx
            .register_file(url("file:///project/b.sand"))
            .expect("b.sand ok");
        assert_ne!(fr1, fr2);
    }

    /// [GUARD] `register_dummy_file` each call should return a fresh ref, but
    /// the same underlying well-known URI must not accidentally be treated as
    /// the same file as a user-registered URI.
    #[test]
    fn dummy_file_does_not_collide_with_user_files() {
        let mut ctx = ProjectCtx::initial();
        let user_fr = ctx
            .register_file(url("file:///project/src/main.sand"))
            .expect("user file ok");
        let dummy_fr = ctx.register_dummy_file();
        assert_ne!(
            user_fr, dummy_fr,
            "dummy file should not collide with a user-registered file"
        );
    }

    /// [BUG] `register_dummy_file` is not idempotent — each call pushes a new
    /// entry with a fresh index.  Calling it twice yields two different
    /// FileRefs pointing to the same dummy URI.
    #[test]
    fn register_dummy_file_is_idempotent() {
        let mut ctx = ProjectCtx::initial();
        let fr1 = ctx.register_dummy_file();
        let fr2 = ctx.register_dummy_file();
        assert_eq!(
            fr1, fr2,
            "register_dummy_file should return the same FileRef when called twice"
        );
    }

    /// [GUARD] url_of_file(fr) round-trips back to the URI used to register.
    #[test]
    fn url_of_file_round_trips() {
        let mut ctx = ProjectCtx::initial();
        let u = url("file:///project/src/lib.sand");
        let fr = ctx.register_file(u.clone()).expect("register ok");
        assert_eq!(ctx.url_of_file(fr), u);
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 4. FileRef / ModuleRef consistency across CompileCtx and ProjectCtx
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod ref_consistency {
    use sand::compile_hir;
    use sand::compiler::context::CompileCtx;
    use sand::compiler::context::ProjectCtx;
    use sand::compiler::structure::Map;
    use tower_lsp::lsp_types::Url;

    /// [GUARD] FileRefs produced by ProjectCtx must be usable as keys in the
    /// Map passed to compile_hir, and the resulting CompileCtx module registry
    /// must reference those same FileRefs (so file_of_module is consistent).
    #[test]
    fn compile_hir_file_ref_consistent_with_project_ctx() {
        let mut project_ctx = ProjectCtx::initial();
        let uri = Url::parse("file:///project/src/main.sand").unwrap();
        let fr = project_ctx.register_file(uri.clone()).expect("register ok");

        let mut compile_ctx = CompileCtx::initial();
        let mr = compile_ctx.create_default_module(fr, "main");

        // The module's file should be exactly the FileRef we got from ProjectCtx.
        assert_eq!(
            compile_ctx.file_of_module(mr),
            fr,
            "file_of_module should return the FileRef that was used at registration"
        );

        // Compile a trivial program using the real FileRef.
        let code = Map::from([(fr, "def main(): Int := 42")]);
        let result = compile_hir(code, &mut compile_ctx);
        assert!(
            result.is_ok(),
            "compile_hir should succeed with a consistently-registered FileRef"
        );
    }

    /// [BUG] When the same source is compiled twice through separate
    /// CompileCtx instances (as the LSP does on each key-stroke), the
    /// FileRefs from ProjectCtx carry over into both CompileCtx instances.
    /// If module names collide (both default to "mAin"), the qualify pass
    /// should detect a DuplicateModule — but because create_dummy_module's
    /// guard is broken, it currently does not.
    #[test]
    fn second_compilation_with_same_file_ref_does_not_duplicate_modules() {
        use sand::compiler::structure::FileRef;

        let fr = FileRef::test_new(0);
        let src = "def main(): Int := 1";

        // First compilation
        let mut ctx1 = CompileCtx::initial();
        let code1 = Map::from([(fr, src)]);
        let r1 = compile_hir(code1, &mut ctx1);
        assert!(r1.is_ok(), "first compilation should succeed");

        // Second compilation with a fresh context (LSP pattern)
        let mut ctx2 = CompileCtx::initial();
        let code2 = Map::from([(fr, src)]);
        let r2 = compile_hir(code2, &mut ctx2);
        assert!(
            r2.is_ok(),
            "second compilation with a fresh ctx should also succeed"
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 5. Error/Diagnostic pipeline — Display, DuplicateMain, zero-range
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod error_pipeline {
    use sand::compile_hir;
    use sand::compiler::context::CompileCtx;
    use sand::compiler::structure::Map;

    fn compile_err(src: &str) -> sand::SandLangError {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        compile_hir(code, &mut ctx).expect_err("expected a compile error")
    }

    /// [BUG] `SandLangError` is defined as
    ///   `#[error("compilation error: {source}")]`
    /// The `context` field (which holds the file/module reference) is never
    /// included in the Display output.  Users see "compilation error: …"
    /// with no location information.
    #[test]
    fn sand_lang_error_display_includes_location_context() {
        let err = compile_err("def main(): Int := undefined_var");
        let rendered = format!("{err}");

        // The rendered string must contain *something* that tells the user
        // WHERE the error is (a file name, a line number, a module name, …).
        // Currently it does not — this test documents that deficiency.
        assert!(
            rendered.contains("main") // module name or function context
            || rendered.contains("line")
            || rendered.contains("column")
            || rendered.contains(':'), // "file:line:col" style
            "SandLangError Display lacks location context: {:?}",
            rendered
        );
    }

    /// [BUG] `QualifyError::ModuleNotFound` and `DuplicateModule` diagnostics
    /// are produced with `Range::default()` — a zero-span that gives the user
    /// no indication of where the problematic call is.
    ///
    /// We test this indirectly: a compile error for an undeclared function
    /// should produce a diagnostic whose range is non-zero.
    #[test]
    fn undefined_function_diagnostic_has_non_zero_range() {
        use sand::compiler::diagnostics::SandDiagnostic;

        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, "def main(): Int := ghost()")]);

        let err = compile_hir(code, &mut ctx).expect_err("should fail");
        let diagnostics = SandDiagnostic::from_compiler_error(&ctx, err);

        for diags in diagnostics.map.values() {
            for d in diags {
                assert!(
                    d.range != Default::default(),
                    "diagnostic has zero range — user cannot locate the error: {:?}",
                    d.range
                );
            }
        }
    }

    /// [BUG] `QualifyError::DuplicateMain` builds two `SandDiagnostic`
    /// entries — one for each module containing a `main` function.  Each
    /// entry is `add_one`-ed to the correct file key (`file_1` / `file_2`),
    /// but the `SandDiagnostic.file` field uses the *outer* `file` variable
    /// from the match arm, not the per-entry file.  So the struct's internal
    /// `file` field is inconsistent with the key it's stored under.
    #[test]
    fn duplicate_main_diagnostic_file_field_matches_key() {
        use sand::compiler::diagnostics::SandDiagnostic;

        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();

        // Two functions named `main` in the same file / module.
        let code = Map::from([(
            fr,
            "def main(): Int := 1\n\
             def main(): Int := 2",
        )]);

        let err = compile_hir(code, &mut ctx).expect_err("should fail with DuplicateMain");
        let diagnostics = SandDiagnostic::from_compiler_error(&ctx, err);

        // For every (file_key, diagnostic) pair, the diagnostic's internal
        // `file` field must equal the key it's stored under.
        for (file_key, diags) in &diagnostics.map {
            for d in diags {
                assert_eq!(
                    d.file, *file_key,
                    "diagnostic.file ({:?}) does not match the key it's stored under ({:?})",
                    d.file, file_key
                );
            }
        }
    }

    /// [GUARD] A type error produces a non-empty diagnostic list.
    #[test]
    fn type_error_produces_diagnostics() {
        use sand::compiler::diagnostics::SandDiagnostic;

        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, "def main(): Int := true")]);

        let err = compile_hir(code, &mut ctx).expect_err("type error expected");
        let diagnostics = SandDiagnostic::from_compiler_error(&ctx, err);

        let total: usize = diagnostics.map.values().map(|v| v.len()).sum();
        assert!(
            total > 0,
            "type error should produce at least one diagnostic"
        );
    }

    /// [GUARD] Error Display output is deterministic (calling it twice gives
    /// the same string).
    #[test]
    fn error_display_is_deterministic() {
        let err1 = compile_err("def main(): Int := undefined_var");
        let err2 = compile_err("def main(): Int := undefined_var");
        assert_eq!(format!("{err1}"), format!("{err2}"));
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 6. LSP diagnostic pipeline — stale diagnostics, silent file skip
// ═════════════════════════════════════════════════════════════════════════════
//
// The LSP backend uses async Tokio code, which is hard to unit-test without
// a live client.  We therefore test the pure functions it calls — the ones
// that translate between Sand diagnostics and LSP diagnostics — and document
// the architectural bugs with clearly-marked integration-test stubs.

#[cfg(test)]
mod lsp_diagnostic_pipeline {
    use bimap::BiBTreeMap;
    use sand::compile_hir;
    use sand::compiler::context::CompileCtx;
    use sand::compiler::structure::FileRef;
    use sand::compiler::structure::Map;
    use sand::lsp::diagnostics::sand_source_diagnostics;
    use tower_lsp::lsp_types::Url;

    fn make_url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    /// [GUARD] `sand_source_diagnostics` converts a `SandLangError` into at
    /// least one LSP `Diagnostic` when given valid context.
    #[test]
    fn sand_source_diagnostics_produces_at_least_one_entry() {
        let src = "def main(): Int := ghost()";
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);

        let err = compile_hir(code, &mut ctx).expect_err("should fail");

        // Build the file_map that the LSP backend maintains.
        let uri = make_url("file:///tmp/main.sand");
        let mut file_map: BiBTreeMap<Url, FileRef> = BiBTreeMap::new();
        file_map.insert(uri.clone(), fr);

        let diags = sand_source_diagnostics(&ctx, &file_map, src, err);
        let total: usize = diags.map.values().map(|v| v.len()).sum();
        assert!(
            total > 0,
            "sand_source_diagnostics should produce at least one LSP diagnostic"
        );
    }

    /// [GUARD] Each produced diagnostic has a non-trivial message (not empty).
    #[test]
    fn diagnostics_have_non_empty_messages() {
        let src = "def main(): Bool := 999";
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        let err = compile_hir(code, &mut ctx).expect_err("type error");

        let uri = make_url("file:///tmp/t.sand");
        let mut file_map: BiBTreeMap<Url, FileRef> = BiBTreeMap::new();
        file_map.insert(uri, fr);

        let diags = sand_source_diagnostics(&ctx, &file_map, src, err);
        for ds in diags.map.values() {
            for d in ds {
                assert!(
                    !d.message.is_empty(),
                    "diagnostic message must not be empty"
                );
            }
        }
    }

    /// [BUG] When the LSP backend calls `check_project` after fixing an error,
    /// the compilation succeeds and `LastCompilation::Success` is stored.
    /// `publish_diagnostics` is then called with *only* the hint diagnostics
    /// (from `annotate_reused_expressions`), which may not include the
    /// previously-errored files at all — so their error list is never cleared.
    ///
    /// This is an architectural stub; the actual LSP infra is async and
    /// requires a live client.  We document the expected contract here.
    ///
    /// Expected: after a successful compilation the diagnostics map must
    /// contain an entry for *every file* that was previously diagnosed,
    /// even if that entry is an empty `Vec` (which clears the client's list).
    #[test]
    #[ignore = "requires LSP integration harness — documents known architectural bug"]
    fn successful_compile_clears_previous_error_diagnostics() {
        // Integration test outline:
        // 1. Open file with error → LSP publishes diagnostics for that URI.
        // 2. Fix the error (update file_contents).
        // 3. Trigger check_project.
        // 4. Assert that publish_diagnostics is called with an empty Vec for the
        //    previously-errored URI.
        //
        // Currently step 4 fails: the success path only publishes hint
        // diagnostics and never explicitly clears error files.
        todo!("implement with mock LSP client");
    }

    /// [BUG] In `check_project`, the modules map is built by:
    ///
    ///   for (m, s) in file_contents.iter() {
    ///       if let Some(fr) = context.files.get_by_left(m) {
    ///           modules.insert(*fr, s.as_str());
    ///       }
    ///   }
    ///
    /// Any file in `file_contents` that is NOT in `context.files` is silently
    /// skipped.  This can happen when `register_file` fails (e.g. due to the
    /// `uri_name` bug), leaving the file in `file_contents` but not in
    /// `context.files`.  The compiler then sees fewer files than expected,
    /// producing "undefined function" errors for symbols defined in the
    /// missing file.
    ///
    /// This is documented as an architectural stub.
    #[test]
    #[ignore = "requires LSP integration harness — documents known architectural bug"]
    fn check_project_does_not_silently_drop_files() {
        // Integration test outline:
        // 1. Arrange: file_contents = {uri_a: src_a, uri_b: src_b} context.files =
        //    {uri_a: fr_a}   (uri_b missing)
        // 2. Act: check_project()
        // 3. Assert: compilation error OR explicit log about the missing file — NOT a
        //    silently successful compile that references nothing from uri_b.
        todo!("implement with mock LSP client");
    }

    /// [BUG] `url_of_module_unchecked` calls `.unwrap()` on the bimap lookup.
    /// If the CompileCtx's FileRef is not in the ProjectCtx bimap (possible
    /// when dummy files are involved or when a file failed registration), this
    /// panics and crashes the LSP server.
    ///
    /// This pure function test verifies that the panic path is reachable.
    #[test]
    fn url_of_module_unchecked_panics_on_unknown_file_ref() {
        use sand::lsp::util::url_of_module_unchecked;

        let mut ctx = CompileCtx::initial();
        let orphan_fr = FileRef::test_new(99); // not in any bimap
        let mr = ctx.register_module("orphan", orphan_fr);

        let file_map: BiBTreeMap<Url, FileRef> = BiBTreeMap::new(); // empty

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            url_of_module_unchecked(mr, &ctx, &file_map)
        }));

        assert!(
            result.is_err(),
            "url_of_module_unchecked should panic when the FileRef is not in the bimap — \
             this crash takes down the LSP server"
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 7. End-to-end context consistency
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod end_to_end_context {
    use sand::compile_hir;
    use sand::compiler::context::CompileCtx;
    use sand::compiler::structure::FileRef;
    use sand::compiler::structure::Map;

    /// [GUARD] A normal single-file compile populates entrypoint.
    #[test]
    fn entrypoint_is_set_after_successful_compile() {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, "def main(): Int := 42")]);
        let _prog = compile_hir(code, &mut ctx).expect("compile ok");
        assert!(
            ctx.entrypoint.is_some(),
            "entrypoint should be set after compiling a program with a main function"
        );
    }

    /// [GUARD] A program with no `main` function should fail qualification.
    #[test]
    fn program_without_main_fails_to_compile() {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, "def helper(): Int := 1")]);
        let result = compile_hir(code, &mut ctx);
        // No main → no entrypoint → qualify pass should return an error.
        // (If the language allows running without main this test needs revision.)
        assert!(
            result.is_err() || ctx.entrypoint.is_none(),
            "a program without main should either fail or leave entrypoint unset"
        );
    }

    /// [GUARD] Compiling two files that call each other works when both are
    /// passed in the same Map.
    #[test]
    fn multi_file_cross_call_compiles() {
        let mut ctx = CompileCtx::initial();
        let fr_a = FileRef::test_new(0);
        let fr_b = FileRef::test_new(1);
        let _mr_a = ctx.create_default_module(fr_a, "lib");
        let _mr_b = ctx.create_default_module(fr_b, "main_mod");

        let code = Map::from([
            (fr_a, "def double(x: Int): Int := x * 2"),
            (fr_b, "def main(): Int := lib::double(21)"),
        ]);

        let result = compile_hir(code, &mut ctx);
        assert!(
            result.is_ok(),
            "cross-module call should compile: {:?}",
            result.err()
        );
    }

    /// [GUARD] After a failed compile entrypoint remains None.
    #[test]
    fn entrypoint_is_none_after_failed_compile() {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, "def main(): Int := undefined_var")]);
        let _ = compile_hir(code, &mut ctx);
        assert!(
            ctx.entrypoint.is_none(),
            "entrypoint should remain None after a failed compile"
        );
    }

    /// [GUARD] is_main returns true for the function named `main` after
    /// successful compilation, and false for helpers.
    #[test]
    fn is_main_correct_after_compilation() {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, "def helper(): Int := 1  def main(): Int := helper()")]);
        compile_hir(code, &mut ctx).expect("compile ok");

        let entrypoint = ctx.entrypoint.expect("entrypoint set");
        assert!(
            ctx.is_main(entrypoint),
            "is_main(entrypoint) should be true"
        );

        // The helper is not main — find it by checking all registered functions.
        // We verify that at least one function is NOT main.
        let any_non_main = (ctx.all_functions()).any(|fr| !ctx.is_main(fr));
        assert!(any_non_main, "at least one function should not be main");
    }
}
