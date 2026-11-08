// Службы: что из D-Bus и порталов делает сам шелл (сервер уведомлений, трей, обложки музыки из сети)
// и модули: окно пароля polkit, выбор экрана, автоподключение (yami-agents, его файл agents.json),
// диалог файлов (yami-files). Каждую можно выключить и отдать другой программе (mako, waybar,
// polkit-gnome, GTK-портал).
// Порталы пишутся в ~/.config/xdg-desktop-portal/niri-portals.conf (только если файл наш).

use crate::app::{App, State};
use crate::config::Settings;
use crate::gfx::R;
use crate::settings::{step_in, title};
use crate::ui::{Act, Tone, Ui};

const PORTALS_HEAD: &str = "# yami: written by yami-shell (settings → services); edit it there or in ~/.config/yami/shell.json";

/// niri-portals.conf: файлы и показ экрана — yami или GTK/GNOME, остальное — как в niri
pub fn portals_conf(s: &Settings) -> String {
    let fc = if s.portal_files && crate::proc::have("yami-files") { "yami;gtk;" } else { "gtk;" };
    let sc = if AgentsCfg::load().screencast && crate::proc::have("yami-agents") { "yami-agents;gnome;" } else { "gnome;" };
    format!(
        "{PORTALS_HEAD}\n# file dialogs come from yami-files, the screen-share picker from yami-agents;\n# the rest as niri ships it\n[preferred]\ndefault=gnome;gtk;\norg.freedesktop.impl.portal.Access=gtk;\norg.freedesktop.impl.portal.Notification=gtk;\norg.freedesktop.impl.portal.Secret=gnome-keyring;\norg.freedesktop.impl.portal.FileChooser={fc}\norg.freedesktop.impl.portal.ScreenCast={sc}\n"
    )
}

/// переписать, если файла нет или он наш; свой (без «# yami:») — не трогаем
pub fn write_portals(s: &Settings) {
    let p = crate::config::config_dir().join("xdg-desktop-portal/niri-portals.conf");
    // файл без нашей метки — свой у человека, не трогаем; тот же текст write_atomic не пишет (а зовут
    // сюда каждое изменение настроек, каждый шаг ползунка)
    // без наших бэкендов свой файл не заводим (порталы — как их отдаёт niri)
    let mine = match std::fs::read_to_string(&p) {
        Ok(t) => t.starts_with("# yami:"),
        Err(_) => crate::proc::have("yami-files") || crate::proc::have("yami-agents"),
    };
    if mine {
        let _ = crate::config::write_atomic(&p, &portals_conf(s));
    }
}

/// переключатели вкладки; true — ключ наш
pub fn toggle(s: &mut Settings, k: &str) -> bool {
    let v = match k {
        "agent_notif" => &mut s.agent_notif,
        "agent_tray" => &mut s.agent_tray,
        "notif_images" => &mut s.notif_images,
        "notif_cast" => &mut s.notif_cast,
        "media_art_net" => &mut s.media_art_net,
        "lyrics_net" => &mut s.lyrics_net,
        "portal_files" => &mut s.portal_files,
        _ => return false,
    };
    *v = !*v;
    true
}

impl App {
    pub fn agents_cycle(&mut self, k: &str) -> bool {
        let s = &mut self.st.set;
        match k {
            "notif_max" => s.notif_max = step_in(&[1, 2, 3, 4, 5, 6], s.notif_max as i32) as u32,
            "notif_history" => s.notif_history = step_in(&[20, 50, 100, 200], s.notif_history as i32) as u32,
            "restart_shell" => {
                self.save_now();
                self.restart = true;
                return true;
            }
            _ => return false,
        }
        self.settings_changed();
        true
    }
}

/// файл настроек yami-agents: шелл только правит его и просит службу перечитать
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentsCfg {
    pub polkit: bool,
    pub screencast: bool,
    pub automount: bool,
    pub automount_open: bool,
}

impl Default for AgentsCfg {
    fn default() -> AgentsCfg {
        AgentsCfg { polkit: true, screencast: true, automount: true, automount_open: false }
    }
}

