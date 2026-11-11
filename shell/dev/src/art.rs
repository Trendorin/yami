// Нарисованные картинки README в .github/assets: пиксельный логотип и карточки возможностей (по картинке на
// язык), из самого проекта: цвета — как у перекраски, значки — из gfx.rs icon7 библиотеки yami-px, шрифты — из
// fonts/. Настоящие снимки сеанса делает tools/readme-shots.sh.  yami-dev readme-art [#акцент]

use crate::recolor_data::KANJI;
use crate::recolor_math::{derive, mix, rgb as hex_rgb, wallpaper_img, P};
use ab_glyph::{point, Font, FontRef, ScaleFont};
use image::{imageops, Rgba, RgbaImage};
use regex::Regex;
use std::collections::BTreeMap;

const BAYER: [f64; 16] = [0., 8., 2., 10., 12., 4., 14., 6., 3., 11., 1., 9., 15., 7., 13., 5.];

type C = Rgba<u8>;

fn c4(h: &str) -> C {
    let (r, g, b) = hex_rgb(h);
    Rgba([r as u8, g as u8, b as u8, 255])
}

struct D<'a> {
    im: RgbaImage,
    font: FontRef<'a>,
    p: P,
    icons: BTreeMap<String, Vec<String>>,
}

impl D<'_> {
    fn col(&self, k: &str) -> C {
        c4(self.p.g(k))
    }
    fn point(&mut self, x: i64, y: i64, c: C) {
        if x >= 0 && y >= 0 && (x as u32) < self.im.width() && (y as u32) < self.im.height() {
            self.im.put_pixel(x as u32, y as u32, c);
        }
    }
    /// прямоугольник, углы включительно
    fn rect(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, c: C) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.point(x, y, c);
            }
        }
    }
    fn notched(&mut self, x: i64, y: i64, w: i64, h: i64, c: C) {
        self.rect(x + 2, y, x + w - 3, y + h - 1, c);
        self.rect(x, y + 2, x + w - 1, y + h - 3, c);
        for (a, b) in [(x + 1, y + 1), (x + w - 2, y + 1), (x + 1, y + h - 2), (x + w - 2, y + h - 2)] {
            self.point(a, b, c);
        }
    }
    /// прямоугольник со срезанными углами, рамка 1 px и жёсткая тень (как gfx.rs pxbox)
    fn pxbox(&mut self, x: i64, y: i64, w: i64, h: i64, fill: C, edge: C, shadow: Option<C>) {
        if let Some(s) = shadow {
            self.notched(x + 3, y + 3, w, h, s);
        }
        self.notched(x, y, w, h, edge);
        self.notched(x + 1, y + 1, w - 2, h - 2, fill);
    }
    fn icon(&mut self, name: &str, x: i64, y: i64, cell: i64, c: C) {
        let rows = self.icons.get(name).cloned().unwrap_or_default();
        for (j, row) in rows.iter().enumerate() {
            for (i, ch) in row.chars().enumerate() {
                if ch == '#' {
                    let (i, j) = (i as i64, j as i64);
                    self.rect(x + i * cell, y + j * cell, x + (i + 1) * cell - 1, y + (j + 1) * cell - 1, c);
                }
            }
        }
    }
    fn dissolve(&mut self, x: i64, y: i64, w: i64, h: i64, c: C) {
        for yy in 0..h {
            for xx in 0..w {
                if BAYER[((yy & 3) * 4 + (xx & 3)) as usize] / 16.0 < 1.0 - xx as f64 / w as f64 {
                    self.point(x + xx, y + yy, c);
                }
            }
        }
    }
    fn scale(&self, size: f32) -> ab_glyph::PxScale {
        // размер в пикселях на em, а не на высоту строки
        let k = self.font.height_unscaled() / self.font.units_per_em().unwrap_or(1000.0);
        ab_glyph::PxScale::from(size * k)
    }
    fn width(&self, s: &str, size: f32) -> f32 {
        let f = self.font.as_scaled(self.scale(size));
        // FreeType с хинтингом: продвижение — целое число пикселей
        let mut w = 0.0;
        let mut prev = None;
        for c in s.chars() {
            let id = f.glyph_id(c);
            if let Some(p) = prev {
                w += f.kern(p, id).round();
            }
            w += f.h_advance(id).round();
            prev = Some(id);
        }
        w
    }
    /// целые пиксели, без сглаживания
    fn text(&mut self, x: f32, y: f32, s: &str, size: f32, c: C) {
        let scale = self.scale(size);
        let font = self.font.clone();
        let f = font.as_scaled(scale);
        let base = y + f.ascent().ceil();
        let mut pen = x;
        let mut prev = None;
        for ch in s.chars() {
            let id = f.glyph_id(ch);
            if let Some(p) = prev {
                pen += f.kern(p, id).round();
            }
            prev = Some(id);
            let g = id.with_scale_and_position(scale, point(pen, base));
            if let Some(o) = font.outline_glyph(g) {
                let b = o.px_bounds();
                let mut hits = vec![];
                o.draw(|gx, gy, v| {
                    if v >= 0.5 {
                        hits.push((b.min.x as i64 + gx as i64, b.min.y as i64 + gy as i64));
                    }
                });
                for (a, q) in hits {
                    self.point(a, q, c);
                }
            }
            pen += f.h_advance(id).round();
        }
    }
}

