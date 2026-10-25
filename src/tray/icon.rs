//! Menu bar artwork: the app's glyph as a template image, and the status dots for profile
//! rows. Both are RGBA at 2× for Retina.
//!
//! The glyph is `assets/brand/tray.svg`, rasterized with GPUI's own SVG renderer, so no image
//! crate is needed. The dots are drawn in code from a distance function.

use std::sync::Arc;

use colima_gpui::assets::Assets;
use gpui::{DevicePixels, SvgRenderer, SvgSize, size};

/// Menu bar icon canvas: tray-icon draws it at most 22 pt tall, so 44 px is 2×.
pub const GLYPH_PX: u32 = 44;
/// Menu item icon canvas: muda draws it at most 18 pt tall.
pub const DOT_PX: u32 = 36;

/// The menu bar glyph: three stacked blocks forming a "C", as solid silhouettes.
const TRAY_SVG: &[u8] = include_bytes!("../../assets/brand/tray.svg");

/// The glyph in black with alpha coverage, for a template image (macOS tints it to match a
/// light or dark menu bar).
pub fn glyph() -> Result<Vec<u8>, String> {
    let renderer = SvgRenderer::new(Arc::new(Assets));
    let svg = renderer
        .parse_svg(TRAY_SVG)
        .map_err(|error| error.to_string())?;
    let px = DevicePixels(GLYPH_PX as i32);
    let image = renderer
        .render_parsed(&svg, SvgSize::ExactSize(size(px, px)))
        .map_err(|error| error.to_string())?;
    let rendered = image.size(0);
    if rendered != size(px, px) {
        return Err(format!("tray glyph rendered at {rendered:?}"));
    }
    // The frame is BGRA with premultiplied alpha; a template image only needs the alpha.
    let bgra = image.as_bytes(0).ok_or("tray glyph has no frame")?;
    Ok(bgra
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|[_, _, _, a]| [0, 0, 0, *a])
        .collect())
}

/// A filled status dot of `color`, 8 pt across in an 18 pt menu icon.
pub fn dot(color: [u8; 4]) -> Vec<u8> {
    let center = DOT_PX as f32 / 2.;
    rasterize(DOT_PX, color, |(x, y)| {
        8. - ((x - center).powi(2) + (y - center).powi(2)).sqrt()
    })
}

/// Fills a `size`² RGBA canvas with `color`, scaling its alpha by coverage. `inside(p)` is the
/// signed distance into the shape at pixel center `p` (positive inside), so a 1 px ramp
/// around the edge anti-aliases it.
fn rasterize(size: u32, color: [u8; 4], inside: impl Fn((f32, f32)) -> f32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let coverage = (inside((x as f32 + 0.5, y as f32 + 0.5)) + 0.5).clamp(0., 1.);
            let [r, g, b, a] = color;
            rgba.extend([r, g, b, (a as f32 * coverage).round() as u8]);
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(rgba: &[u8], size: u32, x: u32, y: u32) -> u8 {
        rgba[((y * size + x) * 4 + 3) as usize]
    }

    #[test]
    fn glyph_renders_the_tray_svg_as_a_template() {
        let rgba = glyph().unwrap();
        assert_eq!(rgba.len(), (GLYPH_PX * GLYPH_PX * 4) as usize);
        // The middle of the green (top) block is solid; the "C" opening on the right is clear.
        assert_eq!(alpha(&rgba, GLYPH_PX, 22, 12), 255);
        assert_eq!(alpha(&rgba, GLYPH_PX, 32, 21), 0);
        // The glyph is ~18 of 22 pt tall, so the canvas edges stay transparent.
        assert!((0..GLYPH_PX).all(|x| alpha(&rgba, GLYPH_PX, x, 0) == 0));
        assert!((0..GLYPH_PX).all(|x| alpha(&rgba, GLYPH_PX, x, 43) == 0));
        // Edges are anti-aliased, not just on/off.
        assert!(rgba.chunks(4).any(|px| px[3] > 0 && px[3] < 255));
        // Template images only use alpha: every pixel is black.
        assert!(rgba.chunks(4).all(|px| px[..3] == [0, 0, 0]));
    }

    #[test]
    fn dot_is_a_centered_disc_in_its_color() {
        let rgba = dot([30, 200, 90, 255]);
        assert_eq!(rgba.len(), (DOT_PX * DOT_PX * 4) as usize);
        assert_eq!(alpha(&rgba, DOT_PX, 18, 18), 255);
        assert_eq!(
            &rgba[((18 * DOT_PX + 18) * 4) as usize..][..3],
            &[30, 200, 90]
        );
        assert_eq!(alpha(&rgba, DOT_PX, 2, 18), 0);
        assert_eq!(alpha(&rgba, DOT_PX, 18, 30), 0);
    }
}
