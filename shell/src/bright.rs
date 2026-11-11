// Яркость экрана для OSD (из yami-lite: там нить опрашивает sysfs раз в 150 мс). Здесь опроса нет:
// ядро шлёт событие udev при каждой смене подсветки (клавиши, brightnessctl, сама прошивка), фильтр в
// ядре пропускает только подсистему backlight — в простое нить спит. Без подсветки (настольный
// монитор) нить не заводится вовсе.

use crate::app::Msg;
use calloop::channel::Sender;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;

const NETLINK_KOBJECT_UEVENT: i32 = 15;
const SO_ATTACH_FILTER: i32 = 26;
/// «libudev» + магия заголовка
const MAGIC: u32 = 0xfeed_cafe;
/// MurmurHash2 слова «backlight» — так libudev кладёт подсистему в заголовок (тест ниже)
const BACKLIGHT: u32 = 0xc370_b302;

/// подсветка есть: хоть одно устройство в /sys/class/backlight
pub fn present() -> bool {
    std::fs::read_dir("/sys/class/backlight").is_ok_and(|mut d| d.next().is_some())
}

/// доля яркости устройства (папка в /sys): brightness / max_brightness
fn level(dir: &Path) -> Option<f32> {
    let n = |f: &str| std::fs::read_to_string(dir.join(f)).ok()?.trim().parse::<f32>().ok();
    let max = n("max_brightness").filter(|m| *m > 0.0)?;
    Some((n("brightness")? / max).clamp(0.0, 1.0))
}

/// сокет событий udev, только backlight; None — нет netlink (контейнер)
fn open() -> Option<OwnedFd> {
    // SAFETY: обычные вызовы сокетов; структуры — наши, на стеке, на время вызова
    unsafe {
        let fd = libc::socket(libc::AF_NETLINK, libc::SOCK_RAW | libc::SOCK_CLOEXEC, NETLINK_KOBJECT_UEVENT);
        if fd < 0 {
            return None;
        }
        let fd = OwnedFd::from_raw_fd(fd);
        let mut sa: libc::sockaddr_nl = std::mem::zeroed();
        sa.nl_family = libc::AF_NETLINK as u16;
        // группа 2 — события после udev (в заголовке libudev есть хеш подсистемы — по нему фильтр)
        sa.nl_groups = 2;
        if libc::bind(fd.as_raw_fd(), &sa as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_nl>() as u32) < 0 {
            return None;
        }
        let f = |code: u16, jt: u8, jf: u8, k: u32| libc::sock_filter { code, jt, jf, k };
        let (ld, jeq, ret) = (0x20, 0x15, 0x06);
        let prog = [f(ld, 0, 0, 8), f(jeq, 1, 0, MAGIC), f(ret, 0, 0, 0), f(ld, 0, 0, 24), f(jeq, 1, 0, BACKLIGHT), f(ret, 0, 0, 0), f(ret, 0, 0, 0xffff_ffff)];
        let fp = libc::sock_fprog { len: prog.len() as u16, filter: prog.as_ptr() as *mut _ };
        // фильтр не встал — тогда не будить шелл на каждое событие системы: без него не работаем
        if libc::setsockopt(fd.as_raw_fd(), libc::SOL_SOCKET, SO_ATTACH_FILTER, &fp as *const _ as *const libc::c_void, std::mem::size_of::<libc::sock_fprog>() as u32) < 0 {
            return None;
        }
        Some(fd)
    }
}

/// DEVPATH события (свойства — строки «КЛЮЧ=значение» через \0)
fn devpath(msg: &[u8]) -> Option<String> {
    msg.split(|b| *b == 0).find_map(|p| p.strip_prefix(b"DEVPATH=")).map(|p| String::from_utf8_lossy(p).into_owned())
}

/// нить: событие подсветки → её новая доля, если изменилась
pub fn spawn(tx: Sender<Msg>) {
    if !present() {
        return;
    }
    let _ = std::thread::Builder::new().name("bright".into()).spawn(move || {
        let Some(fd) = open() else { return };
        let mut last: Option<f32> = None;
        let mut b = [0u8; 8192];
        loop {
            // SAFETY: читаем в свой буфер его длины
            let n = unsafe { libc::recv(fd.as_raw_fd(), b.as_mut_ptr() as *mut libc::c_void, b.len(), 0) };
            if n <= 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return;
            }
            let Some(dp) = devpath(&b[..n as usize]) else { continue };
            let Some(v) = level(&Path::new("/sys").join(dp.trim_start_matches('/'))) else { continue };
            if last.is_some_and(|l| (l - v).abs() < 0.001) {
                continue;
            }
            last = Some(v);
            if tx.send(Msg::Bright(v)).is_err() {
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    /// хеш подсистемы, как его считает libudev (MurmurHash2, seed 0) — тот же, что в yami-agents
    fn murmur2(b: &[u8]) -> u32 {
        let m = 0x5bd1_e995u32;
        let mut h = b.len() as u32;
        let mut c = b.chunks_exact(4);
        for k in &mut c {
            let mut k = u32::from_le_bytes([k[0], k[1], k[2], k[3]]).wrapping_mul(m);
            k ^= k >> 24;
            h = h.wrapping_mul(m) ^ k.wrapping_mul(m);
        }
        let r = c.remainder();
        if !r.is_empty() {
            for (i, x) in r.iter().enumerate() {
                h ^= (*x as u32) << (8 * i);
            }
            h = h.wrapping_mul(m);
        }
        h ^= h >> 13;
        h = h.wrapping_mul(m);
        h ^ (h >> 15)
    }

    #[test]
    fn backlight_hash() {
        assert_eq!(murmur2(b"block"), 0xf003_1db7);
        assert_eq!(murmur2(b"backlight"), super::BACKLIGHT);
    }

    #[test]
    fn devpath_from_message() {
        let m = b"libudev\0\xfe\xed\xca\xfe....ACTION=change\0DEVPATH=/devices/pci0000:00/0000:00:02.0/drm/card1/card1-eDP-1/intel_backlight\0SUBSYSTEM=backlight\0";
        assert_eq!(super::devpath(m).as_deref(), Some("/devices/pci0000:00/0000:00:02.0/drm/card1/card1-eDP-1/intel_backlight"));
    }
}
