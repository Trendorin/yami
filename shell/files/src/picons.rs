// Значки файлов. В сглаженных стилях — из системного набора значков (если в нём нашёлся нужный).
// В пиксельном — битмапы 16×16 в цветах темы, масштаб — целыми клетками.
// Папка — в акценте, документ — светлый лист с загнутым углом и глифом типа (те же 7×7, что
// в шелле). Особые папки (дом, загрузки…) несут свой глиф, ссылки — стрелку в углу.

use crate::gfx::{icon7, Canvas, C, R};
use crate::mime::Kind;
use crate::theme::Theme;

const FOLDER: [&str; 16] = [
    "................",
    "................",
    ".OOOOO..........",
    "ODDDDDO.........",
    "ODDDDDDOOOOOOOO.",
    "OEEEEEEEEEEEEEEO",
    "OAAAAAAAAAAAAAAO",
    "OAAAAAAAAAAAAAAO",
    "OAAAAAAAAAAAAAAO",
    "OAAAAAAAAAAAAAAO",
    "OAAAAAAAAAAAAAAO",
    "OAAAAAAAAAAAAAAO",
    "OAAAAAAAAAAAAAAO",
    "ODDDDDDDDDDDDDDO",
    ".OOOOOOOOOOOOOO.",
    "................",
];

const PAGE: [&str; 16] = [
    "..OOOOOOOOO.....",
    "..OPPPPPPPOO....",
    "..OPPPPPPPOFO...",
    "..OPPPPPPPOFFO..",
    "..OPPPPPPPOOOOO.",
    "..OPPPPPPPPPPPO.",
    "..OPPPPPPPPPPPO.",
    "..OPPPPPPPPPPPO.",
    "..OPPPPPPPPPPPO.",
    "..OPPPPPPPPPPPO.",
    "..OPPPPPPPPPPPO.",
    "..OPPPPPPPPPPPO.",
    "..OPPPPPPPPPPPO.",
    "..OPPPPPPPPPPPO.",
    "..OFFFFFFFFFFFO.",
    "..OOOOOOOOOOOOO.",
];

const TRASH: [&str; 16] = [
    "................",
    "......OOOO......",
    ".OOOOOOOOOOOOOO.",
    ".OEEEEEEEEEEEEO.",
    ".OOOOOOOOOOOOOO.",
    "..OAAAAAAAAAAO..",
    "..OADAADAADAAO..",
    "..OADAADAADAAO..",
    "..OADAADAADAAO..",
    "..OADAADAADAAO..",
    "..OADAADAADAAO..",
    "..OADAADAADAAO..",
    "..OAAAAAAAAAAO..",
    "..ODDDDDDDDDDO..",
    "...OOOOOOOOOO...",
    "................",
];

const TRASH_FULL: [&str; 16] = [
    "....PP..FP......",
    "...PFP.PPF.P....",
    ".OOOOOOOOOOOOOO.",
    ".OEEEEEEEEEEEEO.",
    ".OOOOOOOOOOOOOO.",
    "..OAAAAAAAAAAO..",
    "..OADAADAADAAO..",
    "..OADAADAADAAO..",
    "..OADAADAADAAO..",
    "..OADAADAADAAO..",
    "..OADAADAADAAO..",
    "..OADAADAADAAO..",
    "..OAAAAAAAAAAO..",
    "..ODDDDDDDDDDO..",
    "...OOOOOOOOOO...",
    "................",
];

const DRIVE: [&str; 16] = [
    "................",
    "................",
    "................",
    "................",
    ".OOOOOOOOOOOOOO.",
    "OPPPPPPPPPPPPPPO",
    "OPPPPPPPPPPPPPPO",
    "OPPPPPPPPPPPPPPO",
    "OOOOOOOOOOOOOOOO",
    "OFFFFFFFFFFFFFFO",
    "OFFFFFFFFFFAAFFO",
    "OFFFFFFFFFFFFFFO",
    ".OOOOOOOOOOOOOO.",
    "................",
    "................",
    "................",
];

