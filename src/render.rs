//! PiP window rendering: D3D11 swapchain, VideoProcessor for GPU frames,
//! Direct2D for JPEG images / software frames, and a DirectWrite overlay.

use crate::decoder::{nv12_to_bgra, Frame};
use crate::view::Zoom;
use std::mem::ManuallyDrop;
use windows::core::{w, Interface, Result, HSTRING};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::{HMODULE, HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

pub struct Overlay {
    pub text: String,
    pub live: bool,
    /// Close button: 0 = hidden, 1 = shown (mouse over the window), 2 = highlighted (mouse over the button),
    /// 3 = shown with the sound button highlighted (mouse over that one).
    pub close: u8,
    /// Sound button: 0 = no camera audio (no button), 1 = sound on, 2 = muted.
    pub sound: u8,
    /// The part of the picture that is shown (Ctrl + mouse wheel).
    pub zoom: Zoom,
}

/// Close button in the top-right corner of the window, in client pixels (left, top, right, bottom).
/// Used both for drawing and for hit-testing the mouse.
pub fn close_button_rect(client_width: i32, dpi: u32) -> (i32, i32, i32, i32) {
    let scale = dpi.max(96) as f32 / 96.0;
    let size = (24.0 * scale).round() as i32;
    let margin = (8.0 * scale).round() as i32;
    (client_width - margin - size, margin, client_width - margin, margin + size)
}

/// Sound (mute) button, left of the close button and of the same size.
pub fn sound_button_rect(client_width: i32, dpi: u32) -> (i32, i32, i32, i32) {
    let (l, t, r, b) = close_button_rect(client_width, dpi);
    let gap = (6.0 * dpi.max(96) as f32 / 96.0).round() as i32;
    (l - gap - (r - l), t, l - gap, b)
}

enum Last {
    None,
    Image,
    Gpu(Frame),
}

struct Vp {
    enumr: ID3D11VideoProcessorEnumerator,
    proc: ID3D11VideoProcessor,
    output: ID3D11VideoProcessorOutputView,
    in_size: (u32, u32),
    inputs: Vec<(usize, u32, ID3D11VideoProcessorInputView)>,
}

pub struct Renderer {
    hwnd: HWND,
    pub device: ID3D11Device,
    pub video_ok: bool,
    swapchain: IDXGISwapChain1,
    video_device: Option<ID3D11VideoDevice>,
    video_context: Option<ID3D11VideoContext>,
    d2d: ID2D1DeviceContext,
    d2f: ID2D1Factory1,
    target: Option<ID2D1Bitmap1>,
    dwrite: IDWriteFactory,
    font: IDWriteTextFormat,
    font_px: f32,
    white: ID2D1SolidColorBrush,
    shade: ID2D1SolidColorBrush,
    red: ID2D1SolidColorBrush,
    grey: ID2D1SolidColorBrush,
    /// Lightens a button under the mouse.
    light: ID2D1SolidColorBrush,
    wic: IWICImagingFactory,
    size: (u32, u32),
    vp: Option<Vp>,
    image: Option<ID2D1Bitmap1>,
    image_size: (u32, u32),
    bgra: Vec<u8>,
    last: Last,
}

fn color(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

/// The zoomed part of a picture of `size` pixels: (left, top, right, bottom).
pub fn part(size: (u32, u32), zoom: Zoom) -> (f32, f32, f32, f32) {
    let (w, h) = (size.0 as f32, size.1 as f32);
    (zoom.x * w, zoom.y * h, (zoom.x + zoom.size) * w, (zoom.y + zoom.size) * h)
}

/// Rectangle with the aspect ratio of `src` fitted inside `dst` (letterbox).
pub fn fit(src: (u32, u32), dst: (u32, u32)) -> (f32, f32, f32, f32) {
    let (sw, sh) = (src.0.max(1) as f32, src.1.max(1) as f32);
    let (dw, dh) = (dst.0 as f32, dst.1 as f32);
    let scale = (dw / sw).min(dh / sh);
    let (w, h) = (sw * scale, sh * scale);
    let (x, y) = ((dw - w) / 2.0, (dh - h) / 2.0);
    (x, y, x + w, y + h)
}

fn client_size(hwnd: HWND) -> (u32, u32) {
    let mut rc = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
    }
    ((rc.right - rc.left).max(1) as u32, (rc.bottom - rc.top).max(1) as u32)
}

/// Quick preview via GDI before D3D11 is ready (a cold start of the GPU driver
/// takes ~150 ms). Decodes the JPEG via WIC and draws it straight into the window.
pub fn gdi_paint_jpeg(hwnd: HWND, jpeg: &[u8]) -> Result<()> {
    use windows::Win32::Graphics::Gdi::*;
    unsafe {
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = wic.CreateStream()?;
        stream.InitializeFromMemory(jpeg)?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)?;
        let frame = dec.GetFrame(0)?;
        let conv = wic.CreateFormatConverter()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppBGR, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut)?;
        let (mut w, mut h) = (0u32, 0u32);
        conv.GetSize(&mut w, &mut h)?;
        let mut px = vec![0u8; (w * h * 4) as usize];
        conv.CopyPixels(std::ptr::null(), w * 4, &mut px)?;
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let size = client_size(hwnd);
        let (l, t, r, b) = fit((w, h), size);
        let hdc = GetDC(hwnd);
        SetStretchBltMode(hdc, HALFTONE);
        StretchDIBits(
            hdc,
            l as i32,
            t as i32,
            (r - l) as i32,
            (b - t) as i32,
            0,
            0,
            w as i32,
            h as i32,
            Some(px.as_ptr() as *const _),
            &bmi,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
        ReleaseDC(hwnd, hdc);
    }
    Ok(())
}

