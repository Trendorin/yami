// Окно мастера: одна поверхность layer-shell на весь экран, ввод — в wiz.rs через зоны.

use crate::run::{self, Ev};
use crate::wiz::{self, Act, Page, Ui, W, RESTYLE};
use calloop::channel;
use calloop::EventLoop;
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

pub struct App {
    conn: Connection,
    qh: QueueHandle<App>,
    handle: calloop::LoopHandle<'static, App>,
    registry: RegistryState,
    seat: SeatState,
    outputs: OutputState,
    shm: Shm,
    layer: LayerSurface,
    _frac: Option<frac::WpFractionalScaleV1>,
    vp: Option<wp_viewport::WpViewport>,
    cursor_mgr: Option<CursorShapeManager>,
    cursor_dev: Option<WpCursorShapeDeviceV1>,
    cursor_now: Option<Shape>,
    enter_serial: u32,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    mods: Modifiers,
    w: i32,
    h: i32,
    scale: f64,
    pool: Option<SlotPool>,
    zones: Vec<Zone<Act>>,
    configured: bool,
    dirty: bool,
    frame_pending: bool,
    anim: bool,
    ptr: Option<(f64, f64)>,
    press: Option<Act>,
    scratch: Vec<u32>,
    fonts: Fonts,
    icons: Icons,
    wiz: W,
    tx: channel::Sender<Ev>,
    quit: bool,
}

pub fn run(wiz: W) -> i32 {
    let Ok(conn) = Connection::connect_to_env() else {
        eprintln!("yami-setup: no Wayland display");
        return 1;
    };
    let Ok((globals, mut queue)) = registry_queue_init::<App>(&conn) else { return 1 };
    let qh = queue.handle();
    let Ok(mut ev) = EventLoop::<'static, App>::try_new() else { return 1 };
    let handle = ev.handle();
    let (tx, rx) = channel::channel::<Ev>();
    let _ = handle.insert_source(rx, |e, _, app: &mut App| {
        if let channel::Event::Msg(m) = e {
            app.wiz.on_event(m);
            app.dirty = true;
        }
    });
    let (Ok(compositor), Ok(shm), Ok(layer_shell)) = (CompositorState::bind(&globals, &qh), Shm::bind(&globals, &qh), LayerShell::bind(&globals, &qh)) else {
        eprintln!("yami-setup: the compositor has no layer-shell");
        return 1;
    };
    let frac_mgr: Option<frac_mgr::WpFractionalScaleManagerV1> = globals.bind(&qh, 1..=1, ()).ok();
    let viewporter: Option<wp_viewporter::WpViewporter> = globals.bind(&qh, 1..=1, ()).ok();
    let surface = compositor.create_surface(&qh);
    let fr = frac_mgr.as_ref().map(|m| m.get_fractional_scale(&surface, &qh, 0u32));
    let vp = viewporter.as_ref().map(|v| v.get_viewport(&surface, &qh, ()));
    let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some("yami-setup"), None);
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    layer.commit();
    let fonts = Fonts::with(wiz.look.font("setup"));
    let mut app = App {
        conn: conn.clone(), qh: qh.clone(), handle: handle.clone(), registry: RegistryState::new(&globals), seat: SeatState::new(&globals, &qh), outputs: OutputState::new(&globals, &qh), shm, layer, _frac: fr, vp,
        cursor_mgr: CursorShapeManager::bind(&globals, &qh).ok(), cursor_dev: None, cursor_now: None, enter_serial: 0, pointer: None, keyboard: None, mods: Modifiers::default(),
        w: 0, h: 0, scale: 1.0, pool: None, zones: vec![], configured: false, dirty: true, frame_pending: false, anim: false, ptr: None, press: None, scratch: vec![], fonts, icons: Icons::new(""), wiz, tx, quit: false,
    };
    app.wiz.no_password = !run::needs_password();
    app.wiz.fill_from_system();
    app.apply_look();
    let _ = queue.roundtrip(&mut app);
    let _ = WaylandSource::new(conn.clone(), queue).insert(handle.clone());
    loop {
        if ev.dispatch(Some(Duration::from_millis(250)), &mut app).is_err() {
            return 1;
        }
        app.flush();
        if app.quit {
            let _ = app.conn.flush();
            return 0;
        }
    }
}

