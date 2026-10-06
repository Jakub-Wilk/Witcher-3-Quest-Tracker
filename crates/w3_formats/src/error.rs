use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The data does not look like the expected format (bad magic, offsets out of range, ...).
    #[error("{0}")]
    Format(String),

    /// The data ended before a complete value could be read. For a save this usually means the
    /// game is still writing it.
    #[error("unexpected end of data")]
    Truncated,
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io { path: path.into(), source }
    }

    pub(crate) fn format(msg: impl Into<String>) -> Self {
        Error::Format(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
