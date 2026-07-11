#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

const DARK: [Rgb; 3] = [
    Rgb(0xff, 0x5a, 0x5f),
    Rgb(0xf6, 0xc3, 0x44),
    Rgb(0x43, 0xd1, 0x7a),
];
const LIGHT: [Rgb; 3] = [
    Rgb(0xc6, 0x28, 0x28),
    Rgb(0x8a, 0x65, 0x00),
    Rgb(0x14, 0x7a, 0x3f),
];

pub fn quota_color(percent: f64, dark: bool) -> Rgb {
    let value = percent.clamp(0.0, 100.0);
    let palette = if dark { DARK } else { LIGHT };
    let (a, b, t) = if value <= 50.0 {
        (palette[0], palette[1], value / 50.0)
    } else {
        (palette[1], palette[2], (value - 50.0) / 50.0)
    };
    Rgb(mix(a.0, b.0, t), mix(a.1, b.1, t), mix(a.2, b.2, t))
}

fn mix(a: u8, b: u8, t: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * t).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_color_anchors_and_clamping() {
        assert_eq!(quota_color(0.0, true), DARK[0]);
        assert_eq!(quota_color(50.0, true), DARK[1]);
        assert_eq!(quota_color(100.0, true), DARK[2]);
        assert_eq!(quota_color(-1.0, false), LIGHT[0]);
        assert_eq!(quota_color(101.0, false), LIGHT[2]);
    }
}
