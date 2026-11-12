// Положить в корзину по спецификации freedesktop: ~/.local/share/Trash для домашнего диска,
// <точка монтирования>/.Trash-<uid> для остальных (общая с Dolphin, Nautilus, gio trash). Одно на всех:
// yami-files (там же список, возврат, очистка) и yami-view. Диск приносят со стороны, поэтому корзина на
// нём — только своя настоящая папка: не ссылка, наша, 0700; запись о файле создаётся первой и атомарно.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub fn uid() -> u32 {
    // SAFETY: getuid не ошибается и ничего не трогает
    unsafe { libc::getuid() }
}

pub fn home_trash() -> PathBuf {
    crate::paths::data_dir().join("Trash")
}

/// точка монтирования, на которой лежит p
pub fn topdir(p: &Path) -> Option<PathBuf> {
    let dev = std::fs::symlink_metadata(p).ok()?.dev();
    let mut cur = p.to_path_buf();
    loop {
        let parent = cur.parent()?.to_path_buf();
        if std::fs::metadata(&parent).ok()?.dev() != dev {
            return Some(cur);
        }
        if parent == Path::new("/") {
            return Some(parent);
        }
        cur = parent;
    }
}

/// настоящая папка (не ссылка) и наша
pub fn own_dir(d: &Path) -> bool {
    std::fs::symlink_metadata(d).is_ok_and(|m| m.is_dir() && m.uid() == uid())
}

/// корзина на чужом диске — настоящая: сама .Trash-<uid> и её files — наши папки, не ссылки; info —
/// тоже, если есть. Диск приносят со стороны: files, сделанная ссылкой на /home, показала бы домашние
/// папки «файлами в корзине», и «очистить корзину» удалила бы их на самом деле. Та же проверка у glib
pub fn real_trash(t: &Path) -> bool {
    let info = t.join("info");
    own_dir(t) && own_dir(&t.join("files")) && (std::fs::symlink_metadata(&info).is_err() || own_dir(&info))
}

/// корзина для p и корень его диска (None — домашняя); ht — домашняя корзина в папке данных data
fn trash_for(p: &Path, ht: &Path, data: &Path) -> Option<(PathBuf, Option<PathBuf>)> {
    let ht = ht.to_path_buf();
    let _ = std::fs::create_dir_all(&ht);
    let hdev = std::fs::metadata(data).ok()?.dev();
    let pdev = std::fs::symlink_metadata(p).ok()?.dev();
    if hdev == pdev {
        // у новой учётной записи корзины ещё нет: files и info делаются здесь, иначе запись не создать
        let _ = std::fs::create_dir_all(ht.join("files"));
        let _ = std::fs::create_dir_all(ht.join("info"));
        return Some((ht, None));
    }
    let top = topdir(p)?;
    let t = top.join(format!(".Trash-{}", uid()));
    // под этим именем уже что-то не наше (ссылка, чужая папка): туда не кладём и ничего там не создаём
    if std::fs::symlink_metadata(&t).is_ok() && !own_dir(&t) {
        return None;
    }
    // своя корзина на общем диске — только своя (0700, как велит спецификация): удалённое не читают другие
    {
        use std::os::unix::fs::DirBuilderExt;
        let _ = std::fs::DirBuilder::new().mode(0o700).create(&t);
    }
    if std::fs::create_dir_all(t.join("files")).is_ok() && std::fs::create_dir_all(t.join("info")).is_ok() && real_trash(&t) {
        return Some((t, Some(top)));
    }
    None
}

/// путь для строки Path= в .trashinfo: байты как есть, кроме безопасных — %XX
pub fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

fn stamp() -> String {
    let t = crate::time::local_now();
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", t.year, t.mon, t.day, t.hour, t.min, t.sec)
}

pub enum PutErr {
    /// корзины для этого диска нет и не будет (под её именем чужое, диск только для чтения)
    NoTrash,
    Io(std::io::Error),
}

/// в корзину; Ok — путь в корзине. Занятое имя — «имя (2)»
pub fn put(p: &Path) -> Result<PathBuf, PutErr> {
    put_in(p, &home_trash(), &crate::paths::data_dir())
}

