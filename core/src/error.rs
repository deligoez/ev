use serde_json::{Map, Value, json};

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
    /// An error with an id and its values (spec/error-ids.md); the sentences above move here
    /// phase by phase.
    #[error("{}", .0.text())]
    Said(Box<Said>),
}

/// What kind of failure an error with an id is; it decides the exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    Usage,
    NotFound,
    Ambiguous,
    Refused,
}

/// An error by its id: the values its sentence names, where it happened (`at`, outermost
/// last), and what a refusal or an ambiguity carries along.
#[derive(Debug)]
pub struct Said {
    pub fault: Fault,
    pub id: &'static str,
    pub values: Map<String, Value>,
    pub at: Vec<Value>,
    pub details: Value,
    pub candidates: Vec<Value>,
}

impl Said {
    /// The English sentence, with what it is about in front (`line 2: …`).
    pub fn text(&self) -> String {
        let template = crate::errors::template(self.id).unwrap_or(self.id);
        let mut out = fill(template, &self.values);
        for a in &self.at {
            out = format!("{}: {out}", at_text(a));
        }
        out
    }
}

/// `{name}` in a template replaced by that value: a string as it is, anything else as JSON.
/// A name with no value stays as written, which the tests catch.
pub fn fill(template: &str, values: &Map<String, Value>) -> String {
    let mut out = template.to_string();
    for (k, v) in values {
        let text = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out = out.replace(&format!("{{{k}}}"), &text);
    }
    out
}

/// One step of where an error happened, in English: `line 2`, or what it is about as given.
pub fn at_text(a: &Value) -> String {
    match (&a["line"], a["of"].as_str()) {
        (Value::Number(n), _) => format!("line {n}"),
        (_, Some(of)) => of.to_string(),
        _ => a.to_string(),
    }
}

impl Error {
    /// An error by its id, with the values its sentence names.
    pub(crate) fn said(fault: Fault, id: &'static str, values: Value) -> Error {
        Error::Said(Box::new(Said {
            fault,
            id,
            values: match values {
                Value::Object(m) => m,
                _ => Map::new(),
            },
            at: Vec::new(),
            details: Value::Null,
            candidates: Vec::new(),
        }))
    }

    /// The candidates of an ambiguous reference.
    pub(crate) fn with_candidates(mut self, candidates: Vec<Value>) -> Error {
        if let Error::Said(s) = &mut self {
            s.candidates = candidates;
        }
        self
    }

    pub fn code(&self) -> i32 {
        match self {
            Error::Internal(_) => 1,
            Error::Usage(_) => 2,
            Error::NotFound(_) => 3,
            Error::Ambiguous { .. } => 4,
            Error::Refused { .. } => 5,
            Error::NewerSchema { .. } => 6,
            Error::Said(s) => match s.fault {
                Fault::Usage => 2,
                Fault::NotFound => 3,
                Fault::Ambiguous => 4,
                Fault::Refused => 5,
            },
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
            Error::Said(s) => match s.fault {
                Fault::Usage => "usage",
                Fault::NotFound => "not_found",
                Fault::Ambiguous => "ambiguous",
                Fault::Refused => "refused",
            },
        }
    }

    /// Nothing matched the reference, whether the error has an id yet or not.
    pub fn is_not_found(&self) -> bool {
        self.code() == 3
    }

    /// The error's id, once it has one.
    pub fn id(&self) -> Option<&'static str> {
        match self {
            Error::Said(s) => Some(s.id),
            _ => None,
        }
    }

    /// The error with its id, values and context, for a reader that words it itself.
    pub fn said_parts(&self) -> Option<&Said> {
        match self {
            Error::Said(s) => Some(s),
            _ => None,
        }
    }

    /// The stderr payload of spec §11.1; an error with an id adds `id`, `values` and `at`
    /// (spec/error-ids.md).
    pub fn to_json(&self) -> Value {
        let mut e = json!({"code": self.code(), "kind": self.kind(), "message": self.to_string()});
        match self {
            Error::Ambiguous { candidates, .. } => e["candidates"] = json!(candidates),
            Error::Refused { details, .. } if !details.is_null() => e["details"] = details.clone(),
            Error::Said(s) => {
                e["id"] = json!(s.id);
                if !s.values.is_empty() {
                    e["values"] = Value::Object(s.values.clone());
                }
                if !s.at.is_empty() {
                    // Outermost first, as the sentence reads.
                    e["at"] = json!(s.at.iter().rev().collect::<Vec<_>>());
                }
                if !s.candidates.is_empty() {
                    e["candidates"] = json!(s.candidates);
                }
                if !s.details.is_null() {
                    e["details"] = s.details.clone();
                }
            }
            _ => {}
        }
        json!({ "error": e })
    }

    /// Prefixes the message with a batch line number.
    pub fn at_line(self, line: usize) -> Self {
        match self {
            Error::Said(mut s) => {
                s.at.push(json!({ "line": line }));
                Error::Said(s)
            }
            other => other.prefixed(&format!("line {line}")),
        }
    }

    /// Prefixes the message with what it is about (`line 2`, `f3`).
    pub fn prefixed(self, what: &str) -> Self {
        let p = |m: String| format!("{what}: {m}");
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
            Error::Said(mut s) => {
                s.at.push(json!({ "of": what }));
                Error::Said(s)
            }
            other => other,
        }
    }
}

/// A request written wrong, by its id (exit 2).
pub(crate) fn usage(id: &'static str, values: Value) -> Error {
    Error::said(Fault::Usage, id, values)
}

/// Nothing matched, by its id (exit 3).
pub(crate) fn not_found(id: &'static str, values: Value) -> Error {
    Error::said(Fault::NotFound, id, values)
}

/// A refusal by its id (exit 5), with what it carries for the agent to act on, if anything.
pub(crate) fn refuse(id: &'static str, values: Value, details: Value) -> Error {
    let mut e = Error::said(Fault::Refused, id, values);
    if let Error::Said(s) = &mut e {
        s.details = details;
    }
    e
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
