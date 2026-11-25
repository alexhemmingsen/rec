//! Where the Stop pill goes, and coordinate conversions.
//!
//! All rectangles are in global logical coordinates with the origin at the
//! top-left of the main display and y growing downwards.

use rec_capture::{DisplayInfo, Rect};

/// The pill's top edge, below the top of the display. The overlay's "Drag to
/// record" hint uses the same spot (`.overlay-hint` in styles.css), so the
/// pill takes its place.
pub const PILL_TOP: f64 = 48.0;
/// Room around the pill for its shadow; the window is that much larger.
pub const PILL_MARGIN: f64 = 20.0;
/// The pill window (pill plus margin).
/// Wide enough for the pen button and its keys beside the Stop pill, with
/// Windows' longer key names ("Ctrl" "Shift" "D").
pub const PILL_WIDTH: f64 = 480.0;
pub const PILL_HEIGHT: f64 = 36.0 + 2.0 * PILL_MARGIN;

/// The Stop pill's window: centred at the top of `display`.
pub fn pill_frame(display: Rect) -> Rect {
    Rect::new(
        display.x + (display.width - PILL_WIDTH) / 2.0,
        display.y + PILL_TOP - PILL_MARGIN,
        PILL_WIDTH,
        PILL_HEIGHT,
    )
}

/// Convert a region relative to a display into global coordinates.
pub fn to_global(region: Rect, display: Rect) -> Rect {
    Rect::new(
        display.x + region.x,
        display.y + region.y,
        region.width,
        region.height,
    )
}

/// A global logical rect on `display`, in global physical pixels.
pub fn to_pixels(rect: Rect, display: &DisplayInfo) -> Rect {
    let s = display.scale_factor;
    Rect::new(
        display.pixel_frame.x + (rect.x - display.frame.x) * s,
        display.pixel_frame.y + (rect.y - display.frame.y) * s,
        rect.width * s,
        rect.height * s,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_to_pixels_on_a_scaled_secondary_display() {
        // A 150% display to the right of a 100% 1920x1080 one.
        let display = DisplayInfo {
            id: 2,
            frame: Rect::new(1280.0, 0.0, 1706.67, 960.0),
            pixel_frame: Rect::new(1920.0, 0.0, 2560.0, 1440.0),
            scale_factor: 1.5,
            is_main: false,
        };
        let r = to_pixels(Rect::new(1380.0, 100.0, 168.0, 40.0), &display);
        assert_eq!(r, Rect::new(2070.0, 150.0, 252.0, 60.0));
    }

    #[test]
    fn pill_is_centred_at_the_top_of_the_display() {
        let display = Rect::new(0.0, 0.0, 1440.0, 900.0);
        let f = pill_frame(display);
        assert_eq!(f.x + f.width / 2.0, 720.0);
        assert_eq!(f.y + PILL_MARGIN, PILL_TOP);
    }

    #[test]
    fn pill_follows_a_secondary_display_with_an_offset() {
        let display = Rect::new(-1920.0, -200.0, 1920.0, 1080.0);
        let f = pill_frame(display);
        assert_eq!(f.x + f.width / 2.0, -960.0);
        assert_eq!(f.y, -200.0 + PILL_TOP - PILL_MARGIN);
    }

    #[test]
    fn to_global_offsets_by_the_display() {
        let display = Rect::new(-1920.0, -200.0, 1920.0, 1080.0);
        let region = to_global(Rect::new(100.0, 100.0, 400.0, 300.0), display);
        assert_eq!(region, Rect::new(-1820.0, -100.0, 400.0, 300.0));
    }
}
