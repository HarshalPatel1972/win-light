//! Extracts the real shell icon for a file or app as a PNG data URL.

use crate::win::{is_shell_target, to_wide, ComGuard};
use base64::Engine;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF, SIIGBF_ICONONLY, SIIGBF_THUMBNAILONLY,
};

/// Icon edge length in pixels; large enough to stay sharp on high-DPI displays.
const ICON_SIZE: i32 = 48;

/// Edge length of the preview pane's picture.
const PREVIEW_SIZE: i32 = 320;

/// The largest size Windows icons are drawn at; the preview shows it smaller,
/// so it is always scaled down, never up.
const LARGE_ICON_SIZE: i32 = 256;

/// Upper bound on cached icons before the cache is reset.
const MAX_CACHED: usize = 2000;

/// Previews are large, so far fewer are kept.
const MAX_CACHED_PREVIEWS: usize = 48;

/// Caches rendered icons so each one is extracted at most once.
#[derive(Default)]
pub struct IconCache {
    icons: Mutex<HashMap<String, Option<String>>>,
    previews: Mutex<HashMap<String, Option<(String, bool)>>>,
}

impl IconCache {
    /// Return the icon for `filepath` as a `data:image/png;base64,...` URL.
    pub fn get(&self, filepath: &str) -> Option<String> {
        let key = cache_key(filepath);
        if let Some(cached) = self.icons.lock().unwrap().get(&key) {
            return cached.clone();
        }

        let icon = render(filepath, ICON_SIZE, SIIGBF_ICONONLY);

        let mut icons = self.icons.lock().unwrap();
        if icons.len() >= MAX_CACHED {
            icons.clear();
        }
        icons.insert(key, icon.clone());
        icon
    }

    /// A large picture of `filepath`, and whether it is a real thumbnail of
    /// the contents (images, PDFs, videos, Office files) rather than an icon.
    ///
    /// The two are asked for separately because they must be shown
    /// differently: a thumbnail has real detail at full size, while an icon
    /// stretched to that size turns to mush.
    pub fn preview(&self, filepath: &str) -> Option<(String, bool)> {
        if let Some(cached) = self.previews.lock().unwrap().get(filepath) {
            return cached.clone();
        }

        let picture = render(filepath, PREVIEW_SIZE, SIIGBF_THUMBNAILONLY)
            .map(|thumbnail| (thumbnail, true))
            .or_else(|| render(filepath, LARGE_ICON_SIZE, SIIGBF_ICONONLY).map(|icon| (icon, false)));

        let mut previews = self.previews.lock().unwrap();
        if previews.len() >= MAX_CACHED_PREVIEWS {
            previews.clear();
        }
        previews.insert(filepath.to_string(), picture.clone());
        picture
    }
}

/// Files of the same type share one icon, except those that carry their own.
fn cache_key(filepath: &str) -> String {
    if is_shell_target(filepath) {
        return filepath.to_string();
    }
    let path = Path::new(filepath);
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "" | "exe" | "lnk" | "url" | "ico" | "msi" | "appx" | "msix" => filepath.to_string(),
        _ => format!("*.{}", ext),
    }
}

/// Cut away fully transparent borders. When an icon has no large version,
/// Windows returns the small one sitting in a corner of an otherwise empty
/// canvas; trimming leaves just the drawing, which the UI can then size itself.
fn trim_transparent(width: u32, height: u32, rgba: Vec<u8>) -> (u32, u32, Vec<u8>) {
    let (w, h) = (width as usize, height as usize);
    let visible = |x: usize, y: usize| rgba[(y * w + x) * 4 + 3] != 0;
    let rows: Vec<usize> = (0..h).filter(|&y| (0..w).any(|x| visible(x, y))).collect();
    let cols: Vec<usize> = (0..w).filter(|&x| (0..h).any(|y| visible(x, y))).collect();
    let (Some(&top), Some(&bottom), Some(&left), Some(&right)) = (rows.first(), rows.last(), cols.first(), cols.last())
    else {
        return (width, height, rgba);
    };

    let mut trimmed = Vec::with_capacity((right - left + 1) * (bottom - top + 1) * 4);
    for y in top..=bottom {
        trimmed.extend_from_slice(&rgba[(y * w + left) * 4..(y * w + right + 1) * 4]);
    }
    ((right - left + 1) as u32, (bottom - top + 1) as u32, trimmed)
}

fn render(filepath: &str, size: i32, flags: SIIGBF) -> Option<String> {
    let _com = ComGuard::new();
    let (width, height, rgba) = unsafe { shell_image_rgba(filepath, size, flags)? };
    // Only the big preview icon is trimmed; list icons keep their own padding
    // so that they all line up.
    let (width, height, rgba) = if size == LARGE_ICON_SIZE {
        trim_transparent(width, height, rgba)
    } else {
        (width, height, rgba)
    };

    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(png_bytes);
    Some(format!("data:image/png;base64,{}", encoded))
}

