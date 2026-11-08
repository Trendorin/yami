// Данные перекраски: рисунки клетками (курсор, значки программ, 闇) и большие текстовые заготовки QSS,
// меню браузера и цветов niri. В заготовках {p['ключ']} и {выражение} подставляет recolor::pyf, {{ и }} — скобки.

pub const QSS: &str = r###"/* YAMI: written by yami recolor. Пиксельный вид шелла поверх Fusion: фаски, рамки и
   флажки — картинки 9-slice из палитры (qss/yami/), всё остальное — палитра qt6ct. */
QToolTip {{ background: {p['panel']}; color: {p['text']}; border: 1px solid {p['accent']}; padding: 3px; }}
QMenu {{ background: {p['panel']}; border: 1px solid {p['border']}; padding: 2px; }}
QMenu::item {{ padding: 4px 18px 4px 10px; }}
QMenu::item:selected {{ background: {p['selection']}; color: {hi(p)}; }}
QMenu::separator {{ height: 1px; background: {p['border']}; margin: 3px 4px; }}
QScrollBar:vertical {{ background: {p['bg']}; width: 10px; margin: 0; }}
QScrollBar:horizontal {{ background: {p['bg']}; height: 10px; margin: 0; }}
QScrollBar::handle {{ background: {p['border']}; min-height: 24px; min-width: 24px; }}
QScrollBar::handle:hover {{ background: {p['accent']}; }}
QScrollBar::add-line, QScrollBar::sub-line {{ width: 0; height: 0; }}
QScrollBar::add-page, QScrollBar::sub-page {{ background: none; }}
QPushButton {{ {sl('btn')} padding: 1px 10px; color: {p['text']}; min-height: 16px; }}
QPushButton:hover {{ {sl('btn-hover')} color: {hi(p)}; }}
QPushButton:pressed, QPushButton:checked {{ {sl('btn-press')} padding: 2px 10px 0 10px; }}
QPushButton:default {{ {sl('btn-accent')} color: {p['onAccent']}; }}
QPushButton:default:hover {{ {sl('btn-accent-hover')} }}
QPushButton:disabled {{ {sl('btn-off')} color: {p['textFaint']}; }}
QLineEdit, QAbstractSpinBox {{ {sl('field')} padding: 0 4px; color: {p['text']}; selection-background-color: {p['selection']}; }}
QLineEdit:focus, QAbstractSpinBox:focus {{ {sl('field-focus')} }}
QLineEdit:disabled, QAbstractSpinBox:disabled {{ color: {p['textFaint']}; }}
QTextEdit:focus, QPlainTextEdit:focus {{ border: 1px solid {p['accent']}; }}
QComboBox {{ {sl('btn')} padding: 0 6px; color: {p['text']}; min-height: 16px; }}
QComboBox:hover, QComboBox:on {{ {sl('btn-hover')} }}
QComboBox:editable {{ {sl('field')} }}
QComboBox::drop-down {{ border: 0; width: 16px; }}
QComboBox::down-arrow {{ image: {u('arrow')}; width: 7px; height: 7px; }}
QComboBox QAbstractItemView {{ background: {p['panel']}; border: 1px solid {p['border']}; selection-background-color: {p['selection']}; selection-color: {hi(p)}; outline: 0; }}
QCheckBox::indicator, QRadioButton::indicator, QGroupBox::indicator {{ width: 16px; height: 16px; }}
QCheckBox::indicator:unchecked, QGroupBox::indicator:unchecked {{ image: {u('check')}; }}
QCheckBox::indicator:unchecked:hover {{ image: {u('check-hover')}; }}
QCheckBox::indicator:checked, QGroupBox::indicator:checked {{ image: {u('check-on')}; }}
QCheckBox::indicator:checked:hover {{ image: {u('check-on-hover')}; }}
QCheckBox::indicator:indeterminate {{ image: {u('check-part')}; }}
QRadioButton::indicator:unchecked {{ image: {u('radio')}; }}
QRadioButton::indicator:unchecked:hover {{ image: {u('radio-hover')}; }}
QRadioButton::indicator:checked {{ image: {u('radio-on')}; }}
QProgressBar {{ {sl('field')} text-align: center; color: {p['text']}; }}
QProgressBar::chunk {{ background: {p['accent']}; }}
QSlider::groove:horizontal {{ height: 6px; background: {p['codeBg']}; border: 1px solid {p['border']}; }}
QSlider::sub-page:horizontal {{ background: {p['deep']}; border: 1px solid {p['border']}; }}
QSlider::handle:horizontal {{ background: {p['accent']}; width: 8px; margin: -6px 0; }}
QSlider::handle:horizontal:hover {{ background: {p['bright']}; }}
QTabBar::tab {{ background: {p['panel']}; color: {p['textDim']}; border: 1px solid {p['border']}; padding: 4px 12px; margin-right: 2px; }}
QTabBar::tab:hover {{ color: {p['text']}; }}
QTabBar::tab:selected {{ background: {p['surface']}; color: {p['text']}; border-bottom: 2px solid {p['accent']}; }}
QTabWidget::pane {{ border: 1px solid {p['border']}; }}
QHeaderView::section {{ background: {p['panel']}; color: {p['textDim']}; border: 0; border-right: 1px solid {p['border']}; border-bottom: 1px solid {p['border']}; padding: 3px 6px; }}
QGroupBox {{ border: 1px solid {p['border']}; margin-top: 14px; padding-top: 4px; }}
QGroupBox::title {{ subcontrol-origin: margin; left: 8px; padding: 0 4px; color: {p['bright']}; }}
"###;

