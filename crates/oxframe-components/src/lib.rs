//! Blocks for explainer videos. Each one is added like any other node,
//! takes its look from the project theme, and speaks three words:
//! [`enter`](TitleCard::enter), [`next`](TitleCard::next) and [`exit`](TitleCard::exit).
//!
//! ```ignore
//! let steps = view.add(BulletList::new(["Take the middle", "Compare", "Drop half"]));
//! steps.enter().await;
//! for _ in 0..3 { steps.next().await; }
//! ```

#![warn(missing_docs)]

mod callout;
mod code;
mod flow;
mod host;
mod lower;
mod title;

pub use callout::{Callout, CalloutBuilder};
pub use code::{CodeExplainer, CodeExplainerBuilder};
pub use flow::{FlowDiagram, FlowDiagramBuilder};
pub use lower::{ChapterMarker, ChapterMarkerBuilder, LowerThird, LowerThirdBuilder};
pub use title::{
    BulletList, BulletListBuilder, EndCard, EndCardBuilder, TitleCard, TitleCardBuilder,
};
