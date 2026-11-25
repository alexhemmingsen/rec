//! Where the Stop pill goes.
//!
//! All rectangles are in global logical coordinates with the origin at the
//! top-left of the main display and y growing downwards.

use rec_capture::Rect;

pub const PILL_WIDTH: f64 = 168.0;
pub const PILL_HEIGHT: f64 = 40.0;
const GAP: f64 = 8.0;

/// Top-left corner for a `w` x `h` pill next to `region` on `display`:
/// just below the region's bottom-right corner, else just above its top-right
/// corner, else inside its bottom-right corner (the region fills the screen;
/// the pill is left out of the video anyway). Always kept on the display.
pub fn pill_origin(region: Rect, display: Rect, w: f64, h: f64) -> (f64, f64) {
    let right = region.x + region.width;
    let bottom = region.y + region.height;
    let display_bottom = display.y + display.height;

    let below = bottom + GAP;
    let above = region.y - GAP - h;
    let (x, y) = if below + h <= display_bottom - GAP {
        (right - w, below)
    } else if above >= display.y + GAP {
        (right - w, above)
    } else {
        (right - w - GAP * 2.0, bottom - h - GAP * 2.0)
    };

    let min_x = display.x + GAP;
    let max_x = (display.x + display.width - w - GAP).max(min_x);
    let min_y = display.y + GAP;
    let max_y = (display_bottom - h - GAP).max(min_y);
    (x.clamp(min_x, max_x), y.clamp(min_y, max_y))
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

#[cfg(test)]
mod tests {
    use super::*;

    const DISPLAY: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1440.0,
        height: 900.0,
    };

    fn overlaps(a: Rect, x: f64, y: f64) -> bool {
        x < a.x + a.width && x + PILL_WIDTH > a.x && y < a.y + a.height && y + PILL_HEIGHT > a.y
    }

    #[test]
    fn goes_below_the_bottom_right_corner() {
        let region = Rect::new(100.0, 100.0, 400.0, 300.0);
        let (x, y) = pill_origin(region, DISPLAY, PILL_WIDTH, PILL_HEIGHT);
        assert_eq!((x, y), (500.0 - PILL_WIDTH, 408.0));
        assert!(!overlaps(region, x, y));
    }

    #[test]
    fn goes_above_when_there_is_no_room_below() {
        let region = Rect::new(100.0, 500.0, 400.0, 380.0);
        let (x, y) = pill_origin(region, DISPLAY, PILL_WIDTH, PILL_HEIGHT);
        assert_eq!(y, 500.0 - 8.0 - PILL_HEIGHT);
        assert_eq!(x, 500.0 - PILL_WIDTH);
        assert!(!overlaps(region, x, y));
    }

    #[test]
    fn goes_inside_when_the_region_fills_the_screen() {
        let (x, y) = pill_origin(DISPLAY, DISPLAY, PILL_WIDTH, PILL_HEIGHT);
        assert!(x + PILL_WIDTH <= DISPLAY.width && y + PILL_HEIGHT <= DISPLAY.height);
        assert!(
            x > DISPLAY.width / 2.0 && y > DISPLAY.height / 2.0,
            "bottom-right: {x},{y}"
        );
    }

    #[test]
    fn stays_on_the_display_for_regions_at_the_left_edge() {
        let region = Rect::new(0.0, 100.0, 50.0, 50.0);
        let (x, _) = pill_origin(region, DISPLAY, PILL_WIDTH, PILL_HEIGHT);
        assert_eq!(x, 8.0);
    }

    #[test]
    fn works_on_a_secondary_display_with_an_offset() {
        let display = Rect::new(-1920.0, -200.0, 1920.0, 1080.0);
        let region = to_global(Rect::new(100.0, 100.0, 400.0, 300.0), display);
        assert_eq!(region, Rect::new(-1820.0, -100.0, 400.0, 300.0));
        let (x, y) = pill_origin(region, display, PILL_WIDTH, PILL_HEIGHT);
        assert_eq!((x, y), (-1420.0 - PILL_WIDTH, 208.0));
    }
}
