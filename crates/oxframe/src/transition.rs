//! Scene transitions. Declared at the start of a scene, they blend the
//! previous scene's last frame with the frames this scene produces while
//! the call is awaited.

use oxframe_2d::current_theme;
use oxframe_core::{
    Blend, Color, CubicBezier, Dir, Image, Kind, LayerPose, Recorded, ShaderTransition,
    begin_transition, has_previous, time, wait_for,
};

/// How the previous scene gives way to this one.
///
/// [`shader`](Self::shader) paints the blend with a shader preset such as
/// [`water_wipe`](crate::shaders::water_wipe).
///
/// # Example
///
/// ```no_run
/// use oxframe::prelude::*;
///
/// # #[oxframe::scene]
/// # async fn intro(view: View) { wait_for(0.2).await; }
/// #[oxframe::scene]
/// async fn outro(view: View) {
///     transition(Transition::slide(Dir::Left)).await;
///     let end = view.add(EndCard::new("Thanks for watching"));
///     end.enter().await;
/// }
///
/// # fn main() {
/// let _ = Project::new("video").scene(intro()).scene(outro());
/// # }
/// ```
#[non_exhaustive]
#[derive(Clone, Copy)]
#[must_use = "a transition does nothing until it is passed to `transition`"]
pub struct Transition {
    kind: Kind,
    seconds: Option<f64>,
    ease: Option<CubicBezier>,
    overlap: bool,
}

impl Transition {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            seconds: None,
            ease: None,
            overlap: false,
        }
    }

    /// Crossfade. Duration and easing come from the theme unless set.
    pub fn fade() -> Self {
        Self::new(Kind::Fade)
    }

    /// Both scenes travel toward `dir`.
    pub fn slide(dir: Dir) -> Self {
        Self::new(Kind::Slide(dir))
    }

    /// The incoming scene is revealed from `dir`.
    pub fn wipe(dir: Dir) -> Self {
        Self::new(Kind::Wipe(dir))
    }

    /// The incoming scene grows up to fill the frame.
    pub fn zoom_in() -> Self {
        Self::new(Kind::ZoomIn)
    }

    /// The outgoing scene grows and fades, uncovering the incoming one.
    pub fn zoom_out() -> Self {
        Self::new(Kind::ZoomOut)
    }

    /// `blend` receives eased progress in 0..=1 and places both layers.
    ///
    /// A closure that captures nothing is a function, which is what this
    /// stores. Opacity, offset, scale and [`LayerPose::wipe`] are the
    /// effects a custom transition can use.
    pub fn custom(blend: fn(f64) -> Blend) -> Self {
        Self::new(Kind::Custom(blend))
    }

    /// A shader preset paints the blend from both scenes' pixels.
    ///
    /// ```ignore
    /// transition(Transition::shader(shaders::water_wipe())).await;
    /// ```
    pub fn shader(shader: ShaderTransition) -> Self {
        Self::new(Kind::Shader(shader))
    }

    /// Blend length in seconds. Without this, the theme's normal motion is used.
    pub fn duration(mut self, seconds: f64) -> Self {
        self.seconds = Some(seconds);
        self
    }

    /// Easing of the blend. Without this, the theme's enter easing is used.
    pub fn ease(mut self, ease: CubicBezier) -> Self {
        self.ease = Some(ease);
        self
    }

    /// The previous scene keeps playing through the blend. By default it
    /// holds its last frame.
    pub fn overlap(mut self) -> Self {
        self.overlap = true;
        self
    }

    pub(crate) fn recorded(
        self,
        start: f64,
        fallback_seconds: f64,
        fallback_ease: CubicBezier,
    ) -> Recorded {
        let seconds = self.seconds.unwrap_or(fallback_seconds);
        let seconds = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        Recorded::new(
            self.kind,
            seconds,
            self.ease.unwrap_or(fallback_ease),
            self.overlap,
            start,
        )
    }
}

/// Plays `spec` at the start of a scene.
///
/// The call waits out the blend. Nodes added before it are on screen while
/// it runs; nodes added after appear when it finishes. On the first scene
/// there is nothing to blend, so the call returns immediately. The length
/// is saved in `meta.json` and the timeline can change it.
pub async fn transition(spec: Transition) {
    if !has_previous() {
        return;
    }
    let motion = current_theme().motion;
    let spec = spec.recorded(time(), motion.normal, motion.enter);
    let seconds = begin_transition(spec);
    if seconds > 0.0 {
        wait_for(seconds).await;
    }
}