/// Graphics device; without a usable graphics card (some virtual machines and remote
/// sessions) the WARP software rasterizer is used instead.
pub fn create_device(flags: D3D11_CREATE_DEVICE_FLAG) -> Result<ID3D11Device> {
    static WARP_LOGGED: std::sync::Once = std::sync::Once::new();
    let levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_10_0];
    let mut last: Result<ID3D11Device> = Err(windows::Win32::Foundation::E_FAIL.into());
    for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
        let mut device = None;
        let r =
            unsafe { D3D11CreateDevice(None, driver, HMODULE::default(), flags, Some(&levels), D3D11_SDK_VERSION, Some(&mut device), None, None) };
        match (r, device) {
            (Ok(()), Some(d)) => {
                if driver == D3D_DRIVER_TYPE_WARP {
                    WARP_LOGGED.call_once(|| crate::log!("No usable graphics card - drawing with WARP (software)"));
                }
                return Ok(d);
            }
            (Err(e), _) => last = Err(e),
            (Ok(()), None) => {}
        }
    }
    last
}

impl Renderer {
    pub fn new(hwnd: HWND) -> Result<Renderer> {
        unsafe {
            let (device, video_ok) = match create_device(D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT) {
                Ok(d) => (d, true),
                Err(_) => (create_device(D3D11_CREATE_DEVICE_BGRA_SUPPORT)?, false),
            };
            if let Ok(mt) = device.cast::<ID3D11Multithread>() {
                let _ = mt.SetMultithreadProtected(true);
            }
            let dxgi_device: IDXGIDevice = device.cast()?;
            let factory: IDXGIFactory2 = dxgi_device.GetAdapter()?.GetParent()?;
            let size = client_size(hwnd);
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: size.0,
                Height: size.1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                ..Default::default()
            };
            let swapchain = factory.CreateSwapChainForHwnd(&device, hwnd, &desc, None, None)?;
            let _ = factory.MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER | DXGI_MWA_NO_WINDOW_CHANGES);

