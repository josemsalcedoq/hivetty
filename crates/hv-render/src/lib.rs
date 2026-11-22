//! GPU renderer for the terminal grid (wgpu: Metal / Vulkan / DX12 / GL).
//!
//! Spike-quality text path (SP-1): glyphon (cosmic-text + swash) lays out each row with a fixed
//! monospace advance so glyphs stay on the cell grid. Phase F1 replaces it with our own glyph
//! atlas, built-in box drawing and damage tracking (docs/architecture.md section 4).

mod boxdraw;
mod rects;

use std::fmt;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use glyphon::{
    Attrs, Buffer, Cache, Color as TextColor, ColorMode, Family, FontSystem, Metrics, Resolution,
    Shaping, Style, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight,
};
use hv_grid::{CursorKind, Rgb8, SnapCell, Snapshot};
use wgpu::rwh::{HasDisplayHandle, HasWindowHandle};

use crate::rects::{Rect, RectPipeline};

/// Monospace families tried in order; the first one installed wins.
const PREFERRED_FONTS: &[&str] = &[
    "JetBrains Mono",
    "Cascadia Mono",
    "Cascadia Code",
    "SF Mono",
    "Menlo",
    "Consolas",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
];

/// Label of the pill shown while scrolled up (UI strings move to hv-ui/src/strings.rs in F2).
const JUMP_LABEL: &str = "↓ Jump to bottom";

/// Pill background (`bg.elevated` in docs/ui-spec.md) and border.
const PILL_BG: Rgb8 = Rgb8::hex(0x18181B);
const PILL_BORDER: Rgb8 = Rgb8::hex(0x3A3A40);

/// Padding around the grid, in logical pixels.
const PADDING: f32 = 8.0;

/// One frame of egui chrome, ready to paint (from `hv-ui`).
pub struct UiFrame {
    /// Tessellated egui shapes.
    pub primitives: Vec<egui::ClippedPrimitive>,
    /// Font atlas and image updates. Applied and cleared by the renderer.
    pub textures: egui::TexturesDelta,
    /// egui's pixels per point.
    pub pixels_per_point: f32,
}

impl fmt::Debug for UiFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiFrame")
            .field("primitives", &self.primitives.len())
            .finish_non_exhaustive()
    }
}

/// Renders terminal snapshots into a window surface or an offscreen texture.
pub struct Renderer {
    /// `None` when rendering offscreen (screenshots, golden tests).
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    adapter_info: wgpu::AdapterInfo,
    rects: RectPipeline,
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    font_family: String,
    font_size: f32,
    scale: f32,
    cell_w: f32,
    cell_h: f32,
    rows: Vec<RowCache>,
    /// Second text pass for labels drawn on top of overlay rectangles (the jump pill).
    ui_text: TextRenderer,
    pill_label: Buffer,
    /// Scrollback search bar text (`None` = hidden) and its laid-out label.
    search_text: Option<String>,
    search_label: Buffer,
    /// IME composition text drawn at the cursor (F1-36), and its laid-out label.
    preedit_text: Option<String>,
    preedit_label: Buffer,
    /// Visual bell flash in progress.
    flash: bool,
    /// Where the grid goes, `[x, y, w, h]` in physical pixels; `None` = the whole surface.
    area: Option<[f32; 4]>,
    /// egui renderer for the window chrome (ADR 0003), sharing this device and pass.
    egui: egui_wgpu::Renderer,
    /// Where the jump pill was drawn last frame, for hit testing: `[x, y, w, h]`.
    pill_rect: Option<[f32; 4]>,
}

/// Layout of one row, rebuilt only when its cells change.
struct RowCache {
    cells: Vec<SnapCell>,
    cursor_col: Option<usize>,
    buffer: Buffer,
}

