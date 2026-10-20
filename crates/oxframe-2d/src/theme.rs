//! Design tokens. A [`Theme`] is a palette, a type scale, spacing, radii,
//! shadows and motion. Nodes read it while a scene runs.
//!
//! Built-in themes: [`Theme::dark`] (the default), [`Theme::light`],
//! [`Theme::midnight`], [`Theme::paper`] and [`Theme::vivid`].
//! [`Theme::accent`] retints a theme from one brand color and keeps text
//! at WCAG 4.5:1 against the background.

use std::cell::RefCell;

use oxframe_core::{Color, CubicBezier, Seconds, Vec2, presets};

/// A color role. Use it wherever a [`Color`] is accepted: `.fill(Accent)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// Page background.
    Background,
    /// Cards and panels, a step off the background.
    Surface,
    /// Primary text.
    Text,
    /// Secondary text. Still readable on the background and on [`Surface`](Ink::Surface).
    Muted,
    /// Brand color.
    Accent,
    /// Text drawn on top of [`Accent`](Ink::Accent).
    OnAccent,
    /// Positive state.
    Success,
    /// Caution.
    Warning,
    /// Failure.
    Danger,
}

/// A step on the space, radius and shadow scales: `.padding(Md)`, `.radius(Lg)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Smallest step.
    Xs,
    /// Small.
    Sm,
    /// Medium, the ordinary gap.
    Md,
    /// Large.
    Lg,
    /// Largest.
    Xl,
}

/// How long a motion token lasts: `.to(1.0, Normal)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    /// A small change.
    Fast,
    /// The ordinary duration. `fade_in` uses this.
    Normal,
    /// A large change.
    Slow,
}

impl Seconds for Pace {
    fn seconds(self) -> f64 {
        let m = current().motion;
        match self {
            Pace::Fast => m.fast,
            Pace::Normal => m.normal,
            Pace::Slow => m.slow,
        }
    }
}

/// One text role: size in pixels, weight 100..=900, line height as a multiple.
#[derive(Debug, Clone, Copy)]
pub struct TypeStyle {
    /// Font size in pixels.
    pub size: f64,
    /// 100..=900.
    pub weight: f64,
    /// Multiple of the font size.
    pub line_height: f64,
}

/// Display, headings, body, caption and code.
#[derive(Debug, Clone, Copy)]
pub struct TypeScale {
    /// The largest title.
    pub display: TypeStyle,
    /// Page title.
    pub h1: TypeStyle,
    /// Section title.
    pub h2: TypeStyle,
    /// Running text.
    pub body: TypeStyle,
    /// Secondary text.
    pub caption: TypeStyle,
    /// Monospace.
    pub code: TypeStyle,
}

/// Five pixel sizes, smallest to largest.
#[derive(Debug, Clone, Copy)]
pub struct Scale {
    /// Extra small.
    pub xs: f64,
    /// Small.
    pub sm: f64,
    /// Medium.
    pub md: f64,
    /// Large.
    pub lg: f64,
    /// Extra large.
    pub xl: f64,
}

impl Scale {
    fn get(self, step: Step) -> f64 {
        match step {
            Step::Xs => self.xs,
            Step::Sm => self.sm,
            Step::Md => self.md,
            Step::Lg => self.lg,
            Step::Xl => self.xl,
        }
    }
}

/// A drop shadow.
#[derive(Debug, Clone, Copy)]
pub struct ShadowSpec {
    /// Shadow color.
    pub color: Color,
    /// Blur radius in pixels.
    pub blur: f64,
    /// Offset in pixels; y is down.
    pub offset: Vec2,
}

/// Durations in seconds and the eases for entering, leaving and emphasis.
#[derive(Debug, Clone, Copy)]
pub struct Motion {
    /// Short.
    pub fast: f64,
    /// Ordinary. `fade_in` uses this.
    pub normal: f64,
    /// Long.
    pub slow: f64,
    /// Ease for something appearing.
    pub enter: CubicBezier,
    /// Ease for something leaving.
    pub exit: CubicBezier,
    /// Ease for a bump or a pop.
    pub emphasis: CubicBezier,
}

struct Raw {
    background: Color,
    surface: Color,
    text: Color,
    muted: Color,
    accent: Color,
    success: Color,
    warning: Color,
    danger: Color,
    type_scale: TypeScale,
    motion: Motion,
}

