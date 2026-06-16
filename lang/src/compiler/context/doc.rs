use crate::compiler::structure::FileRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::TypeclassRef;
use crate::lang::types::AdtRef;

/// The definition a [type reference](CompileCtx::record_type_ref) points at.
#[derive(Debug, Clone, Copy)]
pub enum DefTarget<'tcx> {
    Adt(AdtRef<'tcx>),
    Typeclass(TypeclassRef),
}

/// One recorded type/typeclass name reference: its source span and target.
#[derive(Debug, Clone, Copy)]
pub struct TypeRefEntry<'tcx> {
    pub file: FileRef,
    pub range: Range,
    pub target: DefTarget<'tcx>,
}
