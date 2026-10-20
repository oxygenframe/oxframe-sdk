use std::collections::BTreeMap;
use std::fmt;

use oxframe_2d::{
    DrawItem, Layer, LineCap, LineJoin, Paint as MPaint, Primitive, RasterImage, SceneGraph,
    StrokeStyle,
};
use oxframe_core::{BlendMode, Color, Gradient, GradientKind};
use vello::kurbo::{Affine, Cap, Join, Rect, Stroke};
use vello::peniko::{
    Blob, Brush, Compose, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality,
    Mix,
};
use vello::wgpu;
use vello::{AaConfig, RenderParams, Renderer as VelloRenderer, RendererOptions, Scene};

use crate::{CpuRenderer, Image, RenderOptions, Renderer, scene_to_pixels};

/// Why the GPU renderer could not start.
#[derive(Debug)]
pub enum GpuError {
    /// No usable GPU adapter.
    NoAdapter(String),
    /// The adapter refused to create a device.
    Device(String),
    /// vello could not create its renderer.
    Vello(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter(e) => write!(f, "no GPU adapter available ({e}); use --cpu"),
            Self::Device(e) => write!(f, "cannot create GPU device: {e}"),
            Self::Vello(e) => write!(f, "vello: {e}"),
        }
    }
}

impl std::error::Error for GpuError {}

/// vello backend rendering headlessly into a texture and reading it back.
pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: VelloRenderer,
    scene: Scene,
    target: Option<Target>,
    images: BTreeMap<u64, ImageData>,
    adapter_name: String,
    software: bool,
    /// Rasterizes layers with filters (blur, shadow) exactly like the CPU
    /// backend; the result is drawn as an image.
    cpu: CpuRenderer,
    /// Browser: the canvas the frames are presented to.
    #[cfg(target_arch = "wasm32")]
    canvas: Option<CanvasTarget>,
}

#[cfg(target_arch = "wasm32")]
struct CanvasTarget {
    canvas: web_sys::OffscreenCanvas,
    surface: wgpu::Surface<'static>,
    blitter: wgpu::util::TextureBlitter,
    format: wgpu::TextureFormat,
    size: (u32, u32),
}

struct Target {
    width: u32,
    height: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    buffer: wgpu::Buffer,
    padded_row: u32,
}

