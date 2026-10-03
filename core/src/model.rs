use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::Error;

macro_rules! label_enum {
    ($name:ident, $what:literal, { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
        #[serde(rename_all = "lowercase")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = Error;
            fn from_str(s: &str) -> Result<Self, Error> {
                match s.trim() {
                    $($text => Ok($name::$variant),)+
                    other => Err(Error::Usage(format!(
                        concat!("unknown ", $what, " `{}`; expected one of: {}"),
                        other,
                        [$($text),+].join(", ")
                    ))),
                }
            }
        }
    };
}

label_enum!(Kind, "kind", {
    Home => "home",
    Room => "room",
    Furniture => "furniture",
    Container => "container",
    Item => "item",
});

label_enum!(State, "state", {
    Active => "active",
    Candidate => "candidate",
    Gone => "gone",
});

label_enum!(Disposition, "disposition", {
    Trash => "trash",
    Give => "give",
    Sell => "sell",
    Return => "return",
    // Photographed, then thrown out: the paper leaves, its copy stays on the record, which
    // `ev find` keeps listing. `gone` refuses it while the record has no photo or document.
    Digitize => "digitize",
    // Closing a record that should never have existed (a misreading, a duplicate): it keeps its
    // history but nothing actually left the home.
    Mistake => "mistake",
    // Used up: a tape run out, a dead cell, a consumable spent. It left by being used, which is
    // neither throwing a good thing away nor giving it; only `gone` takes it.
    Used => "used",
    // A portion that joined another portion of the same thing in its place (spec/portions.md
    // §4.2): its units live on in the other record. ev's own; never given on the command line.
    Merged => "merged",
});

/// A stored node with every field of spec §3.1.
#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: i64,
    pub name: String,
    pub kind: Kind,
    pub parent_id: Option<i64>,
    pub code: Option<String>,
    pub address: Option<String>,
    pub qty: Option<i64>,
    pub note: Option<String>,
    pub theme: Option<String>,
    pub fill: Option<i64>,
    /// Outer size `WxDxH` (spec §23), normalized; for boxes that may be swapped for bigger ones.
    pub size: Option<String>,
    pub tags: Vec<String>,
    pub photos: Vec<String>,
    pub state: State,
    pub disposition: Option<Disposition>,
    pub lost: bool,
    pub pending_to: Option<i64>,
    /// Place the node belongs to when it is not ours (spec §13).
    pub owner: Option<String>,
    /// Place holding our node while it is lent out.
    pub with: Option<String>,
    /// Place the node should be taken to.
    pub to: Option<String>,
    /// A parking place: what is in it waits for its final place (spec §30).
    pub temporary: bool,
    pub created_at: String,
    pub updated_at: String,
    /// What it is beyond its name, read off its label (purchases spec §3.1).
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    /// The thing this record is a portion of, when it is kept in several places
    /// (spec/portions.md): the id of the thing's first record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathSegment {
    pub id: i64,
    pub code: Option<String>,
    pub name: String,
}

/// The short form of a node used inside every payload (spec §11.2): a row names its place once,
/// by `path_text`; `lost` appears only when true (spec/output.md).
#[derive(Debug, Clone, Serialize)]
pub struct NodeRef {
    pub id: i64,
    pub code: Option<String>,
    pub name: String,
    pub kind: Kind,
    pub state: State,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub lost: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disposition: Option<Disposition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qty: Option<i64>,
    pub path_text: String,
}

/// Input for `ev add`, one line of `ev add --batch` (spec §11.5).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewNode {
    #[serde(default)]
    pub key: Option<String>,
    /// Empty with `of`, which names the thing.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default, rename = "in")]
    pub parent: Option<String>,
    #[serde(default)]
    pub lost: bool,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub qty: Option<i64>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub fill: Option<i64>,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub photos: Vec<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub temporary: bool,
    #[serde(default)]
    pub make: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub serial: Option<String>,
    /// More of a thing already recorded (spec/portions.md §4.3): its name, kind, make, model,
    /// size and tags come from this record, and the new units are a portion of the same thing.
    #[serde(default)]
    pub of: Option<String>,
}
