// Стиль отрисовки: чем панели и кнопки отличаются, кроме цветов. Холст (gfx.rs) берёт его здесь, поэтому
// код приложений один на все стили: зовёт pxbox и notched, а углы, тени и значки решает стиль.
// PIXEL — вид yami (фаски, жёсткая тень, битмапы клетками), PLAIN — нейтральный, для чужих райсов.
// Что выбрано, решает look.rs по ~/.config/yami/style.json.

use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Corners {
    /// срезанные на клетку углы
    Notch,
    /// скругление со сглаживанием
    Round,
    /// прямые углы
    Square,
}

impl Corners {
    pub fn from_id(s: &str) -> Option<Corners> {
        match s {
            "notch" => Some(Corners::Notch),
            "round" => Some(Corners::Round),
            "square" => Some(Corners::Square),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shadow {
    /// сплошная со сдвигом вниз-вправо
    Hard,
    /// размытая вокруг панели
    Soft,
    Off,
}

impl Shadow {
    pub fn from_id(s: &str) -> Option<Shadow> {
        match s {
            "hard" => Some(Shadow::Hard),
            "soft" => Some(Shadow::Soft),
            "none" | "off" => Some(Shadow::Off),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Style {
    pub corners: Corners,
    /// радиус скругления панели, пикселей (только Round)
    pub radius: u8,
    pub shadow: Shadow,
    /// светлая и тёмная грань на кнопках
    pub bevel: bool,
    /// шкалы из отдельных клеток; иначе сплошная полоса
    pub cells: bool,
    /// значки 7×7 со скруглением и сглаживанием, увеличение HiDPI без ступенек
    pub smooth: bool,
}

impl Style {
    pub const PIXEL: Style = Style { corners: Corners::Notch, radius: 0, shadow: Shadow::Hard, bevel: true, cells: true, smooth: false };
    pub const PLAIN: Style = Style { corners: Corners::Round, radius: 8, shadow: Shadow::Soft, bevel: false, cells: false, smooth: true };

    fn pack(self) -> u32 {
        let c = match self.corners {
            Corners::Notch => 0,
            Corners::Round => 1,
            Corners::Square => 2,
        };
        let s = match self.shadow {
            Shadow::Hard => 0,
            Shadow::Soft => 1,
            Shadow::Off => 2,
        };
        c | s << 2 | (self.bevel as u32) << 4 | (self.cells as u32) << 5 | (self.smooth as u32) << 6 | (self.radius as u32) << 8
    }

    fn unpack(v: u32) -> Style {
        Style {
            corners: match v & 3 {
                1 => Corners::Round,
                2 => Corners::Square,
                _ => Corners::Notch,
            },
            shadow: match v >> 2 & 3 {
                1 => Shadow::Soft,
                2 => Shadow::Off,
                _ => Shadow::Hard,
            },
            bevel: v >> 4 & 1 == 1,
            cells: v >> 5 & 1 == 1,
            smooth: v >> 6 & 1 == 1,
            radius: (v >> 8 & 255) as u8,
        }
    }
}

// PIXEL до первого set: снимки и тесты, которые не читают настройки, рисуют вид yami
// (bevel | cells: 1 << 4 | 1 << 5)
static STYLE: AtomicU32 = AtomicU32::new(0x30);

/// стиль для всех новых холстов процесса (как и тема, он один на процесс)
pub fn set(s: Style) {
    STYLE.store(s.pack(), Ordering::Relaxed);
}

pub fn current() -> Style {
    Style::unpack(STYLE.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_is_lossless_and_the_static_starts_as_pixel() {
        assert_eq!(Style::PIXEL.pack(), 0x30);
        for s in [Style::PIXEL, Style::PLAIN, Style { corners: Corners::Square, radius: 255, shadow: Shadow::Off, bevel: true, cells: false, smooth: true }] {
            assert_eq!(Style::unpack(s.pack()), s);
        }
    }
}
