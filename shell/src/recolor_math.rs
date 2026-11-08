// Цвета перекраски: разбор и смешивание, производные оттенки палитры (те же, что считает шелл), обои из палитры.
// Без файлов и настроек: общий код шелла и инструментов разработки (yami-dev).

use image::{Rgb, RgbImage};
use std::collections::HashMap;

// ── цвета ───────────────────────────────────────────────────────────────────
/// округление до ближайшего, половина — к чётному: иначе оттенки разойдутся с уже записанными файлами
pub fn round_even(v: f64) -> f64 {
    if (v - v.trunc()).abs() == 0.5 {
        2.0 * (v / 2.0).round()
    } else {
        v.round()
    }
}

pub fn rgb(h: &str) -> (i64, i64, i64) {
    let h = h.trim_start_matches('#');
    let n = |i: usize| h.get(i..i + 2).and_then(|s| i64::from_str_radix(s, 16).ok()).unwrap_or(0);
    (n(0), n(2), n(4))
}

pub fn hexc(r: f64, g: f64, b: f64) -> String {
    let c = |v: f64| round_even(v).clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", c(r), c(g), c(b))
}

pub fn mix(a: &str, b: &str, t: f64) -> String {
    let (a, b) = (rgb(a), rgb(b));
    let m = |x: i64, y: i64| x as f64 + (y as f64 - x as f64) * t;
    hexc(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

fn hue(r: f64, g: f64, b: f64, maxc: f64, range: f64) -> f64 {
    let (rc, gc, bc) = ((maxc - r) / range, (maxc - g) / range, (maxc - b) / range);
    let h = if r == maxc {
        bc - gc
    } else if g == maxc {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    (h / 6.0).rem_euclid(1.0)
}

fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (maxc, minc) = (r.max(g).max(b), r.min(g).min(b));
    if minc == maxc {
        return (0.0, 0.0, maxc);
    }
    (hue(r, g, b, maxc, maxc - minc), (maxc - minc) / maxc, maxc)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (v, v, v);
    }
    let i = (h * 6.0).trunc();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match (i as i64).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// (оттенок, светлота, насыщенность)
pub fn rgb_to_hls(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (maxc, minc) = (r.max(g).max(b), r.min(g).min(b));
    let l = (maxc + minc) / 2.0;
    if minc == maxc {
        return (0.0, l, 0.0);
    }
    let range = maxc - minc;
    let s = if l <= 0.5 { range / (maxc + minc) } else { range / (2.0 - maxc - minc) };
    (hue(r, g, b, maxc, range), l, s)
}

pub fn unit(h: &str) -> (f64, f64, f64) {
    let (r, g, b) = rgb(h);
    (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0)
}

pub fn scale_v(h: &str, k: f64) -> String {
    let (r, g, b) = unit(h);
    let (hh, mut s, mut v) = rgb_to_hsv(r, g, b);
    v *= k;
    // как Qt.lighter: что не влезло в яркость — убираем из насыщенности
    if v > 1.0 {
        s = (s - (v - 1.0)).max(0.0);
        v = 1.0;
    }
    let (r, g, b) = hsv_to_rgb(hh, s, v);
    hexc(r * 255.0, g * 255.0, b * 255.0)
}

/// палитра: те же имена, что в palette.json шелла (theme.rs)
#[derive(Clone, Default)]
pub struct P(pub HashMap<String, String>);

impl P {
    pub fn g(&self, k: &str) -> &str {
        self.0.get(k).map_or("#000000", |s| s.as_str())
    }
    pub fn light(&self) -> bool {
        self.0.get("mode").is_some_and(|m| m == "light")
    }
    /// самый контрастный текст: белый в тёмной теме, чёрный в светлой
    pub fn hi(&self) -> String {
        self.0.get("textHi").cloned().unwrap_or_else(|| if self.light() { "#000000".into() } else { "#ffffff".into() })
    }
    pub fn of(rows: Vec<(&str, String)>) -> P {
        P(rows.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
    pub fn parse(text: &str) -> Option<P> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        Some(P(v.as_object()?.iter().filter_map(|(k, x)| Some((k.clone(), x.as_str()?.to_string()))).collect()))
    }
}

/// те же оттенки, что выводит шелл (theme.rs), — для запуска без шелла
pub fn derive(accent: &str, tint: f64, light: bool) -> P {
    if light {
        return derive_light(accent, tint);
    }
    let bg = mix("#080707", accent, 0.035 * tint);
    let panel = mix("#0e0c0c", accent, 0.055 * tint);
    let (r, g, b) = unit(accent);
    let lig = rgb_to_hls(r, g, b).1;
    P::of(vec![
        ("surface", mix("#161313", accent, 0.08 * tint)),
        ("surfaceHi", mix("#1f1b1b", accent, 0.12 * tint)),
        ("codeBg", mix("#050404", accent, 0.02 * tint)),
        ("border", mix(&panel, accent, 0.3)),
        ("dim", mix(&panel, accent, 0.45)),
        ("accent", accent.to_string()),
        ("deep", scale_v(accent, 1.0 / 1.6)),
        ("glow", scale_v(accent, 1.0 / 1.15)),
        ("bright", scale_v(accent, 1.3)),
        ("selection", mix(&bg, accent, 0.55)),
        ("text", mix("#ebe5e4", accent, 0.05)),
        ("textDim", mix("#8c8282", accent, 0.12)),
        ("textFaint", mix("#5c5252", accent, 0.1)),
        ("onAccent", if lig > 0.6 { "#0b0909" } else { "#ffffff" }.to_string()),
        ("textHi", "#ffffff".to_string()),
        ("ok", "#78c878".to_string()),
        ("warn", "#ffaa28".to_string()),
        ("mode", "dark".to_string()),
        ("bg", bg),
        ("panel", panel),
    ])
}

/// светлая тема как в theme.rs (Theme::light): акцент темнеет до читаемого на бумаге
fn derive_light(accent: &str, tint: f64) -> P {
    let lig = |h: &str| {
        let (r, g, b) = unit(h);
        rgb_to_hls(r, g, b).1
    };
    let mut a = accent.to_string();
    for _ in 0..12 {
        if lig(&a) <= 0.46 {
            break;
        }
        a = scale_v(&a, 0.9);
    }
    let bg = mix("#f4f0ed", accent, 0.04 * tint);
    let panel = mix("#ebe6e2", accent, 0.06 * tint);
    P::of(vec![
        ("surface", mix("#ddd6d1", accent, 0.08 * tint)),
        ("surfaceHi", mix("#d0c8c2", accent, 0.12 * tint)),
        ("codeBg", mix("#fbf9f7", accent, 0.02 * tint)),
        ("border", mix(&panel, &a, 0.38)),
        ("dim", mix(&panel, &a, 0.55)),
        ("deep", scale_v(&a, 1.0 / 1.35)),
        ("glow", scale_v(&a, 1.0 / 1.1)),
        ("bright", scale_v(&a, 0.82)),
        ("selection", mix(&bg, accent, 0.3)),
        ("text", mix("#1c1717", &a, 0.06)),
        ("textDim", mix("#5e5555", &a, 0.1)),
        ("textFaint", mix("#958c89", &a, 0.08)),
        ("onAccent", if lig(&a) > 0.6 { "#0b0909" } else { "#ffffff" }.to_string()),
        ("textHi", "#000000".to_string()),
        ("ok", "#2e8b3a".to_string()),
        ("warn", "#b86a00".to_string()),
        ("mode", "light".to_string()),
        ("accent", a),
        ("bg", bg),
        ("panel", panel),
    ])
}

// ── обои ────────────────────────────────────────────────────────────────────
const BAYER: [f64; 16] = [0., 8., 2., 10., 12., 4., 14., 6., 3., 11., 1., 9., 15., 7., 13., 5.];

/// картинка в четверть экрана: шелл растягивает её ×4 без сглаживания
pub fn wallpaper_img(p: &P, w: u32, h: u32) -> RgbImage {
    let (bg, ac, dot) = (rgb(p.g("bg")), rgb(p.g("accent")), rgb(p.g("border")));
    let (bg, ac, dot) = ([bg.0 as f64, bg.1 as f64, bg.2 as f64], [ac.0 as f64, ac.1 as f64, ac.2 as f64], [dot.0 as f64, dot.1 as f64, dot.2 as f64]);
    // на бумаге свечение акцента бледнее — сильнее
    let (levels, peak) = (7.0_f64, if p.light() { 0.3 } else { 0.2 });
    let (fw, fh) = (w as f64, h as f64);
    let d = (fw * fw + fh * fh).powf(0.5);
    let (r1, r2) = (d * 0.75, d * 0.45);
    let mut img = RgbImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f64, y as f64);
            let a = (1.0 - ((fw - fx).powi(2) + (fh - fy).powi(2)).powf(0.5) / r1).max(0.0);
            let b = (1.0 - (fx * fx + fy * fy).powf(0.5) / r2).max(0.0);
            let g = a * a + b * b * 0.4;
            let q = (g * levels + BAYER[((y & 3) * 4 + (x & 3)) as usize] / 16.0).trunc() / levels * peak;
            let mut c = [bg[0] + (ac[0] - bg[0]) * q, bg[1] + (ac[1] - bg[1]) * q, bg[2] + (ac[2] - bg[2]) * q];
            if x % 10 == 5 && y % 10 == 5 {
                c = [c[0] + (dot[0] - c[0]) * 0.6, c[1] + (dot[1] - c[1]) * 0.6, c[2] + (dot[2] - c[2]) * 0.6];
            }
            img.put_pixel(x, y, Rgb([round_even(c[0]) as u8, round_even(c[1]) as u8, round_even(c[2]) as u8]));
        }
    }
    img
}
