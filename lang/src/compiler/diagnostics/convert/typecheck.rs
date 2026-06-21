//! turn AstTypeError to SandDiagnostics

use crate::compiler::context::CompileCtx;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::compiler::diagnostics::SdRelatedInfo;
use crate::compiler::structure::FileRef;
use crate::passes::type_ast::AstTypeError;
use crate::passes::type_ast::errors::required_by_suffix;

pub fn type_error_to_diagnostic<'tcx>(
    ctx: &CompileCtx<'tcx>,
    file: FileRef,
    err: &AstTypeError<'tcx>,
) -> SandDiagnostics {
    use crate::passes::type_ast::AstTypeError::*;
    let mut diagnostics = SandDiagnostics::default();
    match err {
        NotCallable { range, .. } => {
            diagnostics.add_one(file, SandDiagnostic::error(file, *range, err.to_string()));
        }
        UnboundVariable { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *range,
                    format!("unbound variable '{}'", name),
                    SdRelatedInfo {
                        file,
                        range: *range,
                        message: "no binding found for this variable".into(),
                    },
                ),
            );
        }
        ImmutableAssignment { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *range,
                    format!("cannot assign to immutable variable '{}'", name),
                    SdRelatedInfo {
                        file,
                        range: *range,
                        message: "variable is not declared with 'mut'".into(),
                    },
                ),
            );
        }
        UndefinedFunction { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *range,
                    format!("undefined function '{}'", name),
                    SdRelatedInfo {
                        file,
                        range: *range,
                        message: "no function with this name was found".into(),
                    },
                ),
            );
        }
        TypeError {
            message,
            expected,
            found,
            range,
        } => {
            let diagnostic_message = format!(
                "{} (expected {}, found {})",
                message,
                ctx.display_ty(*expected),
                ctx.display_ty(*found)
            );

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: format!(
                    "expected type: {}, found type: {}",
                    ctx.display_ty(*expected),
                    ctx.display_ty(*found)
                ),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(file, *range, diagnostic_message, related),
            );
        }
        FunctionCallTypeError {
            message,
            expected,
            found,
            range,
        } => {
            let fmt_tys = |tys: &[_]| {
                tys.iter()
                    .map(|t| ctx.display_ty(*t).to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let diagnostic_message = format!(
                "{} (expected [{}], found [{}])",
                message,
                fmt_tys(expected),
                fmt_tys(found)
            );

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: format!(
                    "expected argument types: [{}], found argument types: [{}]",
                    fmt_tys(expected),
                    fmt_tys(found),
                ),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(file, *range, diagnostic_message, related),
            );
        }
        TagWithoutContext { variant, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "bare tag '#{variant}' cannot be used here: no expected type to resolve it against"
                    ),
                ),
            );
        }
        TagInNonEnumContext { variant, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!("bare tag '#{variant}' used where a non-enum type was expected"),
                ),
            );
        }
        UnknownTagVariant {
            variant,
            enum_name,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!("unknown variant '{variant}' on enum type '{enum_name}'"),
                ),
            );
        }
        TagPayloadOnNullaryVariant { variant, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!("variant '#{variant}' takes no payload, but a payload was provided"),
                ),
            );
        }
        TagMissingPayload { variant, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!("variant '#{variant}' expects a payload, but none was provided"),
                ),
            );
        }
        MatchNonAggregateScrutinee { ty, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "match scrutinee has type {}; match requires an enum type",
                        ctx.display_ty(*ty)
                    ),
                ),
            );
        }
        NonExhaustiveMatch {
            enum_name,
            uncovered,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "match on '{enum_name}' is not exhaustive; missing variants: {}",
                        uncovered.join(", ")
                    ),
                ),
            );
        }
        MatchWrongEnumType {
            expected_enum,
            found_enum,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "match arm pattern is for enum '{found_enum}' but scrutinee has type '{expected_enum}'"
                    ),
                ),
            );
        }
        ConstructorPayloadMismatch {
            enum_name,
            variant,
            expected_payload,
            range,
        } => {
            let message = if *expected_payload {
                format!(
                    "constructor '{enum_name}#{variant}' expects a payload, but none was supplied"
                )
            } else {
                format!(
                    "constructor '{enum_name}#{variant}' does not take a payload, but one was supplied"
                )
            };
            diagnostics.add_one(file, SandDiagnostic::error(file, *range, message));
        }
        PatternPayloadMismatch {
            enum_name,
            variant,
            expected_payload,
            range,
        } => {
            let message = if *expected_payload {
                format!(
                    "pattern '{enum_name}#{variant}' should destructure its payload but doesn't (e.g. write '{enum_name}#{variant}(x)' or '{enum_name}#{variant}(_)')"
                )
            } else {
                format!(
                    "variant '{enum_name}#{variant}' does not carry a payload, but the pattern tries to destructure one"
                )
            };
            diagnostics.add_one(file, SandDiagnostic::error(file, *range, message));
        }
        PatternArityMismatch {
            expected,
            found,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "tuple pattern has {found} element(s) but the matched type has {expected}"
                    ),
                ),
            );
        }
        PatternTypeMismatch { message, range } => {
            diagnostics.add_one(file, SandDiagnostic::error(file, *range, message.clone()));
        }
        LetPatternElseMissing { range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    "`let E#V(…) = …` requires an `else` branch because the pattern is refutable"
                        .to_string(),
                ),
            );
        }

        NestedVariantInLetPattern { range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    "the sub-pattern inside a `let E#V(…)` constructor must be irrefutable (bindings, wildcards, tuple-of-bindings); use `match` for nested refutable patterns".to_string(),
                ),
            );
        }

        LetPatternElseNotIrrefutable { range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    "the `else` expression must be a constructor of the same variant as the LHS pattern so that destructuring the fallback always succeeds".to_string(),
                ),
            );
        }

        CannotInferTypeArguments { enum_name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "cannot infer the type arguments of generic enum '{enum_name}'; add a type annotation"
                    ),
                ),
            );
        }

        RegionEscape { range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    "borrow would escape its scope: the value it refers to does not live long enough".to_string(),
                ),
            );
        }

        RegionConstraintUnsatisfied {
            longer,
            shorter,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "call does not satisfy the callee's lifetime constraint `'{longer} >= '{shorter}`"
                    ),
                ),
            );
        }

        TypeclassNoInstance {
            class,
            ty,
            range,
            required_by,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "no instance of typeclass '{class}' for type {ty}{}",
                        required_by_suffix(class, required_by)
                    ),
                ),
            );
        }
        TypeclassCannotResolve { method, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "cannot determine the receiver type for method '{method}' from its arguments"
                    ),
                ),
            );
        }
        TypeclassNeedsConstraint { method, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "method '{method}' is called on a type parameter not constrained by a `where` clause"
                    ),
                ),
            );
        }

        MutBorrowOfImmutable { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "cannot mutably borrow immutable variable '{name}'; declare it `let mut {name}` (or a `mut` parameter)"
                    ),
                ),
            );
        }

        DerefOfNonReference { ty, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "cannot dereference value of type {ty}: `*` requires a reference (`&T` or `&mut T`)"
                    ),
                ),
            );
        }
        PtrOpError { message, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!("invalid raw-pointer operation: {message}"),
                ),
            );
        }
    }
    diagnostics
}