/// Ask the shell for the item's picture and return it as straight-alpha RGBA.
unsafe fn shell_image_rgba(filepath: &str, size: i32, flags: SIIGBF) -> Option<(u32, u32, Vec<u8>)> {
    let path_w = to_wide(filepath);
    let factory: IShellItemImageFactory =
        SHCreateItemFromParsingName(PCWSTR(path_w.as_ptr()), None).ok()?;
    let bitmap: HBITMAP = factory
        .GetImage(SIZE { cx: size, cy: size }, flags)
        .ok()?;

    let pixels = bitmap_to_rgba(bitmap);
    let _ = DeleteObject(bitmap);
    pixels
}

unsafe fn bitmap_to_rgba(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    let mut info = BITMAP::default();
    let written = GetObjectW(
        bitmap,
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut info as *mut BITMAP as *mut _),
    );
    if written == 0 || info.bmWidth <= 0 || info.bmHeight <= 0 {
        return None;
    }
    let (width, height) = (info.bmWidth, info.bmHeight);

    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // negative height = top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let dc = CreateCompatibleDC(None);
    let rows = GetDIBits(
        dc,
        bitmap,
        0,
        height as u32,
        Some(pixels.as_mut_ptr() as *mut _),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    let _ = DeleteDC(dc);
    if rows == 0 {
        return None;
    }

    bgra_premultiplied_to_rgba(&mut pixels);
    Some((width as u32, height as u32, pixels))
}

/// Shell bitmaps are BGRA with premultiplied alpha; PNG wants straight RGBA.
/// Bitmaps without an alpha channel come back with alpha 0 everywhere and are
/// treated as fully opaque.
fn bgra_premultiplied_to_rgba(pixels: &mut [u8]) {
    let has_alpha = pixels.chunks_exact(4).any(|p| p[3] != 0);
    for p in pixels.chunks_exact_mut(4) {
        p.swap(0, 2);
        if !has_alpha {
            p[3] = 255;
        } else if p[3] != 0 && p[3] != 255 {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_groups_documents_by_extension() {
        assert_eq!(cache_key(r"C:\a\report.PDF"), "*.pdf");
        assert_eq!(cache_key(r"C:\b\other.pdf"), "*.pdf");
        assert_eq!(cache_key(r"C:\a\app.exe"), r"C:\a\app.exe");
        assert_eq!(cache_key(r"C:\a\folder"), r"C:\a\folder");
        assert_eq!(cache_key(r"shell:AppsFolder\X!App"), r"shell:AppsFolder\X!App");
    }

    #[test]
    fn pixel_conversion() {
        // Half-transparent premultiplied blue-ish pixel, then an opaque one.
        let mut px = vec![100, 50, 25, 128, 1, 2, 3, 255];
        bgra_premultiplied_to_rgba(&mut px);
        assert_eq!(px, vec![50, 100, 199, 128, 3, 2, 1, 255]);

        // No alpha channel at all: becomes opaque.
        let mut opaque = vec![10, 20, 30, 0];
        bgra_premultiplied_to_rgba(&mut opaque);
        assert_eq!(opaque, vec![30, 20, 10, 255]);
    }

    #[test]
    fn extracts_icon_for_a_real_file() {
        let exe = std::env::current_exe().unwrap();
        let icon = IconCache::default().get(&exe.to_string_lossy());
        let icon = icon.expect("no icon returned for the test executable");
        assert!(icon.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn renders_a_large_preview_for_a_real_file() {
        let exe = std::env::current_exe().unwrap();
        let cache = IconCache::default();
        let (picture, is_thumbnail) = cache.preview(&exe.to_string_lossy()).expect("no preview returned");
        assert!(picture.starts_with("data:image/png;base64,"));
        assert!(!is_thumbnail, "a program has an icon, not a thumbnail of its contents");
    }

    #[test]
    fn an_image_file_gets_a_real_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("picture.png");
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 64, 64);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&[200u8; 64 * 64 * 4]).unwrap();
        }
        std::fs::write(&path, bytes).unwrap();

        let (_, is_thumbnail) = IconCache::default().preview(&path.to_string_lossy()).expect("no preview");
        assert!(is_thumbnail);
    }

    #[test]
    fn transparent_borders_are_trimmed() {
        // 4×4 canvas with a single opaque 2×1 block at (1,2)-(2,2)
        let mut canvas = vec![0u8; 4 * 4 * 4];
        for x in 1..=2 {
            canvas[(2 * 4 + x) * 4..(2 * 4 + x) * 4 + 4].copy_from_slice(&[9, 9, 9, 255]);
        }
        let (w, h, pixels) = trim_transparent(4, 4, canvas);
        assert_eq!((w, h), (2, 1));
        assert_eq!(pixels, vec![9, 9, 9, 255, 9, 9, 9, 255]);

        // Nothing visible at all: returned as is
        assert_eq!(trim_transparent(2, 2, vec![0u8; 16]).0, 2);
    }
}