const USB: [&str; 16] = [
    "....OOOOOOOO....",
    "....OPPPPPPO....",
    "....OPFPPFPO....",
    "....OPPPPPPO....",
    "...OOOOOOOOOO...",
    "...OEEEEEEEEO...",
    "...OAAAAAAAAO...",
    "...OAAAAAAAAO...",
    "...OAAAAAAAAO...",
    "...OAAAAAAAAO...",
    "...OAAAAAAAAO...",
    "...OAAAAAAAAO...",
    "...ODDDDDDDDO...",
    "....OOOOOOOO....",
    "................",
    "................",
];

const PHONE: [&str; 16] = [
    "....OOOOOOOO....",
    "...OPPPPPPPPO...",
    "...OPPOOOOPPO...",
    "...OAAAAAAAAO...",
    "...OAEEEEEEAO...",
    "...OAEAAAAAAO...",
    "...OAAAAAAAAO...",
    "...OAAAAAAAAO...",
    "...OAAAAAAAAO...",
    "...OAAAAAAAAO...",
    "...OAAAAAAAAO...",
    "...ODDDDDDDDO...",
    "...OPPPPPPPPO...",
    "...OPPPOOPPPO...",
    "....OOOOOOOO....",
    "................",
];

const CAMERA: [&str; 16] = [
    "................",
    "................",
    "................",
    ".....OOOOO......",
    "....OPPPPPO.....",
    ".OOOOOOOOOOOOOO.",
    "OPPPPPOOOOPPEEPO",
    "OPPPPOAAAAOPPPPO",
    "OPPPOAAEEAAOPPPO",
    "OPPPOAAEAAAOPPPO",
    "OPPPOAAAAAAOPPPO",
    "OPPPPOAAAAOPPPPO",
    "OFFFFFOOOOFFFFFO",
    ".OOOOOOOOOOOOOO.",
    "................",
    "................",
];

const NET: [&str; 16] = [
    "................",
    "..OOOOOOOOOOOO..",
    "..OPPPPPPPPPPO..",
    "..OPAAAAAAAAPO..",
    "..OPAEEAAAAAPO..",
    "..OPAAAAAAAAPO..",
    "..OPAAAAAAAAPO..",
    "..OPPPPPPPPPPO..",
    "..OOOOOOOOOOOO..",
    ".......OO.......",
    ".......OO.......",
    "OOOOOOOOOOOOOOOO",
    "OFFFO.......OFFO",
    "OOOOO.......OOOO",
    "................",
    "................",
];

/// дополнительные глифы 7×7 для типов файлов
fn glyph(name: &str) -> Option<&'static [&'static str; 7]> {
    Some(match name {
        "lines" => &["#####..", ".......", "######.", ".......", "####...", ".......", "#####.."],
        "code" => &[".......", "..#.#..", ".#..#..", "#...#..", ".#.#...", "..##.#.", "...#..."],
        "zip" => &["...#...", "..#....", "...#...", "..#....", "..###..", "..#.#..", "..###.."],
        "term" => &["#......", ".#.....", "..#....", ".#.....", "#......", ".......", "...####"],
        "font" => &["..#....", ".#.#...", "#...#..", "#####..", "#...#..", "#...#.#", ".....##"],
        "disc" => &["..###..", ".#...#.", "#..#..#", "#.#.#.#", "#..#..#", ".#...#.", "..###.."],
        "box" => &["..###..", ".#...#.", "#######", "#..#..#", "#..#..#", "#..#..#", "#######"],
        "globe" => &["..###..", ".#.#.#.", "#######", "#..#..#", "#######", ".#.#.#.", "..###.."],
        "pdf" => &["###....", "#.#....", "###....", "#......", "#..###.", "...#..#", "...###."],
        "house" => &["...#...", "..###..", ".#####.", "#######", ".#...#.", ".#.#.#.", ".#.#.#."],
        "screen" => &["#######", "#.....#", "#.....#", "#.....#", "#######", "...#...", "..###.."],
        "film" => &["#.###.#", "#######", "#.....#", "#.....#", "#.....#", "#######", "#.###.#"],
        "star" => &["...#...", "...#...", "#######", ".#####.", "..###..", ".##.##.", "#.....#"],
        "root" => &["#######", "#.....#", "#.###.#", "#.....#", "#.###.#", "#.....#", "#######"],
        _ => return icon7(name),
    })
}