const CARDS: [[&str; 7]; 10] = [
    ["ram", "~15 MB shell", "bar, launcher, dock, notifications, widgets:", "one Rust binary, no toolkit, no GPU context", "шелл на 15 МБ", "бар, лаунчер, док, уведомления, виджеты:", "один бинарь на Rust, без тулкитов и GPU"],
    ["drop", "one accent", "pick a colour: Qt, KDE, GTK, kitty, browser,", "the cursor and the login screen follow", "один акцент", "выбери цвет: Qt, KDE, GTK, kitty, браузер,", "курсор и экран входа перекрасятся"],
    ["lock", "login screen", "our own greeter in Rust: the lock screen's", "face; greetd checks the password", "экран входа", "свой, на Rust: лицо экрана блокировки;", "пароль проверяет greetd"],
    ["play", "made for games", "tearing and VRR on yami-niri; while you play", "notifications wait, power goes to max", "для игр", "tearing и VRR на yami-niri; пока играешь,", "уведомления ждут, питание на максимум"],
    ["folder", "files and player", "yami-files: tabs, split view, trash and the", "file dialog of every app; yami-view, yami-play", "файлы и плеер", "yami-files: вкладки, корзина и диалог", "выбора файлов для всех; просмотр, плеер"],
    ["camera", "screenshots", "area, window or screen; loupe, colour picker,", "pen, arrows, mosaic, numbered badges", "скриншоты", "область, окно или экран; лупа, пипетка,", "перо, стрелки, мозаика, номерки"],
    ["eye", "stream curtain", "viewers of a stream see a frozen, blurred", "screen while you keep working", "шторка стрима", "зрители видят размытый стоп-кадр,", "а ты спокойно работаешь дальше"],
    ["down", "signed updates", "one click or yami update, built from source,", "tags checked by the author's key, rollback", "обновления", "в один клик или yami update, сборка", "из исходников, подпись автора, откат"],
    ["screen", "any distro", "Arch, Fedora, Debian, openSUSE, Void, Alpine,", "Gentoo: one installer; NixOS: a flake", "8 дистрибутивов", "Arch, Fedora, Debian, openSUSE, Void, Alpine,", "Gentoo — установщик; NixOS — flake"],
    ["text", "eight languages", "en ru pt-BR es de fr uk pl,", "switched live, the login screen too", "восемь языков", "en ru pt-BR es de fr uk pl,", "на лету, экран входа тоже"],
];

