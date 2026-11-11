// Индикатор приватности в баре (модуль `privacy`, идея из форка yami-lite): горит, пока программа
// пишет микрофон (audio.rs `recorders`: записи PipeWire не с мониторов звука), держит камеру
// (camera.rs) или показывает экран (трансляции niri, niri.rs `casts`); ничего — модуля не видно.
// Щелчок — всплывашка: кто именно, выключить микрофон, остановить трансляцию (только PipeWire: так
// умеет niri), опустить шторку. Что из трёх следить — privacyMic/privacyCam/privacyScreen; следится
// только пока модуль в баре (`privacy_sync`). Кто смотрит трансляцию PipeWire — по связям графа
// (`pw-dump`), один раз на поток.

use crate::app::{App, Msg, SKind, State};
use crate::font::PX;
use crate::gfx::R;
use crate::ui::{Act, Cur, Tone, Ui};
use serde_json::Value;
use std::sync::atomic::Ordering;

/// что сейчас горит: микрофон, камера, экран (с учётом настроек)
pub fn lit(st: &State) -> (bool, bool, bool) {
    let s = &st.set;
    (s.privacy_mic && !st.mic.is_empty(), s.privacy_cam && !st.camera.is_empty(), s.privacy_screen && !st.niri.casts.is_empty())
}

/// модуль бара: значки того, что занято; ничего — ширина 0 (модуль спрятан)
pub fn bar(ui: &mut Ui, st: &State, x: i32, h: i32, measure: bool) -> i32 {
    let (m, c, s) = lit(st);
    let icons: Vec<&str> = [(m, "mic"), (c, "camera"), (s, "screen")].into_iter().filter(|p| p.0).map(|p| p.1).collect();
    if icons.is_empty() {
        return 0;
    }
    let w = icons.len() as i32 * 18 - 4;
    if !measure {
        let t = ui.t;
        let r = R::new(x - 2, 0, w + 4, h - 1);
        let c = if ui.hover(r) { t.text_hi } else { t.warn };
        for (i, ic) in icons.iter().enumerate() {
            ui.cv.icon(ic, x + i as i32 * 18, h / 2 - 7, 2, c);
        }
        ui.zone(r, Act::Popup("privacy"), Cur::Pointer);
    }
    w
}

pub fn popup_h(st: &State) -> i32 {
    let s = &st.set;
    let casts = if s.privacy_screen { st.niri.casts.len().clamp(1, 6) as i32 * 44 } else { 0 };
    44 + s.privacy_mic as i32 * 44 + s.privacy_cam as i32 * 44 + casts + if !st.niri.casts.is_empty() && st.niri.cast_only { 40 } else { 0 } + 8
}

/// строка: значок, что, кто; горит — цветом тревоги
fn row(ui: &mut Ui, r: R, icon: &str, what: &str, who: &str, on: bool) {
    let t = ui.t;
    ui.cv.fill(r, if on { t.surface_hi } else { t.surface });
    ui.cv.icon(icon, r.x + 10, r.y + 6, 2, if on { t.warn } else { t.text_faint });
    ui.text_in(PX, R::new(r.x + 34, r.y + 2, r.w - 150, 18), what, if on { t.text_hi } else { t.text_dim });
    ui.text_in(PX, R::new(r.x + 34, r.y + 20, r.w - 150, 18), who, t.text_faint);
}

pub fn popup(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let (m, c, sc) = lit(st);
    let n = m as usize + c as usize + sc as usize;
    let sub = if n == 0 { crate::i18n::tr("никто не слушает и не смотрит").to_string() } else { String::new() };
    let mut y = crate::popups::head(ui, b, "eye", "приватность", &sub);
    let nobody = crate::i18n::tr("никто");
    if s.privacy_mic {
        let r = R::new(b.x, y, b.w, 40);
        let who = if st.mic.is_empty() { nobody.to_string() } else { st.mic.join(", ") };
        row(ui, r, "mic", "микрофон", &who, m);
        let off = st.audio.mic_muted;
        ui.button(R::new(r.right() - 110, r.y + 7, 102, 26), if off { "включить мик" } else { "заглушить" }, if off { "mute" } else { "mic" }, if off { Tone::Plain } else { Tone::Ghost }, false, st.audio.ready, Act::MicMute);
        y += 44;
    }
    if s.privacy_cam {
        let who = if st.camera.is_empty() { nobody.to_string() } else { st.camera.iter().map(|p| if p == "pipewire" { crate::i18n::tr("через PipeWire").to_string() } else { p.clone() }).collect::<Vec<_>>().join(", ") };
        row(ui, R::new(b.x, y, b.w, 40), "camera", "камера", &who, c);
        y += 44;
    }
    if s.privacy_screen {
        if st.niri.casts.is_empty() {
            row(ui, R::new(b.x, y, b.w, 40), "screen", "показ экрана", nobody, false);
            y += 44;
        }
        for cast in st.niri.casts.iter().take(6) {
            let r = R::new(b.x, y, b.w, 40);
            let (icon, what) = match (cast.window, cast.output.as_str()) {
                (Some(id), _) => {
                    let w = st.niri.wins.iter().find(|w| w.id == id);
                    let name = w.map(|w| if w.title.is_empty() { w.app_id.clone() } else { w.title.clone() }).unwrap_or_default();
                    ("window", crate::tf!("окно: {}", "window: {}", "janela: {}", name))
                }
                (None, "") => ("screen", crate::i18n::tr("экран: ещё не выбран").to_string()),
                (None, o) => ("screen", crate::tf!("экран {}", "screen {}", "tela {}", o)),
            };
            let who = if !cast.who.is_empty() {
                cast.who.clone()
            } else if cast.pipewire {
                crate::i18n::tr("через портал").to_string()
            } else {
                crate::i18n::tr("программа не назвалась").to_string()
            };
            let who = if cast.active { who } else { crate::tf!("{} · на паузе", "{} · paused", "{} · em pausa", who) };
            row(ui, r, icon, &what, &who, cast.active);
            // wlr-screencopy niri остановить не умеет
            if cast.pipewire {
                ui.button(R::new(r.right() - 110, r.y + 7, 102, 26), "остановить", "stop", Tone::Plain, false, true, Act::CastStop(cast.session));
            }
            y += 44;
        }
        // шторка: зрители видят замороженный экран, ты — свой (curtain.rs, yami-niri ≥ 4)
        if !st.niri.casts.is_empty() && st.niri.cast_only {
            let on = st.curtain.is_some();
            ui.button(R::new(b.x, y + 4, b.w, 28), if on { "поднять шторку" } else { "опустить шторку" }, "eye", Tone::Plain, on, true, Act::Curtain);
        }
    }
}

