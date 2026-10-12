//! Cursor feedback is UI output; native hosts translate this vocabulary.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Cursor {
    #[default]
    Default,
    Grab,
    Grabbing,
    Move,
    ResizeEW,
    ResizeNS,
    ResizeNESW,
    ResizeNWSE,
    Text,
    Crosshair,
    Wait,
}
pub fn resize_for_direction(direction: [f32; 2]) -> Cursor {
    if direction.iter().any(|v| !v.is_finite()) || direction[0].abs() + direction[1].abs() < 1e-6 {
        return Cursor::Default;
    }
    let angle = direction[1]
        .atan2(direction[0])
        .to_degrees()
        .rem_euclid(180.);
    if !(22.5..157.5).contains(&angle) {
        Cursor::ResizeEW
    } else if angle < 67.5 {
        Cursor::ResizeNWSE
    } else if angle < 112.5 {
        Cursor::ResizeNS
    } else {
        Cursor::ResizeNESW
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotated_and_mirrored_handles_keep_their_screen_direction() {
        assert_eq!(resize_for_direction([1., 0.]), Cursor::ResizeEW);
        assert_eq!(resize_for_direction([-1., -1.]), Cursor::ResizeNWSE);
        assert_eq!(resize_for_direction([0., -1.]), Cursor::ResizeNS);
        assert_eq!(resize_for_direction([-1., 1.]), Cursor::ResizeNESW);
        assert_eq!(resize_for_direction([0., 0.]), Cursor::Default);
    }
}
