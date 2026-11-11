// Экран запирается перед любым сном: крышка ноутбука, `systemctl suspend`, сон по таймеру системы, а
// не только из меню питания (lockSleep; идея из форка yami-lite). Шелл держит у logind «отложенную»
// блокировку сна (Inhibit "sleep" "delay"): собираясь спать, logind шлёт PrepareForSleep(true) и ждёт,
// пока её отпустят (не дольше InhibitDelayMaxSec, по умолчанию 5 с). Нить просит главную запереть
// экран и отпускает блокировку, когда niri подтвердил запирание (ext-session-lock «locked»), или сразу,
// если lockSleep выключен; после пробуждения (PrepareForSleep(false)) берёт её снова.
// Нет logind (elogind тоже его имя на шине) — нить просто выходит.

use crate::app::{App, Msg};
use calloop::channel::Sender;
use std::sync::mpsc;
use std::time::Duration;
use zbus::blocking::{Connection, MessageIterator, Proxy};

/// ждать подтверждения запирания не дольше: потом сон всё равно отпускается
const WAIT: Duration = Duration::from_secs(3);

fn inhibit(c: &Connection) -> Option<zbus::zvariant::OwnedFd> {
    let p = Proxy::new(c, "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager").ok()?;
    let m = p.call_method("Inhibit", &("sleep", "yami", crate::i18n::tr("запереть экран перед сном"), "delay")).ok()?;
    m.body().deserialize::<zbus::zvariant::OwnedFd>().ok()
}

/// нить сна; возвращает, куда главная сообщит «заперто» (или «запирать не нужно»)
pub fn spawn(tx: Sender<Msg>) -> mpsc::Sender<()> {
    let (ack_tx, ack_rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("sleep".into())
        .spawn(move || {
            let Some(c) = crate::net::system() else { return };
            let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).interface("org.freedesktop.login1.Manager").and_then(|b| b.member("PrepareForSleep")).map(|b| b.build());
            let Ok(rule) = rule else { return };
            let Ok(it) = MessageIterator::for_match_rule(rule, &c, Some(8)) else { return };
            let mut fd = inhibit(&c);
            if fd.is_none() {
                return;
            }
            for m in it {
                let Ok(m) = m else { continue };
                let Ok(start) = m.body().deserialize::<bool>() else { continue };
                if start {
                    // подтверждения прошлых раз — мимо
                    while ack_rx.try_recv().is_ok() {}
                    if tx.send(Msg::PrepareSleep).is_err() {
                        return;
                    }
                    let _ = ack_rx.recv_timeout(WAIT);
                    // отпустить: logind продолжит засыпание
                    fd = None;
                } else if fd.is_none() {
                    fd = inhibit(&c);
                }
            }
            drop(fd);
        })
        .ok();
    ack_tx
}

impl App {
    /// система засыпает: запереть экран (если так настроено); уже заперт — сразу отпустить сон
    pub fn prepare_sleep(&mut self) {
        if !self.st.set.lock_sleep || self.st.locked {
            self.sleep_locked();
            return;
        }
        self.lock_now();
        // запереть не вышло (другой экран блокировки уже держит сессию) — сну не мешать
        if !self.st.locked {
            self.sleep_locked();
        }
    }

    /// экран заперт (niri подтвердил) или запирания не будет: нить сна отпускает блокировку
    pub fn sleep_locked(&mut self) {
        if let Some(a) = &self.sleep_ack {
            let _ = a.send(());
        }
    }
}
