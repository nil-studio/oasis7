use super::*;

// Stroke widths are authored logical pixels. Endpoints remain projected world
// coordinates; only the perpendicular thickness enters the shared pixel unit.
pub(super) fn thickness(authored: f32, width: f64, height: f64, dx: f64, dy: f64) -> f32 {
    let pixel = pixel_unit(width, height);
    let length = dx.hypot(dy);
    let unit = if length > 0.0 {
        1.0 / ((dy / length / pixel.x).hypot(dx / length / pixel.y))
    } else {
        pixel.min_element()
    };
    (f64::from(authored.max(1.0)) * unit) as f32
}

pub(super) fn pixel_unit(width: f64, height: f64) -> bevy::math::DVec2 {
    pixel_pipeline::canvas_layout(width, height)
        .map(|layout| layout.css / layout.frame.as_dvec2() * layout.scale as f64)
        .unwrap_or(bevy::math::DVec2::ONE)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stroke_normal_uses_actual_axis_unit_and_keeps_a_logical_pixel_minimum() {
        assert_eq!(thickness(1.5, 1280.0, 720.0, 1.0, 0.0), 3.0);
        assert_eq!(thickness(1.5, 1280.0, 720.0, 0.0, 1.0), 3.0);
        assert_eq!(thickness(0.8, 3840.0, 2160.0, 1.0, 0.0), 6.0);
    }
}