/// акцент — тот же, что на скриншотах (палитра шелла), а без настроек — «кровь»
fn accent(root: &std::path::Path) -> String {
    let st: serde_json::Value = std::env::var_os("HOME").and_then(|h| std::fs::read_to_string(std::path::Path::new(&h).join(".config/yami/shell.json")).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let choices = std::fs::read_to_string(root.join("installer/choices.sh")).unwrap_or_default();
    let list = |name: &str| Regex::new(&format!(r"{name}=\(([^)]*)\)")).ok().and_then(|r| r.captures(&choices).map(|m| m[1].split_whitespace().map(String::from).collect::<Vec<_>>())).unwrap_or_default();
    let (ids, hexes) = (list("PAL_ID"), list("PAL_HEX"));
    let own = st["accent"].as_str().unwrap_or("").trim_start_matches('#').to_string();
    if own.len() == 6 && own.bytes().all(|b| b.is_ascii_hexdigit()) {
        return format!("#{own}");
    }
    let want = st["palette"].as_str().unwrap_or("blood");
    format!("#{}", ids.iter().zip(hexes.iter()).find(|(i, _)| i.as_str() == want).map_or("c4161c", |(_, h)| h.as_str()))
}

/// значки 7×7 из библиотеки yami-px: рядом с этим репозиторием или в YAMI_MODULES_DIR
fn icons(root: &std::path::Path) -> BTreeMap<String, Vec<String>> {
    let dir = std::env::var_os("YAMI_MODULES_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| root.join("../yami-modules"));
    let src = std::fs::read_to_string(dir.join("yami-px/src/gfx.rs")).unwrap_or_default();
    let block = Regex::new(r#""(\w+)" => &\[((?:"[.#]{7}",? ?){7})\]"#).unwrap();
    let row = Regex::new(r#""([.#]{7})""#).unwrap();
    block.captures_iter(&src).map(|m| (m[1].to_string(), row.captures_iter(&m[2]).map(|r| r[1].to_string()).collect())).collect()
}

/// все карточки в одном окне шелла: шапка, точечный фон, сетка 2×N — одна картинка
fn features(d: &mut D, lang: &str) -> Result<RgbaImage, String> {
    let (w, pad, gap, hh, ch) = (452i64, 10i64, 8i64, 18i64, 60i64);
    let cw = (w - 2 * pad - gap) / 2;
    let rows = (CARDS.len() as i64 + 1) / 2;
    let h = hh + pad + rows * ch + (rows - 1) * gap + pad;
    d.im = RgbaImage::new((w + 4) as u32, (h + 4) as u32);
    let (bg, border, surface, panel, accent, bright, on_accent, code, text, dim, faint) = (d.col("bg"), d.col("border"), d.col("surface"), d.col("panel"), d.col("accent"), d.col("bright"), d.col("onAccent"), d.col("codeBg"), d.col("text"), d.col("textDim"), d.col("textFaint"));
    d.pxbox(0, 0, w, h, bg, border, Some(Rgba([0, 0, 0, 160])));
    // точечная сетка, как на обоях
    for yy in (hh + 6..h - 2).step_by(8) {
        for xx in (6..w - 2).step_by(8) {
            d.point(xx, yy, surface);
        }
    }
    d.rect(2, 1, w - 3, hh - 1, panel);
    d.rect(1, 2, w - 2, hh - 1, panel);
    d.rect(1, hh, w - 2, hh, border);
    d.rect(10, hh / 2 - 3, 15, hh / 2 + 2, accent);
    let (title, sub) = if lang == "en" { ("yami", "a pixel desktop for niri") } else { ("yami", "пиксельный рабочий стол для niri") };
    d.text(22.0, 4.0, title, 8.0, text);
    let tw = d.width(title, 8.0);
    d.text(24.0 + tw, 4.0, sub, 8.0, faint);
    d.dissolve(w - 130, 2, 120, hh - 3, surface);
    for (k, c) in CARDS.iter().enumerate() {
        let k = k as i64;
        let (t, l1, l2) = if lang == "en" { (c[1], c[2], c[3]) } else { (c[4], c[5], c[6]) };
        let (x, y) = (pad + (k % 2) * (cw + gap), hh + pad + (k / 2) * (ch + gap));
        let room = (cw - 46 - 8) as f32;
        let up = t.to_uppercase();
        for (s, size) in [(up.as_str(), 16.0), (l1, 8.0), (l2, 8.0)] {
            if d.width(s, size) > room {
                return Err(format!("readme-art: «{s}» is {:.0} px wide, the card has {room}", d.width(s, size)));
            }
        }
        d.pxbox(x, y, cw, ch, panel, border, Some(code));
        d.pxbox(x + 10, y + 10, 25, 25, accent, bright, None);
        d.icon(c[0], x + 12, y + 12, 3, on_accent);
        d.text((x + 46) as f32, (y + 6) as f32, &up, 16.0, text);
        d.rect(x + 46, y + 27, x + 46 + 15, y + 28, accent);
        d.text((x + 46) as f32, (y + 33) as f32, l1, 8.0, dim);
        d.text((x + 46) as f32, (y + 44) as f32, l2, 8.0, dim);
    }
    Ok(imageops::resize(&d.im, d.im.width() * 2, d.im.height() * 2, imageops::FilterType::Nearest))
}

fn bands(p: &P) -> [String; 4] {
    [p.g("bright").to_string(), mix(p.g("bright"), p.g("accent"), 0.5), p.g("accent").to_string(), mix(p.g("accent"), p.g("deep"), 0.5)]
}

fn on(x: i64, y: i64) -> bool {
    y >= 0 && y < KANJI.len() as i64 && x >= 0 && x < KANJI[0].len() as i64 && KANJI[y as usize].as_bytes()[x as usize] == b'#'
}

/// 闇 как в fastfetch на тёмной плашке со срезанными углами — SVG из квадратов, читается и в светлой теме GitHub
fn logo(p: &P) -> String {
    let (gw, gh) = (KANJI[0].len() as i64, KANJI.len() as i64);
    let pad = 3;
    let (w, h) = (gw + 1 + 2 * pad, gh + 2 * pad);
    let (bands, shadow) = (bands(p), mix(p.g("deep"), p.g("bg"), 0.45));
    let mut cells: Vec<(String, Vec<(i64, i64)>)> = vec![];
    let mut put = |c: &str, xy: (i64, i64)| match cells.iter_mut().find(|(k, _)| k == c) {
        Some((_, v)) => v.push(xy),
        None => cells.push((c.to_string(), vec![xy])),
    };
    for y in 0..gh {
        for x in 0..=gw {
            if on(x, y) {
                put(&bands[((y * 4 / gh) as usize).min(3)], (x, y));
            } else if on(x - 1, y - 1) && (x >= gw || y >= gh - 1) {
                put(&shadow, (x, y));
            }
        }
    }
    let tile = format!("<path fill=\"{}\" d=\"M2 0h{}v1h1v1h1v{}h-1v1h-1v1H2v-1H1v-1H0V2h1V1h1z\"/><path fill=\"{}\" d=\"M2 1h{}v1h1v{}h-1v1H2v-1H1V2h1z\"/>", p.g("border"), w - 4, h - 4, p.g("bg"), w - 4, h - 4);
    let glyph: String = cells.iter().map(|(c, pts)| format!("<path fill=\"{c}\" d=\"{}\"/>", pts.iter().map(|(x, y)| format!("M{} {}h1v1h-1z", x + pad, y + pad)).collect::<String>())).collect();
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" shape-rendering=\"crispEdges\">{tile}{glyph}</svg>\n")
}

/// превью ссылки (GitHub → Settings → Social preview, 1280×640): обои палитры, 闇 и надписи Tiny5 на сетке 2×2
fn social(d: &mut D) -> RgbaImage {
    let (w, h, cell) = (640i64, 320i64, 6i64);
    let wall = wallpaper_img(&d.p, w as u32, h as u32);
    d.im = image::DynamicImage::ImageRgb8(wall).to_rgba8();
    let (bands, shadow) = (bands(&d.p), mix(d.p.g("deep"), d.p.g("bg"), 0.45));
    let (gw, gh) = (KANJI[0].len() as i64, KANJI.len() as i64);
    let (x0, y0) = (70, (h - gh * cell) / 2);
    for y in 0..=gh {
        for x in 0..=gw {
            let c = if on(x, y) { Some(bands[((y * 4 / gh) as usize).min(3)].as_str()) } else if on(x - 1, y - 1) { Some(shadow.as_str()) } else { None };
            if let Some(c) = c {
                d.rect(x0 + x * cell, y0 + y * cell, x0 + (x + 1) * cell - 1, y0 + (y + 1) * cell - 1, c4(c));
            }
        }
    }
    let tx = (x0 + (gw + 1) * cell + 40) as f32;
    let (text, dim, faint, accent) = (d.col("text"), d.col("textDim"), d.col("textFaint"), d.col("accent"));
    d.text(tx, 104.0, "yami", 48.0, text);
    d.rect(tx as i64, 160, tx as i64 + 31, 163, accent);
    d.text(tx, 176.0, "a pixel-art desktop for niri", 16.0, dim);
    d.text(tx, 204.0, "Rust Wayland shell in ~15 MB · own file manager · one accent for every app", 8.0, faint);
    imageops::resize(&d.im, (w * 2) as u32, (h * 2) as u32, imageops::FilterType::Nearest)
}

pub fn run(args: &[String]) -> i32 {
    let root = crate::root();
    let out = root.join(".github/assets");
    let _ = std::fs::create_dir_all(&out);
    let acc = args.first().cloned().unwrap_or_else(|| accent(&root));
    let tiny = match std::fs::read(root.join("fonts/Tiny5.ttf")) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("readme-art: fonts/Tiny5.ttf: {e}");
            return 1;
        }
    };
    let Ok(font) = FontRef::try_from_slice(&tiny) else { return 1 };
    let p = derive(&acc, 1.0, false);
    let mut d = D { im: RgbaImage::new(1, 1), font, p, icons: icons(&root) };
    if std::fs::write(out.join("logo.svg"), logo(&d.p)).is_err() {
        return 1;
    }
    for lang in ["en", "ru"] {
        match features(&mut d, lang) {
            Ok(im) => {
                if !save_png(&out.join(format!("features-{lang}.png")), im.width(), im.height(), im.as_raw(), image::ExtendedColorType::Rgba8) {
                    return 1;
                }
            }
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        }
    }
    let soc = social(&mut d);
    let soc = image::DynamicImage::ImageRgba8(soc).to_rgb8();
    if !save_png(&out.join("social.png"), soc.width(), soc.height(), soc.as_raw(), image::ExtendedColorType::Rgb8) {
        return 1;
    }
    println!("readme-art: logo, features-{{en,ru}}.png ({} cards) and social.png in .github/assets", CARDS.len());
    0
}

/// PNG с лучшим сжатием: save() у image 0.25 пишет быстро и в 4–15 раз крупнее, а картинки лежат в git
fn save_png(path: &std::path::Path, w: u32, h: u32, data: &[u8], color: image::ExtendedColorType) -> bool {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let Ok(f) = std::fs::File::create(path) else { return false };
    PngEncoder::new_with_quality(std::io::BufWriter::new(f), CompressionType::Best, FilterType::Adaptive).write_image(data, w, h, color).is_ok()
}

/// то же для любой картинки: снимки README (yami-dev shots) тоже лежат в git
pub(crate) fn save_best(im: &image::DynamicImage, path: &std::path::Path) -> Result<(), String> {
    if save_png(path, im.width(), im.height(), im.as_bytes(), im.color().into()) {
        Ok(())
    } else {
        Err(format!("cannot write {}", path.display()))
    }
}