/// особая папка: свой глиф на значке
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Special {
    None,
    Home,
    Desktop,
    Docs,
    Downloads,
    Music,
    Pictures,
    Videos,
    Trash,
    TrashFull,
    Drive,
    Usb,
    Root,
    Star,
    Phone,
    Camera,
    Net,
}

fn bitmap(cv: &mut Canvas, rows: &[&str; 16], x: i32, y: i32, u: i32, pal: &dyn Fn(u8) -> Option<C>) {
    for (j, row) in rows.iter().enumerate() {
        let b = row.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let ch = b[i];
            let Some(c) = pal(ch) else {
                i += 1;
                continue;
            };
            // одинаковые клетки подряд — одним прямоугольником
            let mut k = i + 1;
            while k < b.len() && b[k] == ch {
                k += 1;
            }
            cv.fill(R::new(x + i as i32 * u, y + j as i32 * u, (k - i) as i32 * u, u), c);
            i = k;
        }
    }
}

fn glyph_at(cv: &mut Canvas, name: &str, x: i32, y: i32, u: i32, c: C) {
    if let Some(rows) = glyph(name) {
        for (j, row) in rows.iter().enumerate() {
            for (i, ch) in row.bytes().enumerate() {
                if ch == b'#' {
                    cv.fill(R::new(x + i as i32 * u, y + j as i32 * u, u, u), c);
                }
            }
        }
    }
}

pub fn kind_glyph(k: Kind) -> &'static str {
    match k {
        Kind::Image => "image",
        Kind::Video => "film",
        Kind::Audio => "music",
        Kind::Text => "lines",
        Kind::Code => "code",
        Kind::Doc => "lines",
        Kind::Pdf => "pdf",
        Kind::Archive => "zip",
        Kind::Exec => "term",
        Kind::Font => "font",
        Kind::Disk => "disc",
        Kind::Package => "box",
        Kind::Web => "globe",
        Kind::Dir | Kind::Other => "",
    }
}

thread_local! {
    /// системный набор значков для сглаженных стилей: заводится при первом значке
    static SYSTEM: std::cell::RefCell<Option<yami_px::icons::Icons>> = const { std::cell::RefCell::new(None) };
}

/// имя значка по спецификации freedesktop
fn system_name(kind: Kind, sp: Special) -> &'static str {
    match sp {
        Special::Home => return "user-home",
        Special::Desktop => return "user-desktop",
        Special::Docs => return "folder-documents",
        Special::Downloads => return "folder-download",
        Special::Music => return "folder-music",
        Special::Pictures => return "folder-pictures",
        Special::Videos => return "folder-videos",
        Special::Trash => return "user-trash",
        Special::TrashFull => return "user-trash-full",
        Special::Drive | Special::Root => return "drive-harddisk",
        Special::Usb => return "drive-removable-media",
        Special::Phone => return "phone",
        Special::Camera => return "camera-photo",
        Special::Net => return "folder-remote",
        Special::Star | Special::None => {}
    }
    match kind {
        Kind::Dir => "folder",
        Kind::Image => "image-x-generic",
        Kind::Video => "video-x-generic",
        Kind::Audio => "audio-x-generic",
        Kind::Code => "text-x-script",
        Kind::Doc => "x-office-document",
        Kind::Pdf => "application-pdf",
        Kind::Archive | Kind::Package => "package-x-generic",
        Kind::Exec => "application-x-executable",
        Kind::Font => "font-x-generic",
        Kind::Disk => "media-optical",
        Kind::Web => "text-html",
        Kind::Text | Kind::Other => "text-x-generic",
    }
}

/// значок из системного набора; false — такого нет, рисуем свой
fn draw_system(cv: &mut Canvas, kind: Kind, sp: Special, x: i32, y: i32, size: i32, k: f32) -> bool {
    let img = SYSTEM.with(|s| {
        let mut s = s.borrow_mut();
        let ic = s.get_or_insert_with(|| yami_px::icons::Icons::new(&yami_px::icons::system_theme()));
        ic.get(system_name(kind, sp), size)
    });
    match img {
        Some(i) => {
            cv.image(&i, x + (size - i.w) / 2, y + (size - i.h) / 2, k);
            true
        }
        None => false,
    }
}

