//! Coordinates come from other apps' accessibility trees and web pages, so they can sit anywhere in `i32`. At the edge
//! of that range the geometry saturates or widens instead of overflowing, which panics in debug builds and wraps to
//! the far side of the screen in release builds.

use somicus_uigraph::{
    CoordinateTransformer, DisplayInfo, LogicalPoint, PhysicalPoint, PhysicalRect,
};

fn display_at(x: i32, scale_factor: f64) -> DisplayInfo {
    DisplayInfo {
        display_id: 1,
        bounds_px: PhysicalRect::new(x, 0, 1920, 1080),
        scale_factor,
        work_area_px: PhysicalRect::new(x, 0, 1920, 1080),
        is_primary: true,
    }
}

#[test]
fn the_centre_of_a_rect_at_the_edge_saturates() {
    assert_eq!(
        PhysicalRect::new(i32::MAX - 1, i32::MIN, 4, u32::MAX).center(),
        PhysicalPoint::new(i32::MAX, -1)
    );
    // A width above i32::MAX is still a width, not a negative offset.
    assert_eq!(
        PhysicalRect::new(i32::MIN, 0, u32::MAX, 2).center(),
        PhysicalPoint::new(-1, 1)
    );
}

#[test]
fn rects_spanning_the_whole_range_intersect_without_overflow() {
    let whole = PhysicalRect::new(i32::MIN, i32::MIN, u32::MAX, u32::MAX);
    assert_eq!(
        whole.intersection(&whole),
        Some(PhysicalRect::new(i32::MIN, i32::MIN, u32::MAX, u32::MAX))
    );
    assert!((whole.iou(&whole) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn points_far_from_a_display_convert_without_overflow() -> Result<(), somicus_uigraph::UiGraphError>
{
    let logical = CoordinateTransformer::physical_to_display_logical(
        &display_at(1, 1.0),
        PhysicalPoint::new(i32::MIN, i32::MAX),
    )?;
    assert!((logical.x - (f64::from(i32::MIN) - 1.0)).abs() < 0.5);
    assert!((logical.y - f64::from(i32::MAX)).abs() < 0.5);

    let physical = CoordinateTransformer::display_logical_to_physical(
        &display_at(i32::MAX - 1, 1.0),
        LogicalPoint::new(10.0, -10.0),
    )?;
    assert_eq!(physical, PhysicalPoint::new(i32::MAX, -10));
    Ok(())
}

#[test]
fn dom_points_at_the_edge_convert_without_overflow() {
    let web_area = PhysicalRect::new(i32::MAX - 1, 0, 100, 100);
    assert_eq!(
        CoordinateTransformer::dom_viewport_to_physical(
            web_area,
            LogicalPoint::new(10.0, 10.0),
            1.0
        ),
        PhysicalPoint::new(i32::MAX, 10)
    );
    let whole = PhysicalRect::new(i32::MIN, 0, u32::MAX, 100);
    let viewport = CoordinateTransformer::physical_to_dom_viewport(
        whole,
        PhysicalPoint::new(i32::MAX - 1, 5),
        1.0,
    );
    assert_eq!(
        viewport,
        Some(LogicalPoint::new(f64::from(u32::MAX) - 1.0, 5.0))
    );
}