pub const FF_MENUS: &str = r###"/* YAMI: written by yami recolor (settings → fonts → browser). Delete the file to drop it.
   Only the browser's menus: its theme, toolbars and tabs stay as they are. */
:root {{
  --menu-background-color: {p['panel']} !important;
  --menu-color: {p['text']} !important;
  --menu-border-color: {edge} !important;
  --menuitem-hover-background-color: {p['deep']} !important;
  --menuitem-disabled-hover-background-color: transparent !important;
  --menu-disabled-color: {p['textFaint']} !important;
}}
menupopup {{ --panel-padding: 4px !important; --panel-background: {p['panel']} !important; --panel-color: {p['text']} !important; --panel-border-color: {edge} !important; --panel-border-radius: 0px !important; }}
menupopup::part(content) {{
  border-radius: 0 !important;
  border: 1px solid {edge} !important;
  background: {p['panel']} !important;
  box-shadow: 4px 4px 0 {shadow} !important;
}}
menu, menuitem, menucaption {{
  border-radius: 0 !important;
  padding-block: 2px !important;
  color: {p['text']} !important;
}}
menu[_moz-menuactive], menuitem[_moz-menuactive] {{
  background-color: {p['deep']} !important;
  color: #ffffff !important;
  box-shadow: inset 2px 0 0 {p['bright']} !important;
}}
menu[disabled], menuitem[disabled] {{ color: {p['textFaint']} !important; }}
menuseparator {{ border-top: 1px solid {p['border']} !important; padding-block: 3px !important; }}
tooltip {{
  appearance: none !important;
  background: {p['panel']} !important;
  color: {p['text']} !important;
  border: 1px solid {p['accent']} !important;
  border-radius: 0 !important;
  padding: 3px 7px !important;
}}
"###;

pub const FF_FONT: &str = r###"/* the pixel font exactly on its grid ({px} px), else it blurs; web pages keep their fonts */
#navigator-toolbox, #urlbar, .urlbarView, menupopup, panel, tooltip, #sidebar-box, #sidebar-main,
findbar, #statuspanel-label, .tab-label, .toolbarbutton-text, #identity-box, menu, menuitem {{
  font-family: "{UI_FONT}", "DotGothic16", monospace !important;
  font-size: {px}px !important;
}}
menu, menuitem, menucaption {{ min-height: {px + 8}px !important; }}
"###;

pub const NIRI_KDL: &str = r###"// YAMI: пишет yami recolor по палитре шелла. Руками не править.
layout {{
    border {{
        active-color   "{p['accent']}"
        inactive-color "{p['border']}"
        urgent-color   "{p['bright']}"
    }}
    tab-indicator {{
        active-color   "{p['accent']}"
        inactive-color "{p['border']}"
        urgent-color   "{p['bright']}"
    }}
    insert-hint {{
        color "{p['accent']}60"
    }}
}}
overview {{
    backdrop-color "{p['bg']}"
}}
recent-windows {{
    highlight {{
        active-color "{p['accent']}ff"
        urgent-color "{p['bright']}ff"
    }}
}}
"###;

