//! A code block, a caption, and a list of focus steps.

use std::cell::Cell;
use std::rc::Rc;

use oxframe_2d::{BaseProps, Code, Muted, Node, NodeBuilder, NodeId, Normal, Txt};
use oxframe_core::{BoxFuture, Seconds, Tween, all};

use crate::host;

/// Code with a caption. `enter` shows the code. Each `next` focuses one step
/// and sets the caption.
pub struct CodeExplainer {
    root: Node,
    code: Code,
    caption: Txt,
    steps: Rc<Vec<(usize, usize, String)>>,
    cursor: Rc<Cell<usize>>,
}

impl CodeExplainer {
    /// The source is shown as Rust. Add steps with [`step`](CodeExplainerBuilder::step).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(code: impl Into<String>) -> CodeExplainerBuilder {
        CodeExplainerBuilder {
            base: BaseProps::default(),
            code: code.into(),
            steps: Vec::new(),
        }
    }

    /// Fades the block in.
    pub fn enter(&self) -> Tween<f64> {
        self.root.fade_in()
    }

    /// Focuses the next step and shows its caption.
    pub fn next(&self) -> BoxFuture {
        let n = self.steps.len();
        let i = self.cursor.get();
        let (first, last, text) = if n == 0 {
            (0, 0, String::new())
        } else {
            let step = &self.steps[i.min(n - 1)];
            if i < n {
                self.cursor.set(i + 1);
            }
            (step.0, step.1, step.2.clone())
        };
        self.caption.text().set(text);
        let focus = self.code.focus_lines(first, last, Normal.seconds());
        let fade = if i == 0 {
            self.caption.fade_in()
        } else {
            self.caption.opacity().to(1.0, Normal)
        };
        Box::pin(async move {
            all((focus, fade)).await;
        })
    }

    /// Fades the block out.
    pub fn exit(&self) -> Tween<f64> {
        self.root.fade_out()
    }

    /// How many steps were declared.
    pub fn len(&self) -> usize {
        self.steps.len()
    }
    /// No steps were declared.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// Builder for [`CodeExplainer`].
#[must_use = "a builder does nothing until it is passed to `add`"]
pub struct CodeExplainerBuilder {
    base: BaseProps,
    code: String,
    steps: Vec<(usize, usize, String)>,
}

impl CodeExplainerBuilder {
    /// A focus range (1-based, inclusive) and the caption `next` shows with it.
    pub fn step(mut self, first: usize, last: usize, caption: impl Into<String>) -> Self {
        self.steps.push((first, last, caption.into()));
        self
    }
}

impl NodeBuilder for CodeExplainerBuilder {
    type Handle = CodeExplainer;
    fn base(&mut self) -> &mut BaseProps {
        &mut self.base
    }
    fn build(self, _: &oxframe_2d::SceneGraph, parent: NodeId) -> CodeExplainer {
        let root = host::root(parent, self.base);
        let code = root.add(
            Code::new()
                .code(self.code)
                .language("rust")
                .font_size(26.0)
                .y(-24.0)
                .name("code"),
        );
        let (_, h) = code.size();
        let caption = root.add(
            Txt::new()
                .text(" ")
                .caption()
                .fill(Muted)
                .y(h / 2.0 + 36.0)
                .opacity(0.0)
                .name("caption"),
        );
        CodeExplainer {
            root: root.node(),
            code,
            caption,
            steps: Rc::new(self.steps),
            cursor: Rc::new(Cell::new(0)),
        }
    }
}