/// Paints `from` and `to` (straight alpha, transparent outside the scene)
/// into one frame and lays that over `background`.
pub(crate) fn composite(
    from: &Image,
    to: &Image,
    blend: &Blend,
    t: f64,
    background: Color,
) -> Image {
    assert_eq!(from.width, to.width, "transition layers differ in width");
    assert_eq!(from.height, to.height, "transition layers differ in height");
    let w = from.width.max(1);
    let h = from.height.max(1);
    let n = (w as usize) * (h as usize);
    let mut buf = vec![[0.0f32; 4]; n];
    if blend.mix {
        mix_into(&mut buf, from, to, t as f32);
    } else if blend.incoming_on_top {
        paint(&mut buf, from, &blend.from);
        paint(&mut buf, to, &blend.to);
    } else {
        paint(&mut buf, to, &blend.to);
        paint(&mut buf, from, &blend.from);
    }
    flatten(&buf, w, h, background)
}

/// Paints a shader transition from `from` and `to` at eased progress `t`
/// and lays it over `background`.
pub(crate) fn composite_shader(
    from: &Image,
    to: &Image,
    shader: ShaderTransition,
    t: f64,
    background: Color,
) -> Image {
    assert_eq!(from.width, to.width, "transition layers differ in width");
    assert_eq!(from.height, to.height, "transition layers differ in height");
    let w = from.width.max(1);
    let h = from.height.max(1);
    let mut buf = vec![[0.0f32; 4]; (w as usize) * (h as usize)];
    // water_wipe is the only shader preset so far.
    composite_water(&mut buf, from, to, shader.wave_height() as f32, t as f32);
    flatten(&buf, w, h, background)
}

/// The water_wipe preset. The waterline starts below the frame and ends
/// above it, so progress 0 is exactly `from` and 1 is exactly `to`.
fn composite_water(buf: &mut [[f32; 4]], from: &Image, to: &Image, wave: f32, t: f32) {
    let t = t.clamp(0.0, 1.0);
    let (w, h) = (from.width as usize, from.height as usize);
    let (fw, fh) = (w as f32, h as f32);
    let amp = wave * fh;
    let band = (fh * 0.01).max(1.5);
    let depth = fh * 0.12;
    let tau = std::f32::consts::TAU;
    for x in 0..w {
        let fx = x as f32 + 0.5;
        let swell = (fx / fw * tau * 2.0 + t * 7.0).sin() * 0.7
            + (fx / fw * tau * 5.0 - t * 4.0).sin() * 0.3;
        // At 0 the line and its foam sit below the frame, at 1 above it.
        let line = (1.0 - t) * (fh + 2.0 * amp + 2.0 * band) - amp - band + amp * swell;
        for y in 0..h {
            let fy = y as f32 + 0.5;
            let i = y * w + x;
            let d = fy - line;
            let px = if d <= 0.0 {
                let j = i * 4;
                straight(&from.data[j..j + 4])
            } else {
                // Calm water at the end, so progress 1 is exactly `to`.
                let fade = (-d / depth).exp() * (1.0 - t);
                let shift = amp * 0.6 * fade * (fy * 0.045 + fx * 0.012 - t * 9.0).sin();
                let sx = (fx + shift * 0.5).clamp(0.0, fw - 1.0) as usize;
                let sy = (fy + shift).clamp(0.0, fh - 1.0) as usize;
                let j = (sy * w + sx) * 4;
                let (r, g, b, a) = straight(&to.data[j..j + 4]);
                let lift = 0.18 * fade;
                (
                    r + (1.0 - r) * lift,
                    g + (1.0 - g) * lift,
                    b + (1.0 - b) * lift,
                    a,
                )
            };
            let foam = if d.abs() < band {
                0.75 * (1.0 - d.abs() / band)
            } else {
                0.0
            };
            let (r, g, b, a) = px;
            // Premultiplied, with white foam over the pixel.
            let mixc = |c: f32| c * a * (1.0 - foam) + foam;
            buf[i] = [mixc(r), mixc(g), mixc(b), a + (1.0 - a) * foam];
        }
    }
}

