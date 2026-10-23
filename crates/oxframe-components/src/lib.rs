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

/// `From<X>` and `From<&X>` for `Node`, so a component goes wherever a node
/// does: `caption.next_to(&end_card, Side::Bottom)`. Invoked in each
/// component's module, where its root field is visible.
macro_rules! into_node {
    ($($t:ty => |$c:ident| $root:expr;)*) => {$(
        impl From<$t> for oxframe_2d::Node {
            fn from($c: $t) -> oxframe_2d::Node {
                $root
            }
        }
        impl From<&$t> for oxframe_2d::Node {
            fn from($c: &$t) -> oxframe_2d::Node {
                $root
            }
        }
    )*};
}

mod axes;
mod callout;
mod code;
mod flow;
mod host;
mod lower;
mod matrix;
mod table;
mod title;

pub use axes::{Axes, AxesBuilder, Bars, Interval, Mat2, NumberLine, Plot, Series, Vector};
pub use callout::{Callout, CalloutBuilder};
pub use code::{CodeExplainer, CodeExplainerBuilder};
pub use flow::{FlowDiagram, FlowDiagramBuilder};
pub use lower::{ChapterMarker, ChapterMarkerBuilder, LowerThird, LowerThirdBuilder};
pub use matrix::{Matrix, MatrixBuilder};
pub use table::{Table, TableBuilder, TableRow};
pub use title::{
    BulletList, BulletListBuilder, EndCard, EndCardBuilder, TitleCard, TitleCardBuilder,
};
