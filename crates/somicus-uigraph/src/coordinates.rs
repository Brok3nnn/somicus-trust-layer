//! Canonical coordinate systems and multi-monitor / mixed-DPI transformations.
//!
//! The canonical coordinate space uses global physical pixels
//! of the virtual desktop. Overlay windows convert to display-local logical coordinates
//! at render time using their display's scale factor.

use serde::{Deserialize, Serialize};

use crate::error::UiGraphError;
use crate::geometry::{PhysicalPoint, PhysicalRect};
use crate::graph::DisplayInfo;

/// A 2D point in logical (DPI-scaled / DIP) coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct LogicalPoint {
    /// X coordinate in logical pixels.
    pub x: f64,
    /// Y coordinate in logical pixels.
    pub y: f64,
}

impl LogicalPoint {
    /// Creates a new logical point.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A 2D rectangle in logical (DPI-scaled / DIP) coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct LogicalRect {
    /// X origin in logical pixels.
    pub x: f64,
    /// Y origin in logical pixels.
    pub y: f64,
    /// Width in logical pixels.
    pub width: f64,
    /// Height in logical pixels.
    pub height: f64,
}

impl LogicalRect {
    /// Creates a new logical rectangle.
    #[must_use]
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Multi-monitor and mixed-DPI coordinate transformer.
#[derive(Debug, Clone, Default)]
pub struct CoordinateTransformer;

impl CoordinateTransformer {
    /// Finds the display containing the given physical point.
    #[must_use]
    pub fn find_display(displays: &[DisplayInfo], point: PhysicalPoint) -> Option<&DisplayInfo> {
        displays.iter().find(|d| d.bounds_px.contains_point(point))
    }

    /// Converts a global physical screen point to a display-local logical point.
    ///
    /// # Errors
    /// Returns `UiGraphError::DisplayNotFound` if the display ID is invalid or
    /// scale factor is non-positive.
    pub fn physical_to_display_logical(
        display: &DisplayInfo,
        point: PhysicalPoint,
    ) -> Result<LogicalPoint, UiGraphError> {
        if display.scale_factor <= 0.0 {
            return Err(UiGraphError::InvalidCoordinate(
                "scale_factor must be positive".to_string(),
            ));
        }

        // In f64 the difference of two i32 values is exact; in i32 it can overflow.
        let rel_x = f64::from(point.x) - f64::from(display.bounds_px.x);
        let rel_y = f64::from(point.y) - f64::from(display.bounds_px.y);

        Ok(LogicalPoint {
            x: rel_x / display.scale_factor,
            y: rel_y / display.scale_factor,
        })
    }

    /// Converts a display-local logical point back to a global physical screen point.
    ///
    /// # Errors
    /// Returns `UiGraphError::InvalidCoordinate` if scale factor is non-positive.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "`as` from f64 saturates at the ends of i32 and maps NaN to 0, which is what the caller needs"
    )]
    pub fn display_logical_to_physical(
        display: &DisplayInfo,
        point: LogicalPoint,
    ) -> Result<PhysicalPoint, UiGraphError> {
        if display.scale_factor <= 0.0 {
            return Err(UiGraphError::InvalidCoordinate(
                "scale_factor must be positive".to_string(),
            ));
        }

        let px_x = (point.x * display.scale_factor).round() as i32;
        let px_y = (point.y * display.scale_factor).round() as i32;

        Ok(PhysicalPoint {
            x: display.bounds_px.x.saturating_add(px_x),
            y: display.bounds_px.y.saturating_add(px_y),
        })
    }

    /// Converts a DOM viewport coordinate (CSS px relative to web area) to a global physical point.
    ///
    /// # Arguments
    /// * `web_area_bounds_px` - The physical bounds of the browser's web area on screen.
    /// * `dom_point` - Coordinate in CSS pixels relative to the DOM viewport.
    /// * `dpr` - Device pixel ratio (or browser zoom factor).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "`as` from f64 saturates at the ends of i32 and maps NaN to 0, which is what the caller needs"
    )]
    pub fn dom_viewport_to_physical(
        web_area_bounds_px: PhysicalRect,
        dom_point: LogicalPoint,
        dpr: f64,
    ) -> PhysicalPoint {
        let effective_dpr = if dpr <= 0.0 { 1.0 } else { dpr };
        let offset_x = (dom_point.x * effective_dpr).round() as i32;
        let offset_y = (dom_point.y * effective_dpr).round() as i32;

        PhysicalPoint {
            x: web_area_bounds_px.x.saturating_add(offset_x),
            y: web_area_bounds_px.y.saturating_add(offset_y),
        }
    }

    /// Converts a global physical point to DOM viewport CSS coordinates, if within web area bounds.
    #[must_use]
    pub fn physical_to_dom_viewport(
        web_area_bounds_px: PhysicalRect,
        point: PhysicalPoint,
        dpr: f64,
    ) -> Option<LogicalPoint> {
        if !web_area_bounds_px.contains_point(point) {
            return None;
        }

        let effective_dpr = if dpr <= 0.0 { 1.0 } else { dpr };
        let rel_x = f64::from(point.x) - f64::from(web_area_bounds_px.x);
        let rel_y = f64::from(point.y) - f64::from(web_area_bounds_px.y);

        Some(LogicalPoint {
            x: rel_x / effective_dpr,
            y: rel_y / effective_dpr,
        })
    }
}