fn mix_into(buf: &mut [[f32; 4]], from: &Image, to: &Image, t: f32) {
    let t = t.clamp(0.0, 1.0);
    for (dst, (a, b)) in buf
        .iter_mut()
        .zip(from.data.chunks_exact(4).zip(to.data.chunks_exact(4)))
    {
        let (ar, ag, ab, aa) = straight(a);
        let (br, bg, bb, ba) = straight(b);
        let u = 1.0 - t;
        dst[0] = ar * aa * u + br * ba * t;
        dst[1] = ag * aa * u + bg * ba * t;
        dst[2] = ab * aa * u + bb * ba * t;
        dst[3] = aa * u + ba * t;
    }
}

fn paint(buf: &mut [[f32; 4]], img: &Image, pose: &LayerPose) {
    let opacity = pose.opacity.clamp(0.0, 1.0) as f32;
    let scale = pose.scale as f32;
    if opacity <= 0.0 || !scale.is_finite() || scale.abs() < 1.0e-4 {
        return;
    }
    let w = img.width as i32;
    let h = img.height as i32;
    let dx = pose.dx as f32 * w as f32;
    let dy = pose.dy as f32 * h as f32;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    for y in 0..h {
        for x in 0..w {
            if let Some((dir, amount)) = pose.wipe {
                let amount = amount.clamp(0.0, 1.0) as f32;
                let shown = match dir {
                    Dir::Left => (x as f32) < amount * w as f32,
                    Dir::Right => (x as f32) >= (1.0 - amount) * w as f32,
                    Dir::Up => (y as f32) < amount * h as f32,
                    Dir::Down => (y as f32) >= (1.0 - amount) * h as f32,
                };
                if !shown {
                    continue;
                }
            }
            let sx = ((x as f32) - dx - cx) / scale + cx;
            let sy = ((y as f32) - dy - cy) / scale + cy;
            let (ix, iy) = (sx.floor() as i32, sy.floor() as i32);
            if ix < 0 || iy < 0 || ix >= w || iy >= h {
                continue;
            }
            let i = (iy as u32 * img.width + ix as u32) as usize * 4;
            let (r, g, b, a) = straight(&img.data[i..i + 4]);
            let a = a * opacity;
            let dst = &mut buf[(y as u32 * img.width + x as u32) as usize];
            let inv = 1.0 - a;
            dst[0] = r * a + dst[0] * inv;
            dst[1] = g * a + dst[1] * inv;
            dst[2] = b * a + dst[2] * inv;
            dst[3] = a + dst[3] * inv;
        }
    }
}

fn flatten(buf: &[[f32; 4]], w: u32, h: u32, background: Color) -> Image {
    let bg_a = background.a;
    let (bg_r, bg_g, bg_b) = (
        background.r * bg_a,
        background.g * bg_a,
        background.b * bg_a,
    );
    let mut data = Vec::with_capacity(buf.len() * 4);
    for px in buf {
        let inv = 1.0 - px[3];
        let a = px[3] + bg_a * inv;
        let (r, g, b) = if a < 1.0e-5 {
            (0.0, 0.0, 0.0)
        } else {
            (
                (px[0] + bg_r * inv) / a,
                (px[1] + bg_g * inv) / a,
                (px[2] + bg_b * inv) / a,
            )
        };
        data.extend_from_slice(&[byte(r), byte(g), byte(b), byte(a)]);
    }
    Image {
        width: w,
        height: h,
        data,
    }
}

