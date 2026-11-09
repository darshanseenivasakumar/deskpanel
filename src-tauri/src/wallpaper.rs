//! Self-drawn "acrylic": render the wallpaper exactly as Windows lays it out on this monitor,
//! at quarter resolution, blur + saturate it once, and hand it to the webview as a PNG.
//! Behind a desktop-layer panel there is only wallpaper, so this matches reality, needs no
//! window focus (unlike DWM acrylic), and animates for free with the panel's CSS.
use base64::Engine;
use image::{imageops, imageops::FilterType, ImageBuffer, Rgb, RgbImage};
use windows::Win32::Foundation::RECT;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::*;

/// Render target is 1/DOWNSCALE of the monitor. The result is blurred, so upscaling is invisible.
const DOWNSCALE: u32 = 4;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Glass {
    /// data: URL of the blurred PNG.
    pub url: String,
    /// UNblurred wallpaper at full monitor resolution (JPEG bytes, kept in Rust). The webview only
    /// receives the window-sized crop (`crop_backdrop`), drawn in a thin ring behind the panel so
    /// its anti-aliased corners blend into real wallpaper pixels while the window is
    /// region-clipped on the desktop (see desktop::set_region). Cropping saves ~10 MB of RAM.
    #[serde(skip)]
    pub full_jpeg: std::sync::Arc<Vec<u8>>,
    /// Monitor size in physical px (the PNG represents this whole area).
    pub screen_w: i32,
    pub screen_h: i32,
    pub mode: String,
    pub build_ms: u128,
}

struct Info {
    path: Option<String>,
    pos: DESKTOP_WALLPAPER_POSITION,
    bg: [u8; 3],
}

fn query(monitor: RECT) -> windows::core::Result<Info> {
    unsafe {
        // Fine if COM is already initialised on this thread (S_FALSE / RPC_E_CHANGED_MODE).
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let dw: IDesktopWallpaper = CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)?;
        let n = dw.GetMonitorDevicePathCount()?;
        let mut path = None;
        for i in 0..n {
            let id = dw.GetMonitorDevicePathAt(i)?;
            let r = dw.GetMonitorRECT(windows::core::PCWSTR(id.0))?;
            if r == monitor || n == 1 {
                let p = dw.GetWallpaper(windows::core::PCWSTR(id.0))?;
                let s = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const _));
                CoTaskMemFree(Some(id.0 as *const _));
                if !s.is_empty() {
                    path = Some(s);
                }
                break;
            }
            CoTaskMemFree(Some(id.0 as *const _));
        }
        let pos = dw.GetPosition()?;
        let c = dw.GetBackgroundColor()?.0; // COLORREF 0x00BBGGRR
        Ok(Info { path, pos, bg: [(c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8] })
    }
}

fn mode_name(p: DESKTOP_WALLPAPER_POSITION) -> &'static str {
    match p {
        DWPOS_CENTER => "center",
        DWPOS_TILE => "tile",
        DWPOS_STRETCH => "stretch",
        DWPOS_FIT => "fit",
        DWPOS_FILL => "fill",
        DWPOS_SPAN => "span",
        _ => "unknown",
    }
}

/// Lay the wallpaper out on a (cw x ch) canvas exactly like Windows does on the monitor.
fn compose(img: &RgbImage, pos: DESKTOP_WALLPAPER_POSITION, bg: [u8; 3], cw: u32, ch: u32, down: u32) -> RgbImage {
    let mut canvas: RgbImage = ImageBuffer::from_pixel(cw, ch, Rgb(bg));
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    let place = |canvas: &mut RgbImage, scaled: &RgbImage| {
        let x = (cw as i64 - scaled.width() as i64) / 2;
        let y = (ch as i64 - scaled.height() as i64) / 2;
        imageops::overlay(canvas, scaled, x, y);
    };
    let resize = |s: f64| imageops::resize(img, ((iw * s).round() as u32).max(1), ((ih * s).round() as u32).max(1), FilterType::Triangle);
    match pos {
        DWPOS_STRETCH => canvas = imageops::resize(img, cw, ch, FilterType::Triangle),
        DWPOS_FIT => place(&mut canvas, &resize((cw as f64 / iw).min(ch as f64 / ih))),
        DWPOS_CENTER => place(&mut canvas, &resize(1.0 / down as f64)),
        DWPOS_TILE => {
            let t = resize(1.0 / down as f64);
            let mut y = 0;
            while y < ch as i64 {
                let mut x = 0;
                while x < cw as i64 {
                    imageops::overlay(&mut canvas, &t, x, y);
                    x += t.width() as i64;
                }
                y += t.height() as i64;
            }
        }
        // FILL, SPAN (single monitor) and anything unknown: cover + centre crop.
        _ => place(&mut canvas, &resize((cw as f64 / iw).max(ch as f64 / ih))),
    }
    canvas
}

