//! check intrinsic functions and their arguments

use crate::ir_types::typed_hir;
use crate::lang::intrinsics::INTRINSICS;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::types::Ty;

pub fn get_intrinsic_call(
    call: &str,
    typed_args: &[typed_hir::Expr],
) -> Option<(typed_hir::Expression, Ty)> {
    if let Ok(intr) = Intrinsic::try_from(call) {
        let (_, fn_sig) = &INTRINSICS[&intr];
        Some((
            typed_hir::Expression::IntrinsicCall {
                fn_name: intr,
                args: typed_args.to_vec(),
            },
            fn_sig.ret_ty,
        ))
    } else {
        None
    }
}

pub fn is_intrinsic_call(call: &str) -> bool {
    Intrinsic::try_from(call).is_ok()
}