pub const KANJI: [&str; 16] = [
    "######..######",
    "#....#..#....#",
    "######..######",
    "#....#..#....#",
    "######..######",
    "#.....#......#",
    "#..########..#",
    "#....#..#....#",
    "#.##########.#",
    "#............#",
    "#...######...#",
    "#...#....#...#",
    "#...######...#",
    "#...#....#...#",
    "#...######.###",
    "..............",
];

/// имя, горячая точка, 12 строк клетками: o контур, a акцент, b блик, w текст
pub const CURSOR_ART: [(&str, (u32, u32), [&str; 12]); 13] = [
    ("default", (0, 0), ["o...........", "oo..........", "obo.........", "oaao........", "oaaao.......", "oaaaao......", "oaaaaao.....", "oaaaaaao....", "oaaaoooo....", "oaoao.......", "oo.oao......", "o...oo......"]),
    ("pointer", (4, 0), ["...oo.......", "..obao......", "..oaao......", "..oaaoooo...", "..oaaabaooo.", "ooaaaaaaabo.", "obaaaaaaaao.", ".oaaaaaaaao.", "..oaaaaaaao.", "..oaaaaaaoo.", "...oaaaaao..", "...oooooo..."]),
    ("text", (5, 6), ["...ooo.ooo..", "...owwowwo..", "...ooowooo..", ".....owo....", ".....owo....", ".....owo....", ".....owo....", ".....owo....", ".....owo....", "...ooowooo..", "...owwowwo..", "...ooo.ooo.."]),
    ("crosshair", (5, 5), [".....o......", ".....a......", ".....a......", ".....a......", "............", "oaaa.b.aaao.", "............", ".....a......", ".....a......", ".....a......", ".....o......", "............"]),
    ("not-allowed", (5, 5), ["...oooooo...", "..oaaaaaao..", ".oaaooooaao.", "oaaob...oaao", "oao.b....oao", "oao..b...oao", "oao...b..oao", "oao....b.oao", "oaao...obaao", ".oaaooooaao.", "..oaaaaaao..", "...oooooo..."]),
    ("grab", (5, 5), ["...oo.oo....", "..oaooaoo...", "..oaooaoao..", ".ooaooaoaoo.", "oaoaaaaaaao.", "oaaaaaaaaao.", "oaaaaaaaaao.", ".oaaaaaaabo.", "..oaaaaaao..", "..oaaaaaao..", "...oooooo...", "............"]),
    ("grabbing", (5, 5), ["............", "............", "...oooooo...", "..oaoaoaoao.", ".ooaaaaaaao.", "oaoaaaaaaao.", "oaaaaaaaaao.", ".oaaaaaaabo.", "..oaaaaaao..", "..oaaaaaao..", "...oooooo...", "............"]),
    ("move", (5, 5), [".....o......", "....oao.....", "...oaaao....", ".....a......", ".o...a...o..", "oaaaabaaaao.", ".o...a...o..", ".....a......", "...oaaao....", "....oao.....", ".....o......", "............"]),
    ("ns-resize", (5, 5), [".....o......", "....oao.....", "...oaaao....", "..ooaaaoo...", "....oao.....", "....obo.....", "....oao.....", "..ooaaaoo...", "...oaaao....", "....oao.....", ".....o......", "............"]),
    ("ew-resize", (5, 5), ["............", "............", "............", "...o...o....", "..oo...oo...", ".oaoooooao..", "oaaaabaaaao.", ".oaoooooao..", "..oo...oo...", "...o...o....", "............", "............"]),
    ("nwse-resize", (5, 5), ["ooooo.......", "oaaao.......", "oaao........", "oaoao.......", "oo.oao......", "....obo.....", ".....oao.oo.", "......oaoao.", "........oao.", ".......oaao.", ".......oooo.", "............"]),
    ("nesw-resize", (6, 5), [".......ooooo", ".......oaaao", "........oaao", ".......oaoao", "......oao.oo", ".....obo....", ".oo.oao.....", ".oaoao......", ".oao........", ".oaao.......", ".oooo.......", "............"]),
    ("help", (0, 0), ["o.......ooo.", "oo.....oaaao", "obo....o.oao", "oaao.....oao", "oaaao...oao.", "oaaaao..oo..", "oaaaaao.....", "oaaaaaao.oo.", "oaaaoooo.oo.", "oaoao.......", "oo.oao......", "o...oo......"]),
];

