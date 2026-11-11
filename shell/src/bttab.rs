// Настройки → bluetooth (#36): адаптер (вкл, поиск, переподключение), подключённые, сопряжённые и
// найденные рядом устройства с зарядом; подключить, отключить, сопрячь, забыть. Данные — st.bt
// (net.rs: BlueZ по системной шине, сигналы держат их свежими), действия — те же Act, что у
// всплывашки бара; виджет «bluetooth» — widgets.rs `bt`.

use crate::app::State;
use crate::font::PX;
use crate::gfx::R;
use crate::net::BtDev;
use crate::settings::title;
use crate::ui::{Act, Tone, Ui};

pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.bt;
    let half = (b.w - 20) / 2;
    let x2 = b.x + half + 20;
    let mut y = title(ui, b.x, b.y, b.w, "адаптер");
    if !s.present {
        ui.wrap(PX, R::new(b.x, y, b.w, 60), "адаптер не найден: в машине нет bluetooth или не запущена служба bluetooth (systemctl enable --now bluetooth)", t.text_faint, 3);
        return;
    }
    ui.toggle(R::new(b.x, y, half, 40), "bluetooth", if s.powered { "включён" } else { "выключен" }, s.powered, Act::BtPower(!s.powered));
    ui.toggle(R::new(x2, y, half, 40), "переподключать сопряжённые", "наушники и мыши сразу", st.set.bt_reconnect, Act::SBool("bt_reconnect"));
    y += 46;
    if !s.msg.is_empty() {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), &crate::tf!("не вышло: {}", "failed: {}", "falhou: {}", s.msg), t.warn);
        y += 26;
    }
    if !s.powered {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "выключен: устройства появятся, когда включишь", t.text_faint);
        return;
    }
    ui.button(R::new(b.x, y, 220, 28), if s.discovering { "ищу рядом…" } else { "искать устройства" }, "search", Tone::Plain, s.discovering, !s.discovering, Act::BtScan);
    if crate::proc::have("blueman-manager") || crate::proc::have("overskride") {
        ui.button(R::new(b.x + 230, y, 220, 28), "внешний менеджер", "gear", Tone::Ghost, false, true, Act::BtManager);
    }
    y += 40;
    let groups: [(&str, fn(&BtDev) -> bool); 3] = [("подключены", |d| d.connected), ("сопряжены", |d| d.paired && !d.connected), ("рядом", |d| !d.paired)];
    for (name, keep) in groups {
        let list: Vec<&BtDev> = s.devices.iter().filter(|d| keep(d)).collect();
        if list.is_empty() && name != "рядом" {
            continue;
        }
        y = title(ui, b.x, y, b.w, name);
        if list.is_empty() {
            let hint = if s.discovering { "ищу… поднеси устройство и включи на нём сопряжение" } else { "никого: нажми «искать устройства»" };
            ui.text_in(PX, R::new(b.x, y, b.w, 20), hint, t.text_faint);
            y += 28;
        }
        for d in list.into_iter().take(16) {
            row(ui, st, d, R::new(b.x, y, b.w, 36));
            y += 40;
        }
        y += 6;
    }
}

/// строка устройства: значок, имя, состояние и заряд; справа — подключить/отключить и «забыть»
fn row(ui: &mut Ui, st: &State, d: &BtDev, r: R) {
    let t = ui.t;
    let busy = st.bt.busy == d.path;
    ui.cv.fill(r, if d.connected { t.surface_hi } else { t.surface });
    ui.cv.icon(d.icon7(), r.x + 10, r.cy() - 7, 2, if d.connected { t.ember } else { t.text_dim });
    let (bw, fw) = (120, if d.paired { 34 } else { 0 });
    let right = r.right() - bw - fw - 16;
    let mut tx = r.x + 36;
    let nw = ui.text_in(PX, R::new(tx, r.y, (right - tx - 120).max(60), r.h), &d.name, if d.connected { t.text_hi } else { t.text });
    tx += nw + 12;
    if let Some(p) = d.battery {
        let col = if p <= 15 { t.warn } else { t.ok };
        ui.cv.battery(tx, r.cy() - 7, 2, p as f32 / 100.0, t.text_dim, col, t.surface_hi.lerp(t.border, 0.45));
        ui.text_in(PX, R::new(tx + 28, r.y, 50, r.h), &format!("{p}%"), t.text_dim);
    }
    let what = if d.connected { "отключить" } else if d.paired { "подключить" } else { "сопрячь" };
    let label = if busy { "подождите…" } else { what };
    ui.button(R::new(r.right() - bw - fw - 8, r.y + 4, bw, r.h - 8), label, "", if d.connected { Tone::Ghost } else { Tone::Plain }, false, !busy, Act::BtDev(d.path.clone()));
    if d.paired {
        ui.button(R::new(r.right() - fw - 4, r.y + 4, fw - 4, r.h - 8), "", "trash", Tone::Ghost, false, !busy, Act::BtForget(d.path.clone()));
    }
}
