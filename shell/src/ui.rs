// Действия панелей шелла. Сам немедленный UI (кнопки, поля, зоны) — общий, в yami-px:
// им же рисуют файловый менеджер и диалоги.

pub use yami_px::ui::{Cur, Field, Tone};

pub type Ui<'a, 'b> = yami_px::ui::Ui<'a, 'b, Act>;
pub type Zone = yami_px::ui::Zone<Act>;

#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// глушит клики (фон панели)
    Block,
    CloseModal,
    // бар
    Launcher,
    Power,
    Settings,
    Ws(u64),
    Task(u64),
    Tray(String),
    Mode,
    Kb,
    Popup(&'static str),
    // лаунчер
    LRun(usize),
    LTab(bool),
    LPin(usize),
    LDel(usize),
    LWipe,
    // питание
    PowerDo(usize),
    // настройки
    STab(&'static str),
    /// первая настройка (welcome.rs): шаг, назад/дальше, язык, стиль, модуль, установка, niri, чужая программа
    WStep(usize),
    WBack,
    WNext,
    WLang(&'static str),
    WPlain(bool),
    WMod(usize),
    WInstall,
    WNiri(&'static str),
    WOther(usize),
    /// «все настройки» шага: номер вкладки, u8::MAX — обратно к шагу
    WMore(u8),
    /// выбор на странице мастера: что именно, см. wizpages::set
    WSet(&'static str),
    /// вопрос при закрытии мастера: 0 — продолжить, 1 — закрыть и вернуться позже, 2 — больше не показывать
    WLeave(u8),
    /// настройки → вид: набор значков / курсоров по номеру карточки (iconsets.rs)
    SIconSet(usize),
    SCursorSet(usize),
    /// колесо над содержимым вкладки и над списком вкладок
    SScroll,
    SSideScroll,
    SPreset(usize),
    SBool(&'static str),
    /// настройки → цвет → программы: красить ли эту программу (config::THEME_APPS)
    SThemeApp(&'static str),
    /// свой шаблон, изменённый руками, — перезаписать (#27)
    STplForce(usize),
    /// свой шаблон: включить или выключить (#31, templatesOff)
    STplToggle(usize),
    /// готовый шаблон из каталога (catalog.rs) — в свои
    STplAdd(usize),
    SSlider(&'static str, f32, f32, i32),
    SCycle(&'static str),
    /// место на схеме экрана: бар (bar_pos), карточки уведомлений (notif_pos), OSD (osd_pos) — сразу значение
    SPlace(&'static str, &'static str),
    /// настройки → анимации: группа (config::ANIM_GROUPS) и что в ней менять (animtab.rs)
    SAnim(u8, u8),
    /// число в пикселях (config::NUMS): на шаг вниз (-1) или вверх (+1); колесо — тоже
    SNum(&'static str, i32),
    /// вкладка «обновления»: выбрать, куда откатиться («prev» — прошлая сборка, иначе версия)
    UpdSel(String),
    /// шторка для трансляций: опустить / поднять
    Curtain,
    /// вкладка «обои» (wall.rs)
    SWall(&'static str),
    /// выбор обоев (walls.rs): клетка, колесо над сеткой, другая папка, другой файл, назад
    WallPick(usize),
    WallsScroll,
    WallsDir,
    WallsFile,
    WallsBack,
    /// модуль бара во вкладке «бар»: тащить (сторона, индекс), новый, убрать
    SModDrag(usize, usize),
    SModNew(String),
    /// щелчок по модулю-скрипту: его команда
    Plug(String),
    NightToggle,
    ReplySend,
    SModDel(usize, usize),
    /// свои темы: применить, удалить (themetab.rs)
    STheme(usize),
    SThemeDel(usize),
    SWidgetAdd(&'static str),
    SWidgetDel(u32),
    SWidgetOpt(u32, &'static str),
    SWidgetSel(u32),
    SEdit,
    SResetPos,
    SField(&'static str),
    // меню
    Menu(usize),
    MenuBack,
    // всплывашки
    CalMonth(i32),
    VolSlider,
    /// панель управления (control.rs): вертикальные ползунки громкости и яркости
    CtlVol,
    CtlBright,
    /// пустое место бара: ПКМ — панель управления
    BarBg,
    /// запереть экран сейчас (плитка панели)
    LockNow,
    /// настройки → бар: плитка панели управления вкл-выкл
    SCtlTile(&'static str),
    VolMute,
    Sink(String),
    /// микрофон по умолчанию (всплывашка звука, #10)
    Source(String),
    MicSlider,
    MicMute,
    AppVol(u32),
    AppMute(u32),
    MediaPlay,
    MediaNext,
    MediaPrev,
    /// перемотка по полосе позиции (тянуть; сама перемотка — при отпускании)
    MediaSeek,
    /// вид всплывашки музыки — следующий (#13)
    MediaView,
    /// шапка всплывашки музыки: другой плеер (#24)
    MediaPlayer,
    Wifi(bool),
    WifiNet(String),
    WifiRescan,
    NetEditor,
    BtPower(bool),
    BtDev(String),
    BtScan,
    /// забыть сопряжённое устройство (#36)
    BtForget(String),
    /// настройки → bluetooth: все устройства (#36)
    BtSettings,
    /// закрыть всплывашку и открыть вкладку настроек (панель управления → «настроить бар»)
    OpenTab(&'static str),
    /// остановить трансляцию PipeWire (сеанс niri)
    CastStop(u64),
    /// щелчком по окну спрятать его программу из трансляций (или вернуть)
    CastPick,
    /// полный менеджер bluetooth (blueman или bluetoothctl в терминале)
    BtManager,
    Dnd,
    NotifClear,
    NotifDel(u32),
    NotifAct(u32, String),
    NotifClick(u32),
    Pavu,
    SysMon,
    // заголовки окон
    TDrag(u64),
    TMin(u64),
    TMax(u64),
    TClose(u64),
    /// угол плавающего окна на «столе»: тянуть — размер
    TSize(u64),
    /// заголовок окна настроек: тянуть — окно едет
    MDrag,
    // виджеты
    WDrag(u32),
    WMenu(u32),
    EditDone,
    DockItem(usize),
    DockLauncher,
    DockTrash,
    DockPin(String, i32),
    LockField,
    Prof(String),
    Disp(&'static str),
    DispSel(usize),
    DispDrag(usize),
    // клавиши (keys.rs)
    KeyCat(&'static str),
    KeyRec(String),
    KeyReset(String),
    KeyDel(usize),
    KeyAdd,
    KeyScroll,
}

impl Act {
    pub fn is_drag(&self) -> bool {
        matches!(self, Act::SSlider(..) | Act::VolSlider | Act::CtlVol | Act::CtlBright | Act::MicSlider | Act::AppVol(_) | Act::MediaSeek | Act::TDrag(_) | Act::TSize(_) | Act::MDrag | Act::WDrag(_) | Act::DispDrag(_) | Act::SModDrag(..) | Act::SModNew(_))
    }
}

impl yami_px::ui::Action for Act {
    fn block() -> Self {
        Act::Block
    }
}