impl AgentsCfg {
    fn path() -> std::path::PathBuf {
        crate::config::config_dir().join("yami/agents.json")
    }

    pub fn load() -> AgentsCfg {
        std::fs::read_to_string(Self::path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    /// переключить часть; служба перечитает настройки, когда освободится
    pub fn flip(k: &str) {
        let mut c = AgentsCfg::load();
        match k {
            "agent_polkit" => c.polkit = !c.polkit,
            "agent_cast" => c.screencast = !c.screencast,
            "automount" => c.automount = !c.automount,
            "automount_open" => c.automount_open = !c.automount_open,
            _ => return,
        }
        let _ = crate::config::write_atomic(&Self::path(), &(serde_json::to_string_pretty(&c).unwrap_or_default() + "\n"));
        App::spawn(&["yami-agents".to_string(), "--reload".to_string()]);
    }
}

pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let ag = AgentsCfg::load();
    let t = ui.t;
    let half = (b.w - 20) / 2;
    let x2 = b.x + half + 20;
    // части модулей (yami-agents, yami-files) — только если модуль стоит
    let here = st.mods_here;
    let mut y = title(ui, b.x, b.y, b.w, if here.agents { "трей и пароли" } else { "трей" });
    ui.toggle(R::new(b.x, y, half, 40), "трей", "значки программ в баре", s.agent_tray, Act::SBool("agent_tray"));
    if here.agents {
        ui.toggle(R::new(x2, y, half, 40), "окно пароля polkit", "выкл — другой агент", ag.polkit, Act::SBool("agent_polkit"));
    }
    y += 46;
    if here.agents {
        y = title(ui, b.x, y, b.w, "флешки и телефоны");
        ui.toggle(R::new(b.x, y, half, 40), "подключать сами", "флешки, диски, телефоны, камеры", ag.automount, Act::SBool("automount"));
        ui.toggle(R::new(x2, y, half, 40), "сразу открывать в файлах", "иначе — карточка «открыть»", ag.automount_open, Act::SBool("automount_open"));
        y += 46;
    }
    if here.files || here.agents {
        y = title(ui, b.x, y, b.w, "порталы");
        let mut x = b.x;
        if here.files {
            ui.toggle(R::new(x, y, half, 40), "диалог файлов yami-files", "выкл — диалог GTK", s.portal_files, Act::SBool("portal_files"));
            x = x2;
        }
        if here.agents {
            ui.toggle(R::new(x, y, half, 40), "выбор экрана для трансляций", "выкл — окно GNOME", ag.screencast, Act::SBool("agent_cast"));
        }
        y += 46;
    }
    y = title(ui, b.x, y, b.w, "сеть");
    ui.toggle(R::new(b.x, y, half, 40), "обложки музыки из сети", "ссылки плеера; выкл — с диска", s.media_art_net, Act::SBool("media_art_net"));
    ui.toggle(R::new(x2, y, half, 40), "тексты песен из сети", "lrclib.net; выкл — плеер и .lrc", s.lyrics_net, Act::SBool("lyrics_net"));
    y += 46;
    // погода (#6): город или координаты для open-meteo.com, единицы
    let gf = st.field_focus == Some("weather_city");
    ui.field(R::new(b.x, y, half, 26), &st.weather_field, if s.weather_city.is_empty() { "погода: город или широта, долгота" } else { &s.weather_city }, gf, false, Act::SField("weather_city"));
    let units = if s.weather_units == "f" { "°F, mph" } else { "°C, км/ч" };
    crate::settings::opt_btn(ui, R::new(x2, y, half, 26), "погода", units, Act::SCycle("weather_units"));
    y += 44;
    ui.text_in(crate::font::PX, R::new(b.x, y, b.w, 20), "трей и пароли — после перезапуска; порталы — после входа", t.text_faint);
    y += 26;
    ui.button(R::new(b.x, y, 220, 28), "перезапустить шелл", "reboot", Tone::Plain, false, true, Act::SCycle("restart_shell"));
}