impl fmt::Debug for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Renderer")
            .field("adapter", &self.adapter_info.name)
            .field("backend", &self.adapter_info.backend)
            .field("font", &self.font_family)
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// Creates a renderer for `window` (physical size `width` x `height`, DPI `scale`).
    pub fn new<W>(
        window: Arc<W>,
        width: u32,
        height: u32,
        scale: f32,
        font_size: f32,
    ) -> Result<Self>
    where
        W: HasWindowHandle + HasDisplayHandle + fmt::Debug + Send + Sync + 'static,
    {
        let instance = wgpu::Instance::new(with_preferred_backends(
            wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(Arc::clone(
                &window,
            ))),
        ));
        let surface = instance
            .create_surface(window)
            .context("cannot create GPU surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .context("no compatible GPU adapter")?;
        let adapter_info = adapter.get_info();
        log::info!("GPU: {} ({:?})", adapter_info.name, adapter_info.backend);

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hivetty"),
            ..Default::default()
        }))
        .context("cannot open GPU device")?;

        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or_else(|| anyhow!("surface not supported by this adapter"))?;
        // Colors are sRGB bytes; a non-sRGB target lets them pass through unchanged.
        let caps = surface.get_capabilities(&adapter);
        if let Some(format) = caps.formats.iter().copied().find(|f| !f.is_srgb()) {
            config.format = format;
        }
        config.present_mode = wgpu::PresentMode::Fifo;
        surface.configure(&device, &config);

        Ok(Self::finish(
            Some(surface),
            device,
            queue,
            config,
            adapter_info,
            scale,
            font_size,
        ))
    }

    /// Creates a renderer that draws into an offscreen texture of `width` x `height` pixels.
    pub fn new_headless(width: u32, height: u32, scale: f32, font_size: f32) -> Result<Self> {
        let instance = wgpu::Instance::new(with_preferred_backends(
            wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
        ));
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            ..Default::default()
        }))
        .context("no GPU adapter")?;
        let adapter_info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hivetty-headless"),
            ..Default::default()
        }))
        .context("cannot open GPU device")?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format: wgpu::TextureFormat::Rgba8Unorm,
            color_space: wgpu::SurfaceColorSpace::default(),
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: Vec::new(),
        };
        Ok(Self::finish(
            None,
            device,
            queue,
            config,
            adapter_info,
            scale,
            font_size,
        ))
    }

    fn finish(
        surface: Option<wgpu::Surface<'static>>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        adapter_info: wgpu::AdapterInfo,
        scale: f32,
        font_size: f32,
    ) -> Self {
        let rects = RectPipeline::new(&device, config.format);
        let egui = egui_wgpu::Renderer::new(
            &device,
            config.format,
            egui_wgpu::RendererOptions::default(),
        );
        let mut font_system = FontSystem::new();
        let font_family = pick_font(&font_system);
        log::info!("terminal font: {font_family}");
        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas =
            TextAtlas::with_color_mode(&device, &queue, &cache, config.format, ColorMode::Web);
        let text_renderer =
            TextRenderer::new(&mut atlas, &device, wgpu::MultisampleState::default(), None);
        let ui_text =
            TextRenderer::new(&mut atlas, &device, wgpu::MultisampleState::default(), None);

        let (cell_w, cell_h) = measure_cell(&mut font_system, &font_family, font_size * scale);
        let search_label = Buffer::new(&mut font_system, Metrics::new(font_size * scale, cell_h));
        let preedit_label = Buffer::new(&mut font_system, Metrics::new(font_size * scale, cell_h));
        let mut pill_label = Buffer::new(&mut font_system, Metrics::new(font_size * scale, cell_h));
        pill_label.set_size(Some(cell_w * 40.0), Some(cell_h));
        pill_label.set_text(
            JUMP_LABEL,
            &Attrs::new().family(Family::Name(&font_family)),
            Shaping::Advanced,
            None,
        );
        pill_label.shape_until_scroll(&mut font_system, false);
        Self {
            surface,
            device,
            queue,
            config,
            adapter_info,
            rects,
            font_system,
            swash_cache: SwashCache::new(),
            viewport,
            atlas,
            text_renderer,
            font_family,
            font_size,
            scale,
            cell_w,
            cell_h,
            rows: Vec::new(),
            ui_text,
            pill_label,
            pill_rect: None,
            search_text: None,
            search_label,
            preedit_text: None,
            preedit_label,
            flash: false,
            area: None,
            egui,
        }
    }

    /// Whether `(x, y)` (physical pixels) is on the "Jump to bottom" pill drawn last frame.
    pub fn hits_jump_to_bottom(&self, x: f32, y: f32) -> bool {
        self.pill_rect
            .is_some_and(|[px, py, w, h]| x >= px && x <= px + w && y >= py && y <= py + h)
    }

    /// GPU name and backend, for `--doctor` and logs.
    pub fn adapter_description(&self) -> String {
        format!(
            "{} ({:?}, {:?})",
            self.adapter_info.name, self.adapter_info.backend, self.adapter_info.device_type
        )
    }

    /// Reacts to a window resize (physical pixels).
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &self.config);
        }
    }

    /// Reacts to a DPI change.
    pub fn set_scale(&mut self, scale: f32) {
        if (scale - self.scale).abs() > f32::EPSILON {
            self.scale = scale;
            self.relayout_font();
        }
    }

    /// Changes the font size (points before DPI scaling).
    pub fn set_font_size(&mut self, size: f32) {
        self.font_size = size.clamp(6.0, 72.0);
        self.relayout_font();
    }

    /// Uses `family` for the terminal text if it is installed; `None` restores the automatic
    /// choice. Returns false (and keeps the current font) if the family is not installed.
    pub fn set_font_family(&mut self, family: Option<&str>) -> bool {
        let wanted = match family {
            None => pick_font(&self.font_system),
            Some(name) if is_installed(&self.font_system, name) => name.to_owned(),
            Some(_) => return false,
        };
        if wanted != self.font_family {
            self.font_family = wanted;
            self.relayout_font();
            let attrs = Attrs::new().family(Family::Name(&self.font_family));
            self.pill_label
                .set_text(JUMP_LABEL, &attrs, Shaping::Advanced, None);
            self.pill_label
                .shape_until_scroll(&mut self.font_system, false);
        }
        true
    }

    /// Current font size.
    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    fn relayout_font(&mut self) {
        let (w, h) = measure_cell(
            &mut self.font_system,
            &self.font_family,
            self.font_size * self.scale,
        );
        self.cell_w = w;
        self.cell_h = h;
        self.rows.clear();
        self.pill_label
            .set_metrics(Metrics::new(self.font_size * self.scale, h));
        self.pill_label.set_size(Some(w * 40.0), Some(h));
        self.pill_label
            .shape_until_scroll(&mut self.font_system, false);
    }

    /// Cell size in physical pixels.
    pub fn cell_size(&self) -> (f32, f32) {
        (self.cell_w, self.cell_h)
    }

    /// Pixel size needed to show exactly `cols` x `rows` cells (plus padding).
    pub fn size_for_grid(&self, cols: u16, rows: u16) -> (u32, u32) {
        let pad = (PADDING * self.scale).round() * 2.0;
        (
            (f32::from(cols) * self.cell_w + pad).ceil() as u32,
            (f32::from(rows) * self.cell_h + pad).ceil() as u32,
        )
    }

    /// Restricts the grid to `[x, y, w, h]` (physical pixels), e.g. the space egui leaves next
    /// to the sidebar. `None` uses the whole surface.
    pub fn set_grid_area(&mut self, area: Option<[f32; 4]>) {
        self.area = area;
    }

    fn area(&self) -> [f32; 4] {
        self.area.unwrap_or([
            0.0,
            0.0,
            self.config.width as f32,
            self.config.height as f32,
        ])
    }

    /// Top-left corner of the first cell.
    fn origin(&self) -> (f32, f32) {
        let pad = (PADDING * self.scale).round();
        let [x, y, ..] = self.area();
        ((x + pad).round(), (y + pad).round())
    }

    /// Position and size of a cell in physical pixels: `(x, y, w, h)`.
    pub fn cell_rect(&self, row: usize, col: usize) -> (f32, f32, f32, f32) {
        let (ox, oy) = self.origin();
        (
            ox + col as f32 * self.cell_w,
            oy + row as f32 * self.cell_h,
            self.cell_w,
            self.cell_h,
        )
    }

    /// The cell under a point in physical pixels: `(row, col, right_half)`, clamped to the grid.
    pub fn cell_at(&self, x: f32, y: f32) -> (usize, usize, bool) {
        let (ox, oy) = self.origin();
        let (cols, rows) = self.grid_size();
        let fx = ((x - ox) / self.cell_w).max(0.0);
        let fy = ((y - oy) / self.cell_h).max(0.0);
        let col = (fx.floor() as usize).min(usize::from(cols).saturating_sub(1));
        let row = (fy.floor() as usize).min(usize::from(rows).saturating_sub(1));
        (row, col, fx.fract() >= 0.5)
    }

    /// How many columns and rows fit in the window.
    pub fn grid_size(&self) -> (u16, u16) {
        let pad = PADDING * self.scale * 2.0;
        let [_, _, area_w, area_h] = self.area();
        let cols = ((area_w - pad) / self.cell_w).floor().max(1.0);
        let rows = ((area_h - pad) / self.cell_h).floor().max(1.0);
        (cols as u16, rows as u16)
    }

    /// Draws one frame to the window.
    pub fn render(&mut self, snap: &Snapshot) -> Result<()> {
        self.render_with_ui(snap, None)
    }

    /// Draws one frame to the window: the terminal grid, then the egui chrome on top.
    pub fn render_with_ui(&mut self, snap: &Snapshot, ui: Option<&mut UiFrame>) -> Result<()> {
        let Some(surface) = &self.surface else {
            return Err(anyhow!("headless renderer: use render_to_rgba"));
        };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&self.device, &self.config);
                return Ok(());
            }
            _ => return Ok(()), // timeout or occluded: skip this frame
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut ui = ui;
        let (extra, encoder) = self.encode(snap, &view, ui.as_deref_mut())?;
        self.queue
            .submit(extra.into_iter().chain([encoder.finish()]));
        self.queue.present(frame);
        self.finish_ui(ui);
        self.atlas.trim();
        Ok(())
    }

    /// Draws one frame offscreen and returns it as tightly packed RGBA8 rows.
    pub fn render_to_rgba(&mut self, snap: &Snapshot) -> Result<(u32, u32, Vec<u8>)> {
        self.render_to_rgba_with_ui(snap, None)
    }

    /// Like [`Renderer::render_to_rgba`], with egui chrome on top.
    pub fn render_to_rgba_with_ui(
        &mut self,
        snap: &Snapshot,
        ui: Option<&mut UiFrame>,
    ) -> Result<(u32, u32, Vec<u8>)> {
        let mut ui = ui;
        let (width, height) = (self.config.width, self.config.height);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (extra, mut encoder) = self.encode(snap, &view, ui.as_deref_mut())?;
        let padded_row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue
            .submit(extra.into_iter().chain([encoder.finish()]));
        self.finish_ui(ui);
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| anyhow!("GPU poll failed: {e}"))?;
        rx.recv()?.map_err(|e| anyhow!("readback failed: {e}"))?;
        let data = slice
            .get_mapped_range()
            .map_err(|e| anyhow!("cannot read mapped buffer: {e:?}"))?;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in data.chunks(padded_row as usize).take(height as usize) {
            rgba.extend_from_slice(&row[..(width * 4) as usize]);
        }
        drop(data);
        readback.unmap();
        Ok((width, height, rgba))
    }

    /// Records the draw commands for one frame into `view`.
    fn encode(
        &mut self,
        snap: &Snapshot,
        view: &wgpu::TextureView,
        ui: Option<&mut UiFrame>,
    ) -> Result<(Vec<wgpu::CommandBuffer>, wgpu::CommandEncoder)> {
        let (ox, oy) = self.origin();
        let (back, mut over) = self.build_rects(snap, (ox, oy));
        self.update_rows(snap);
        let pill = self.layout_pill(snap, &mut over);
        let search = self.layout_search(&mut over);
        let preedit = self.layout_preedit(snap, &mut over);
        if self.flash {
            let (w, h) = (self.config.width as f32, self.config.height as f32);
            over.push(Rect::with_alpha(0.0, 0.0, w, h, snap.foreground, 0.12));
        }

        let width = self.config.width;
        let height = self.config.height;
        self.viewport
            .update(&self.queue, Resolution { width, height });
        let areas = self.rows.iter().enumerate().map(|(row, cache)| TextArea {
            buffer: &cache.buffer,
            left: ox,
            top: oy + row as f32 * self.cell_h,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            },
            default_color: to_text_color(snap.foreground),
            custom_glyphs: &[],
        });
        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut self.swash_cache,
            )
            .map_err(|e| anyhow!("text prepare failed: {e}"))?;
        let bounds = TextBounds {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        };
        let search_area = search.map(|(x, y)| TextArea {
            buffer: &self.search_label,
            left: x,
            top: y,
            scale: 1.0,
            bounds,
            default_color: to_text_color(snap.foreground),
            custom_glyphs: &[],
        });
        let pill_area = pill.map(|(x, y)| TextArea {
            buffer: &self.pill_label,
            left: x,
            top: y,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            },
            default_color: to_text_color(snap.foreground),
            custom_glyphs: &[],
        });
        let preedit_area = preedit.map(|(x, y)| TextArea {
            buffer: &self.preedit_label,
            left: x,
            top: y,
            scale: 1.0,
            bounds,
            default_color: to_text_color(snap.foreground),
            custom_glyphs: &[],
        });
        self.ui_text
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                pill_area.into_iter().chain(search_area).chain(preedit_area),
                &mut self.swash_cache,
            )
            .map_err(|e| anyhow!("label prepare failed: {e}"))?;

        self.rects.upload(
            &self.device,
            &self.queue,
            [width as f32, height as f32],
            &back,
            &over,
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: ui.as_ref().map_or(1.0, |u| u.pixels_per_point),
        };
        let mut extra = Vec::new();
        if let Some(ui) = ui.as_ref() {
            for (id, deltas) in &ui.textures.set {
                for delta in deltas {
                    self.egui
                        .update_texture(&self.device, &self.queue, *id, delta);
                }
            }
            extra = self.egui.update_buffers(
                &self.device,
                &self.queue,
                &mut encoder,
                &ui.primitives,
                &screen,
            );
        }
        {
            let bg = snap.background;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terminal"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(bg.r) / 255.0,
                            g: f64::from(bg.g) / 255.0,
                            b: f64::from(bg.b) / 255.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.rects.draw_background(&mut pass);
            self.text_renderer
                .render(&self.atlas, &self.viewport, &mut pass)
                .map_err(|e| anyhow!("text render failed: {e}"))?;
            self.rects.draw_overlay(&mut pass);
            self.ui_text
                .render(&self.atlas, &self.viewport, &mut pass)
                .map_err(|e| anyhow!("label render failed: {e}"))?;
            if let Some(ui) = ui.as_ref() {
                let mut pass = pass.forget_lifetime();
                self.egui.render(&mut pass, &ui.primitives, &screen);
            }
        }
        Ok((extra, encoder))
    }

    /// Frees egui textures that are no longer used and marks the deltas as handled.
    fn finish_ui(&mut self, ui: Option<&mut UiFrame>) {
        if let Some(ui) = ui {
            for id in &ui.textures.free {
                self.egui.free_texture(id);
            }
            ui.textures.clear();
        }
    }

    /// Adds the "Jump to bottom" pill when the view is scrolled up. Returns where its label goes.
    /// Turns the visual bell flash on or off.
    pub fn set_flash(&mut self, on: bool) {
        self.flash = on;
    }

    /// Shows the search bar with `text` (e.g. `Find: foo`), or hides it with `None`.
    pub fn set_search_bar(&mut self, text: Option<&str>) {
        if self.search_text.as_deref() == text {
            return;
        }
        self.search_text = text.map(str::to_owned);
        if let Some(text) = text {
            let attrs = Attrs::new().family(Family::Name(&self.font_family));
            self.search_label
                .set_metrics(Metrics::new(self.font_size * self.scale, self.cell_h));
            self.search_label
                .set_size(Some(self.cell_w * 200.0), Some(self.cell_h));
            self.search_label
                .set_text(text, &attrs, Shaping::Advanced, None);
            self.search_label
                .shape_until_scroll(&mut self.font_system, false);
        }
    }

    /// Shows the IME composition text at the cursor, or hides it with `None` or `""`.
    pub fn set_preedit(&mut self, text: Option<&str>) {
        let text = text.filter(|t| !t.is_empty());
        if self.preedit_text.as_deref() == text {
            return;
        }
        self.preedit_text = text.map(str::to_owned);
        if let Some(text) = text {
            let attrs = Attrs::new().family(Family::Name(&self.font_family));
            self.preedit_label
                .set_metrics(Metrics::new(self.font_size * self.scale, self.cell_h));
            self.preedit_label
                .set_size(Some(self.cell_w * 400.0), Some(self.cell_h));
            self.preedit_label
                .set_text(text, &attrs, Shaping::Advanced, None);
            self.preedit_label
                .shape_until_scroll(&mut self.font_system, false);
        }
    }

    /// Covers the cells under the cursor with the composition text, underlined like other
    /// terminals do. Returns where its label goes.
    fn layout_preedit(&mut self, snap: &Snapshot, over: &mut Vec<Rect>) -> Option<(f32, f32)> {
        self.preedit_text.as_ref()?;
        let cursor = snap.cursor?;
        let (x, y, _, ch) = self.cell_rect(cursor.row, cursor.col);
        let width = self
            .preedit_label
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0f32, f32::max)
            .max(self.cell_w)
            .ceil();
        // Keep it inside the grid area: shift left when it would overflow the right edge.
        let [ax, _, aw, _] = self.area();
        let x = x.min(ax + aw - width).max(ax).round();
        let thickness = self.scale.round().max(1.0);
        over.push(Rect::new(x, y, width, ch, snap.background));
        over.push(Rect::new(
            x,
            y + ch - thickness,
            width,
            thickness,
            snap.foreground,
        ));
        Some((x, y))
    }

    /// Adds the search bar (top right). Returns where its label goes.
    fn layout_search(&mut self, over: &mut Vec<Rect>) -> Option<(f32, f32)> {
        let text = self.search_text.as_deref()?;
        let (cw, ch) = (self.cell_w, self.cell_h);
        let chars = text.chars().count().max(24) as f32;
        let (w, h) = ((chars * cw + cw * 2.0).round(), (ch * 1.5).round());
        let margin = (PADDING * self.scale).round();
        let [ax, ay, aw, _] = self.area();
        let x = (ax + aw - w - margin).max(ax).round();
        let y = ay + margin;
        let border = self.scale.round().max(1.0);
        over.push(Rect::new(
            x - border,
            y - border,
            w + border * 2.0,
            h + border * 2.0,
            PILL_BORDER,
        ));
        over.push(Rect::new(x, y, w, h, PILL_BG));
        Some((x + cw, y + ((h - ch) / 2.0).round()))
    }

    fn layout_pill(&mut self, snap: &Snapshot, over: &mut Vec<Rect>) -> Option<(f32, f32)> {
        if snap.display_offset == 0 {
            self.pill_rect = None;
            return None;
        }
        let (cw, ch) = (self.cell_w, self.cell_h);
        let label_w = JUMP_LABEL.chars().count() as f32 * cw;
        let (w, h) = ((label_w + cw * 2.0).round(), (ch * 1.5).round());
        let [ax, ay, aw, ah] = self.area();
        let x = (ax + (aw - w) / 2.0).round();
        let y = (ay + ah - h - ch).round();
        let border = self.scale.round().max(1.0);
        over.push(Rect::new(
            x - border,
            y - border,
            w + border * 2.0,
            h + border * 2.0,
            PILL_BORDER,
        ));
        over.push(Rect::new(x, y, w, h, PILL_BG));
        self.pill_rect = Some([x, y, w, h]);
        Some((x + cw, y + ((h - ch) / 2.0).round()))
    }

    /// Background rectangles (merged runs) plus overlay rectangles (underlines, thin cursors).
    fn build_rects(&self, snap: &Snapshot, (ox, oy): (f32, f32)) -> (Vec<Rect>, Vec<Rect>) {
        let (cw, ch) = (self.cell_w, self.cell_h);
        let mut back = Vec::new();
        let mut over = Vec::new();
        for row in 0..snap.rows {
            let y = oy + row as f32 * ch;
            let mut col = 0;
            while col < snap.cols {
                let bg = snap.cell(row, col).bg;
                let start = col;
                while col < snap.cols && snap.cell(row, col).bg == bg {
                    col += 1;
                }
                if bg != snap.background {
                    back.push(Rect::new(
                        ox + start as f32 * cw,
                        y,
                        (col - start) as f32 * cw,
                        ch,
                        bg,
                    ));
                }
            }
            for c in 0..snap.cols {
                let cell = snap.cell(row, c);
                if boxdraw::is_builtin(cell.ch) {
                    let fg = match snap.cursor {
                        Some(cur)
                            if cur.row == row && cur.col == c && cur.shape == CursorKind::Block =>
                        {
                            snap.background
                        }
                        _ => cell.fg,
                    };
                    over.extend(boxdraw::rects_for(
                        cell.ch,
                        ox + c as f32 * cw,
                        y,
                        cw,
                        ch,
                        fg,
                        self.scale,
                    ));
                }
                if cell.underline {
                    let thickness = (self.scale).max(1.0).round();
                    over.push(Rect::new(
                        ox + c as f32 * cw,
                        y + ch - thickness * 2.0,
                        cw,
                        thickness,
                        cell.fg,
                    ));
                }
            }
        }
        if let Some(cursor) = snap.cursor {
            let x = ox + cursor.col as f32 * cw;
            let y = oy + cursor.row as f32 * ch;
            let color = snap.foreground;
            let thin = (2.0 * self.scale).round();
            match cursor.shape {
                CursorKind::Block => back.push(Rect::new(x, y, cw, ch, color)),
                CursorKind::Beam => over.push(Rect::new(x, y, thin, ch, color)),
                CursorKind::Underline => over.push(Rect::new(x, y + ch - thin, cw, thin, color)),
            }
        }
        (back, over)
    }

    /// Re-lays out the rows whose cells (or cursor) changed since the last frame.
    fn update_rows(&mut self, snap: &Snapshot) {
        let metrics = Metrics::new(self.font_size * self.scale, self.cell_h);
        self.rows.truncate(snap.rows);
        while self.rows.len() < snap.rows {
            let mut buffer = Buffer::new(&mut self.font_system, metrics);
            buffer.set_monospace_width(Some(self.cell_w));
            self.rows.push(RowCache {
                cells: Vec::new(),
                cursor_col: None,
                buffer,
            });
        }
        let width = snap.cols as f32 * self.cell_w + self.cell_w;
        for (row, cache) in self.rows.iter_mut().enumerate() {
            let cells = &snap.cells[row * snap.cols..(row + 1) * snap.cols];
            let cursor_col = snap
                .cursor
                .filter(|c| c.row == row && c.shape == CursorKind::Block)
                .map(|c| c.col);
            if cache.cells == cells && cache.cursor_col == cursor_col {
                continue;
            }
            cache.cells = cells.to_vec();
            cache.cursor_col = cursor_col;

            let spans = row_spans(cells, cursor_col, snap.background);
            let family = Family::Name(&self.font_family);
            let default_attrs = Attrs::new().family(family);
            cache.buffer.set_metrics(metrics);
            cache.buffer.set_monospace_width(Some(self.cell_w));
            cache.buffer.set_size(Some(width), Some(self.cell_h));
            cache.buffer.set_rich_text(
                spans.iter().map(|span| {
                    let mut attrs = Attrs::new().family(family).color(to_text_color(span.fg));
                    if span.bold {
                        attrs = attrs.weight(Weight::BOLD);
                    }
                    if span.italic {
                        attrs = attrs.style(Style::Italic);
                    }
                    (span.text.as_str(), attrs)
                }),
                &default_attrs,
                Shaping::Advanced,
                None,
            );
            cache
                .buffer
                .shape_until_scroll(&mut self.font_system, false);
        }
    }
}

