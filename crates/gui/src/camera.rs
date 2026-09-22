//! The view: which part of the board the window shows.
//!
//! Board space uses one unit per cell, with the top-left intersection at
//! `(0, 0)` and y growing downward. Screen space uses physical pixels.

/// How far the slab reaches from the centre line, in cells.
const SLAB_HALF: f32 = 7.85;
/// The distance the whole board plus a small margin needs, in cells.
const FIT_SPAN: f32 = 16.3;
/// The furthest the view may zoom in, as a multiple of the fitted scale.
const MAX_ZOOM: f32 = 40.0;
/// How close a click must be to an intersection to count, in cells.
const HIT_RADIUS: f32 = 0.45;
/// The narrowest the panel may be, mirrored from the settings schema.
const MIN_PIXELS_PER_CELL: f32 = 4.0;

/// Where the view is looking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// The board point at the centre of the window, in cells.
    pub centre: [f32; 2],
    /// The scale of the view.
    pub pixels_per_cell: f32,
    /// True when the board is shown from the other side.
    pub flipped: bool,
}

impl Camera {
    /// A view that fits the whole board in a window of `size` physical pixels.
    pub fn fit(size: [u32; 2]) -> Camera {
        Camera {
            centre: [7.0, 7.0],
            pixels_per_cell: Camera::fit_scale(size),
            flipped: false,
        }
    }

    /// The scale at which the whole board fits the window.
    pub fn fit_scale(size: [u32; 2]) -> f32 {
        let smaller = size[0].min(size[1]).max(1) as f32;
        (smaller / FIT_SPAN).max(MIN_PIXELS_PER_CELL)
    }

    /// The lowest and highest permitted scale for a window.
    pub fn zoom_range(size: [u32; 2]) -> (f32, f32) {
        let fit = Camera::fit_scale(size);
        (fit, fit * MAX_ZOOM)
    }

    /// A board point in cells to a screen point in pixels.
    ///
    /// The inverse of [`Camera::to_board`]. The overlays and the last-move
    /// marker will place themselves with it.
    #[allow(dead_code, reason = "the overlays will place themselves with it")]
    pub fn to_screen(self, size: [f32; 2], board: [f32; 2]) -> [f32; 2] {
        let mut dx = board[0] - self.centre[0];
        let mut dy = board[1] - self.centre[1];
        if self.flipped {
            dx = -dx;
            dy = -dy;
        }
        [
            size[0] * 0.5 + dx * self.pixels_per_cell,
            size[1] * 0.5 + dy * self.pixels_per_cell,
        ]
    }

    /// A screen point in pixels to a board point in cells.
    pub fn to_board(self, size: [f32; 2], screen: [f32; 2]) -> [f32; 2] {
        let mut dx = (screen[0] - size[0] * 0.5) / self.pixels_per_cell;
        let mut dy = (screen[1] - size[1] * 0.5) / self.pixels_per_cell;
        if self.flipped {
            dx = -dx;
            dy = -dy;
        }
        [self.centre[0] + dx, self.centre[1] + dy]
    }

    /// Zoom by `factor`, keeping the board point under `anchor` in place.
    pub fn zoom_about(&mut self, size: [u32; 2], anchor: [f32; 2], factor: f32) {
        let (low, high) = Camera::zoom_range(size);
        let pixels = (self.pixels_per_cell * factor).clamp(low, high);
        let before = self.to_board([size[0] as f32, size[1] as f32], anchor);
        self.pixels_per_cell = pixels;
        let after = self.to_board([size[0] as f32, size[1] as f32], anchor);
        self.centre[0] += before[0] - after[0];
        self.centre[1] += before[1] - after[1];
        self.clamp(size);
    }

    /// Move the view by a screen-space delta.
    pub fn pan(&mut self, size: [u32; 2], delta: [f32; 2]) {
        let sign = if self.flipped { -1.0 } else { 1.0 };
        self.centre[0] -= sign * delta[0] / self.pixels_per_cell;
        self.centre[1] -= sign * delta[1] / self.pixels_per_cell;
        self.clamp(size);
    }

    /// Return to the fitted view.
    pub fn reset(&mut self, size: [u32; 2]) {
        *self = Camera::fit(size);
    }

    /// Keep the board on screen, and centre it when the whole board fits.
    pub fn clamp(&mut self, size: [u32; 2]) {
        let (low, high) = Camera::zoom_range(size);
        self.pixels_per_cell = self.pixels_per_cell.clamp(low, high);
        let half_x = size[0] as f32 / (2.0 * self.pixels_per_cell);
        let half_y = size[1] as f32 / (2.0 * self.pixels_per_cell);
        for (axis, half) in [(0, half_x), (1, half_y)] {
            if half >= SLAB_HALF {
                self.centre[axis] = 7.0;
            } else {
                let limit = SLAB_HALF - half;
                self.centre[axis] = self.centre[axis].clamp(7.0 - limit, 7.0 + limit);
            }
        }
    }

