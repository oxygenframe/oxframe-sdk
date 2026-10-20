//! What a scene records when it starts a transition, and how two layers
//! combine. The builder users call lives in `oxframe`.

use crate::CubicBezier;
use crate::TIME_EPSILON;
use crate::ease::Ease;

/// Edge a slide or a wipe moves toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Toward the left.
    Left,
    /// Toward the right.
    Right,
    /// Toward the top.
    Up,
    /// Toward the bottom.
    Down,
}

impl Dir {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// How one of the two layers is drawn. Offsets are fractions of the frame
/// (positive x moves the layer right, positive y moves it down). Scale is
/// around the frame center. A wipe reveals that fraction of the layer from
/// `dir` and hides the rest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerPose {
    /// 0 is invisible, 1 is solid.
    pub opacity: f64,
    /// Horizontal shift, as a fraction of the frame width.
    pub dx: f64,
    /// Vertical shift, as a fraction of the frame height.
    pub dy: f64,
    /// Scale around the center. 1 leaves the layer as it is.
    pub scale: f64,
    /// Reveal this much of the layer (0..1) from this edge. `None` shows it all.
    pub wipe: Option<(Dir, f64)>,
}

impl Default for LayerPose {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            dx: 0.0,
            dy: 0.0,
            scale: 1.0,
            wipe: None,
        }
    }
}

impl LayerPose {
    /// Fully visible, unmoved, unscaled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Multiplies the layer's alpha.
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }

    /// Shifts the layer by fractions of the frame.
    pub fn offset(mut self, dx: f64, dy: f64) -> Self {
        self.dx = dx;
        self.dy = dy;
        self
    }

    /// Scales the layer around the center.
    pub fn scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }

    /// Shows `amount` of the layer (0..1) starting from `dir`.
    pub fn wipe(mut self, dir: Dir, amount: f64) -> Self {
        self.wipe = Some((dir, amount));
        self
    }
}

/// The two layers of one transition frame.
///
/// `mix` crossfades them. Otherwise the back layer is painted, then the
/// front one, using each pose's opacity, offset, scale and wipe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blend {
    /// The outgoing scene.
    pub from: LayerPose,
    /// The incoming scene.
    pub to: LayerPose,
    /// The incoming layer is painted after the outgoing one.
    pub incoming_on_top: bool,
    /// Crossfade instead of painting one layer over the other.
    pub mix: bool,
}

impl Default for Blend {
    fn default() -> Self {
        Self {
            from: LayerPose::new(),
            to: LayerPose::new(),
            incoming_on_top: true,
            mix: false,
        }
    }
}

/// Which built-in, or a function of progress. Not part of the prelude;
/// scenes build one through [`crate`] re-exports in `oxframe::Transition`.
#[derive(Clone, Copy)]
pub enum Kind {
    /// Crossfade.
    Fade,
    /// Both layers travel toward `dir`.
    Slide(Dir),
    /// The incoming layer is revealed from `dir`.
    Wipe(Dir),
    /// The incoming layer grows onto the outgoing one.
    ZoomIn,
    /// The outgoing layer grows and fades off the incoming one.
    ZoomOut,
    /// `progress` in 0..1 (already eased) decides the poses.
    Custom(fn(f64) -> Blend),
}

impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Fade => "fade",
            Self::Slide(_) => "slide",
            Self::Wipe(_) => "wipe",
            Self::ZoomIn => "zoom_in",
            Self::ZoomOut => "zoom_out",
            Self::Custom(_) => "custom",
        }
    }

    pub(crate) fn dir(self) -> Option<Dir> {
        match self {
            Self::Slide(d) | Self::Wipe(d) => Some(d),
            _ => None,
        }
    }
}

/// A transition a scene has started. Copied out of the executor after the
/// scene has been measured.
#[derive(Clone, Copy)]
pub struct Recorded {
    pub(crate) kind: Kind,
    pub(crate) seconds: f64,
    pub(crate) ease: CubicBezier,
    pub(crate) overlap: bool,
    /// Scene time at which the blend starts.
    pub(crate) start: f64,
}

impl Recorded {
    /// A transition that starts at `start` seconds into its scene.
    pub fn new(kind: Kind, seconds: f64, ease: CubicBezier, overlap: bool, start: f64) -> Self {
        Self {
            kind,
            seconds,
            ease,
            overlap,
            start,
        }
    }

    /// `"fade"`, `"slide"`, `"wipe"`, `"zoom_in"`, `"zoom_out"` or `"custom"`.
    pub fn kind_name(self) -> &'static str {
        self.kind.name()
    }

    /// `"left"`, `"right"`, `"up"` or `"down"` for a slide or a wipe.
    pub fn dir_name(self) -> Option<&'static str> {
        self.kind.dir().map(Dir::name)
    }

    /// The previous scene keeps playing instead of holding its last frame.
    pub fn overlap(self) -> bool {
        self.overlap
    }

    /// Blend length in seconds.
    pub fn seconds(self) -> f64 {
        self.seconds
    }

    /// Scene time at which the blend starts.
    pub fn start(self) -> f64 {
        self.start
    }

    /// This scene time is still inside the blend.
    pub fn active(self, now: f64) -> bool {
        self.seconds > 0.0
            && now + TIME_EPSILON >= self.start
            && now + TIME_EPSILON < self.start + self.seconds
    }

    /// Eased progress at `now`, clamped to 0..=1.
    pub fn progress(self, now: f64) -> f64 {
        if self.seconds <= 0.0 {
            return 1.0;
        }
        let t = ((now - self.start) / self.seconds).clamp(0.0, 1.0);
        self.ease.ease(t).clamp(0.0, 1.0)
    }

    /// Layer poses at `now`.
    pub fn blend_at(self, now: f64) -> Blend {
        blend(self.kind, self.progress(now))
    }
}

/// Poses for a built-in at eased progress `t` (0 = outgoing, 1 = incoming).
pub(crate) fn blend(kind: Kind, t: f64) -> Blend {
    let t = t.clamp(0.0, 1.0);
    match kind {
        Kind::Fade => Blend {
            mix: true,
            ..Blend::default()
        },
        Kind::Slide(dir) => {
            let (fx, fy, tx, ty) = shift(dir, t);
            Blend {
                from: LayerPose::new().offset(fx, fy),
                to: LayerPose::new().offset(tx, ty),
                ..Blend::default()
            }
        }
        Kind::Wipe(dir) => Blend {
            to: LayerPose::new().wipe(dir, t),
            ..Blend::default()
        },
        Kind::ZoomIn => Blend {
            to: LayerPose::new().opacity(t).scale(0.6 + 0.4 * t),
            ..Blend::default()
        },
        Kind::ZoomOut => Blend {
            from: LayerPose::new().opacity(1.0 - t).scale(1.0 + 0.4 * t),
            incoming_on_top: false,
            ..Blend::default()
        },
        Kind::Custom(f) => f(t),
    }
}

fn shift(dir: Dir, t: f64) -> (f64, f64, f64, f64) {
    match dir {
        Dir::Left => (-t, 0.0, 1.0 - t, 0.0),
        Dir::Right => (t, 0.0, t - 1.0, 0.0),
        Dir::Up => (0.0, -t, 0.0, 1.0 - t),
        Dir::Down => (0.0, t, 0.0, t - 1.0),
    }
}