impl App {
    fn flush(&mut self) {
        if self.dirty && self.configured && !self.frame_pending {
            self.draw();
        }
        let _ = self.conn.flush();
    }

    fn draw(&mut self) {
        self.dirty = false;
        let (w, h) = (self.w, self.h);
        if w <= 0 || h <= 0 {
            return;
        }
        let (pw, ph) = if self.vp.is_none() || self.scale == 1.0 { (w, h) } else { yami_px::gfx::phys(w, h, self.scale) };
        let need = (pw * ph * 4) as usize;
        if self.pool.as_ref().map(|p| p.len() < need).unwrap_or(true) {
            self.pool = SlotPool::new(need * 2, &self.shm).ok();
        }
        let Some(pool) = self.pool.as_mut() else { return };
        let Ok((buffer, bytes)) = pool.create_buffer(pw, ph, pw * 4, wl_shm::Format::Argb8888) else { return };
        // SAFETY: slot-пул выравнивает буфер по 4 байтам, длина кратна 4, буфер живёт до attach
        let out = unsafe { std::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut u32, bytes.len() / 4) };
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        scratch.resize((w * h) as usize, 0);
        {
            let mut ui: Ui = yami_px::ui::Ui { cv: Canvas::new(&mut scratch, w, h), f: &mut self.fonts, ic: &mut self.icons, t: self.wiz.look.theme, zones: vec![], ptr: self.ptr, down: self.press.is_some(), anim: false, blink: true, tr: |s| s };
            wiz::draw(&mut ui, &self.wiz, w, h);
            self.zones = std::mem::take(&mut ui.zones);
            self.anim = ui.anim;
        }
        if (pw, ph) != (w, h) {
            yami_px::gfx::upscale(&scratch, w, h, out, pw, ph);
        } else {
            out[..scratch.len()].copy_from_slice(&scratch);
        }
        self.scratch = scratch;
        let surf = self.layer.wl_surface().clone();
        if let Some(vp) = &self.vp {
            vp.set_destination(w, h);
        }
        surf.damage_buffer(0, 0, pw, ph);
        if self.anim {
            surf.frame(&self.qh, surf.clone());
            self.frame_pending = true;
        }
        let _ = buffer.attach_to(&surf);
        surf.commit();
    }

    /// вид изменился: весь мастер перерисовать новым стилем
    fn apply_look(&mut self) {
        let font = self.wiz.look.font("setup").to_string();
        self.wiz.look = Look::from_json(&self.wiz.style_json(), Some(self.wiz.light));
        yami_px::i18n::set(&self.wiz.lang);
        if self.wiz.look.font("setup") != font {
            self.fonts = Fonts::with(self.wiz.look.font("setup"));
        }
        self.dirty = true;
    }

    /// выбор — в настройки шелла: он заметит файл и перекрасится сам
    fn save_look(&self) {
        let path = yami_px::paths::shell_settings();
        let mut v = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
        if let (Some(o), Some(new)) = (v.as_object_mut(), self.wiz.settings_json().as_object()) {
            for (k, val) in new {
                o.insert(k.clone(), val.clone());
            }
        }
        let _ = yami_px::paths::write_atomic(&path, &(serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"));
    }

    fn finish_marker(&self) {
        let _ = std::fs::remove_file(yami_px::paths::state_dir().join("yami/first-run"));
    }

    fn start_install(&mut self) {
        self.save_look();
        let mods: Vec<String> = self.wiz.mods.iter().filter(|m| m.on).map(|m| m.id.to_string()).collect();
        let restyle: Vec<String> = RESTYLE.iter().enumerate().filter(|(i, _)| self.wiz.restyle[*i]).map(|(_, r)| r.0.to_string()).collect();
        let pass = std::mem::take(&mut self.wiz.field.text);
        self.wiz.field.wipe();
        self.wiz.wrong = false;
        self.wiz.failed = None;
        self.wiz.lines.clear();
        self.wiz.step = None;
        self.wiz.running = true;
        self.wiz.turn(Page::Install);
        let tx = self.tx.clone();
        run::start(mods, restyle, self.wiz.lang.clone(), pass, move |e| {
            let _ = tx.send(e);
        });
    }

    fn act(&mut self, a: Act) {
        match a {
            Act::Lang(id) => {
                self.wiz.lang = id.to_string();
                self.apply_look();
            }
            Act::Preset(plain) => {
                self.wiz.plain = plain;
                self.apply_look();
            }
            Act::Accent(c) => {
                self.wiz.accent = c;
                self.apply_look();
            }
            Act::Light(v) => {
                self.wiz.light = v;
                self.apply_look();
            }
            Act::Module(i) => {
                if let Some(m) = self.wiz.mods.get_mut(i) {
                    m.on = !m.on;
                }
            }
            Act::Restyle(i) => {
                if let Some(r) = self.wiz.restyle.get_mut(i) {
                    *r = !*r;
                }
            }
            Act::Next => match wiz::next_of(self.wiz.page) {
                Some(p) => self.wiz.turn(p),
                None if self.wiz.page == Page::Confirm => self.start_install(),
                None => {}
            },
            Act::Back => {
                if self.wiz.page == Page::Install {
                    if !self.wiz.running {
                        self.wiz.failed = None;
                        self.wiz.turn(Page::Confirm);
                    }
                } else if let Some(p) = wiz::prev_of(self.wiz.page) {
                    self.wiz.turn(p);
                }
            }
            Act::Skip | Act::Finish => {
                self.finish_marker();
                self.quit = true;
            }
            Act::Pass | Act::Block => {}
        }
        self.dirty = true;
    }

    fn key(&mut self, e: &KeyEvent) {
        match e.keysym {
            Keysym::Return | Keysym::KP_Enter => self.act(if self.wiz.page == Page::Done { Act::Finish } else { Act::Next }),
            Keysym::Escape if !self.wiz.running => self.act(Act::Back),
            _ if self.wiz.page == Page::Confirm => {
                let f = &mut self.wiz.field;
                match e.keysym {
                    Keysym::BackSpace => f.backspace(self.mods.ctrl),
                    Keysym::Delete => f.delete(),
                    Keysym::Left => f.left(),
                    Keysym::Right => f.right(),
                    _ => {
                        if let Some(s) = e.utf8.as_deref().filter(|s| !s.is_empty() && !s.chars().any(|c| c.is_control())) {
                            f.insert(s);
                        }
                    }
                }
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn zone_at(&self, x: f64, y: f64) -> Option<Zone<Act>> {
        self.zones.iter().rev().find(|z| z.r.contains(x, y)).cloned()
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

    fn on_pointer(&mut self, e: &PointerEvent) {
        let (x, y) = e.position;
        match e.kind {
            PointerEventKind::Enter { serial } => {
                self.enter_serial = serial;
                self.cursor_now = None;
                self.ptr = Some((x, y));
                self.set_cursor(Shape::Default);
            }
            PointerEventKind::Leave { .. } => {
                self.ptr = None;
                self.press = None;
            }
            PointerEventKind::Motion { .. } => {
                self.ptr = Some((x, y));
                let shape = match self.zone_at(x, y).map(|z| z.cur) {
                    Some(Cur::Pointer) => Shape::Pointer,
                    Some(Cur::Text) => Shape::Text,
                    _ => Shape::Default,
                };
                self.set_cursor(shape);
            }
            PointerEventKind::Press { button, .. } if button == 0x110 => self.press = self.zone_at(x, y).map(|z| z.act),
            PointerEventKind::Release { .. } => {
                if let Some(a) = self.press.take() {
                    if self.zone_at(x, y).is_some_and(|z| z.act == a) {
                        self.act(a);
                    }
                }
            }
            _ => {}
        }
        self.dirty = true;
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.frame_pending = false;
        if self.anim {
            self.dirty = true;
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
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.quit = true;
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface, c: LayerSurfaceConfigure, _: u32) {
        self.w = c.new_size.0 as i32;
        self.h = c.new_size.1 as i32;
        self.configured = true;
        self.dirty = true;
        self.frame_pending = false;
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
            let _ = id;
            if app.scale != v {
                app.scale = v;
                app.dirty = true;
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
