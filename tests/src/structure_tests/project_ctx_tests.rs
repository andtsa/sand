//! Tests for ProjectCtx file registration
//!
//! Verifies project context file registration, tracking, and URL lookups.

use lang::compiler::context::ProjectCtx;
use url::Url;

fn url(s: &str) -> Url {
    Url::parse(s).unwrap()
}

#[test]
fn register_file_is_idempotent() {
    let mut ctx = ProjectCtx::initial();
    let u = url("file:///project/src/main.sand");
    let fr1 = ctx.register_file(u.clone()).expect("first register ok");
    let fr2 = ctx.register_file(u.clone()).expect("second register ok");
    assert_eq!(fr1, fr2, "same URI should return the same FileRef");
}

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

#[test]
fn dummy_file_does_not_collide_with_user_files() {
    let mut ctx = ProjectCtx::initial();
    let user_fr = ctx
        .register_file(url("file:///project/src/main.sand"))
        .expect("user file ok");
    let dummy_fr = ctx.dummy_file();
    assert_ne!(
        user_fr, dummy_fr,
        "dummy file should not collide with a user-registered file"
    );
}

#[test]
fn dummy_file_is_idempotent() {
    let mut ctx = ProjectCtx::initial();
    let fr1 = ctx.dummy_file();
    let fr2 = ctx.dummy_file();
    assert_eq!(
        fr1, fr2,
        "dummy_file() should return the same FileRef when called twice"
    );
}

#[test]
fn url_of_file_round_trips() {
    let mut ctx = ProjectCtx::initial();
    let u = url("file:///project/src/lib.sand");
    let fr = ctx.register_file(u.clone()).expect("register ok");
    assert_eq!(ctx.url_of_file(fr), u);
}
