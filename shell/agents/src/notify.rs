// Уведомления по стандарту freedesktop: их показывает любой сервер (шелл yami, mako, dunst…).

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::Value;

/// показать; есть open — кнопка «открыть», ждать её до wait секунд. true — нажали
pub fn send(summary: &str, body: &str, icon: &str, open: Option<&str>, wait: u64) -> bool {
    let Ok(conn) = Connection::session() else { return false };
    let Ok(p) = Proxy::new(&conn, "org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications") else { return false };
    let actions: Vec<&str> = open.map(|l| vec!["default", l]).unwrap_or_default();
    let hints: HashMap<&str, Value> = HashMap::new();
    let signals = open.and_then(|_| p.receive_all_signals().ok());
    let Ok(reply) = p.call_method("Notify", &("yami-agents", 0u32, icon, summary, body, actions, hints, -1i32)) else { return false };
    let Ok(id) = reply.body().deserialize::<u32>() else { return false };
    let Some(signals) = signals else { return false };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for s in signals {
            let Some(name) = s.header().member().map(|m| m.to_string()) else { continue };
            let done = match name.as_str() {
                "ActionInvoked" => s.body().deserialize::<(u32, String)>().ok().filter(|(n, _)| *n == id).map(|_| true),
                "NotificationClosed" => s.body().deserialize::<(u32, u32)>().ok().filter(|(n, _)| *n == id).map(|_| false),
                _ => None,
            };
            if let Some(d) = done {
                let _ = tx.send(d);
                return;
            }
        }
    });
    rx.recv_timeout(Duration::from_secs(wait)).unwrap_or(false)
}