/// забыть набор значков: сменилась тема
pub fn reset_system() {
    SYSTEM.with(|s| *s.borrow_mut() = None);
}

pub struct Look {
    pub dim: bool,
    pub link: bool,
    pub selected: bool,
}

/// значок размера 16·u в (x, y)
pub fn draw(cv: &mut Canvas, t: &Theme, kind: Kind, sp: Special, x: i32, y: i32, u: i32, lk: Look) {
    let u = u.max(1);
    let k = if lk.dim { 0.55 } else { 1.0 };
    let dark = t.blood.lerp(C(0xff000000), 0.45);
    if cv.style.smooth && draw_system(cv, kind, sp, x, y, 16 * u, k) {
        if lk.link {
            let b = R::new(x, y + 10 * u, 6 * u, 6 * u);
            cv.fill(b, t.panel.fade(k));
            cv.icon_in("arrow", b, u.max(1) / 2 + (u == 1) as i32, t.text.fade(k));
        }
        return;
    }
    let accent = if lk.selected { t.ember } else { t.red };
    // бумага — светлая в обеих темах (в светлой текст тёмный, брать его нельзя)
    let (paper, fold) = if t.light {
        (t.code_bg.lerp(t.bg, 0.3).alpha(1.0), t.surface_hi.lerp(t.text_faint, 0.25).alpha(1.0))
    } else {
        (t.text.lerp(t.surface_hi, 0.18), t.text_dim.lerp(t.surface_hi, 0.2))
    };
    let pal = |ch: u8| -> Option<C> {
        let c = match ch {
            b'O' => dark,
            b'A' => accent,
            b'D' => t.blood,
            b'E' => t.ember.lerp(C(0xffffffff), 0.15),
            b'P' => paper,
            b'F' => fold,
            _ => return None,
        };
        Some(c.fade(k))
    };
    let (rows, g, gc): (&[&str; 16], &str, C) = match (kind, sp) {
        (_, Special::Trash) => (&TRASH, "", dark),
        (_, Special::TrashFull) => (&TRASH_FULL, "", dark),
        (_, Special::Drive) => (&DRIVE, "", dark),
        (_, Special::Root) => (&DRIVE, "", dark),
        (_, Special::Usb) => (&USB, "", dark),
        (_, Special::Phone) => (&PHONE, "", dark),
        (_, Special::Camera) => (&CAMERA, "", dark),
        (_, Special::Net) => (&NET, "", dark),
        (Kind::Dir, s) => (
            &FOLDER,
            match s {
                Special::Home => "house",
                Special::Desktop => "screen",
                Special::Docs => "lines",
                Special::Downloads => "down",
                Special::Music => "music",
                Special::Pictures => "image",
                Special::Videos => "film",
                Special::Star => "star",
                _ => "",
            },
            t.blood.lerp(C(0xff000000), 0.25),
        ),
        (k2, _) => (&PAGE, kind_glyph(k2), accent),
    };
    bitmap(cv, rows, x, y, u, &pal);
    if !g.is_empty() {
        let (gx, gy) = if std::ptr::eq(rows, &FOLDER) { (x + 9 * u / 2 - u / 2 + u / 2, y + 7 * u) } else { (x + 5 * u, y + 6 * u) };
        // глиф 7 клеток: на 16-клеточной папке — ровно по центру тела
        let gx = if std::ptr::eq(rows, &FOLDER) { x + (16 * u - 7 * u) / 2 } else { gx };
        glyph_at(cv, g, gx, gy, u, gc.fade(k));
    }
    if lk.link {
        // стрелка ссылки: тёмная плашка 6×6 в левом нижнем углу
        let b = R::new(x, y + 10 * u, 6 * u, 6 * u);
        cv.fill(b, dark.fade(k));
        cv.fill(R::new(b.x + u, b.y + u, 4 * u, 4 * u), t.ember.fade(k));
        cv.fill(R::new(b.x + 2 * u, b.y + 3 * u, u, u), dark.fade(k));
        cv.fill(R::new(b.x + 2 * u, b.y + 2 * u, 2 * u, u), dark.fade(k));
    }
}
