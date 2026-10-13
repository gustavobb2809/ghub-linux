//! Ícone simbólico do ghub, rasterizado em código (ARGB32, ordem de rede): um
//! anel com um mouse visto de cima (corpo em pílula, roda e divisória vazadas),
//! a mesma silhueta de `packaging/icons/org.ghublinux.Gui-symbolic.svg`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Normal,
    LowBattery,
    Charging,
    Disconnected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixmap {
    pub size: u32,
    /// `size * size` pixels, cada um em 4 bytes: A, R, G, B.
    pub argb: Vec<u8>,
}

pub const SIZES: [u32; 3] = [22, 32, 48];

const GLYPH: [u8; 3] = [0xeb, 0xdb, 0xb2];
const RED: [u8; 3] = [0xfb, 0x49, 0x34];
const GREEN: [u8; 3] = [0xb8, 0xbb, 0x26];
/// Subamostras por eixo (4x4 = 16 por pixel) para suavizar as bordas.
const SS: u32 = 4;

fn in_circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> bool {
    let (dx, dy) = (x - cx, y - cy);
    dx * dx + dy * dy <= r * r
}

/// Distância de (x, y) ao segmento vertical (cx, y0..y1): base das "cápsulas".
fn dist_vseg(x: f32, y: f32, cx: f32, y0: f32, y1: f32) -> f32 {
    let dy = if y < y0 { y0 - y } else if y > y1 { y - y1 } else { 0.0 };
    ((x - cx) * (x - cx) + dy * dy).sqrt()
}

/// O emblema num quadrado unitário: anel + mouse em pílula, com roda e divisória vazadas.
fn glyph_at(x: f32, y: f32) -> bool {
    let d = ((x - 0.5) * (x - 0.5) + (y - 0.5) * (y - 0.5)).sqrt();
    let ring = (0.39..=0.48).contains(&d);
    let body = dist_vseg(x, y, 0.5, 0.40, 0.60) <= 0.17;
    let wheel = dist_vseg(x, y, 0.5, 0.335, 0.41) <= 0.035;
    let divider = (y - 0.55).abs() <= 0.025;
    ring || (body && !wheel && !divider)
}

fn sample(state: IconState, x: f32, y: f32) -> Option<[u8; 3]> {
    match state {
        IconState::LowBattery if in_circle(x, y, 0.78, 0.78, 0.2) => return Some(RED),
        IconState::Charging if in_circle(x, y, 0.78, 0.78, 0.2) => return Some(GREEN),
        _ => {}
    }
    glyph_at(x, y).then_some(GLYPH)
}

pub fn render(state: IconState, size: u32) -> Pixmap {
    let mut argb = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut covered, mut r, mut g, mut b) = (0u32, 0u32, 0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    if let Some(c) = sample(state, x, y) {
                        covered += 1;
                        r += u32::from(c[0]);
                        g += u32::from(c[1]);
                        b += u32::from(c[2]);
                    }
                }
            }
            if covered == 0 {
                argb.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let mut alpha = covered * 255 / (SS * SS);
            if state == IconState::Disconnected {
                alpha = alpha * 2 / 5;
            }
            argb.extend_from_slice(&[
                alpha as u8,
                (r / covered) as u8,
                (g / covered) as u8,
                (b / covered) as u8,
            ]);
        }
    }
    Pixmap { size, argb }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * p.size + x) * 4) as usize;
        [p.argb[i], p.argb[i + 1], p.argb[i + 2], p.argb[i + 3]]
    }

    #[test]
    fn has_the_expected_buffer_size_for_every_size() {
        for size in SIZES {
            for state in [
                IconState::Normal,
                IconState::LowBattery,
                IconState::Charging,
                IconState::Disconnected,
            ] {
                let p = render(state, size);
                assert_eq!(p.size, size);
                assert_eq!(p.argb.len(), (size * size * 4) as usize);
            }
        }
    }

    #[test]
    fn corners_are_transparent() {
        let p = render(IconState::Normal, 48);
        assert_eq!(px(&p, 0, 0)[0], 0);
        assert_eq!(px(&p, 47, 0)[0], 0);
        assert_eq!(px(&p, 0, 47)[0], 0);
    }

    #[test]
    fn ring_and_body_are_opaque_glyph_color() {
        let p = render(IconState::Normal, 48);
        assert_eq!(px(&p, 24, 3), [255, 0xeb, 0xdb, 0xb2], "anel (topo)");
        assert_eq!(px(&p, 24, 33), [255, 0xeb, 0xdb, 0xb2], "corpo do mouse");
        assert_eq!(px(&p, 3, 24), [255, 0xeb, 0xdb, 0xb2], "anel (esquerda)");
    }

    #[test]
    fn gap_wheel_and_divider_are_cut_out() {
        let p = render(IconState::Normal, 48);
        assert_eq!(px(&p, 24, 7)[0], 0, "vão entre anel e corpo");
        assert_eq!(px(&p, 24, 18)[0], 0, "roda");
        assert_eq!(px(&p, 24, 26)[0], 0, "divisória");
        assert_eq!(px(&p, 24, 24)[0], 255, "corpo entre a roda e a divisória");
    }

    #[test]
    fn low_battery_has_a_red_badge_and_charging_a_green_one() {
        let low = render(IconState::LowBattery, 48);
        let charging = render(IconState::Charging, 48);
        let normal = render(IconState::Normal, 48);
        assert_eq!(px(&low, 37, 37), [255, 0xfb, 0x49, 0x34]);
        assert_eq!(px(&charging, 37, 37), [255, 0xb8, 0xbb, 0x26]);
        assert_ne!(px(&normal, 37, 37), px(&low, 37, 37));
    }

    #[test]
    fn disconnected_is_dimmer_but_visible() {
        let normal = render(IconState::Normal, 48);
        let off = render(IconState::Disconnected, 48);
        assert_eq!(px(&normal, 24, 33)[0], 255);
        let alpha = px(&off, 24, 33)[0];
        assert!((90..=110).contains(&alpha), "alpha = {alpha}");
    }

    #[test]
    fn rendering_is_deterministic_and_states_differ() {
        assert_eq!(render(IconState::Normal, 22), render(IconState::Normal, 22));
        let all = [
            render(IconState::Normal, 22),
            render(IconState::LowBattery, 22),
            render(IconState::Charging, 22),
            render(IconState::Disconnected, 22),
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j], "{i} vs {j}");
            }
        }
    }
}