/// Colors, type, space, radii, shadows and motion for one look.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Page background. `Project::theme` copies this onto the project.
    pub background: Color,
    /// Cards and panels.
    pub surface: Color,
    /// Primary text.
    pub text: Color,
    /// Secondary text.
    pub muted: Color,
    /// Brand color, readable on the background.
    pub accent: Color,
    /// Text on top of the accent.
    pub on_accent: Color,
    /// Positive.
    pub success: Color,
    /// Caution.
    pub warning: Color,
    /// Failure.
    pub danger: Color,
    /// Type roles.
    pub type_scale: TypeScale,
    /// Padding, gap and margin, in pixels.
    pub space: Scale,
    /// Corner radii, in pixels.
    pub radius: Scale,
    /// Shadows for [`Step::Sm`], [`Step::Md`] and [`Step::Lg`].
    pub shadow_sm: ShadowSpec,
    /// Medium shadow.
    pub shadow_md: ShadowSpec,
    /// Large shadow.
    pub shadow_lg: ShadowSpec,
    /// Durations and eases.
    pub motion: Motion,
}

impl Theme {
    /// Near-black background, white text, blue accent. The project default.
    pub fn dark() -> Self {
        Self::palette(Raw {
            background: hex("#141414"),
            surface: hex("#1c1e24"),
            text: Color::WHITE,
            muted: hex("#b7bcc6"),
            accent: hex("#68abdf"),
            success: hex("#3dcc7a"),
            warning: hex("#f5c242"),
            danger: hex("#e13238"),
            type_scale: type_scale(1.0),
            motion: motion(1.0),
        })
    }

    /// Off-white background, ink text.
    pub fn light() -> Self {
        Self::palette(Raw {
            background: hex("#f6f4f1"),
            surface: Color::WHITE,
            text: hex("#1a1c1f"),
            muted: hex("#555b66"),
            accent: hex("#1d6aab"),
            success: hex("#0e8a45"),
            warning: hex("#8a5a00"),
            danger: hex("#c4232c"),
            type_scale: type_scale(1.0),
            motion: motion(1.0),
        })
    }

    /// Blue-black, cool accent.
    pub fn midnight() -> Self {
        Self::palette(Raw {
            background: hex("#0b1020"),
            surface: hex("#151b2e"),
            text: hex("#e8eefc"),
            muted: hex("#9aa6c2"),
            accent: hex("#7ee0ff"),
            success: hex("#3dcc7a"),
            warning: hex("#f5c242"),
            danger: hex("#ff6b6b"),
            type_scale: type_scale(1.0),
            motion: motion(1.05),
        })
    }

    /// Warm paper and ink.
    pub fn paper() -> Self {
        Self::palette(Raw {
            background: hex("#f3ead7"),
            surface: hex("#fffaf1"),
            text: hex("#2a241b"),
            muted: hex("#6b5e4e"),
            accent: hex("#b4532a"),
            success: hex("#2f7d4a"),
            warning: hex("#8a5a00"),
            danger: hex("#a32020"),
            type_scale: type_scale(0.96),
            motion: motion(1.15),
        })
    }

    /// Dark, with a strong accent and snappier motion.
    pub fn vivid() -> Self {
        Self::palette(Raw {
            background: hex("#0e0e10"),
            surface: hex("#18181c"),
            text: Color::WHITE,
            muted: hex("#c4c4cc"),
            accent: hex("#ff4d6d"),
            success: hex("#3dcc7a"),
            warning: hex("#ffd000"),
            danger: hex("#ff4d6d"),
            type_scale: type_scale(1.08),
            motion: Motion {
                fast: 0.15,
                normal: 0.35,
                slow: 0.6,
                ..motion(1.0)
            },
        })
    }

    /// `dark`, `light`, `midnight`, `paper` or `vivid`.
    pub fn preset(name: &str) -> Option<Self> {
        Some(match name {
            "dark" => Self::dark(),
            "light" => Self::light(),
            "midnight" => Self::midnight(),
            "paper" => Self::paper(),
            "vivid" => Self::vivid(),
            _ => return None,
        })
    }