fn straight(px: &[u8]) -> (f32, f32, f32, f32) {
    (
        px[0] as f32 / 255.0,
        px[1] as f32 / 255.0,
        px[2] as f32 / 255.0,
        px[3] as f32 / 255.0,
    )
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use oxframe_core::{BoxFuture, Color, Image};
    use oxframe_protocol::Meta;
    use oxframe_render::CpuRenderer;

    use super::*;
    use crate::{
        Engine, NodeBuilder, Project, Rect, SceneDef, SourceLocation, View, feedback, presets,
    };

    const RED: Color = Color::rgb(1.0, 0.0, 0.0);
    const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

    fn scene(name: &'static str, factory: fn(View) -> BoxFuture) -> SceneDef {
        SceneDef::new(
            name,
            SourceLocation {
                file: "src/transition.rs",
                manifest_dir: env!("CARGO_MANIFEST_DIR"),
            },
            factory,
        )
    }

    fn project(scenes: Vec<SceneDef>) -> Project {
        let mut p = Project::new("transitions")
            .size(32, 18)
            .fps(10)
            .background(Color::BLACK)
            .write_meta(false)
            .inline_meta(BTreeMap::<String, Meta>::new());
        for s in scenes {
            p = p.scene(s);
        }
        p
    }

    fn cover(view: &View, color: Color) {
        let (w, h) = view.size();
        view.add(Rect::new().size(w.max(h) as f64).fill(color));
    }

    fn at(img: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        img.data[i..i + 4].try_into().unwrap()
    }

    fn near(px: [u8; 4], rgb: [u8; 3]) -> bool {
        px[0].abs_diff(rgb[0]) <= 2 && px[1].abs_diff(rgb[1]) <= 2 && px[2].abs_diff(rgb[2]) <= 2
    }

    fn red_then_wait(view: View) -> BoxFuture {
        Box::pin(async move {
            cover(&view, RED);
            wait_for(0.2).await;
        })
    }

    fn fade_in(view: View) -> BoxFuture {
        Box::pin(async move {
            cover(&view, BLUE);
            transition(Transition::fade().duration(1.0).ease(presets::LINEAR)).await;
            wait_for(0.2).await;
        })
    }
    fn slide_in(view: View) -> BoxFuture {
        Box::pin(async move {
            cover(&view, BLUE);
            transition(
                Transition::slide(Dir::Left)
                    .duration(1.0)
                    .ease(presets::LINEAR),
            )
            .await;
            wait_for(0.2).await;
        })
    }
    fn wipe_in(view: View) -> BoxFuture {
        Box::pin(async move {
            cover(&view, BLUE);
            transition(
                Transition::wipe(Dir::Left)
                    .duration(1.0)
                    .ease(presets::LINEAR),
            )
            .await;
            wait_for(0.2).await;
        })
    }
    fn zoom_in_scene(view: View) -> BoxFuture {
        Box::pin(async move {
            cover(&view, BLUE);
            transition(Transition::zoom_in().duration(1.0).ease(presets::LINEAR)).await;
            wait_for(0.2).await;
        })
    }
    fn zoom_out_scene(view: View) -> BoxFuture {
        Box::pin(async move {
            cover(&view, BLUE);
            transition(Transition::zoom_out().duration(1.0).ease(presets::LINEAR)).await;
            wait_for(0.2).await;
        })
    }
    fn custom_in(view: View) -> BoxFuture {
        Box::pin(async move {
            cover(&view, BLUE);
            transition(
                Transition::custom(|_t| Blend {
                    mix: true,
                    ..Blend::default()
                })
                .duration(1.0)
                .ease(presets::LINEAR),
            )
            .await;
            wait_for(0.2).await;
        })
    }

    fn water_in(view: View) -> BoxFuture {
        Box::pin(async move {
            cover(&view, BLUE);
            transition(
                Transition::shader(crate::shaders::water_wipe())
                    .duration(1.0)
                    .ease(presets::LINEAR),
            )
            .await;
            wait_for(0.2).await;
        })
    }

    #[test]
    fn water_wipe_starts_on_the_old_scene_and_ends_on_the_new() {
        let mut engine = Engine::new(project(vec![
            scene("a", red_then_wait),
            scene("b", water_in),
        ]))
        .unwrap();
        let (start, mid, done) = ends(&engine, "water_wipe");
        let all = |img: &Image, rgb: [u8; 3]| {
            (0..img.height).all(|y| (0..img.width).all(|x| near(at(img, x, y), rgb)))
        };
        assert!(
            all(&draw(&mut engine, start), [255, 0, 0]),
            "0% is the old scene"
        );
        assert!(
            all(&draw(&mut engine, done), [0, 0, 255]),
            "100% is the new scene"
        );
        let half = draw(&mut engine, mid);
        let x = half.width / 2;
        assert!(
            near(at(&half, x, 0), [255, 0, 0]),
            "the water has not reached the top"
        );
        assert!(
            near(at(&half, x, half.height - 1), [0, 0, 255]),
            "the new scene is under the water"
        );
        let inspected = feedback::inspect(
            project(vec![scene("a", red_then_wait), scene("b", water_in)]),
            f64::from(mid) / 10.0,
        )
        .unwrap();
        assert_eq!(inspected["transition"]["kind"], "water_wipe");
    }

    fn draw(engine: &mut Engine, frame: u32) -> Image {
        let size = engine.size();
        engine.render(frame, &mut CpuRenderer::new(), size)
    }

    fn ends(engine: &Engine, kind: &str) -> (u32, u32, u32) {
        let span = engine
            .transition_spans()
            .into_iter()
            .find(|s| s.kind == kind)
            .unwrap_or_else(|| panic!("no {kind} span"));
        let mid = span.start + span.frames / 2;
        let done = span.start + span.frames;
        (span.start, mid, done)
    }

    #[test]
    fn builtins_at_zero_half_and_done() {
        type SceneFn = fn(View) -> BoxFuture;
        let cases: &[(&str, SceneFn)] = &[
            ("fade", fade_in),
            ("slide", slide_in),
            ("wipe", wipe_in),
            ("zoom_in", zoom_in_scene),
            ("zoom_out", zoom_out_scene),
            ("custom", custom_in),
        ];
        for (kind, factory) in cases {
            let mut engine = Engine::new(project(vec![
                scene("a", red_then_wait),
                scene("b", *factory),
            ]))
            .unwrap();
            let (start, mid, done) = ends(&engine, kind);
            let center = (engine.size().0 / 2, engine.size().1 / 2);
            let start_px = at(&draw(&mut engine, start), center.0, center.1);
            let mid_px = at(&draw(&mut engine, mid), center.0, center.1);
            let done_px = at(&draw(&mut engine, done), center.0, center.1);
            assert!(
                near(start_px, [255, 0, 0]),
                "{kind} at 0% is {start_px:?}, wanted red"
            );
            assert!(
                near(done_px, [0, 0, 255]),
                "{kind} at 100% is {done_px:?}, wanted blue"
            );
            match *kind {
                "fade" | "custom" => assert!(
                    near(mid_px, [128, 0, 128]),
                    "{kind} at 50% is {mid_px:?}, wanted purple"
                ),
                "slide" => {
                    // The pair travels left: the outgoing scene is still on the left.
                    let left = at(&draw(&mut engine, mid), 2, center.1);
                    let right = at(&draw(&mut engine, mid), engine.size().0 - 3, center.1);
                    assert!(near(left, [255, 0, 0]), "{kind} left at 50% is {left:?}");
                    assert!(near(right, [0, 0, 255]), "{kind} right at 50% is {right:?}");
                }
                "wipe" => {
                    // The incoming scene is uncovered from the left.
                    let left = at(&draw(&mut engine, mid), 2, center.1);
                    let right = at(&draw(&mut engine, mid), engine.size().0 - 3, center.1);
                    assert!(near(left, [0, 0, 255]), "{kind} left at 50% is {left:?}");
                    assert!(near(right, [255, 0, 0]), "{kind} right at 50% is {right:?}");
                }
                "zoom_in" | "zoom_out" => {
                    assert!(
                        mid_px[0] > 40 && mid_px[2] > 40,
                        "{kind} at 50% is {mid_px:?}"
                    );
                    assert_eq!(mid_px[3], 255, "{kind} stays opaque");
                }
                _ => unreachable!(),
            }
            let again = draw(&mut engine, mid);
            assert_eq!(
                again.data,
                draw(&mut engine, mid).data,
                "{kind} is deterministic"
            );
        }
    }

    #[test]
    fn slide_holds_the_last_frame_unless_overlap() {
        fn src(view: View) -> BoxFuture {
            Box::pin(async move {
                let (w, h) = view.size();
                let r = view.add(Rect::new().size(w.max(h) as f64).fill(RED));
                wait_for(0.8).await;
                r.fill().set(BLUE);
                wait_for(0.2).await;
            })
        }
        fn held(_view: View) -> BoxFuture {
            Box::pin(async move {
                transition(
                    Transition::slide(Dir::Left)
                        .duration(0.5)
                        .ease(presets::LINEAR),
                )
                .await;
                wait_for(0.1).await;
            })
        }
        fn overlapped(_view: View) -> BoxFuture {
            Box::pin(async move {
                transition(
                    Transition::slide(Dir::Left)
                        .duration(0.5)
                        .ease(presets::LINEAR)
                        .overlap(),
                )
                .await;
                wait_for(0.1).await;
            })
        }
        let mut held_engine =
            Engine::new(project(vec![scene("a", src), scene("b", held)])).unwrap();
        let mut over_engine =
            Engine::new(project(vec![scene("a", src), scene("b", overlapped)])).unwrap();
        let (start, _, _) = ends(&held_engine, "slide");
        let (over_start, _, _) = ends(&over_engine, "slide");
        let c = (held_engine.size().0 / 2, held_engine.size().1 / 2);
        assert!(
            near(at(&draw(&mut held_engine, start), c.0, c.1), [0, 0, 255]),
            "a held transition shows the previous scene's last frame"
        );
        assert!(
            near(
                at(&draw(&mut over_engine, over_start), c.0, c.1),
                [255, 0, 0]
            ),
            "overlap keeps the previous scene playing"
        );
        assert!(over_engine.transition_at(over_start).unwrap().overlap);
    }

    #[test]
    fn transparent_background_and_three_aspects() {
        fn empty(_view: View) -> BoxFuture {
            Box::pin(async move {
                wait_for(0.2).await;
            })
        }
        fn dot(view: View) -> BoxFuture {
            Box::pin(async move {
                view.add(Rect::new().size(8.0).fill(BLUE));
                transition(Transition::fade().duration(1.0).ease(presets::LINEAR)).await;
                wait_for(0.2).await;
            })
        }
        let mut engine = Engine::new(
            project(vec![scene("a", empty), scene("b", dot)]).background(Color::TRANSPARENT),
        )
        .unwrap();
        let (start, mid, done) = ends(&engine, "fade");
        let (w, h) = engine.size();
        let center = at(&draw(&mut engine, mid), w / 2, h / 2);
        let corner = at(&draw(&mut engine, mid), 0, 0);
        assert_eq!(at(&draw(&mut engine, start), w / 2, h / 2)[3], 0);
        assert!(
            center[2] > 200 && (center[3] as i32 - 128).abs() <= 2,
            "{center:?}"
        );
        assert_eq!(corner[3], 0, "the corner stays clear");
        assert_eq!(at(&draw(&mut engine, done), w / 2, h / 2)[3], 255);

        for (width, height) in [(32, 18), (18, 32), (18, 18)] {
            let mut shaped = Engine::new(
                project(vec![scene("a", red_then_wait), scene("b", fade_in)])
                    .size(width, height)
                    .background(Color::WHITE),
            )
            .unwrap();
            let (_, mid, _) = ends(&shaped, "fade");
            let px = at(&draw(&mut shaped, mid), width / 2, height / 2);
            assert!(
                near(px, [128, 0, 128]),
                "{width}x{height} midpoint is {px:?}"
            );
        }
    }

    #[test]
    fn duration_and_audio_include_the_blend() {
        fn tail(_view: View) -> BoxFuture {
            Box::pin(async move {
                wait_for(0.2).await;
            })
        }
        let engine = Engine::new(
            project(vec![
                scene("a", red_then_wait),
                scene("b", fade_in),
                scene("c", tail),
            ])
            .audio("voice.mp3"),
        )
        .unwrap();
        let scenes = engine.scenes();
        assert!(scenes[1].transition > 0);
        assert!(scenes[1].frames > scenes[1].transition);
        assert_eq!(scenes[1].start, scenes[0].frames);
        assert_eq!(scenes[2].start, scenes[0].frames + scenes[1].frames);
        assert_eq!(engine.total_frames(), scenes[2].start + scenes[2].frames);
        let (file, offset) = engine.audio().unwrap();
        assert_eq!(file, "voice.mp3");
        assert_eq!(offset, 0.0);
        let shifted = offset - f64::from(scenes[1].start) / f64::from(engine.fps());
        assert!(
            shifted < 0.0,
            "a later scene's audio moves with the timeline"
        );
        let saved = engine
            .meta("b")
            .unwrap()
            .transition
            .as_ref()
            .unwrap()
            .seconds;
        assert!((saved - 1.0).abs() < 1e-9);
    }

    #[test]
    fn timeline_duration_overrides_the_code() {
        let engine = Engine::new(project(vec![
            scene("a", red_then_wait),
            scene("b", fade_in),
        ]))
        .unwrap();
        let mut meta = engine.meta("b").unwrap().clone();
        meta.transition.as_mut().unwrap().seconds = 0.2;
        let mut metas = BTreeMap::new();
        metas.insert("b".to_owned(), meta);
        let shorter = Engine::new(
            project(vec![scene("a", red_then_wait), scene("b", fade_in)]).inline_meta(metas),
        )
        .unwrap();
        assert!(shorter.scenes()[1].transition < engine.scenes()[1].transition);
        assert!(shorter.total_frames() < engine.total_frames());
    }

    #[test]
    fn single_scene_render_skips_the_incoming_blend() {
        let mut engine = Engine::new(project(vec![
            scene("a", red_then_wait),
            scene("b", fade_in),
        ]))
        .unwrap();
        let (start, _, _) = ends(&engine, "fade");
        let c = (engine.size().0 / 2, engine.size().1 / 2);
        assert!(near(at(&draw(&mut engine, start), c.0, c.1), [255, 0, 0]));
        engine.skip_transitions(true);
        assert!(near(at(&draw(&mut engine, start), c.0, c.1), [0, 0, 255]));
    }

    #[test]
    fn first_scene_transition_does_not_wait() {
        fn only(_view: View) -> BoxFuture {
            Box::pin(async move {
                transition(Transition::fade().duration(1.0)).await;
                wait_for(0.2).await;
            })
        }
        fn plain(_view: View) -> BoxFuture {
            Box::pin(async move {
                wait_for(0.2).await;
            })
        }
        let with = Engine::new(project(vec![scene("a", only)])).unwrap();
        let without = Engine::new(project(vec![scene("a", plain)])).unwrap();
        assert_eq!(with.total_frames(), without.total_frames());
        assert!(with.transition_spans().is_empty());
    }

    #[test]
    fn theme_supplies_the_duration() {
        fn soft(view: View) -> BoxFuture {
            Box::pin(async move {
                cover(&view, BLUE);
                transition(Transition::fade()).await;
                wait_for(0.1).await;
            })
        }
        let engine =
            Engine::new(project(vec![scene("a", red_then_wait), scene("b", soft)])).unwrap();
        let seconds = engine
            .meta("b")
            .unwrap()
            .transition
            .as_ref()
            .unwrap()
            .seconds;
        assert!(
            (seconds - 0.45).abs() < 1e-9,
            "dark theme normal motion is 0.45s"
        );
    }

    #[test]
    fn inspect_names_the_transition_and_check_shows_it() {
        let built = project(vec![scene("a", red_then_wait), scene("b", fade_in)]);
        let engine = Engine::new(built.clone()).unwrap();
        let (_, mid, _) = ends(&engine, "fade");
        let t = f64::from(mid) / f64::from(engine.fps());
        let json = feedback::inspect(built.clone(), t).unwrap();
        assert_eq!(json["transition"]["kind"], "fade");
        assert!(json["transition"]["dir"].is_null());
        let progress = json["transition"]["progress"].as_f64().unwrap();
        assert!((progress - 0.5).abs() < 0.08, "{progress}");
        let checked = feedback::check(built, &feedback::Options::default()).unwrap();
        let blended = checked
            .storyboard
            .data
            .chunks_exact(4)
            .any(|p| p[0] > 60 && p[2] > 60 && p[1] < 40);
        assert!(blended, "the storyboard includes a fade frame");
        assert!(checked.report.duration > 1.0);
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn gpu_matches_cpu_on_a_fade() {
        let mut engine = Engine::new(project(vec![
            scene("a", red_then_wait),
            scene("b", fade_in),
        ]))
        .unwrap();
        let (_, mid, _) = ends(&engine, "fade");
        let size = engine.size();
        let cpu = engine.render(mid, &mut CpuRenderer::new(), size);
        let mut gpu = match oxframe_render::GpuRenderer::new() {
            Ok(g) if g.is_software() && std::env::var_os("OXFRAME_GPU_TESTS").is_none() => return,
            Ok(g) => g,
            Err(_) => return,
        };
        let gpu_img = engine.render(mid, &mut gpu, size);
        let c = (size.0 / 2, size.1 / 2);
        let (a, b) = (at(&cpu, c.0, c.1), at(&gpu_img, c.0, c.1));
        let ca = Color::from_rgba8(a[0], a[1], a[2], a[3]).to_oklab();
        let cb = Color::from_rgba8(b[0], b[1], b[2], b[3]).to_oklab();
        let de = ((ca[0] - cb[0]).powi(2) + (ca[1] - cb[1]).powi(2) + (ca[2] - cb[2]).powi(2))
            .sqrt()
            * 100.0;
        assert!(de <= 1.0, "cpu {a:?} gpu {b:?} ΔE {de}");
    }
}
