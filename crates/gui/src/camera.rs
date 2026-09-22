//! The view: which part of the board the window shows.
//!
//! Board space uses one unit per cell, with the top-left intersection at
//! `(0, 0)` and y growing downward. Screen space uses physical pixels.
//!
//! The board is drawn into a viewport, which is the part of the window that the
//! side panel leaves free. Every projection takes that viewport, so the board is
//! centred in the area the user can actually see.

/// How far the slab reaches from the centre line, in cells.
const SLAB_HALF: f32 = 7.85;
/// The distance the whole board plus a wide margin needs, in cells. The margin
/// is what lets the table show around the slab, which is what makes the board
/// look like an object on a surface rather than a texture filling a window.
const FIT_SPAN: f32 = 18.8;
/// The furthest the view may zoom in, as a multiple of the fitted scale.
const MAX_ZOOM: f32 = 40.0;
/// How close a click must be to an intersection to count, in cells.
const HIT_RADIUS: f32 = 0.45;
/// The smallest scale the view may take, so that it can never vanish.
const MIN_PIXELS_PER_CELL: f32 = 4.0;

/// The rectangle the board is drawn into, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// The size of the whole drawing surface, in physical pixels. The projection
    /// needs it for clip space, and it cannot be worked out from the area of the
    /// board, because panels take space at the top and at one side.
    pub frame: [f32; 2],
    /// The left edge.
    pub x: f32,
    /// The top edge.
    pub y: f32,
    /// The width.
    pub width: f32,
    /// The height.
    pub height: f32,
}

impl Viewport {
    /// A viewport at the top left of the window.
    pub fn window(width: u32, height: u32) -> Viewport {
        Viewport {
            frame: [width as f32, height as f32],
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
        }
    }

    /// The wider and taller of the two, for the fitted scale.
    fn smaller_side(&self) -> f32 {
        self.width.min(self.height).max(1.0)
    }
}

/// Where the view is looking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// The board point at the centre of the viewport, in cells.
    pub centre: [f32; 2],
    /// The scale of the view.
    pub pixels_per_cell: f32,
    /// True when the board is shown from the other side.
    pub flipped: bool,
}

impl Camera {
    /// A view that fits the whole board into a viewport.
    pub fn fit(viewport: Viewport) -> Camera {
        Camera {
            centre: [7.0, 7.0],
            pixels_per_cell: Camera::fit_scale(viewport),
            flipped: false,
        }
    }

    /// The scale at which the whole board fits the viewport.
    pub fn fit_scale(viewport: Viewport) -> f32 {
        (viewport.smaller_side() / FIT_SPAN).max(MIN_PIXELS_PER_CELL)
    }

    /// The lowest and highest permitted scale for a viewport.
    pub fn zoom_range(viewport: Viewport) -> (f32, f32) {
        let fit = Camera::fit_scale(viewport);
        (fit, fit * MAX_ZOOM)
    }

    /// A board point in cells to a screen point in pixels.
    pub fn to_screen(self, viewport: Viewport, board: [f32; 2]) -> [f32; 2] {
        let mut dx = board[0] - self.centre[0];
        let mut dy = board[1] - self.centre[1];
        if self.flipped {
            dx = -dx;
            dy = -dy;
        }
        [
            viewport.x + viewport.width * 0.5 + dx * self.pixels_per_cell,
            viewport.y + viewport.height * 0.5 + dy * self.pixels_per_cell,
        ]
    }

    /// A screen point in pixels to a board point in cells.
    pub fn to_board(self, viewport: Viewport, screen: [f32; 2]) -> [f32; 2] {
        let mut dx = (screen[0] - viewport.x - viewport.width * 0.5) / self.pixels_per_cell;
        let mut dy = (screen[1] - viewport.y - viewport.height * 0.5) / self.pixels_per_cell;
        if self.flipped {
            dx = -dx;
            dy = -dy;
        }
        [self.centre[0] + dx, self.centre[1] + dy]
    }

    /// Zoom by `factor`, keeping the board point under `anchor` in place.
    ///
    /// Near the edge of the board the pan limit can win over the anchor: a view
    /// that stayed exactly under the pointer would leave the board behind.
    pub fn zoom_about(&mut self, viewport: Viewport, anchor: [f32; 2], factor: f32) {
        let (low, high) = Camera::zoom_range(viewport);
        let pixels = (self.pixels_per_cell * factor).clamp(low, high);
        let before = self.to_board(viewport, anchor);
        self.pixels_per_cell = pixels;
        let after = self.to_board(viewport, anchor);
        self.centre[0] += before[0] - after[0];
        self.centre[1] += before[1] - after[1];
        self.clamp(viewport);
    }

    /// Move the view by a screen-space delta.
    pub fn pan(&mut self, viewport: Viewport, delta: [f32; 2]) {
        let sign = if self.flipped { -1.0 } else { 1.0 };
        self.centre[0] -= sign * delta[0] / self.pixels_per_cell;
        self.centre[1] -= sign * delta[1] / self.pixels_per_cell;
        self.clamp(viewport);
    }

    /// Return to the fitted view.
    pub fn reset(&mut self, viewport: Viewport) {
        *self = Camera::fit(viewport);
    }