    /// Rebuilds the accent (and the surface tint) from one brand color.
    /// The result stays at least 4.5:1 against the background.
    pub fn accent(mut self, color: Color) -> Self {
        self.surface = self.background.mix(color, 0.16);
        if self.is_light() {
            self.surface = self.surface.mix(Color::WHITE, 0.35);
        }
        let fit = |c: Color| {
            c.readable_on(self.background, 4.5)
                .readable_on(self.surface, 4.5)
        };
        self.accent = fit(color);
        self.on_accent = on_color(self.accent);
        self.text = fit(self.text);
        self.muted = fit(self.muted);
        self.success = fit(self.success);
        self.warning = fit(self.warning);
        self.danger = fit(self.danger);
        self
    }

    /// Whether the background is light (paper, light).
    pub fn is_light(&self) -> bool {
        self.background.luminance() > 0.4
    }

    /// The color of a role.
    pub fn ink(&self, ink: Ink) -> Color {
        match ink {
            Ink::Background => self.background,
            Ink::Surface => self.surface,
            Ink::Text => self.text,
            Ink::Muted => self.muted,
            Ink::Accent => self.accent,
            Ink::OnAccent => self.on_accent,
            Ink::Success => self.success,
            Ink::Warning => self.warning,
            Ink::Danger => self.danger,
        }
    }

    /// Padding, gap or margin for a step.
    pub fn space(&self, step: Step) -> f64 {
        self.space.get(step)
    }

    /// Corner radius for a step.
    pub fn radius(&self, step: Step) -> f64 {
        self.radius.get(step)
    }

    /// Shadow for a step. `Xs` uses the small shadow, `Xl` the large one.
    pub fn shadow(&self, step: Step) -> ShadowSpec {
        match step {
            Step::Xs | Step::Sm => self.shadow_sm,
            Step::Md => self.shadow_md,
            Step::Lg | Step::Xl => self.shadow_lg,
        }
    }

    /// A type role. `code` is the monospace size; the family is set by `.code()`.
    pub fn role(&self, role: TypeRole) -> TypeStyle {
        match role {
            TypeRole::Display => self.type_scale.display,
            TypeRole::H1 => self.type_scale.h1,
            TypeRole::H2 => self.type_scale.h2,
            TypeRole::Body => self.type_scale.body,
            TypeRole::Caption => self.type_scale.caption,
            TypeRole::Code => self.type_scale.code,
        }
    }

    fn palette(raw: Raw) -> Self {
        let Raw {
            background,
            surface,
            text,
            muted,
            accent,
            success,
            warning,
            danger,
            type_scale,
            motion,
        } = raw;
        let fit = |c: Color| c.readable_on(background, 4.5).readable_on(surface, 4.5);
        let accent = fit(accent);
        Self {
            background,
            surface,
            text: fit(text),
            muted: fit(muted),
            accent,
            on_accent: on_color(accent),
            success: fit(success),
            warning: fit(warning),
            danger: fit(danger),
            type_scale,
            space: Scale {
                xs: 8.0,
                sm: 16.0,
                md: 24.0,
                lg: 40.0,
                xl: 64.0,
            },
            radius: Scale {
                xs: 6.0,
                sm: 12.0,
                md: 20.0,
                lg: 32.0,
                xl: 48.0,
            },
            shadow_sm: shadow(0.22, 12.0, 4.0),
            shadow_md: shadow(0.32, 28.0, 12.0),
            shadow_lg: shadow(0.42, 48.0, 24.0),
            motion,
        }
    }
}

/// Which type role `.display()`, `.h1()` and the others select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeRole {
    /// Largest title.
    Display,
    /// Page title.
    H1,
    /// Section title.
    H2,
    /// Running text.
    Body,
    /// Secondary text.
    Caption,
    /// Monospace.
    Code,
}