fn put_in(p: &Path, ht: &Path, data: &Path) -> Result<PathBuf, PutErr> {
    let (t, top) = trash_for(p, ht, data).ok_or(PutErr::NoTrash)?;
    let files = t.join("files");
    let info = t.join("info");
    let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    // .trashinfo создаётся первым и атомарно (O_EXCL): ссылку на его месте он не пройдёт, второй процесс
    // с тем же именем возьмёт следующее
    let mut target = files.join(&name);
    let mut n = 1;
    let infofile = loop {
        let tn = target.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ip = info.join(format!("{tn}.trashinfo"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&ip) {
            // в files под этим именем уже лежит что-то без записи: не затирать, следующее имя
            Ok(_) if std::fs::symlink_metadata(&target).is_ok() => {
                let _ = std::fs::remove_file(&ip);
            }
            Ok(mut f) => {
                use std::io::Write;
                let shown = match &top {
                    Some(top) => p.strip_prefix(top).map(|r| r.to_string_lossy().to_string()).unwrap_or_else(|_| p.to_string_lossy().to_string()),
                    None => p.to_string_lossy().to_string(),
                };
                let _ = write!(f, "[Trash Info]\nPath={}\nDeletionDate={}\n", enc(&shown), stamp());
                break ip;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(PutErr::Io(e)),
        }
        n += 1;
        if n > 10_000 {
            return Err(PutErr::Io(std::io::Error::from(std::io::ErrorKind::AlreadyExists)));
        }
        let (stem, ext) = match name.rfind('.') {
            Some(i) if i > 0 => (&name[..i], &name[i..]),
            _ => (name.as_str(), ""),
        };
        target = files.join(format!("{stem} ({n}){ext}"));
    };
    if let Err(e) = std::fs::rename(p, &target) {
        let _ = std::fs::remove_file(&infofile);
        return Err(PutErr::Io(e));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::{put_in, real_trash};

    /// у новой учётной записи корзины нет совсем: первое удаление делает её само; одинаковые имена
    /// не затирают друг друга, запись о файле лежит рядом
    #[test]
    fn the_first_file_makes_the_home_trash() {
        let d = crate::paths::private_tmp("trash-home").unwrap();
        let data = d.join("share");
        std::fs::create_dir_all(&data).unwrap();
        let ht = data.join("Trash");
        let a = d.join("note.txt");
        std::fs::write(&a, b"one").unwrap();
        let got = put_in(&a, &ht, &data).ok().expect("into a trash that did not exist");
        assert_eq!(got, ht.join("files/note.txt"));
        assert!(!a.exists() && std::fs::read(&got).unwrap() == b"one");
        let info = std::fs::read_to_string(ht.join("info/note.txt.trashinfo")).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=") && info.contains("DeletionDate="), "{info}");
        std::fs::write(&a, b"two").unwrap();
        assert_eq!(put_in(&a, &ht, &data).ok(), Some(ht.join("files/note (2).txt")));
        assert!(ht.join("info/note (2).txt.trashinfo").is_file() && std::fs::read(&got).unwrap() == b"one");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// корзина на принесённом диске, собранная из ссылок, — не корзина: то, на что она указывает, не
    /// показывается «файлами в корзине» и не удаляется её очисткой
    #[test]
    fn a_trash_made_of_links_is_not_a_trash() {
        let d = crate::paths::private_tmp("trash-test").unwrap();
        let home = d.join("home");
        std::fs::create_dir_all(home.join("alice/Documents")).unwrap();
        let mk = |disk: &str| {
            let t = d.join(disk).join(".Trash-1000");
            std::fs::create_dir_all(&t).unwrap();
            t
        };
        // как её делает любой файловый менеджер
        let good = mk("good");
        std::fs::create_dir_all(good.join("files")).unwrap();
        std::fs::create_dir_all(good.join("info")).unwrap();
        assert!(real_trash(&good));
        // без info (ещё ничего не удаляли) — тоже корзина
        let fresh = mk("fresh");
        std::fs::create_dir_all(fresh.join("files")).unwrap();
        assert!(real_trash(&fresh));
        // files — ссылка на /home: «в корзине» оказались бы домашние папки
        let bait = mk("bait");
        std::fs::create_dir_all(bait.join("info")).unwrap();
        std::os::unix::fs::symlink(&home, bait.join("files")).unwrap();
        assert!(bait.join("files/alice").is_dir(), "the link does lead to the home folders");
        assert!(!real_trash(&bait));
        // сама .Trash-<uid> — ссылка
        std::fs::create_dir_all(d.join("linked")).unwrap();
        std::os::unix::fs::symlink(&good, d.join("linked/.Trash-1000")).unwrap();
        assert!(!real_trash(&d.join("linked/.Trash-1000")));
        // info — ссылка
        let info = mk("info");
        std::fs::create_dir_all(info.join("files")).unwrap();
        std::os::unix::fs::symlink(&home, info.join("info")).unwrap();
        assert!(!real_trash(&info));
        // файл вместо папки, и вовсе ничего
        let file = mk("file");
        std::fs::write(file.join("files"), b"").unwrap();
        assert!(!real_trash(&file) && !real_trash(&d.join("nothing/.Trash-1000")));
        assert!(home.join("alice/Documents").is_dir());
        let _ = std::fs::remove_dir_all(&d);
    }
}
