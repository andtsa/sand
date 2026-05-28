//! File system error

use crate::compiler::structure::UriError;
use crate::util::error_ctx::Ctx;

pub type FsCtx = Ctx<String, String>;

#[derive(Debug, thiserror::Error)]
#[error(
    "FS error: {source}{}",
    .context.as_ref().map_or(
        String::new(), |c| format!("\n{c}")
    )
)]
pub struct FsError {
    #[source]
    source: Box<FsErrorSource>,
    context: Option<Ctx<String, String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum FsErrorSource {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Uri(#[from] UriError),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
    #[error("{0}")]
    Other(String),
}

impl FsError {
    pub fn new(err: &str) -> Self {
        Self {
            source: Box::new(FsErrorSource::Other(err.to_string())),
            context: None,
        }
    }

    fn with_context(self, context: Ctx<String, String>) -> Self {
        Self {
            source: self.source,
            context: Some(context),
        }
    }
}

/// trait taken from anyhow crate by dtolnay
pub trait Context<T, E> {
    /// Wrap the error value with additional context.
    fn context(self, context: FsCtx) -> Result<T, FsError>;

    /// Wrap the error value with additional context that is evaluated lazily
    /// only once an error does occur.
    fn with_context<F>(self, f: F) -> Result<T, FsError>
    where
        F: FnOnce() -> FsCtx;
}

impl<E: Into<FsErrorSource>> From<E> for FsError {
    fn from(err: E) -> Self {
        Self {
            source: Box::new(err.into()),
            context: None,
        }
    }
}

impl<T, E> Context<T, E> for Result<T, E>
where
    E: Into<FsError>,
{
    fn context(self, context: FsCtx) -> Result<T, FsError> {
        self.map_err(|e| e.into().with_context(context))
    }

    fn with_context<F>(self, f: F) -> Result<T, FsError>
    where
        F: FnOnce() -> FsCtx,
    {
        self.map_err(|e| e.into().with_context(f()))
    }
}