fn on_color(bg: Color) -> Color {
    if Color::WHITE.contrast_ratio(bg) >= 4.5 {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

fn hex(s: &str) -> Color {
    Color::from_hex(s).expect("theme color")
}

fn shadow(alpha: f32, blur: f64, y: f64) -> ShadowSpec {
    ShadowSpec {
        color: Color::BLACK.with_alpha(alpha),
        blur,
        offset: Vec2::new(0.0, y),
    }
}

fn style(size: f64, weight: f64, line_height: f64) -> TypeStyle {
    TypeStyle {
        size,
        weight,
        line_height,
    }
}

fn type_scale(k: f64) -> TypeScale {
    let s = |n: f64| (n * k).round();
    TypeScale {
        display: style(s(96.0), 700.0, 1.05),
        h1: style(s(64.0), 700.0, 1.1),
        h2: style(s(44.0), 600.0, 1.15),
        body: style(s(32.0), 400.0, 1.35),
        caption: style(s(22.0), 400.0, 1.3),
        code: style(s(28.0), 450.0, 1.45),
    }
}

fn motion(k: f64) -> Motion {
    Motion {
        fast: 0.2 * k,
        normal: 0.45 * k,
        slow: 0.8 * k,
        enter: presets::OUT_CUBIC,
        exit: presets::IN_CUBIC,
        emphasis: presets::OUT_BACK,
    }
}

thread_local! {
    static CURRENT: RefCell<Theme> = RefCell::new(Theme::dark());
}

/// The theme of the scene that is running, or [`Theme::dark`] outside one.
pub fn current() -> Theme {
    CURRENT.with(|t| t.borrow().clone())
}

/// Installs `theme` for the duration of `f`.
pub fn enter<R>(theme: &Theme, f: impl FnOnce() -> R) -> R {
    let prev = CURRENT.with(|t| t.replace(theme.clone()));
    struct Restore(Option<Theme>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(prev) = self.0.take() {
                CURRENT.with(|t| *t.borrow_mut() = prev);
            }
        }
    }
    let _restore = Restore(Some(prev));
    f()
}

/// Something that can be used as a color: a [`Color`] or an [`Ink`].
pub trait IntoPaint {
    /// Resolves to a color. An [`Ink`] uses the current theme.
    fn into_paint(self) -> Color;
}

impl IntoPaint for Color {
    fn into_paint(self) -> Color {
        self
    }
}

impl IntoPaint for Ink {
    fn into_paint(self) -> Color {
        current().ink(self)
    }
}

impl From<Ink> for Color {
    fn from(ink: Ink) -> Color {
        ink.into_paint()
    }
}

/// A padding, gap or margin: pixels, or a [`Step`] on the space scale.
pub trait Len {
    /// Pixels.
    fn px(self) -> f64;
}

impl Len for f64 {
    fn px(self) -> f64 {
        self
    }
}

impl Len for Step {
    fn px(self) -> f64 {
        current().space(self)
    }
}

/// A corner radius: pixels, or a [`Step`] on the radius scale.
pub trait Corner {
    /// Pixels.
    fn radius(self) -> f64;
}

impl Corner for f64 {
    fn radius(self) -> f64 {
        self
    }
}

impl Corner for Step {
    fn radius(self) -> f64 {
        current().radius(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inks(theme: &Theme) -> [Color; 6] {
        [
            theme.text,
            theme.muted,
            theme.accent,
            theme.success,
            theme.warning,
            theme.danger,
        ]
    }

    #[test]
    fn every_preset_is_readable() {
        for theme in [
            Theme::dark(),
            Theme::light(),
            Theme::midnight(),
            Theme::paper(),
            Theme::vivid(),
            Theme::dark().accent(hex("#224466")),
            Theme::light().accent(hex("#ff88aa")),
        ] {
            for c in inks(&theme) {
                assert!(
                    c.contrast_ratio(theme.background) >= 4.5,
                    "bg {:?} {:?}",
                    c,
                    theme.background
                );
                assert!(
                    c.contrast_ratio(theme.surface) >= 4.5,
                    "surface {:?} {:?}",
                    c,
                    theme.surface
                );
            }
            assert!(theme.on_accent.contrast_ratio(theme.accent) >= 4.5);
        }
    }

    #[test]
    fn accent_keeps_contrast_and_tints_the_surface() {
        let dark = Theme::dark();
        let branded = Theme::dark().accent(hex("#224466"));
        assert!(branded.accent.contrast_ratio(branded.background) >= 4.5);
        assert_ne!(branded.surface.to_rgba8(), dark.surface.to_rgba8());
        assert_eq!(branded.background.to_rgba8(), dark.background.to_rgba8());
    }

    #[test]
    fn steps_resolve_on_different_scales() {
        let theme = Theme::dark();
        enter(&theme, || {
            assert_eq!(Step::Md.px(), theme.space.md);
            assert_eq!(Step::Md.radius(), theme.radius.md);
            assert_ne!(Step::Md.px(), Step::Md.radius());
            assert!((Pace::Normal.seconds() - theme.motion.normal).abs() < 1e-9);
        });
    }
}