    /// The intersection under a screen point, when the point is close enough to
    /// one and that intersection is on the board.
    pub fn intersection(&self, size: [u32; 2], screen: [f32; 2]) -> Option<[u8; 2]> {
        let board = self.to_board([size[0] as f32, size[1] as f32], screen);
        let column = board[0].round();
        let row = board[1].round();
        let offset_x = board[0] - column;
        let offset_y = board[1] - row;
        if (offset_x * offset_x + offset_y * offset_y).sqrt() > HIT_RADIUS {
            return None;
        }
        if !(0.0..=14.0).contains(&column) || !(0.0..=14.0).contains(&row) {
            return None;
        }
        Some([column as u8, row as u8])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: [u32; 2] = [1200, 900];

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn a_board_point_survives_a_round_trip_through_the_screen() {
        let mut camera = Camera::fit(SIZE);
        for (centre, pixels, flipped) in [
            ([7.0, 7.0], 40.0, false),
            ([3.5, 9.25], 300.0, false),
            ([3.5, 9.25], 300.0, true),
            ([0.0, 14.0], 12.0, true),
        ] {
            camera.centre = centre;
            camera.pixels_per_cell = pixels;
            camera.flipped = flipped;
            for point in [[0.0, 0.0], [7.0, 7.0], [14.0, 14.0], [2.5, 11.75]] {
                let screen = camera.to_screen([SIZE[0] as f32, SIZE[1] as f32], point);
                let back = camera.to_board([SIZE[0] as f32, SIZE[1] as f32], screen);
                assert!(close(back[0], point[0]) && close(back[1], point[1]));
            }
        }
    }

    #[test]
    fn zoom_keeps_the_anchor_under_the_pointer() {
        let mut camera = Camera::fit(SIZE);
        let anchor = [900.0, 200.0];
        let before = camera.to_board([SIZE[0] as f32, SIZE[1] as f32], anchor);
        camera.zoom_about(SIZE, anchor, 2.5);
        let after = camera.to_board([SIZE[0] as f32, SIZE[1] as f32], anchor);
        assert!(close(before[0], after[0]), "{before:?} against {after:?}");
        assert!(close(before[1], after[1]), "{before:?} against {after:?}");
    }

    #[test]
    fn zoom_is_clamped_to_the_range() {
        let mut camera = Camera::fit(SIZE);
        let (low, high) = Camera::zoom_range(SIZE);
        for _ in 0..40 {
            camera.zoom_about(SIZE, [600.0, 450.0], 1.5);
        }
        assert!(close(camera.pixels_per_cell, high));
        for _ in 0..80 {
            camera.zoom_about(SIZE, [600.0, 450.0], 0.7);
        }
        assert!(close(camera.pixels_per_cell, low));
    }

    #[test]
    fn a_fitted_view_is_centred() {
        let mut camera = Camera::fit(SIZE);
        camera.centre = [0.0, 0.0];
        camera.clamp(SIZE);
        assert!(close(camera.centre[0], 7.0));
        assert!(close(camera.centre[1], 7.0));
    }

    #[test]
    fn panning_cannot_push_the_board_off_screen() {
        let mut camera = Camera::fit(SIZE);
        camera.zoom_about(SIZE, [600.0, 450.0], 6.0);
        for _ in 0..200 {
            camera.pan(SIZE, [500.0, 500.0]);
        }
        let half = SIZE[0] as f32 / (2.0 * camera.pixels_per_cell);
        assert!(camera.centre[0] >= 7.0 - (SLAB_HALF - half) - 1e-3);
        assert!(
            camera.centre[1]
                <= 7.0 + (SLAB_HALF - SIZE[1] as f32 / (2.0 * camera.pixels_per_cell)) + 1e-3
        );
    }

    #[test]
    fn the_centre_intersection_is_h8_by_the_notation() {
        let camera = Camera::fit(SIZE);
        let screen = camera.to_screen([SIZE[0] as f32, SIZE[1] as f32], [7.0, 7.0]);
        assert_eq!(camera.intersection(SIZE, screen), Some([7, 7]));
    }

    #[test]
    fn a_click_between_two_stones_misses_both() {
        let camera = Camera::fit(SIZE);
        let size = [SIZE[0] as f32, SIZE[1] as f32];
        let middle = camera.to_screen(size, [7.5, 7.5]);
        assert_eq!(camera.intersection(SIZE, middle), None);
        // The hit radius is 0.45 of a cell, so 0.44 hits and 0.47 misses.
        let near = camera.to_screen(size, [7.44, 7.0]);
        assert_eq!(camera.intersection(SIZE, near), Some([7, 7]));
        let far = camera.to_screen(size, [7.47, 7.0]);
        assert_eq!(camera.intersection(SIZE, far), None);
    }

    #[test]
    fn a_click_outside_the_board_has_no_intersection() {
        let camera = Camera::fit(SIZE);
        let size = [SIZE[0] as f32, SIZE[1] as f32];
        for point in [[-0.5, 3.0], [14.5, 3.0], [3.0, -1.0], [3.0, 15.0]] {
            let screen = camera.to_screen(size, point);
            assert_eq!(camera.intersection(SIZE, screen), None, "point {point:?}");
        }
    }
}