impl GpuRenderer {
    /// Creates a renderer on the best available adapter (blocks).
    ///
    /// # Errors
    ///
    /// If there is no GPU or the device cannot be created.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Result<Self, GpuError> {
        pollster::block_on(Self::new_async())
    }

    /// Like `new`, for async contexts such as the browser.
    ///
    /// # Errors
    ///
    /// If there is no GPU or the device cannot be created.
    pub async fn new_async() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        Self::with_instance(instance, None).await
    }

    /// Browser: renders into `canvas` (WebGPU). Frames are presented with
    /// [`render_to_canvas`](Self::render_to_canvas); no readback.
    #[cfg(target_arch = "wasm32")]
    pub async fn new_for_canvas(canvas: web_sys::OffscreenCanvas) -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
            .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        let mut r = Self::with_instance(instance, Some(&surface)).await?;
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let blitter = wgpu::util::TextureBlitter::new(&r.device, format);
        r.canvas = Some(CanvasTarget {
            canvas,
            surface,
            blitter,
            format,
            size: (0, 0),
        });
        Ok(r)
    }

    async fn with_instance(
        instance: wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, GpuError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: surface,
                ..Default::default()
            })
            .await
            .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        let info = adapter.get_info();
        let adapter_name = format!("{} ({:?}, {})", info.name, info.backend, info.driver);
        let software = info.device_type == wgpu::DeviceType::Cpu;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("oxframe"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .map_err(|e| GpuError::Device(e.to_string()))?;
        let renderer = VelloRenderer::new(
            &device,
            RendererOptions {
                antialiasing_support: vello::AaSupport::area_only(),
                ..Default::default()
            },
        )
        .map_err(|e| GpuError::Vello(e.to_string()))?;
        Ok(Self {
            device,
            queue,
            renderer,
            scene: Scene::new(),
            target: None,
            images: BTreeMap::new(),
            adapter_name,
            software,
            cpu: CpuRenderer::new(),
            #[cfg(target_arch = "wasm32")]
            canvas: None,
        })
    }

    /// Name of the GPU in use, for logs.
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Whether the adapter emulates a GPU on the CPU (WARP, llvmpipe,
    /// SwiftShader): correct but slow, and not bit-comparable to hardware.
    pub fn is_software(&self) -> bool {
        self.software
    }

    fn target(&mut self, width: u32, height: u32) -> &Target {
        if self
            .target
            .as_ref()
            .is_none_or(|t| t.width != width || t.height != height)
        {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("oxframe target"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                // TEXTURE_BINDING: the browser path samples it when copying to the canvas.
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let padded_row = (width * 4).div_ceil(256) * 256;
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("oxframe readback"),
                size: u64::from(padded_row) * u64::from(height),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.target = Some(Target {
                width,
                height,
                texture,
                view,
                buffer,
                padded_row,
            });
        }
        self.target.as_ref().expect("target was just created")
    }
}

fn color(c: Color) -> vello::peniko::Color {
    vello::peniko::Color::new([c.r, c.g, c.b, c.a])
}

fn image_data(img: &RasterImage) -> ImageData {
    ImageData {
        data: Blob::from(img.rgba.clone()),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: img.width,
        height: img.height,
    }
}

fn brush(p: &MPaint) -> Brush {
    match p {
        MPaint::Solid(c) => Brush::Solid(color(*c)),
        MPaint::Gradient(g) => Brush::Gradient(gradient(g)),
    }
}

fn gradient(g: &Gradient) -> vello::peniko::Gradient {
    let stops: Vec<vello::peniko::ColorStop> = g
        .stops
        .iter()
        .map(|(o, c)| vello::peniko::ColorStop::from((*o as f32, color(*c))))
        .collect();
    let base = match g.kind {
        GradientKind::Linear { from, to } => vello::peniko::Gradient::new_linear(from, to),
        GradientKind::Radial { center, radius } => {
            vello::peniko::Gradient::new_radial(center, radius.max(1e-3) as f32)
        }
    };
    base.with_stops(stops.as_slice())
}

fn stroke(s: &StrokeStyle) -> Stroke {
    let mut st = Stroke::new(s.width)
        .with_caps(match s.cap {
            LineCap::Butt => Cap::Butt,
            LineCap::Round => Cap::Round,
            LineCap::Square => Cap::Square,
        })
        .with_join(match s.join {
            LineJoin::Miter => Join::Miter,
            LineJoin::Round => Join::Round,
            LineJoin::Bevel => Join::Bevel,
        });
    if s.dash.len() >= 2 && s.dash.iter().any(|d| *d > 0.0) {
        st = st.with_dashes(s.dash_offset, s.dash.iter().copied());
    }
    st
}

fn blend(b: BlendMode) -> vello::peniko::BlendMode {
    let mix = match b {
        BlendMode::Normal => Mix::Normal,
        BlendMode::Multiply => Mix::Multiply,
        BlendMode::Screen => Mix::Screen,
        BlendMode::Overlay => Mix::Overlay,
        BlendMode::Darken => Mix::Darken,
        BlendMode::Lighten => Mix::Lighten,
        BlendMode::ColorDodge => Mix::ColorDodge,
        BlendMode::ColorBurn => Mix::ColorBurn,
        BlendMode::HardLight => Mix::HardLight,
        BlendMode::SoftLight => Mix::SoftLight,
        BlendMode::Difference => Mix::Difference,
        BlendMode::Exclusion => Mix::Exclusion,
        BlendMode::Hue => Mix::Hue,
        BlendMode::Saturation => Mix::Saturation,
        BlendMode::Color => Mix::Color,
        BlendMode::Luminosity => Mix::Luminosity,
    };
    vello::peniko::BlendMode::new(mix, Compose::SrcOver)
}

/// Index of the `PopLayer` matching the `PushLayer` at `start`.
fn matching_pop(items: &[DrawItem], start: usize) -> usize {
    let mut depth = 0usize;
    for (i, item) in items.iter().enumerate().skip(start) {
        match item.prim {
            Primitive::PushLayer(_) => depth += 1,
            Primitive::PopLayer => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    items.len() - 1
}

impl GpuRenderer {
    fn encode(&mut self, items: &[DrawItem], view: Affine, w: u32, h: u32) {
        let full = Rect::new(0.0, 0.0, f64::from(w), f64::from(h));
        let mut i = 0;
        while i < items.len() {
            let item = &items[i];
            let tf = view * item.transform;
            match &item.prim {
                Primitive::Fill { path, paint } => {
                    self.scene
                        .fill(Fill::NonZero, tf, &brush(paint), None, path);
                }
                Primitive::Stroke { path, paint, style } => {
                    self.scene
                        .stroke(&stroke(style), tf, &brush(paint), None, path);
                }
                Primitive::Image {
                    image,
                    rect,
                    opacity,
                } => {
                    let data = self
                        .images
                        .entry(image.id)
                        .or_insert_with(|| image_data(image))
                        .clone();
                    let brush = ImageBrush::new(data)
                        .with_quality(ImageQuality::High)
                        .with_alpha(*opacity as f32);
                    let place = tf
                        * Affine::translate((rect.x0, rect.y0))
                        * Affine::scale_non_uniform(
                            rect.width() / f64::from(image.width),
                            rect.height() / f64::from(image.height),
                        );
                    self.scene.draw_image(&brush, place);
                }
                Primitive::PushLayer(layer) if layer.has_filter() => {
                    // Rasterize the whole layer on the CPU (blend and opacity
                    // are applied here, against the real backdrop).
                    let end = matching_pop(items, i);
                    let mut sub: Vec<DrawItem> = items[i..=end].to_vec();
                    sub[0].prim = Primitive::PushLayer(Layer {
                        opacity: 1.0,
                        blend: BlendMode::Normal,
                        ..layer.clone()
                    });
                    let pm = self.cpu.render_items(&sub, view, w, h);
                    let data = ImageData {
                        data: Blob::from(pm.data().to_vec()),
                        format: ImageFormat::Rgba8,
                        alpha_type: ImageAlphaType::AlphaPremultiplied,
                        width: w,
                        height: h,
                    };
                    self.scene.push_layer(
                        Fill::NonZero,
                        blend(layer.blend),
                        layer.opacity as f32,
                        Affine::IDENTITY,
                        &full,
                    );
                    self.scene
                        .draw_image(&ImageBrush::new(data), Affine::IDENTITY);
                    self.scene.pop_layer();
                    i = end;
                }
                Primitive::PushLayer(layer) => match &layer.clip {
                    Some(path) => self.scene.push_layer(
                        Fill::NonZero,
                        blend(layer.blend),
                        layer.opacity as f32,
                        tf,
                        path,
                    ),
                    None => self.scene.push_layer(
                        Fill::NonZero,
                        blend(layer.blend),
                        layer.opacity as f32,
                        Affine::IDENTITY,
                        &full,
                    ),
                },
                Primitive::PopLayer => self.scene.pop_layer(),
            }
            i += 1;
        }
    }
}

impl GpuRenderer {
    /// Encodes the scene and renders it into the internal target texture.
    fn draw_to_target(&mut self, graph: &SceneGraph, options: &RenderOptions) -> (u32, u32) {
        let (w, h) = (options.width.max(1), options.height.max(1));
        let view = scene_to_pixels(graph.size(), (w, h));
        self.scene.reset();
        let items = graph.draw_list();
        self.encode(&items, view, w, h);

        let bg = options.background;
        let params = RenderParams {
            base_color: color(bg),
            width: w,
            height: h,
            antialiasing_method: AaConfig::Area,
        };
        self.target(w, h);
        let Self {
            device,
            queue,
            renderer,
            scene,
            target,
            ..
        } = self;
        let t = target.as_ref().expect("target exists");
        renderer
            .render_to_texture(device, queue, scene, &t.view, &params)
            .expect("vello render failed");
        (w, h)
    }

    /// Browser: rasterizes with the CPU renderer. A transition needs two
    /// pictures, and the canvas can be acquired only once per frame.
    #[cfg(target_arch = "wasm32")]
    pub fn render_cpu(&mut self, graph: &SceneGraph, options: &RenderOptions) -> Image {
        self.cpu.render(graph, options)
    }

    /// Browser: uploads a straight-alpha image and presents it once.
    #[cfg(target_arch = "wasm32")]
    pub fn present_image(&mut self, image: &Image) {
        let (w, h) = (image.width, image.height);
        if w == 0 || h == 0 || image.data.len() < (w as usize) * (h as usize) * 4 {
            return;
        }
        self.target(w, h);
        let padded = (w * 4).div_ceil(256) * 256;
        let mut bytes = vec![0u8; padded as usize * h as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let a = u16::from(image.data[i + 3]);
                let premul = |c: u8| -> u8 { ((u16::from(c) * a + 127) / 255) as u8 };
                let o = (y * padded + x * 4) as usize;
                bytes[o] = premul(image.data[i]);
                bytes[o + 1] = premul(image.data[i + 1]);
                bytes[o + 2] = premul(image.data[i + 2]);
                bytes[o + 3] = image.data[i + 3];
            }
        }
        let texture = self.target.as_ref().expect("target exists").texture.clone();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.present_target(w, h);
    }

    /// Browser: draws the frame into the canvas passed to
    /// [`new_for_canvas`](Self::new_for_canvas) and presents it.
    #[cfg(target_arch = "wasm32")]
    pub fn render_to_canvas(&mut self, graph: &SceneGraph, options: &RenderOptions) {
        let (w, h) = self.draw_to_target(graph, options);
        self.present_target(w, h);
    }

    /// Copies the offscreen target onto the canvas. One acquire per call.
    #[cfg(target_arch = "wasm32")]
    fn present_target(&mut self, w: u32, h: u32) {
        let Self {
            device,
            queue,
            target,
            canvas,
            ..
        } = self;
        let Some(c) = canvas.as_mut() else { return };
        if c.size != (w, h) {
            c.canvas.set_width(w);
            c.canvas.set_height(h);
            c.surface.configure(
                device,
                &wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: c.format,
                    width: w,
                    height: h,
                    present_mode: wgpu::PresentMode::Fifo,
                    desired_maximum_frame_latency: 1,
                    alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
                    color_space: wgpu::SurfaceColorSpace::Auto,
                    view_formats: vec![],
                },
            );
            c.size = (w, h);
        }
        let frame = match c.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oxframe present"),
        });
        let t = target.as_ref().expect("target exists");
        c.blitter.copy(device, &mut encoder, &t.view, &view);
        queue.submit([encoder.finish()]);
        queue.present(frame);
    }
}