/// A run of cells with the same text style.
#[derive(Debug, PartialEq)]
struct Span {
    text: String,
    fg: Rgb8,
    bold: bool,
    italic: bool,
}

/// Groups a row into styled spans. The cell under a block cursor is drawn in the background
/// color so it stays readable on top of the cursor.
fn row_spans(cells: &[SnapCell], cursor_col: Option<usize>, background: Rgb8) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    for (col, cell) in cells.iter().enumerate() {
        if cell.wide_spacer {
            continue;
        }
        // Box drawing and blocks are drawn as rectangles (boxdraw.rs), not by the font.
        let ch = if boxdraw::is_builtin(cell.ch) {
            ' '
        } else {
            cell.ch
        };
        let fg = if cursor_col == Some(col) {
            background
        } else {
            cell.fg
        };
        match spans.last_mut() {
            Some(last) if last.fg == fg && last.bold == cell.bold && last.italic == cell.italic => {
                last.text.push(ch)
            }
            _ => spans.push(Span {
                text: ch.to_string(),
                fg,
                bold: cell.bold,
                italic: cell.italic,
            }),
        }
    }
    spans
}

/// Uses DirectX 12 on Windows (docs/platforms.md) unless `WGPU_BACKEND` overrides it. This also
/// avoids third-party Vulkan layers (overlays, capture hooks) that some machines install.
fn with_preferred_backends(mut desc: wgpu::InstanceDescriptor) -> wgpu::InstanceDescriptor {
    if cfg!(windows) && std::env::var_os("WGPU_BACKEND").is_none() {
        desc.backends = wgpu::Backends::DX12;
    }
    desc
}

