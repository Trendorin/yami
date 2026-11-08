// Разговор с остальной системой: уведомления по стандарту freedesktop (любой сервер: шелл yami, mako,
// dunst…) и окно «сохранить как» (yami-files, иначе zenity).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;
use yami_px::gfx::Img;
use yami_px::paths::find_exe;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{StructureBuilder, Value};

/// миниатюра в уведомлении
pub const THUMB_W: i32 = 112;
pub const THUMB_H: i32 = 72;

/// картинка в виде подсказки image-data: RGBA по строкам
fn image_data(i: &Img) -> Value<'static> {
    let mut b = Vec::with_capacity(i.px.len() * 4);
    for p in &i.px {
        b.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]);
    }
    let s = StructureBuilder::new().add_field(i.w).add_field(i.h).add_field(i.w * 4).add_field(true).add_field(8i32).add_field(4i32).add_field(b).build();
    match s {
        Ok(v) => Value::from(v),
        Err(_) => Value::from(0u8),
    }
}

/// показать уведомление. wait > 0 и есть кнопки — ждать нажатия до wait секунд и вернуть ключ кнопки
pub fn send(summary: &str, body: &str, thumb: Option<&Img>, actions: &[(String, String)], wait: u64) -> Option<String> {
    let conn = Connection::session().ok()?;
    let p = Proxy::new(&conn, "org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications").ok()?;
    let mut hints: HashMap<&str, Value> = HashMap::new();
    if let Some(i) = thumb {
        hints.insert("image-data", image_data(i));
    }
    let list: Vec<&str> = actions.iter().flat_map(|(k, v)| [k.as_str(), v.as_str()]).collect();
    // подписка до показа: иначе быстрый щелчок можно пропустить
    let signals = (wait > 0 && !actions.is_empty()).then(|| p.receive_all_signals().ok()).flatten();
    let reply = p.call_method("Notify", &("yami-shot", 0u32, "", summary, body, list, hints, -1i32)).ok()?;
    let id: u32 = reply.body().deserialize().ok()?;
    let signals = signals?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for s in signals {
            let Some(name) = s.header().member().map(|m| m.to_string()) else { continue };
            match name.as_str() {
                "ActionInvoked" => {
                    if let Ok((n, key)) = s.body().deserialize::<(u32, String)>() {
                        if n == id {
                            let _ = tx.send(Some(key));
                            return;
                        }
                    }
                }
                "NotificationClosed" => {
                    if s.body().deserialize::<(u32, u32)>().is_ok_and(|(n, _)| n == id) {
                        let _ = tx.send(None);
                        return;
                    }
                }
                _ => {}
            }
        }
    });
    rx.recv_timeout(Duration::from_secs(wait)).ok().flatten()
}

pub fn picker_available() -> bool {
    find_exe("yami-files").is_some() || find_exe("zenity").is_some()
}

/// спросить, куда сохранить. None — отменили
pub fn pick_save(dir: &Path, name: &str) -> Option<PathBuf> {
    let out = if find_exe("yami-files").is_some() {
        Command::new("yami-files").args(["--pick", "save", "--name", name, "--filter", "PNG|*.png", "--folder"]).arg(dir).output()
    } else {
        Command::new("zenity").args(["--file-selection", "--save"]).arg(format!("--filename={}", dir.join(name).display())).output()
    }
    .ok()
    .filter(|o| o.status.success())?;
    String::from_utf8_lossy(&out.stdout).lines().next().map(PathBuf::from)
}