pub const CURSOR_ALIASES: [(&str, &[&str]); 14] = [
    ("default", &["left_ptr", "arrow", "top_left_arrow", "context-menu", "copy", "alias", "dnd-copy", "dnd-link", "dnd-ask", "link", "right_ptr", "draft"]),
    ("pointer", &["hand2", "hand1", "hand", "pointing_hand", "9d800788f1b08800ae810202380a0822", "e29285e634086352946a0e7090d73106", "zoom-in", "zoom-out"]),
    ("text", &["xterm", "ibeam", "vertical-text"]),
    ("crosshair", &["cross", "tcross", "cell", "color-picker", "plus", "target"]),
    ("not-allowed", &["crossed_circle", "no-drop", "forbidden", "dnd-no-drop", "circle", "03b6e0fcb3499374a867c041f52298f0"]),
    ("grab", &["openhand", "dnd-none", "5aca4d189052212118709018842178c0"]),
    ("grabbing", &["closedhand", "dnd-move", "208530c400c041818281048008011002"]),
    ("move", &["all-scroll", "fleur", "size_all", "4498f0e0c1937ffe01fd06f973665830", "9081237383d90e509aa00f00170e968f"]),
    ("ns-resize", &["n-resize", "s-resize", "row-resize", "size_ver", "sb_v_double_arrow", "v_double_arrow", "top_side", "bottom_side", "split_v", "00008160000006810000408080010102"]),
    ("ew-resize", &["e-resize", "w-resize", "col-resize", "size_hor", "sb_h_double_arrow", "h_double_arrow", "left_side", "right_side", "split_h", "028006030e0e7ebffc7f7070c0600140"]),
    ("nwse-resize", &["nw-resize", "se-resize", "size_fdiag", "bd_double_arrow", "top_left_corner", "bottom_right_corner", "c7088f0f3e6c8088236ef8e1e3e70000"]),
    ("nesw-resize", &["ne-resize", "sw-resize", "size_bdiag", "fd_double_arrow", "top_right_corner", "bottom_left_corner", "fcf1c3c7cd4491d801f1e1c78f100000"]),
    ("help", &["question_arrow", "whats_this", "left_ptr_help", "5c6cd98b3f3ebcb1f9c7f1c204630408", "d9ce0ab605698f320427677b458ad60b"]),
    ("wait", &["watch", "progress", "left_ptr_watch", "half-busy", "08e8e1c95fe2fc01f976f1e063a24ccd", "3ecb610c1bf2410f44200f48c40d3599"]),
];

/// 16×16: o обводка, d/a/b тёмный, акцент, светлый, w знак на акценте
pub const APP_ART: [(&str, [&str; 16]); 3] = [
    ("yami-files", ["................", ".ooooo..........", "oaaaaao.........", "oaaaaaaoooooooo.", "oaaaaaaaaaaaaaao", "oaaaaaaaaaaaaaao", "oooooooooooooooo", "obbbbbbbbbbbbbbo", "obwwbbbbbbbbbbbo", "obbbbbbbbbbbbbbo", "obbbbbbbbbbbbbbo", "obbbbbbbbbbbbbbo", "obbbbbbbbbbbbbbo", "oddddddddddddddo", "oooooooooooooooo", "................"]),
    ("yami-view", ["................", "................", ".oooooooooooooo.", "owwwwwwwwwwwwwwo", "owddddddddddddwo", "owddddddddbbddwo", "owddddddddbbddwo", "owdddaddddddddwo", "owddaaaddddaddwo", "owdaaaaaddaaadwo", "owaaaaaaaaaaaawo", "owaaaaaaaaaaaawo", "owwwwwwwwwwwwwwo", ".oooooooooooooo.", "................", "................"]),
    ("yami-play", ["................", ".oooooooooooooo.", "obbaaaaaaaaaaaao", "obaaaaaaaaaaaaao", "oaaaaawaaaaaaaao", "oaaaaawwaaaaaaao", "oaaaaawwwaaaaaao", "oaaaaawwwwaaaaao", "oaaaaawwwwwaaaao", "oaaaaawwwwaaaaao", "oaaaaawwwaaaaaao", "oaaaaawwaaaaaaao", "oaaaaawaaaaaaaao", "oddddddddddddddo", ".oooooooooooooo.", "................"]),
];
