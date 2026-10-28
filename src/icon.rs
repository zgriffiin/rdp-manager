// App icon, drawn procedurally so the window icon and the .exe icon stay identical.
// Shared with build.rs through `include!`, so it uses only the standard library.

fn rounded_rect(x: f32, y: f32, (x0, y0, x1, y1): (f32, f32, f32, f32), r: f32) -> bool {
    if x < x0 || x > x1 || y < y0 || y > y1 {
        return false;
    }
    // max/min instead of clamp: rounding can make x0 + r exceed x1 - r for pill shapes.
    let cx = x.max(x0 + r).min(x1 - r);
    let cy = y.max(y0 + r).min(y1 - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

fn triangle(x: f32, y: f32, p: [(f32, f32); 3]) -> bool {
    let side = |a: (f32, f32), b: (f32, f32)| (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0);
    let (d1, d2, d3) = (side(p[0], p[1]), side(p[1], p[2]), side(p[2], p[0]));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

/// Returns premultiplied (r, g, b, a) in 0..=1 for a point in unit coordinates.
fn icon_sample(x: f32, y: f32) -> [f32; 4] {
    if !rounded_rect(x, y, (0.0, 0.0, 1.0, 1.0), 0.22) {
        return [0.0; 4];
    }
    // Diagonal blue gradient background.
    let t = ((x + y) / 2.0).clamp(0.0, 1.0);
    let lerp = |a: f32, b: f32| (a + (b - a) * t) / 255.0;
    let mut c = [lerp(59.0, 29.0), lerp(130.0, 78.0), lerp(246.0, 216.0), 1.0];

    let outer = rounded_rect(x, y, (0.17, 0.22, 0.83, 0.66), 0.06);
    let screen = rounded_rect(x, y, (0.23, 0.28, 0.77, 0.60), 0.02);
    let neck = rounded_rect(x, y, (0.45, 0.65, 0.55, 0.74), 0.0);
    let base = rounded_rect(x, y, (0.31, 0.72, 0.69, 0.79), 0.035);
    let arrow = rounded_rect(x, y, (0.35, 0.415, 0.55, 0.465), 0.0)
        || triangle(x, y, [(0.53, 0.35), (0.66, 0.44), (0.53, 0.53)]);

    let white_alpha = if (outer && !screen) || neck || base || arrow {
        1.0
    } else if screen {
        0.16
    } else {
        0.0
    };
    for ch in c.iter_mut().take(3) {
        *ch = *ch * (1.0 - white_alpha) + white_alpha;
    }
    c
}

/// Renders the icon as straight (unmultiplied) RGBA bytes, `size * size * 4` long.
pub fn render_icon(size: u32) -> Vec<u8> {
    const SS: u32 = 4; // supersampling per axis for anti-aliasing
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    let s = icon_sample(x, y);
                    for i in 0..4 {
                        acc[i] += s[i];
                    }
                }
            }
            let n = (SS * SS) as f32;
            let a = acc[3] / n;
            for ch in &acc[..3] {
                let v = if a > 0.0 { ch / n / a } else { 0.0 };
                out.push((v * 255.0).round().clamp(0.0, 255.0) as u8);
            }
            out.push((a * 255.0).round() as u8);
        }
    }
    out
}