fn to_text_color(c: Rgb8) -> TextColor {
    TextColor::rgb(c.r, c.g, c.b)
}

fn is_installed(fs: &FontSystem, family: &str) -> bool {
    fs.db().faces().any(|face| {
        face.families
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(family))
    })
}

fn pick_font(fs: &FontSystem) -> String {
    let installed: Vec<String> = fs
        .db()
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
        .collect();
    PREFERRED_FONTS
        .iter()
        .find(|want| installed.iter().any(|have| have.eq_ignore_ascii_case(want)))
        .map_or_else(|| "monospace".to_owned(), |s| (*s).to_owned())
}

/// Measures the advance and line height of the chosen font, rounded to whole pixels.
fn measure_cell(fs: &mut FontSystem, family: &str, font_px: f32) -> (f32, f32) {
    let line_h = (font_px * 1.25).round();
    let mut buffer = Buffer::new(fs, Metrics::new(font_px, line_h));
    let sample = "MMMMMMMMMM";
    buffer.set_text(
        sample,
        &Attrs::new().family(Family::Name(family)),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(fs, false);
    let width = buffer
        .layout_runs()
        .next()
        .map_or(font_px * 6.0, |run| run.line_w);
    // Keep the exact (fractional) advance: rounding it makes text drift away from the cell
    // backgrounds by a fraction of a pixel per column (e.g. Menlo is 8.43 px at 14 px).
    let cell_w = (width / sample.len() as f32).max(1.0);
    (cell_w, line_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(ch: char, fg: Rgb8) -> SnapCell {
        SnapCell {
            ch,
            fg,
            bg: Rgb8::default(),
            bold: false,
            italic: false,
            underline: false,
            wide_spacer: false,
            selected: false,
            link: 0,
        }
    }

    #[test]
    fn spans_merge_same_style_and_skip_wide_spacers() {
        let red = Rgb8::hex(0xFF0000);
        let white = Rgb8::hex(0xFFFFFF);
        let mut spacer = cell(' ', red);
        spacer.wide_spacer = true;
        let cells = [
            cell('a', red),
            cell('b', red),
            cell('日', white),
            spacer,
            cell('c', white),
        ];
        let spans = row_spans(&cells, None, Rgb8::default());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text, "ab");
        assert_eq!(spans[1].text, "日c");
    }

    #[test]
    fn block_cursor_inverts_the_glyph_color() {
        let white = Rgb8::hex(0xFFFFFF);
        let bg = Rgb8::hex(0x000000);
        let spans = row_spans(&[cell('x', white), cell('y', white)], Some(1), bg);
        assert_eq!(
            spans[1],
            Span {
                text: "y".into(),
                fg: bg,
                bold: false,
                italic: false
            }
        );
    }
}
