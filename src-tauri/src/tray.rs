//! Draws the notification-area icon: the battery percentage while connected,
//! a headphone glyph otherwise.

use std::ffi::c_void;
use std::sync::OnceLock;

use fontdue::{Font, FontSettings};
use tiny_skia::{FillRule, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};
use windows::core::w;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    /// No headset connected.
    Idle,
    /// Connected, battery level not known yet.
    Connected,
    Battery { level: u8, charging: bool },
}

pub struct Rendered {
    pub rgba: Vec<u8>,
    pub size: u32,
}

/// Windows draws tray icons at the small-icon size for the current scale.
pub fn icon_size() -> u32 {
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) };
    (size.max(16) as u32).min(64)
}

/// The taskbar has its own light/dark setting, separate from apps.
pub fn taskbar_is_light() -> bool {
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    status.is_ok() && value == 1
}

fn font() -> Option<&'static Font> {
    static FONT: OnceLock<Option<Font>> = OnceLock::new();
    FONT.get_or_init(|| {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        ["seguisb.ttf", "segoeuib.ttf", "arialbd.ttf"].iter().find_map(|file| {
            let bytes = std::fs::read(format!("{windir}\\Fonts\\{file}")).ok()?;
            Font::from_bytes(bytes, FontSettings::default()).ok()
        })
    })
    .as_ref()
}

pub fn render(icon: Icon, size: u32, light_taskbar: bool) -> Rendered {
    let ink: [u8; 3] = if light_taskbar { [0x1B, 0x1B, 0x1B] } else { [0xFF, 0xFF, 0xFF] };
    let rgba = match icon {
        Icon::Battery { level, charging } => {
            // Fluent's critical / success text colours for each taskbar theme.
            let colour = match (level <= 20 && !charging, charging, light_taskbar) {
                (true, _, false) => [0xFF, 0x99, 0xA4],
                (true, _, true) => [0xC4, 0x2B, 0x1C],
                (_, true, false) => [0x6C, 0xCB, 0x5F],
                (_, true, true) => [0x0F, 0x7B, 0x0F],
                _ => ink,
            };
            draw_number(level.min(100), size, colour).unwrap_or_else(|| draw_glyph(size, ink, 255))
        }
        Icon::Connected => draw_glyph(size, ink, 255),
        Icon::Idle => draw_glyph(size, ink, 120),
    };
    Rendered { rgba, size }
}

fn draw_number(value: u8, size: u32, colour: [u8; 3]) -> Option<Vec<u8>> {
    let font = font()?;
    let text = value.to_string();
    let target = size as f32;

    // Start tall, then shrink until the digits fit the icon's width.
    let mut px = target * 0.92;
    let mut glyphs;
    loop {
        glyphs = text.chars().map(|c| font.rasterize(c, px)).collect::<Vec<_>>();
        let width: f32 = glyphs.iter().map(|(m, _)| m.advance_width).sum();
        if width <= target + 0.5 || px < 6.0 {
            break;
        }
        px *= (target / width).min(0.97);
    }

    let width: f32 = glyphs.iter().map(|(m, _)| m.advance_width).sum();
    let top = glyphs.iter().map(|(m, _)| m.ymin + m.height as i32).max()?;
    let bottom = glyphs.iter().map(|(m, _)| m.ymin).min()?;
    let baseline = ((target + (top - bottom) as f32) / 2.0).round() as i32 + bottom;

    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let mut pen = ((target - width) / 2.0).max(0.0);
    for (metrics, coverage) in &glyphs {
        let left = (pen + metrics.xmin as f32).round() as i32;
        let glyph_top = baseline - metrics.ymin - metrics.height as i32;
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let (x, y) = (left + col as i32, glyph_top + row as i32);
                if x < 0 || y < 0 || x >= size as i32 || y >= size as i32 {
                    continue;
                }
                let alpha = coverage[row * metrics.width + col];
                let at = ((y as u32 * size + x as u32) * 4) as usize;
                if alpha > rgba[at + 3] {
                    rgba[at..at + 3].copy_from_slice(&colour);
                    rgba[at + 3] = alpha;
                }
            }
        }
        pen += metrics.advance_width;
    }
    Some(rgba)
}

/// A front-on pair of headphones on a 16-unit grid.
fn draw_glyph(size: u32, colour: [u8; 3], alpha: u8) -> Vec<u8> {
    let Some(mut pixmap) = Pixmap::new(size, size) else {
        return vec![0; (size * size * 4) as usize];
    };
    let scale = Transform::from_scale(size as f32 / 16.0, size as f32 / 16.0);
    let mut paint = Paint::default();
    paint.set_color_rgba8(colour[0], colour[1], colour[2], alpha);
    paint.anti_alias = true;

    // Headband: a half circle of radius 5.4 around (8, 8.2).
    const K: f32 = 0.5523 * 5.4;
    let mut band = PathBuilder::new();
    band.move_to(2.6, 9.4);
    band.line_to(2.6, 8.2);
    band.cubic_to(2.6, 8.2 - K, 8.0 - K, 2.8, 8.0, 2.8);
    band.cubic_to(8.0 + K, 2.8, 13.4, 8.2 - K, 13.4, 8.2);
    band.line_to(13.4, 9.4);
    if let Some(path) = band.finish() {
        let stroke = Stroke { width: 1.5, line_cap: LineCap::Round, ..Stroke::default() };
        pixmap.stroke_path(&path, &paint, &stroke, scale, None);
    }

    for x in [1.2f32, 11.4] {
        let (y, w, h, r) = (8.4f32, 3.4f32, 5.8f32, 1.5f32);
        let mut cup = PathBuilder::new();
        cup.move_to(x + r, y);
        cup.line_to(x + w - r, y);
        cup.quad_to(x + w, y, x + w, y + r);
        cup.line_to(x + w, y + h - r);
        cup.quad_to(x + w, y + h, x + w - r, y + h);
        cup.line_to(x + r, y + h);
        cup.quad_to(x, y + h, x, y + h - r);
        cup.line_to(x, y + r);
        cup.quad_to(x, y, x + r, y);
        cup.close();
        if let Some(path) = cup.finish() {
            pixmap.fill_path(&path, &paint, FillRule::Winding, scale, None);
        }
    }

    // tiny-skia stores premultiplied pixels; icons want straight alpha.
    pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted(rgba: &[u8]) -> usize {
        rgba.chunks(4).filter(|p| p[3] > 0).count()
    }

    #[test]
    fn renders_every_icon_kind_at_tray_sizes() {
        for size in [16, 20, 24, 32] {
            for icon in [
                Icon::Idle,
                Icon::Connected,
                Icon::Battery { level: 7, charging: false },
                Icon::Battery { level: 80, charging: true },
                Icon::Battery { level: 100, charging: false },
            ] {
                let rendered = render(icon, size, false);
                assert_eq!(rendered.rgba.len(), (size * size * 4) as usize);
                assert!(painted(&rendered.rgba) > 10, "{icon:?} at {size}px is blank");
            }
        }
    }

    #[test]
    fn low_battery_is_drawn_in_the_critical_colour() {
        let rendered = render(Icon::Battery { level: 12, charging: false }, 32, false);
        let solid = rendered.rgba.chunks(4).find(|p| p[3] == 255).expect("some solid pixel");
        assert_eq!(&solid[..3], &[0xFF, 0x99, 0xA4]);
    }
}