impl Renderer for GpuRenderer {
    fn render(&mut self, graph: &SceneGraph, options: &RenderOptions) -> Image {
        let (w, h) = self.draw_to_target(graph, options);
        let Self {
            device,
            queue,
            target,
            ..
        } = self;
        let t = target.as_ref().expect("target exists");
        let trace = std::env::var_os("OXFRAME_GPU_TRACE").is_some();
        let t1 = std::time::Instant::now();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oxframe readback"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &t.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &t.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(t.padded_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = t.buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("map readback buffer"));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("GPU poll");
        if trace {
            eprintln!(
                "copy+map wait: {:.2} ms",
                t1.elapsed().as_secs_f64() * 1000.0
            );
        }
        // Readback buffers can live in uncached memory (e.g. Mesa ANV on
        // integrated GPUs), where one core reads only ~100 MB/s; reading
        // disjoint row bands from several threads scales almost linearly.
        let row_bytes = (w * 4) as usize;
        let mut data = vec![0u8; row_bytes * h as usize];
        {
            let mapped = slice.get_mapped_range().expect("map readback range");
            let src: &[u8] = &mapped;
            let threads = std::thread::available_parallelism()
                .map_or(4, |n| n.get())
                .min(16);
            let rows_per = (h as usize).div_ceil(threads);
            std::thread::scope(|scope| {
                for (band, out) in data.chunks_mut(rows_per * row_bytes).enumerate() {
                    let padded = t.padded_row as usize;
                    scope.spawn(move || {
                        for (r, dst) in out.chunks_exact_mut(row_bytes).enumerate() {
                            let y = band * rows_per + r;
                            dst.copy_from_slice(&src[y * padded..y * padded + row_bytes]);
                        }
                    });
                }
            });
        }
        t.buffer.unmap();
        if trace {
            eprintln!("readback: {:.2} ms", t1.elapsed().as_secs_f64() * 1000.0);
        }
        // vello writes premultiplied alpha; output is straight alpha.
        for px in data.chunks_exact_mut(4) {
            let a = px[3];
            if a != 0 && a != 255 {
                for c in &mut px[..3] {
                    *c = ((u16::from(*c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8;
                }
            }
        }
        Image {
            width: w,
            height: h,
            data,
        }
    }
}