/// кто читает поток PipeWire `node`: узел на том конце связи → его программа (или её клиент)
fn pw_who(node: u32) -> Option<String> {
    let o = std::process::Command::new("pw-dump").stderr(std::process::Stdio::null()).output().ok()?;
    let all: Value = serde_json::from_slice(&o.stdout).ok()?;
    let objs = all.as_array()?;
    let by_id = |id: u64| objs.iter().find(|x| x["id"].as_u64() == Some(id));
    let props = |x: &Value, k: &str| x["info"]["props"][k].as_str().map(str::to_string);
    let peer = objs.iter().filter(|x| x["type"].as_str() == Some("PipeWire:Interface:Link")).find(|x| x["info"]["output-node-id"].as_u64() == Some(node as u64))?;
    let n = by_id(peer["info"]["input-node-id"].as_u64()?)?;
    // client.id — число (в старых PipeWire строка)
    let cid = &n["info"]["props"]["client.id"];
    let client = cid.as_u64().or_else(|| cid.as_str().and_then(|c| c.parse().ok())).and_then(by_id);
    let name = props(n, "application.name").or_else(|| client.and_then(|c| props(c, "application.name"))).or_else(|| client.and_then(|c| props(c, "application.process.binary"))).or_else(|| props(n, "node.name"))?;
    Some(name)
}

impl App {
    /// модуль в баре — следить за микрофоном и камерой; убрали — перестать и забыть
    pub fn privacy_sync(&mut self) {
        let s = &self.st.set;
        let on = s.on_bar("privacy");
        let mic = on && s.privacy_mic;
        if mic && !crate::audio::WANT_REC.swap(true, Ordering::Relaxed) {
            crate::audio::recorders_now(self.tx.clone());
        } else if !mic {
            crate::audio::WANT_REC.store(false, Ordering::Relaxed);
            self.st.mic.clear();
        }
        let cam = on && s.privacy_cam;
        crate::camera::want(cam, &self.tx);
        if !cam {
            self.st.camera.clear();
        }
        if on && s.privacy_screen {
            self.casts_changed();
        }
    }

    /// трансляции поменялись: у новых потоков PipeWire узнать, кто смотрит (связь появляется чуть
    /// позже потока — до трёх попыток), перерисовать бар и всплывашку
    pub fn casts_changed(&mut self) {
        let s = &self.st.set;
        if s.on_bar("privacy") && s.privacy_screen {
            for c in self.st.niri.casts.iter().filter(|c| c.pipewire && c.who.is_empty()) {
                let Some(node) = c.pw_node else { continue };
                if self.st.cast_asked.contains(&(c.stream, node)) {
                    continue;
                }
                self.st.cast_asked.push((c.stream, node));
                let (tx, stream) = (self.tx.clone(), c.stream);
                let _ = std::thread::Builder::new().name("pw-who".into()).spawn(move || {
                    for _ in 0..3 {
                        std::thread::sleep(std::time::Duration::from_millis(700));
                        if let Some(who) = pw_who(node) {
                            let _ = tx.send(Msg::CastWho(stream, who));
                            return;
                        }
                    }
                });
            }
            let live: Vec<u64> = self.st.niri.casts.iter().map(|c| c.stream).collect();
            self.st.cast_asked.retain(|a| live.contains(&a.0));
        }
        self.privacy_dirty();
    }

    pub fn privacy_dirty(&mut self) {
        let pop = self.st.popup.as_ref().is_some_and(|p| p.kind == "privacy");
        self.dirty(&SKind::Bar);
        if pop {
            self.resize_popup();
            self.dirty(&SKind::Popup);
        }
    }
}
