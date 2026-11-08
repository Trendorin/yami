// Окно оверлея: по поверхности layer-shell на каждый монитор, снимок экрана под ней. Процесс живёт,
// пока открыт оверлей и не кончилась работа в нитях (запись файла, распознавание, ожидание кнопки
// в уведомлении). Буфер — логического размера, увеличивает композитор: под ним застывший экран.

use crate::draw::{self, Act, Ui};
use crate::snap::{self, Mode, Snap};
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
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle,
};
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{Shape, WpCursorShapeDeviceV1};
use yami_px::font::Fonts;
use yami_px::gfx::{Canvas, Img};
use yami_px::icons::Icons;
use yami_px::look::Look;
use yami_px::ui::{Cur, Zone};

pub enum Msg {
    SnapText(bool, Option<String>),
    SnapDone { path: Option<PathBuf>, copied: bool, err: Option<String>, thumb: Option<Img> },
    /// нить закончила
    Finished,
}

struct Out {
    id: u32,
    name: String,
    layer: LayerSurface,
    w: i32,
    h: i32,
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
    handle: LoopHandle<'static, App>,
    registry: RegistryState,
    seat: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    shm: Shm,
    layer_shell: LayerShell,
    cursor_mgr: Option<CursorShapeManager>,
    cursor_dev: Option<WpCursorShapeDeviceV1>,
    cursor_now: Option<Shape>,
    enter_serial: u32,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pub mods: Modifiers,
    surfs: Vec<Out>,
    next_id: u32,
    ptr: Option<(u32, f64, f64)>,
    press: Option<(u32, Act)>,
    pub fonts: Fonts,
    icons: Icons,
    pub look: Look,
    pub cfg: crate::cfg::Cfg,
    pub snap: Option<Snap>,
    pub tx: Sender<Msg>,
    t0: Instant,
    /// нитей в работе: пока они идут, процесс жив, даже когда оверлей уже закрыт
    pending: u32,
    /// оверлей закрыт
    pub closing: bool,
    /// вспышка идёт: таймер ведёт snap_tick
    ticking: bool,
    /// замок «оверлей открыт»: второй запуск закрывает первый (main.rs)
    lock: Option<std::fs::File>,
}

/// монитор с фокусом: niri знает точно; у других композиторов берётся первый
fn niri_focus() -> Option<String> {
    std::env::var_os("NIRI_SOCKET")?;
    let o = std::process::Command::new("niri").args(["msg", "-j", "focused-output"]).output().ok()?;
    serde_json::from_slice::<serde_json::Value>(&o.stdout).ok()?.get("name")?.as_str().map(str::to_string)
}

pub fn run(mode: Mode, delay: u32, look: Look, cfg: crate::cfg::Cfg, lock: Option<std::fs::File>) -> i32 {
    let Ok(conn) = Connection::connect_to_env() else {
        eprintln!("yami-shot: no Wayland display");
        return 1;
    };
    let Ok((globals, mut queue)) = registry_queue_init::<App>(&conn) else { return 1 };
    let qh = queue.handle();
    let Ok(mut ev) = EventLoop::<'static, App>::try_new() else { return 1 };
    let handle = ev.handle();
    let (tx, rx) = channel::channel::<Msg>();
    let _ = handle.insert_source(rx, |e, _, app: &mut App| {
        if let channel::Event::Msg(m) = e {
            app.on_msg(m);
        }
    });
    let (Ok(compositor), Ok(shm), Ok(layer_shell)) = (CompositorState::bind(&globals, &qh), Shm::bind(&globals, &qh), LayerShell::bind(&globals, &qh)) else {
        eprintln!("yami-shot: the compositor has no layer-shell");
        return 1;
    };
    let fonts = Fonts::with(look.font("shot"));
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
        fonts,
        icons: Icons::new(""),
        look,
        cfg,
        snap: None,
        tx,
        t0: Instant::now(),
        pending: 0,
        closing: false,
        ticking: false,
        lock,
    };
    // имена мониторов приходят первым обходом реестра
    for _ in 0..2 {
        if queue.roundtrip(&mut app).is_err() {
            return 1;
        }
    }
    let _ = WaylandSource::new(conn.clone(), queue).insert(handle.clone());
    if delay > 0 {
        crate::notify::send(&crate::tf!("скриншот через {} с", "screenshot in {} s", "captura em {} s", delay), "", None, &[], 0);
        std::thread::sleep(Duration::from_secs(delay as u64));
    } else {
        // меню и всплывашки успевают исчезнуть
        std::thread::sleep(Duration::from_millis(60));
    }
    let names = app.output_names();
    let imgs: HashMap<String, Img> = names.iter().filter_map(|o| yami_px::gfx::grab_output(o).map(|i| (o.clone(), i))).collect();
    app.on_snap_ready(imgs, snap::niri_windows(), mode);
    loop {
        if ev.dispatch(Some(Duration::from_secs(1)), &mut app).is_err() {
            break;
        }
        app.flush();
        if app.closing && app.pending == 0 {
            break;
        }
    }
    0
}

