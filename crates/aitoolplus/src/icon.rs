//! Procedural tray icon artwork for AI ToolPlus.
//!
//! A generated icon avoids external asset loading failures and provides
//! a crisp, mathematically antialiased 32x32 RGBA buffer suitable for
//! Windows high-DPI notification areas.

pub const SIZE: u32 = 32;

/// Tile background color: modern AI ToolPlus blue.
const TILE: [f32; 3] = [0.18, 0.45, 0.96];
/// Inner plus symbol: crisp white.
const MARK: [f32; 3] = [1.0, 1.0, 1.0];

/// Render the icon as straight (non-premultiplied) RGBA, row-major.
pub fn rgba() -> Vec<u8> {
    let size = SIZE as f32;
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            // Outer rounded tile
            let tile = rounded_rect(px, py, size / 2.0, size / 2.0, 14.5, 14.5, 6.5);
            // Inner plus sign: horizontal bar + vertical bar
            let h_bar = rounded_rect(px, py, size / 2.0, size / 2.0, 8.5, 2.8, 1.5);
            let v_bar = rounded_rect(px, py, size / 2.0, size / 2.0, 2.8, 8.5, 1.5);
            let mark = h_bar.max(v_bar);

            let mut rgb = [0.0f32; 3];
            for c in 0..3 {
                rgb[c] = TILE[c] * (1.0 - mark) + MARK[c] * mark;
            }
            let alpha = tile;

            for channel in rgb {
                pixels.push((channel * 255.0).round().clamp(0.0, 255.0) as u8);
            }
            pixels.push((alpha * 255.0).round().clamp(0.0, 255.0) as u8);
        }
    }
    pixels
}

fn rounded_rect(px: f32, py: f32, cx: f32, cy: f32, half_w: f32, half_h: f32, r: f32) -> f32 {
    let dx = (px - cx).abs() - (half_w - r);
    let dy = (py - cy).abs() - (half_h - r);
    let outside = dx.max(0.0).hypot(dy.max(0.0));
    let inside = dx.max(dy).min(0.0);
    let distance = outside + inside - r;
    (0.5 - distance).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(data: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [data[i], data[i + 1], data[i + 2], data[i + 3]]
    }

    #[test]
    fn the_buffer_is_exactly_four_bytes_per_pixel() {
        assert_eq!(rgba().len(), (SIZE * SIZE * 4) as usize);
    }

    #[test]
    fn the_corners_are_transparent_and_the_middle_is_not() {
        let data = rgba();
        assert_eq!(pixel(&data, 0, 0)[3], 0, "top-left corner is rounded off");
        assert_eq!(pixel(&data, SIZE - 1, SIZE - 1)[3], 0);
        assert_eq!(pixel(&data, SIZE / 2, SIZE / 2)[3], 255);
    }

    #[test]
    fn the_plus_mark_is_centered() {
        let data = rgba();
        let centre = pixel(&data, SIZE / 2, SIZE / 2);
        assert_eq!(centre, [255, 255, 255, 255], "center of the plus mark is white");
    }

    #[test]
    fn the_edge_is_antialiased() {
        let data = rgba();
        let partial = (0..SIZE)
            .flat_map(|y| (0..SIZE).map(move |x| (x, y)))
            .filter(|&(x, y)| matches!(pixel(&data, x, y)[3], 1..=254))
            .count();
        assert!(partial > 8, "expected soft edges, found {partial} pixels");
    }
}
