use serde_json::{Value, json};

pub type Result<T> = std::result::Result<T, Error>;

/// Every failure maps to one fixed exit code (spec §6).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Internal(String),
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{message}")]
    Ambiguous {
        message: String,
        candidates: Vec<Value>,
    },
    #[error("{message}")]
    Refused { message: String, details: Value },
    #[error(
        "the database uses schema {found}, newer than this ev supports ({supported}); upgrade ev"
    )]
    NewerSchema { found: i64, supported: i64 },
}

impl Error {
    pub fn code(&self) -> i32 {
        match self {
            Error::Internal(_) => 1,
            Error::Usage(_) => 2,
            Error::NotFound(_) => 3,
            Error::Ambiguous { .. } => 4,
            Error::Refused { .. } => 5,
            Error::NewerSchema { .. } => 6,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Error::Internal(_) => "internal",
            Error::Usage(_) => "usage",
            Error::NotFound(_) => "not_found",
            Error::Ambiguous { .. } => "ambiguous",
            Error::Refused { .. } => "refused",
            Error::NewerSchema { .. } => "newer_schema",
        }
    }

    /// The stderr payload of spec §11.1.
    pub fn to_json(&self) -> Value {
        let mut e = json!({"code": self.code(), "kind": self.kind(), "message": self.to_string()});
        match self {
            Error::Ambiguous { candidates, .. } => e["candidates"] = json!(candidates),
            Error::Refused { details, .. } if !details.is_null() => e["details"] = details.clone(),
            _ => {}
        }
        json!({ "error": e })
    }

    /// Prefixes the message with a batch line number.
    pub fn at_line(self, line: usize) -> Self {
        let p = |m: String| format!("line {line}: {m}");
        match self {
            Error::Internal(m) => Error::Internal(p(m)),
            Error::Usage(m) => Error::Usage(p(m)),
            Error::NotFound(m) => Error::NotFound(p(m)),
            Error::Ambiguous {
                message,
                candidates,
            } => Error::Ambiguous {
                message: p(message),
                candidates,
            },
            Error::Refused { message, details } => Error::Refused {
                message: p(message),
                details,
            },
            other => other,
        }
    }
}

pub(crate) fn refused(message: impl Into<String>, details: Value) -> Error {
    Error::Refused {
        message: message.into(),
        details,
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Internal(format!("database error: {e}"))
    }
}
