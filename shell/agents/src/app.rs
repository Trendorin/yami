// Окна yami-agents: поверхность layer-shell с окном и затемнение под ней — только пока идёт запрос;
// в простое нет ни одной поверхности. Нити служб (polkit, показ экрана, тома) пишут сюда в канал.

use crate::automount::{Am, Vol};
use crate::{polkit, screen};
use calloop::channel::{self, Sender};
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, LoopHandle};
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{cursor_shape::CursorShapeManager, PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use std::sync::mpsc;
use std::time::Duration;
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{Shape, WpCursorShapeDeviceV1};
use wayland_protocols::wp::fractional_scale::v1::client::{wp_fractional_scale_manager_v1 as frac_mgr, wp_fractional_scale_v1 as frac};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use yami_px::font::Fonts;
use yami_px::gfx::Canvas;
use yami_px::icons::Icons;
use yami_px::look::Look;
use yami_px::ui::{Cur, Zone};

#[derive(Clone, PartialEq, Debug)]
pub enum Act {
    Block,
    AuthField,
    AuthOk,
    AuthCancel,
    ScreenSel(usize),
    ScreenOk,
    ScreenCancel,
}

impl yami_px::ui::Action for Act {
    fn block() -> Self {
        Act::Block
    }
}

pub type Ui<'a, 'b> = yami_px::ui::Ui<'a, 'b, Act>;

pub enum Msg {
    Auth(polkit::Ev),
    /// показ экрана спрашивает, что показать: типы источников и куда ответить
    PickScreen(u32, mpsc::Sender<Option<String>>),
    /// тома и заперт ли экран
    Volumes(Vec<Vol>, bool),
    AutoMounted(Vol, Result<Option<String>, String>),
    /// нажали «открыть» в уведомлении: путь или `unlock:устройство имя`
    Open(String),
    /// зашифрованный диск открыт или нет
    Unlocked(String, Result<String, String>),
    Reload,
    /// занятых не осталось: пора ли exec, решает try_reload
    Idle,
}

#[derive(PartialEq, Clone, Copy)]
enum Kind {
    Dim,
    Panel,
}

struct Surf {
    id: u32,
    kind: Kind,
    layer: LayerSurface,
    w: i32,
    h: i32,
    scale: f64,
    _frac: Option<frac::WpFractionalScaleV1>,
    vp: Option<wp_viewport::WpViewport>,
    pool: Option<SlotPool>,
    zones: Vec<Zone<Act>>,
    configured: bool,
    dirty: bool,
    frame_pending: bool,
    anim: bool,
}

pub struct App {
    conn: Connection,
    qh: QueueHandle<App>,
    pub handle: LoopHandle<'static, App>,
    registry: RegistryState,
    seat: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    shm: Shm,
    layer_shell: LayerShell,
    frac_mgr: Option<frac_mgr::WpFractionalScaleManagerV1>,
    viewporter: Option<wp_viewporter::WpViewporter>,
    cursor_mgr: Option<CursorShapeManager>,
    cursor_dev: Option<WpCursorShapeDeviceV1>,
    cursor_now: Option<Shape>,
    enter_serial: u32,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pub mods: Modifiers,
    surfs: Vec<Surf>,
    next_id: u32,
    ptr: Option<(u32, f64, f64)>,
    press: Option<(u32, Act)>,
    scratch: Vec<u32>,
    pub fonts: Fonts,
    pub icons: Icons,
    pub look: Look,
    pub cfg: crate::cfg::Cfg,
    pub tx: Sender<Msg>,
    pub auth: Vec<polkit::Req>,
    pub pick: Option<screen::Pick>,
    pub apps: Vec<yami_px::icons::App>,
    pub am: Am,
    /// первый запуск в этом сеансе (иначе первый взгляд на тома только запоминает их)
    pub restarted: bool,
    reload_wanted: bool,
    _lock: Option<std::fs::File>,
}

/// монитор с фокусом (niri); на других композиторах решает он сам
fn focused_output() -> Option<String> {
    std::env::var_os("NIRI_SOCKET")?;
    let o = std::process::Command::new("niri").args(["msg", "-j", "focused-output"]).output().ok()?;
    serde_json::from_slice::<serde_json::Value>(&o.stdout).ok()?.get("name")?.as_str().map(str::to_string)
}

/// метка «уже запускались в этом сеансе»: папка выполнения живёт до выхода из системы
fn first_start_in_session() -> bool {
    let Some(d) = std::env::var_os("XDG_RUNTIME_DIR") else { return true };
    let p = std::path::PathBuf::from(d).join("yami-agents.started");
    if p.exists() {
        return false;
    }
    let _ = std::fs::write(p, b"");
    true
}

pub fn run(look: Look, cfg: crate::cfg::Cfg, lock: Option<std::fs::File>) -> i32 {
    let Ok(conn) = Connection::connect_to_env() else {
        eprintln!("yami-agents: no Wayland display");
        return 1;
    };
    let Ok((globals, mut queue)) = registry_queue_init::<App>(&conn) else { return 1 };
    let qh = queue.handle();
    let Ok(mut ev) = EventLoop::<'static, App>::try_new() else { return 1 };
    let handle = ev.handle();
    let (tx, rx) = channel::channel::<Msg>();
    {
        let t = tx.clone();
        crate::reload::on_idle(move || {
            let _ = t.send(Msg::Idle);
        });
    }
    let _ = handle.insert_source(rx, |e, _, app: &mut App| {
        if let channel::Event::Msg(m) = e {
            app.on_msg(m);
        }
    });
    let (Ok(compositor), Ok(shm), Ok(layer_shell)) = (CompositorState::bind(&globals, &qh), Shm::bind(&globals, &qh), LayerShell::bind(&globals, &qh)) else {
        eprintln!("yami-agents: the compositor has no layer-shell");
        return 1;
    };
    let fonts = Fonts::with(look.font("agents"));
    let mut app = App {
        conn: conn.clone(),
        qh: qh.clone(),
        handle: handle.clone(),
        registry: RegistryState::new(&globals),
        seat: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        compositor,
        shm,
        layer_shell,
        frac_mgr: globals.bind(&qh, 1..=1, ()).ok(),
        viewporter: globals.bind(&qh, 1..=1, ()).ok(),
        cursor_mgr: CursorShapeManager::bind(&globals, &qh).ok(),
        cursor_dev: None,
        cursor_now: None,
        enter_serial: 0,
        pointer: None,
        keyboard: None,
        mods: Modifiers::default(),
        surfs: vec![],
        next_id: 1,
        ptr: None,
        press: None,
        scratch: vec![],
        fonts,
        icons: Icons::new(&look.icon_theme),
        look,
        cfg,
        tx: tx.clone(),
        auth: vec![],
        pick: None,
        apps: vec![],
        am: Am::default(),
        restarted: !first_start_in_session(),
        reload_wanted: false,
        _lock: lock,
    };
    app.icons.set_style(app.look.theme.icon_ramp(), app.look.icon_pixel, app.look.icon_tint);
    if queue.roundtrip(&mut app).is_err() {
        return 1;
    }
    let _ = WaylandSource::new(conn.clone(), queue).insert(handle.clone());
    if app.cfg.polkit {
        polkit::spawn(tx.clone());
    }
    if app.cfg.screencast {
        screencast::serve(tx.clone());
    }
    app.automount_sync();
    {
        let tx = tx.clone();
        crate::reload::watch(move || {
            let _ = tx.send(Msg::Reload);
        });
    }
    // старт позади: код старта — из памяти
    yami_px::shed_code_later(30);
    loop {
        if ev.dispatch(None, &mut app).is_err() {
            return 1;
        }
        app.flush();
    }
}

use crate::screencast;

impl App {
    pub fn dirty(&mut self) {
        for s in self.surfs.iter_mut().filter(|s| s.kind == Kind::Panel) {
            s.dirty = true;
        }
    }

    fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Auth(e) => self.on_auth(e),
            Msg::PickScreen(types, reply) => self.screen_ask(types, reply),
            Msg::Volumes(v, locked) => self.automount_volumes(v, locked),
            Msg::AutoMounted(v, r) => self.automount_done(v, r),
            Msg::Open(what) => self.note_open(what),
            Msg::Unlocked(name, r) => self.on_unlocked(name, r),
            Msg::Reload => {
                self.reload_wanted = true;
            }
            Msg::Idle => {}
        }
        self.try_reload();
    }

    /// перейти на новый бинарь, как только нет ни запросов, ни показа экрана
    fn try_reload(&mut self) {
        if self.reload_wanted && self.auth.is_empty() && self.pick.is_none() && crate::reload::BUSY.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            crate::reload::exec_self();
        }
    }

    /// окно по запросу: сначала пароли, потом выбор экрана; ничего нет — ни одной поверхности
    pub fn sync_panel(&mut self) {
        let want = if !self.auth.is_empty() {
            Some(("yami-auth", polkit::SIZE))
        } else {
            self.pick.as_ref().map(|p| ("yami-screen", screen::size(p)))
        };
        let Some((ns, (w, h))) = want else {
            self.surfs.clear();
            let _ = self.conn.flush();
            self.try_reload();
            return;
        };
        if self.surfs.iter().any(|s| s.kind == Kind::Panel) {
            // другой запрос в том же окне: размер мог поменяться
            for s in self.surfs.iter_mut().filter(|s| s.kind == Kind::Panel) {
                s.layer.set_size(w as u32, (h + 12) as u32);
                s.layer.commit();
                s.dirty = true;
            }
            return;
        }
        self.look_fresh();
        let focus = focused_output();
        let out = focus.and_then(|n| self.outputs.outputs().find(|o| self.outputs.info(o).is_some_and(|i| i.name.as_deref() == Some(n.as_str()))));
        self.make(Kind::Dim, "yami-dim", out.as_ref(), None);
        self.make(Kind::Panel, ns, out.as_ref(), Some((w, h + 12)));
    }

    /// служба живёт весь сеанс, а тему, палитру, стиль и язык меняют когда угодно: перед новым окном
    /// вид перечитывается, если его файлы поменялись (иначе окно пароля и выбор экрана для OBS
    /// оставались в теме, что была при входе)
    fn look_fresh(&mut self) {
        if !self.look.stale() {
            return;
        }
        let look = Look::load("agents");
        crate::i18n::set(&look.lang);
        if look.font("agents") != self.look.font("agents") {
            self.fonts = Fonts::with(look.font("agents"));
        }
        if look.icon_theme != self.look.icon_theme {
            self.icons = Icons::new(&look.icon_theme);
        }
        self.icons.set_style(look.theme.icon_ramp(), look.icon_pixel, look.icon_tint);
        self.look = look;
    }

    fn make(&mut self, kind: Kind, ns: &str, out: Option<&wl_output::WlOutput>, size: Option<(i32, i32)>) {
        let surface = self.compositor.create_surface(&self.qh);
        let id = self.next_id;
        self.next_id += 1;
        let frac = self.frac_mgr.as_ref().filter(|_| kind == Kind::Panel).map(|m| m.get_fractional_scale(&surface, &self.qh, id));
        let vp = self.viewporter.as_ref().map(|v| v.get_viewport(&surface, &self.qh, ()));
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Overlay, Some(ns), out);
        match size {
            Some((w, h)) => {
                layer.set_anchor(Anchor::empty());
                layer.set_size(w as u32, h as u32);
                layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
            }
            None => {
                layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            }
        }
        layer.set_exclusive_zone(-1);
        layer.commit();
        self.surfs.push(Surf { id, kind, layer, w: 0, h: 0, scale: 1.0, _frac: frac, vp, pool: None, zones: vec![], configured: false, dirty: true, frame_pending: false, anim: false });
    }

    fn flush(&mut self) {
        for i in 0..self.surfs.len() {
            let s = &self.surfs[i];
            if s.dirty && s.configured && !s.frame_pending {
                self.draw(i);
            }
        }
        let _ = self.conn.flush();
    }

    fn draw(&mut self, i: usize) {
        let s = &mut self.surfs[i];
        s.dirty = false;
        let (w, h) = (s.w, s.h);
        if w <= 0 || h <= 0 {
            return;
        }
        let dim = s.kind == Kind::Dim;
        let (bw, bh) = if dim { (1, 1) } else { (w, h) };
        let (pw, ph) = if dim || s.vp.is_none() || s.scale == 1.0 { (bw, bh) } else { yami_px::gfx::phys(w, h, s.scale) };
        let need = (pw * ph * 4) as usize;
        if s.pool.as_ref().map(|p| p.len() < need).unwrap_or(true) {
            s.pool = SlotPool::new(need * 2, &self.shm).ok();
        }
        let Some(pool) = s.pool.as_mut() else { return };
        let Ok((buffer, bytes)) = pool.create_buffer(pw, ph, pw * 4, wl_shm::Format::Argb8888) else { return };
        let (out_ptr, out_len) = (bytes.as_mut_ptr() as *mut u32, bytes.len() / 4);
        // SAFETY: slot-пул выравнивает буфер по 4 байтам, длина кратна 4, буфер живёт до attach
        let out = unsafe { std::slice::from_raw_parts_mut(out_ptr, out_len) };
        if dim {
            out[0] = self.look.theme.bg.alpha(0.45).premul();
        } else {
            let up = (pw, ph) != (w, h);
            let mut scratch = std::mem::take(&mut self.scratch);
            scratch.clear();
            scratch.resize((w * h) as usize, 0);
            let ptr = self.ptr.filter(|p| p.0 == s.id).map(|p| (p.1, p.2));
            let down = self.press.as_ref().is_some_and(|p| p.0 == s.id);
            {
                let mut ui: Ui = yami_px::ui::Ui { cv: Canvas::new(&mut scratch, w, h), f: &mut self.fonts, ic: &mut self.icons, t: self.look.theme, zones: vec![], ptr, down, anim: false, blink: true, tr: crate::i18n::tr };
                if !self.auth.is_empty() {
                    polkit::draw(&mut ui, &self.auth, w, h - 12);
                } else if let Some(p) = &self.pick {
                    screen::draw(&mut ui, p, w, h - 12);
                }
                s.zones = std::mem::take(&mut ui.zones);
                s.anim = ui.anim;
            }
            if up {
                yami_px::gfx::upscale(&scratch, w, h, out, pw, ph);
            } else {
                out[..scratch.len()].copy_from_slice(&scratch);
            }
            self.scratch = scratch;
        }
        let surf = s.layer.wl_surface().clone();
        if let Some(vp) = &s.vp {
            vp.set_destination(w, h);
        }
        surf.damage_buffer(0, 0, pw, ph);
        if s.anim {
            surf.frame(&self.qh, surf.clone());
            s.frame_pending = true;
        }
        let _ = buffer.attach_to(&surf);
        surf.commit();
    }

    fn zone_at(&self, id: u32, x: f64, y: f64) -> Option<Zone<Act>> {
        self.surfs.iter().find(|s| s.id == id)?.zones.iter().rev().find(|z| z.r.contains(x, y)).cloned()
    }

    fn set_cursor(&mut self, c: Shape) {
        if self.cursor_now == Some(c) {
            return;
        }
        self.cursor_now = Some(c);
        if let Some(d) = &self.cursor_dev {
            d.set_shape(self.enter_serial, c);
        }
    }

    fn click(&mut self, a: Act, double: bool) {
        match a {
            Act::AuthOk => self.auth_submit(),
            Act::AuthCancel => self.auth_cancel(),
            Act::ScreenSel(_) | Act::ScreenOk | Act::ScreenCancel => self.screen_click(&a, double),
            Act::AuthField | Act::Block => {}
        }
    }

    fn on_pointer(&mut self, e: &PointerEvent) {
        let Some(id) = self.surfs.iter().find(|o| o.layer.wl_surface() == &e.surface).map(|o| o.id) else { return };
        let (x, y) = e.position;
        match e.kind {
            PointerEventKind::Enter { serial } => {
                self.enter_serial = serial;
                self.cursor_now = None;
                self.ptr = Some((id, x, y));
                self.set_cursor(Shape::Default);
            }
            PointerEventKind::Leave { .. } => {
                self.ptr = None;
                self.press = None;
            }
            PointerEventKind::Motion { .. } => {
                self.ptr = Some((id, x, y));
                let shape = match self.zone_at(id, x, y).map(|z| z.cur) {
                    Some(Cur::Pointer) => Shape::Pointer,
                    Some(Cur::Text) => Shape::Text,
                    _ => Shape::Default,
                };
                self.set_cursor(shape);
            }
            PointerEventKind::Press { button, .. } => {
                if button == 0x110 {
                    self.press = self.zone_at(id, x, y).map(|z| (id, z.act));
                }
            }
            PointerEventKind::Release { .. } => {
                if let Some((pid, a)) = self.press.take() {
                    if pid == id && self.zone_at(id, x, y).is_some_and(|z| z.act == a) {
                        self.click(a, false);
                    }
                }
            }
            PointerEventKind::Axis { vertical, .. } => {
                let d = if vertical.value120 != 0 { vertical.value120.signum() } else { vertical.discrete.signum() };
                if d != 0 && self.pick.is_some() {
                    self.screen_wheel(d < 0);
                }
            }
        }
        self.dirty();
    }

    fn key(&mut self, e: &KeyEvent) {
        if !self.auth.is_empty() {
            self.auth_key(e);
        } else if self.pick.is_some() {
            self.screen_key(e);
        }
    }

    /// таймер служб: пока есть, чего ждать
    pub fn arm(&mut self, every: Duration, f: fn(&mut App) -> bool) {
        let _ = self.handle.insert_source(Timer::from_duration(every), move |_, _, app: &mut App| if f(app) { TimeoutAction::ToDuration(every) } else { TimeoutAction::Drop });
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surf: &wl_surface::WlSurface, _: u32) {
        for s in self.surfs.iter_mut().filter(|s| s.layer.wl_surface() == surf) {
            s.frame_pending = false;
            if s.anim {
                s.dirty = true;
            }
        }
    }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, l: &LayerSurface) {
        self.surfs.retain(|s| &s.layer != l);
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, l: &LayerSurface, c: LayerSurfaceConfigure, _: u32) {
        for s in self.surfs.iter_mut().filter(|s| &s.layer == l) {
            s.w = c.new_size.0 as i32;
            s.h = c.new_size.1 as i32;
            s.configured = true;
            s.dirty = true;
            s.frame_pending = false;
        }
        self.flush();
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            let h = self.handle.clone();
            self.keyboard = self.seat.get_keyboard_with_repeat(qh, &seat, None, h, Box::new(|app: &mut App, _, e| app.key(&e))).ok();
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            if let Ok(p) = self.seat.get_pointer(qh, &seat) {
                if let Some(m) = &self.cursor_mgr {
                    self.cursor_dev = Some(m.get_shape_device(&p, qh));
                }
                self.pointer = Some(p);
            }
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, _: Capability) {}
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for App {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, e: KeyEvent) {
        self.key(&e);
    }
    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, e: KeyEvent) {
        self.key(&e);
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, m: Modifiers, _: RawModifiers, _: u32) {
        self.mods = m;
    }
}

impl PointerHandler for App {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for e in events {
            self.on_pointer(e);
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

impl Dispatch<frac_mgr::WpFractionalScaleManagerV1, ()> for App {
    fn event(_: &mut Self, _: &frac_mgr::WpFractionalScaleManagerV1, _: frac_mgr::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wp_viewporter::WpViewporter, ()> for App {
    fn event(_: &mut Self, _: &wp_viewporter::WpViewporter, _: wp_viewporter::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wp_viewport::WpViewport, ()> for App {
    fn event(_: &mut Self, _: &wp_viewport::WpViewport, _: wp_viewport::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<frac::WpFractionalScaleV1, u32> for App {
    fn event(app: &mut Self, _: &frac::WpFractionalScaleV1, e: frac::Event, id: &u32, _: &Connection, _: &QueueHandle<Self>) {
        if let frac::Event::PreferredScale { scale } = e {
            let v = scale as f64 / 120.0;
            for s in app.surfs.iter_mut().filter(|s| s.id == *id && s.scale != v) {
                s.scale = v;
                s.dirty = true;
            }
        }
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_pointer!(App);
delegate_layer!(App);
delegate_registry!(App);
