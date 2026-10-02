//! Data model, rules and storage for `ev`, the agent-first home inventory.

mod attachments;
mod coverage;
mod docs;
mod error;
mod fold;
mod grid;
mod kits;
mod links;
mod map;
mod marks;
mod model;
mod money;
mod photo;
mod placement;
mod plan;
mod portions;
mod purchase_match;
mod purchases;
mod store;
mod valuations;

pub use coverage::{COVERAGE_KINDS, INVENTORY_SETTINGS, NewCoverage};
pub use docs::{DOC_KINDS, NewDoc};
pub use error::{Error, Result};
pub use fold::fold;
pub use grid::{Cells, GridCorners};
pub use links::LINK_KINDS;
pub use map::{SketchChange, parse_pair, parse_points, reading_order};
pub use model::{Disposition, Kind, NewNode, Node, NodeRef, PathSegment, State};
pub use photo::{Crop, open_upright};
pub use purchases::{BUCKETS, DISMISSALS};
pub use store::{Inventory, SCHEMA_VERSION};
pub use valuations::NewValuation;