            let d2f: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d = d2f.CreateDevice(&dxgi_device)?.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            d2d.SetDpi(96.0, 96.0);
            let brush = |c: D2D1_COLOR_F| d2d.CreateSolidColorBrush(&c, None);
            let white = brush(color(1.0, 1.0, 1.0, 1.0))?;
            let shade = brush(color(0.0, 0.0, 0.0, 0.55))?;
            let red = brush(color(0.94, 0.27, 0.27, 1.0))?;
            let grey = brush(color(0.75, 0.75, 0.75, 1.0))?;
            let light = brush(color(1.0, 1.0, 1.0, 0.3))?;

            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
            let font_px = (13.0 * scale).round();
            let font = dwrite.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font_px,
                w!("en-us"),
            )?;
            let _ = font.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;

            let (video_device, video_context) = if video_ok {
                let ctx = device.GetImmediateContext()?;
                (device.cast::<ID3D11VideoDevice>().ok(), ctx.cast::<ID3D11VideoContext>().ok())
            } else {
                (None, None)
            };
            let video_ok = video_ok && video_device.is_some() && video_context.is_some();
            let mut r = Renderer {
                hwnd,
                device,
                video_ok,
                swapchain,
                video_device,
                video_context,
                d2d,
                d2f,
                target: None,
                dwrite,
                font,
                font_px,
                white,
                shade,
                red,
                grey,
                light,
                wic,
                size,
                vp: None,
                image: None,
                image_size: (0, 0),
                bgra: Vec::new(),
                last: Last::None,
            };
            r.create_target()?;
            Ok(r)
        }
    }

    unsafe fn create_target(&mut self) -> Result<()> {
        let surface: IDXGISurface = self.swapchain.GetBuffer(0)?;
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            colorContext: ManuallyDrop::new(None),
        };
        let target = self.d2d.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
        self.d2d.SetTarget(&target);
        self.target = Some(target);
        Ok(())
    }

    /// Resizes the buffers to the window size. `true` if the size changed.
    pub fn check_resize(&mut self) -> Result<bool> {
        let size = client_size(self.hwnd);
        if size == self.size {
            return Ok(false);
        }
        unsafe {
            self.d2d.SetTarget(None);
            self.target = None;
            self.vp = None;
            self.swapchain.ResizeBuffers(0, size.0, size.1, DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG(0))?;
            self.size = size;
            self.create_target()?;
        }
        Ok(true)
    }

    pub fn show_jpeg(&mut self, jpeg: &[u8], ov: &Overlay) -> Result<(u32, u32)> {
        unsafe {
            let stream = self.wic.CreateStream()?;
            stream.InitializeFromMemory(jpeg)?;
            let dec = self.wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)?;
            let frame = dec.GetFrame(0)?;
            let conv = self.wic.CreateFormatConverter()?;
            conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut)?;
            let (mut w, mut h) = (0u32, 0u32);
            conv.GetSize(&mut w, &mut h)?;
            self.image = Some(self.d2d.CreateBitmapFromWicBitmap(&conv, None)?);
            self.image_size = (w, h);
        }
        self.last = Last::Image;
        self.redraw(ov)?;
        Ok(self.image_size)
    }

    /// Presents a decoded frame. An error on a GPU frame means the caller must
    /// fall back to software decoding.
    pub fn show_frame(&mut self, f: Frame, ov: &Overlay) -> Result<(u32, u32)> {
        match f {
            Frame::Gpu { width, height, .. } => {
                if !self.video_ok {
                    return Err(windows::Win32::Foundation::E_NOTIMPL.into());
                }
                self.last = Last::Gpu(f);
                self.redraw(ov)?;
                Ok((width, height))
            }
            Frame::Cpu { data, stride, width, height, alloc_height } => {
                let mut bgra = std::mem::take(&mut self.bgra);
                nv12_to_bgra(&data, stride, width as usize, height as usize, alloc_height as usize, &mut bgra);
                let r = self.set_bgra(&bgra, width, height);
                self.bgra = bgra;
                r?;
                self.last = Last::Image;
                self.redraw(ov)?;
                Ok((width, height))
            }
        }
    }

    fn set_bgra(&mut self, bgra: &[u8], w: u32, h: u32) -> Result<()> {
        unsafe {
            if let (Some(img), true) = (&self.image, self.image_size == (w, h)) {
                return img.CopyFromMemory(None, bgra.as_ptr() as _, w * 4);
            }
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
                colorContext: ManuallyDrop::new(None),
            };
            self.image = Some(self.d2d.CreateBitmap(D2D_SIZE_U { width: w, height: h }, Some(bgra.as_ptr() as _), w * 4, &props)?);
            self.image_size = (w, h);
        }
        Ok(())
    }

    pub fn redraw(&mut self, ov: &Overlay) -> Result<()> {
        self.check_resize()?;
        unsafe {
            if let Last::Gpu(Frame::Gpu { tex, index, width, height, .. }) = &self.last {
                let (tex, index, crop) = (tex.clone(), *index, (*width, *height));
                self.blit(&tex, index, crop, ov.zoom)?;
                self.d2d.BeginDraw();
            } else {
                self.d2d.BeginDraw();
                self.d2d.Clear(Some(&color(0.0, 0.0, 0.0, 1.0)));
                if let (Last::Image, Some(img)) = (&self.last, &self.image) {
                    let (sl, st, sr, sb) = part(self.image_size, ov.zoom);
                    let (l, t, r, b) = fit(((sr - sl).round() as u32, (sb - st).round() as u32), self.size);
                    self.d2d.DrawBitmap(
                        img,
                        Some(&D2D_RECT_F { left: l, top: t, right: r, bottom: b }),
                        1.0,
                        D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                        Some(&D2D_RECT_F { left: sl, top: st, right: sr, bottom: sb }),
                        None,
                    );
                }
            }
            self.draw_overlay(ov);
            self.d2d.EndDraw(None, None)?;
            self.swapchain.Present(1, DXGI_PRESENT(0)).ok()?;
        }
        Ok(())
    }

    unsafe fn draw_overlay(&self, ov: &Overlay) {
        self.d2d.SetTransform(&Matrix3x2::identity());
        let dpi = GetDpiForWindow(self.hwnd);
        let button = (ov.close > 0).then(|| close_button_rect(self.size.0 as i32, dpi));
        // The sound button comes with the close button, and stays while muted so that it is clear why it is quiet.
        let sound = (ov.sound > 0 && (ov.close > 0 || ov.sound == 2)).then(|| sound_button_rect(self.size.0 as i32, dpi));
        if !ov.text.is_empty() {
            // The caption must not run under the buttons.
            self.draw_caption(ov, sound.or(button).map_or(0.0, |b| (self.size.0 as i32 - b.0) as f32));
        }
        if let Some((l, t, r, b)) = button {
            let (cx, cy, rad) = ((l + r) as f32 / 2.0, (t + b) as f32 / 2.0, (r - l) as f32 / 2.0);
            let circle = D2D1_ELLIPSE { point: D2D_POINT_2F { x: cx, y: cy }, radiusX: rad, radiusY: rad };
            self.d2d.FillEllipse(&circle, if ov.close == 2 { &self.red } else { &self.shade });
            let arm = rad * 0.38;
            let width = (rad / 7.0).max(1.5);
            let p = |x: f32, y: f32| D2D_POINT_2F { x, y };
            self.d2d.DrawLine(p(cx - arm, cy - arm), p(cx + arm, cy + arm), &self.white, width, None);
            self.d2d.DrawLine(p(cx - arm, cy + arm), p(cx + arm, cy - arm), &self.white, width, None);
        }
        if let Some(rect) = sound {
            let _ = self.draw_sound_button(rect, ov.sound == 2, ov.close == 3);
        }
    }

    /// A speaker in a circle: with sound waves, or crossed out when muted.
    unsafe fn draw_sound_button(&self, (l, t, r, b): (i32, i32, i32, i32), muted: bool, hot: bool) -> Result<()> {
        let (cx, cy, rad) = ((l + r) as f32 / 2.0, (t + b) as f32 / 2.0, (r - l) as f32 / 2.0);
        let circle = D2D1_ELLIPSE { point: D2D_POINT_2F { x: cx, y: cy }, radiusX: rad, radiusY: rad };
        self.d2d.FillEllipse(&circle, &self.shade);
        if hot {
            self.d2d.FillEllipse(&circle, &self.light);
        }
        let s = rad * 0.46;
        let o = cx - 0.2 * s;
        let width = (rad / 7.0).max(1.5);
        let p = |x: f32, y: f32| D2D_POINT_2F { x, y };
        let speaker = self.d2f.CreatePathGeometry()?;
        let sink = speaker.Open()?;
        sink.BeginFigure(p(o - 0.75 * s, cy - 0.32 * s), D2D1_FIGURE_BEGIN_FILLED);
        sink.AddLines(&[
            p(o - 0.25 * s, cy - 0.32 * s),
            p(o + 0.3 * s, cy - 0.8 * s),
            p(o + 0.3 * s, cy + 0.8 * s),
            p(o - 0.25 * s, cy + 0.32 * s),
            p(o - 0.75 * s, cy + 0.32 * s),
        ]);
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
        self.d2d.FillGeometry(&speaker, &self.white, None);
        if muted {
            let (x, arm) = (o + 0.85 * s, 0.3 * s);
            self.d2d.DrawLine(p(x - arm, cy - arm), p(x + arm, cy + arm), &self.white, width, None);
            self.d2d.DrawLine(p(x - arm, cy + arm), p(x + arm, cy - arm), &self.white, width, None);
        } else {
            let waves = self.d2f.CreatePathGeometry()?;
            let sink = waves.Open()?;
            for (x, half, radius) in [(o + 0.55 * s, 0.32 * s, 0.4 * s), (o + 0.8 * s, 0.62 * s, 0.75 * s)] {
                sink.BeginFigure(p(x, cy - half), D2D1_FIGURE_BEGIN_HOLLOW);
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: p(x, cy + half),
                    size: D2D_SIZE_F { width: radius, height: radius },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }
            sink.Close()?;
            self.d2d.DrawGeometry(&waves, &self.white, width, None);
        }
        Ok(())
    }

    /// Camera, object and time in the top-left corner; `reserve` = pixels kept free on the right.
    unsafe fn draw_caption(&self, ov: &Overlay, reserve: f32) {
        let pad = (self.font_px * 0.6).round();
        let dot = self.font_px * 0.62;
        let wide: Vec<u16> = HSTRING::from(ov.text.as_str()).as_wide().to_vec();
        let max_w = (self.size.0 as f32 - pad * 4.0 - dot * 2.0 - reserve).max(10.0);
        let Ok(layout) = self.dwrite.CreateTextLayout(&wide, &self.font, max_w, self.font_px * 2.0) else { return };
        let _ = layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        let trimming = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
        let _ = layout.SetTrimming(&trimming, None);
        let mut m = DWRITE_TEXT_METRICS::default();
        let _ = layout.GetMetrics(&mut m);
        let text_w = m.width.min(max_w);
        let x0 = pad;
        let y0 = pad;
        let box_w = pad * 0.8 + dot + pad * 0.6 + text_w + pad * 0.9;
        let box_h = m.height + pad * 0.7;
        let rr = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F { left: x0, top: y0, right: x0 + box_w, bottom: y0 + box_h },
            radiusX: box_h / 2.0,
            radiusY: box_h / 2.0,
        };
        self.d2d.FillRoundedRectangle(&rr, &self.shade);
        let cx = x0 + pad * 0.8 + dot / 2.0;
        let cy = y0 + box_h / 2.0;
        let ellipse = D2D1_ELLIPSE { point: D2D_POINT_2F { x: cx, y: cy }, radiusX: dot / 2.0, radiusY: dot / 2.0 };
        self.d2d.FillEllipse(&ellipse, if ov.live { &self.red } else { &self.grey });
        let origin = D2D_POINT_2F { x: x0 + pad * 0.8 + dot + pad * 0.6, y: y0 + (box_h - m.height) / 2.0 };
        self.d2d.DrawTextLayout(origin, &layout, &self.white, D2D1_DRAW_TEXT_OPTIONS_CLIP);
    }

    unsafe fn blit(&mut self, tex: &ID3D11Texture2D, index: u32, crop: (u32, u32), zoom: Zoom) -> Result<()> {
        let (Some(vd), Some(vc)) = (self.video_device.clone(), self.video_context.clone()) else {
            return Err(windows::Win32::Foundation::E_NOTIMPL.into());
        };
        let mut td = D3D11_TEXTURE2D_DESC::default();
        tex.GetDesc(&mut td);
        let in_size = (td.Width, td.Height);
        if self.vp.as_ref().is_none_or(|v| v.in_size != in_size) {
            self.vp = None;
            let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputFrameRate: DXGI_RATIONAL { Numerator: 30, Denominator: 1 },
                InputWidth: td.Width,
                InputHeight: td.Height,
                OutputFrameRate: DXGI_RATIONAL { Numerator: 30, Denominator: 1 },
                OutputWidth: self.size.0,
                OutputHeight: self.size.1,
                Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
            };
            let enumr = vd.CreateVideoProcessorEnumerator(&content)?;
            let proc = vd.CreateVideoProcessor(&enumr, 0)?;
            let back: ID3D11Texture2D = self.swapchain.GetBuffer(0)?;
            let ovd = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
            };
            let mut output = None;
            vd.CreateVideoProcessorOutputView(&back, &enumr, &ovd, Some(&mut output))?;
            let output = output.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
            vc.VideoProcessorSetStreamFrameFormat(&proc, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            // BT.709, 16–235 range → full-range RGB
            vc.VideoProcessorSetStreamColorSpace(&proc, 0, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: (1 << 2) | (1 << 4) });
            vc.VideoProcessorSetOutputColorSpace(&proc, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 });
            vc.VideoProcessorSetStreamAutoProcessingMode(&proc, 0, false);
            let black = D3D11_VIDEO_COLOR { Anonymous: D3D11_VIDEO_COLOR_0 { RGBA: D3D11_VIDEO_COLOR_RGBA { R: 0.0, G: 0.0, B: 0.0, A: 1.0 } } };
            vc.VideoProcessorSetOutputBackgroundColor(&proc, false, &black);
            self.vp = Some(Vp { enumr, proc, output, in_size, inputs: Vec::new() });
        }
        let vp = self.vp.as_mut().unwrap();
        let key = tex.as_raw() as usize;
        let view = match vp.inputs.iter().find(|(k, i, _)| *k == key && *i == index) {
            Some((_, _, v)) => v.clone(),
            None => {
                let ivd = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                    FourCC: 0,
                    ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: index } },
                };
                let mut v = None;
                vd.CreateVideoProcessorInputView(tex, &vp.enumr, &ivd, Some(&mut v))?;
                let v = v.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
                if vp.inputs.len() > 64 {
                    vp.inputs.clear();
                }
                vp.inputs.push((key, index, v.clone()));
                v
            }
        };
        // The zoomed part at the full resolution of the stream: the video processor scales it.
        let (sl, st, sr, sb) = part(crop, zoom);
        let src = RECT { left: sl.round() as i32, top: st.round() as i32, right: sr.round() as i32, bottom: sb.round() as i32 };
        let (l, t, r, b) = fit(((src.right - src.left).max(1) as u32, (src.bottom - src.top).max(1) as u32), self.size);
        let dst = RECT { left: l.round() as i32, top: t.round() as i32, right: r.round() as i32, bottom: b.round() as i32 };
        let full = RECT { left: 0, top: 0, right: self.size.0 as i32, bottom: self.size.1 as i32 };
        vc.VideoProcessorSetStreamSourceRect(&vp.proc, 0, true, Some(&src));
        vc.VideoProcessorSetStreamDestRect(&vp.proc, 0, true, Some(&dst));
        vc.VideoProcessorSetOutputTargetRect(&vp.proc, true, Some(&full));
        let mut stream = D3D11_VIDEO_PROCESSOR_STREAM { Enable: true.into(), pInputSurface: ManuallyDrop::new(Some(view)), ..Default::default() };
        let r = vc.VideoProcessorBlt(&vp.proc, &vp.output, 0, std::slice::from_ref(&stream));
        ManuallyDrop::drop(&mut stream.pInputSurface);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::{fit, part, Zoom};

    #[test]
    fn zoomed_part() {
        assert_eq!(part((1920, 1080), Zoom::FULL), (0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(part((1920, 1080), Zoom { x: 0.5, y: 0.25, size: 0.25 }), (960.0, 270.0, 1440.0, 540.0));
    }

    #[test]
    fn letterbox_fit() {
        assert_eq!(fit((1920, 1080), (480, 270)), (0.0, 0.0, 480.0, 270.0));
        let (l, t, r, b) = fit((640, 480), (480, 270));
        assert!((l - 60.0).abs() < 0.01 && t == 0.0 && (r - 420.0).abs() < 0.01 && b == 270.0);
        let (l, t, _, b) = fit((1920, 1080), (480, 400));
        assert!(l == 0.0 && (t - 65.0).abs() < 0.01 && (b - 335.0).abs() < 0.01);
    }
}
