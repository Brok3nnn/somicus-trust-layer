//! Canonical screen geometry and coordinate primitives.
//!
//! Per Perception §3, all coordinates in the UI Graph use global physical pixels of the virtual desktop.

use serde::{Deserialize, Serialize};

/// A 2D point in global physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PhysicalPoint {
    /// X coordinate in physical pixels.
    pub x: i32,
    /// Y coordinate in physical pixels.
    pub y: i32,
}

impl PhysicalPoint {
    /// Creates a new physical point.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in global physical screen pixels (Perception §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PhysicalRect {
    /// X origin in physical pixels.
    pub x: i32,
    /// Y origin in physical pixels.
    pub y: i32,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
}

impl PhysicalRect {
    /// Creates a new physical rectangle.
    #[must_use]
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns the area in physical pixels squared.
    #[must_use]
    pub const fn area(&self) -> u64 {
        (self.width as u64) * (self.height as u64)
    }

    /// Returns true if this rectangle has zero area.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Returns the center point in physical pixels, saturating at the edge of the coordinate space.
    #[must_use]
    pub const fn center(&self) -> PhysicalPoint {
        PhysicalPoint {
            x: self.x.saturating_add_unsigned(self.width / 2),
            y: self.y.saturating_add_unsigned(self.height / 2),
        }
    }

    /// Returns true if this rectangle contains the given point.
    #[must_use]
    pub const fn contains_point(&self, point: PhysicalPoint) -> bool {
        let max_x = self.x.saturating_add_unsigned(self.width);
        let max_y = self.y.saturating_add_unsigned(self.height);
        point.x >= self.x && point.x < max_x && point.y >= self.y && point.y < max_y
    }

    /// Computes the intersection rectangle of two bounding boxes, if any.
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self
            .x
            .saturating_add_unsigned(self.width)
            .min(other.x.saturating_add_unsigned(other.width));
        let y2 = self
            .y
            .saturating_add_unsigned(self.height)
            .min(other.y.saturating_add_unsigned(other.height));

        if x2 > x1 && y2 > y1 {
            // The distance between two `i32` values always fits in `u32`, unlike their difference.
            Some(Self {
                x: x1,
                y: y1,
                width: x2.abs_diff(x1),
                height: y2.abs_diff(y1),
            })
        } else {
            None
        }
    }

    /// Computes the Intersection over Union (`IoU`) with another bounding box.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "a ratio of areas needs no more precision than an f32 has"
    )]
    pub fn iou(&self, other: &Self) -> f32 {
        let intersection_area = self.intersection(other).map_or(0, |r| r.area());
        if intersection_area == 0 {
            return 0.0;
        }
        // Two areas of up to `u32::MAX` squared each overflow `u64` when added.
        let union_area =
            u128::from(self.area()) + u128::from(other.area()) - u128::from(intersection_area);
        if union_area == 0 {
            0.0
        } else {
            (intersection_area as f32) / (union_area as f32)
        }
    }
}