impl App {
    pub fn dirty(&mut self) {
        for s in self.surfs.iter_mut() {
            s.dirty = true;
        }
    }

    fn output_names(&self) -> Vec<String> {
        self.outputs.outputs().filter_map(|o| self.outputs.info(&o)?.name).collect()
    }

    /// размер монитора в точках экрана
    pub fn output_size(&self, name: &str) -> (i32, i32) {
        self.outputs.outputs().find_map(|o| self.outputs.info(&o).filter(|i| i.name.as_deref() == Some(name))?.logical_size).unwrap_or((1, 1))
    }

    pub fn focus_out(&self, outs: &[String]) -> String {
        niri_focus().filter(|f| outs.contains(f)).or_else(|| self.output_names().into_iter().find(|n| outs.contains(n))).unwrap_or_default()
    }

    pub fn surf_output(&self, id: u32) -> Option<String> {
        self.surfs.iter().find(|s| s.id == id).map(|s| s.name.clone())
    }

    pub fn surf_size(&self, id: u32) -> (i32, i32) {
        self.surfs.iter().find(|s| s.id == id).map(|s| (s.w, s.h)).unwrap_or((0, 0))
    }

    pub fn make_surface(&mut self, name: &str, keyboard: bool) {
        let Some(o) = self.outputs.outputs().find(|o| self.outputs.info(o).is_some_and(|i| i.name.as_deref() == Some(name))) else { return };
        let surface = self.compositor.create_surface(&self.qh);
        let id = self.next_id;
        self.next_id += 1;
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Overlay, Some("yami-screenshot"), Some(&o));
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(if keyboard { KeyboardInteractivity::Exclusive } else { KeyboardInteractivity::None });
        layer.commit();
        self.surfs.push(Out { id, name: name.to_string(), layer, w: 0, h: 0, pool: None, zones: vec![], configured: false, dirty: true, frame_pending: false, anim: false });
    }

    /// закрыть оверлей; процесс выйдет, когда кончится работа в нитях
    pub fn snap_close(&mut self) {
        self.snap = None;
        self.surfs.clear();
        self.closing = true;
        // дальше процесс только дописывает файл и ждёт кнопку: новому снимку он не мешает
        self.lock = None;
        let _ = self.conn.flush();
    }

    /// работа, ради которой процесс живёт и после закрытия оверлея
    pub fn work<F: FnOnce() + Send + 'static>(&mut self, f: F) {
        self.pending += 1;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            f();
            let _ = tx.send(Msg::Finished);
        });
    }

    /// таймер из оверлея: снимок делает новый процесс, этот закрывается
    pub fn restart_delayed(&mut self, mode: Mode, d: u32) {
        let m = match mode {
            Mode::Area => "area",
            Mode::Window => "window",
            Mode::Screen => "screen",
        };
        let _ = std::process::Command::new(yami_px::paths::self_exe()).args([m, "--delay", &d.to_string()]).spawn();
    }

    fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::SnapText(qr, text) => self.on_snap_text(qr, text),
            Msg::SnapDone { path, copied, err, thumb } => self.on_snap_done(path, copied, err, thumb),
            Msg::Finished => self.pending = self.pending.saturating_sub(1),
        }
    }

    fn flush(&mut self) {
        self.follow_flash();
        for i in 0..self.surfs.len() {
            let s = &self.surfs[i];
            if s.dirty && s.configured && !s.frame_pending {
                self.draw(i);
            }
        }
        let _ = self.conn.flush();
    }

    /// вспышка затвора гаснет сама: пока она идёт, раз в кадр проверять, не пора ли закрыть
    fn follow_flash(&mut self) {
        if self.ticking || !self.snap.as_ref().is_some_and(|s| s.flash.is_some()) {
            return;
        }
        self.ticking = true;
        let _ = self.handle.insert_source(Timer::from_duration(Duration::from_millis(30)), |_, _, app: &mut App| {
            app.snap_tick();
            if app.snap.is_none() {
                app.ticking = false;
                return TimeoutAction::Drop;
            }
            TimeoutAction::ToDuration(Duration::from_millis(30))
        });
    }

    fn draw(&mut self, i: usize) {
        let Some(sn) = self.snap.as_ref() else { return };
        let s = &mut self.surfs[i];
        s.dirty = false;
        let (w, h) = (s.w, s.h);
        if w <= 0 || h <= 0 {
            return;
        }
        let need = (w * h * 4) as usize;
        if s.pool.as_ref().map(|p| p.len() < need).unwrap_or(true) {
            s.pool = SlotPool::new(need * 2, &self.shm).ok();
        }
        let Some(pool) = s.pool.as_mut() else { return };
        let Ok((buffer, bytes)) = pool.create_buffer(w, h, w * 4, wl_shm::Format::Argb8888) else { return };
        // SAFETY: slot-пул выравнивает буфер по 4 байтам (формат ARGB8888), длина кратна 4
        let buf = unsafe { std::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut u32, bytes.len() / 4) };
        buf.fill(0);
        let ptr = self.ptr.filter(|p| p.0 == s.id).map(|p| (p.1, p.2));
        let down = self.press.as_ref().is_some_and(|p| p.0 == s.id);
        {
            let mut ui: Ui = yami_px::ui::Ui { cv: Canvas::new(buf, w, h), f: &mut self.fonts, ic: &mut self.icons, t: self.look.theme, zones: vec![], ptr, down, anim: false, blink: false, tr: crate::i18n::tr };
            draw::draw(&mut ui, sn, self.cfg.loupe, self.t0, &s.name, w, h);
            s.zones = std::mem::take(&mut ui.zones);
            s.anim = ui.anim;
        }
        let surf = s.layer.wl_surface().clone();
        surf.damage_buffer(0, 0, w, h);
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

    fn surf_id(&self, s: &wl_surface::WlSurface) -> Option<u32> {
        self.surfs.iter().find(|o| o.layer.wl_surface() == s).map(|o| o.id)
    }

    /// протяжка, рисование и лупа идут мимо зон; кнопки панели — обычным нажатием
    fn on_pointer(&mut self, e: &PointerEvent) {
        let Some(id) = self.surf_id(&e.surface) else { return };
        let (x, y) = e.position;
        let on_btn = self.zone_at(id, x, y).is_some_and(|z| matches!(z.act, Act::Btn(_)));
        let took = match e.kind {
            PointerEventKind::Motion { .. } => self.snap_pointer(id, 0, x, y, 0, on_btn),
            PointerEventKind::Press { button, .. } => self.snap_pointer(id, 1, x, y, button, on_btn),
            PointerEventKind::Release { button, .. } if self.press.is_none() => self.snap_pointer(id, 2, x, y, button, on_btn),
            PointerEventKind::Axis { vertical, .. } => {
                let d = if vertical.value120 != 0 { vertical.value120.signum() } else { vertical.discrete.signum() };
                if d != 0 {
                    self.snap_wheel(d);
                }
                true
            }
            _ => false,
        };
        if took {
            self.ptr = Some((id, x, y));
            return;
        }
        match e.kind {
            PointerEventKind::Enter { serial } => {
                self.enter_serial = serial;
                self.cursor_now = None;
                self.ptr = Some((id, x, y));
                self.set_cursor(Shape::Crosshair);
                if let Some(sn) = self.snap.as_mut() {
                    if let Some(name) = self.surfs.iter().find(|s| s.id == id).map(|s| s.name.clone()) {
                        sn.ptr = Some((name, x as i32, y as i32));
                    }
                }
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
                    _ => Shape::Crosshair,
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
                        if let Act::Btn(b) = a {
                            self.snap_btn(b);
                        }
                    }
                }
            }
            PointerEventKind::Axis { .. } => {}
        }
        self.dirty();
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
        if self.surfs.is_empty() {
            self.snap_close();
        }
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
            self.keyboard = self.seat.get_keyboard_with_repeat(qh, &seat, None, h, Box::new(|app: &mut App, _, e| app.snap_key(&e))).ok();
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
        self.snap_key(&e);
    }
    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
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

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_pointer!(App);
delegate_layer!(App);
delegate_registry!(App);