    /// Keep the board on screen, and centre it when the whole board fits.
    pub fn clamp(&mut self, viewport: Viewport) {
        let (low, high) = Camera::zoom_range(viewport);
        self.pixels_per_cell = self.pixels_per_cell.clamp(low, high);
        let half_x = viewport.width / (2.0 * self.pixels_per_cell);
        let half_y = viewport.height / (2.0 * self.pixels_per_cell);
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
    pub fn intersection(&self, viewport: Viewport, screen: [f32; 2]) -> Option<[u8; 2]> {
        let board = self.to_board(viewport, screen);
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

    /// A viewport with a side panel, as the window has.
    fn panel_viewport() -> Viewport {
        Viewport {
            frame: [1200.0, 900.0],
            x: 0.0,
            y: 28.0,
            width: 950.0,
            height: 872.0,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn a_board_point_survives_a_round_trip_through_the_screen() {
        let viewport = panel_viewport();
        let mut camera = Camera::fit(viewport);
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
                let screen = camera.to_screen(viewport, point);
                let back = camera.to_board(viewport, screen);
                assert!(close(back[0], point[0]) && close(back[1], point[1]));
            }
        }
    }

    #[test]
    fn the_board_is_centred_in_the_viewport_not_the_window() {
        let viewport = panel_viewport();
        let camera = Camera::fit(viewport);
        let middle = camera.to_screen(viewport, [7.0, 7.0]);
        assert!(close(middle[0], viewport.x + viewport.width / 2.0));
        assert!(close(middle[1], viewport.y + viewport.height / 2.0));
    }

    #[test]
    fn zoom_keeps_the_anchor_under_the_pointer() {
        let viewport = panel_viewport();
        let mut camera = Camera::fit(viewport);
        // An anchor near the middle of the viewport. There the pan limit does not
        // engage, so the point under the pointer must not move at all.
        let anchor = [viewport.width * 0.5 + 30.0, viewport.height * 0.5 + 20.0];
        let before = camera.to_board(viewport, anchor);
        camera.zoom_about(viewport, anchor, 2.5);
        let after = camera.to_board(viewport, anchor);
        assert!(close(before[0], after[0]), "{before:?} against {after:?}");
        assert!(close(before[1], after[1]), "{before:?} against {after:?}");
    }

    #[test]
    fn the_pan_limit_wins_over_the_anchor_near_an_edge() {
        let viewport = panel_viewport();
        let mut camera = Camera::fit(viewport);
        let anchor = [viewport.width - 20.0, 40.0];
        let before = camera.to_board(viewport, anchor);
        camera.zoom_about(viewport, anchor, 3.0);
        let after = camera.to_board(viewport, anchor);
        assert!(!close(before[0], after[0]), "the anchor should have slid");

        let half = viewport.width / (2.0 * camera.pixels_per_cell);
        assert!(
            camera.centre[0] <= 7.0 + (SLAB_HALF - half) + 1e-3,
            "the view left its limits: centre {}",
            camera.centre[0]
        );
    }

    #[test]
    fn zoom_is_clamped_to_the_range() {
        let viewport = panel_viewport();
        let mut camera = Camera::fit(viewport);
        let (low, high) = Camera::zoom_range(viewport);
        for _ in 0..40 {
            camera.zoom_about(viewport, [400.0, 400.0], 1.5);
        }
        assert!(close(camera.pixels_per_cell, high));
        for _ in 0..80 {
            camera.zoom_about(viewport, [400.0, 400.0], 0.7);
        }
        assert!(close(camera.pixels_per_cell, low));
    }

    #[test]
    fn a_fitted_view_is_centred() {
        let viewport = panel_viewport();
        let mut camera = Camera::fit(viewport);
        camera.centre = [0.0, 0.0];
        camera.clamp(viewport);
        assert!(close(camera.centre[0], 7.0));
        assert!(close(camera.centre[1], 7.0));
    }

    #[test]
    fn panning_cannot_push_the_board_off_screen() {
        let viewport = panel_viewport();
        let mut camera = Camera::fit(viewport);
        camera.zoom_about(viewport, [400.0, 400.0], 6.0);
        for _ in 0..200 {
            camera.pan(viewport, [500.0, 500.0]);
        }
        let half_x = viewport.width / (2.0 * camera.pixels_per_cell);
        let half_y = viewport.height / (2.0 * camera.pixels_per_cell);
        assert!(camera.centre[0] >= 7.0 - (SLAB_HALF - half_x) - 1e-3);
        assert!(camera.centre[1] >= 7.0 - (SLAB_HALF - half_y) - 1e-3);
    }

    #[test]
    fn the_centre_intersection_is_found() {
        let viewport = panel_viewport();
        let camera = Camera::fit(viewport);
        let screen = camera.to_screen(viewport, [7.0, 7.0]);
        assert_eq!(camera.intersection(viewport, screen), Some([7, 7]));
    }

    #[test]
    fn a_click_between_two_stones_misses_both() {
        let viewport = panel_viewport();
        let camera = Camera::fit(viewport);
        let middle = camera.to_screen(viewport, [7.5, 7.5]);
        assert_eq!(camera.intersection(viewport, middle), None);
        // The hit radius is 0.45 of a cell, so 0.44 hits and 0.47 misses.
        let near = camera.to_screen(viewport, [7.44, 7.0]);
        assert_eq!(camera.intersection(viewport, near), Some([7, 7]));
        let far = camera.to_screen(viewport, [7.47, 7.0]);
        assert_eq!(camera.intersection(viewport, far), None);
    }

    #[test]
    fn a_click_outside_the_board_has_no_intersection() {
        let viewport = panel_viewport();
        let camera = Camera::fit(viewport);
        for point in [[-0.5, 3.0], [14.5, 3.0], [3.0, -1.0], [3.0, 15.0]] {
            let screen = camera.to_screen(viewport, point);
            assert_eq!(
                camera.intersection(viewport, screen),
                None,
                "point {point:?}"
            );
        }
    }
}