fn saturate(img: &mut RgbImage, s: f32) {
    if (s - 1.0).abs() < 0.01 {
        return;
    }
    for p in img.pixels_mut() {
        let [r, g, b] = p.0.map(|c| c as f32);
        let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        p.0 = [r, g, b].map(|c| (l + (c - l) * s).clamp(0.0, 255.0) as u8);
    }
}

/// Crop the full-resolution wallpaper to a window rect (monitor-relative physical px).
pub fn crop_backdrop(full_jpeg: &[u8], x: i32, y: i32, w: i32, h: i32) -> Option<String> {
    let img = image::load_from_memory_with_format(full_jpeg, image::ImageFormat::Jpeg).ok()?.to_rgb8();
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    // Window may hang off-screen: crop the visible part onto a canvas of the window's size.
    let mut out: RgbImage = ImageBuffer::new(w.max(1) as u32, h.max(1) as u32);
    let (sx, sy) = (x.max(0), y.max(0));
    let (ex, ey) = ((x + w).min(iw), (y + h).min(ih));
    if ex > sx && ey > sy {
        let part = imageops::crop_imm(&img, sx as u32, sy as u32, (ex - sx) as u32, (ey - sy) as u32).to_image();
        imageops::overlay(&mut out, &part, (sx - x) as i64, (sy - y) as i64);
    }
    let mut jpg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 92).encode_image(&out).ok()?;
    Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(&jpg)))
}

/// Pop-over glass: blur + saturate a BGRA screen capture (the area behind the popped panel)
/// with the same look as the wallpaper glass. Returns a PNG data URL at 1/DOWNSCALE size.
pub fn snapshot_glass(bgra: &[u8], w: i32, h: i32, scale: f64, blur_css_px: f32, sat: f32) -> Option<String> {
    let rgb: Vec<u8> = bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
    let full: RgbImage = ImageBuffer::from_raw(w as u32, h as u32, rgb)?;
    let (cw, ch) = ((w as u32 / DOWNSCALE).max(1), (h as u32 / DOWNSCALE).max(1));
    let mut small = imageops::resize(&full, cw, ch, FilterType::Triangle);
    small = imageops::blur(&small, ((blur_css_px as f64 * scale / DOWNSCALE as f64) as f32).max(0.5));
    saturate(&mut small, sat);
    let mut png = Vec::new();
    small.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png)))
}

/// `blur_css_px` is a CSS-style blur radius; `scale` is the monitor DPI scale (1.25 here).
pub fn build(monitor: RECT, scale: f64, blur_css_px: f32, sat: f32) -> Result<Glass, String> {
    let t0 = std::time::Instant::now();
    let (mw, mh) = ((monitor.right - monitor.left) as u32, (monitor.bottom - monitor.top) as u32);
    let (cw, ch) = ((mw / DOWNSCALE).max(1), (mh / DOWNSCALE).max(1));
    let info = query(monitor).map_err(|e| format!("IDesktopWallpaper: {e}"))?;
    let src = info.path.as_ref().and_then(|p| match image::open(p) {
        Ok(img) => Some(img.to_rgb8()),
        Err(e) => {
            crate::log!("wallpaper: cannot decode {p:?}: {e}; using background colour");
            None
        }
    });
    let full = match &src {
        Some(img) => compose(img, info.pos, info.bg, mw, mh, 1),
        None => ImageBuffer::from_pixel(mw, mh, Rgb(info.bg)),
    };
    let mut jpg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 92)
        .encode_image(&full)
        .map_err(|e| format!("jpeg: {e}"))?;
    let full_jpeg = std::sync::Arc::new(jpg);
    let mut canvas = imageops::resize(&full, cw, ch, FilterType::Triangle);
    let sigma = (blur_css_px as f64 * scale / DOWNSCALE as f64) as f32;
    canvas = imageops::blur(&canvas, sigma.max(0.5));
    saturate(&mut canvas, sat);
    let mut png = Vec::new();
    canvas
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("png: {e}"))?;
    let url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png));
    Ok(Glass {
        url,
        full_jpeg,
        screen_w: mw as i32,
        screen_h: mh as i32,
        mode: format!("{} {:?}", mode_name(info.pos), info.path.unwrap_or_default()),
        build_ms: t0.elapsed().as_millis(),
    })
}
