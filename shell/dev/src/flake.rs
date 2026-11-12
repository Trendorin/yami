// flake.lock без nix: tools/release.sh вписывает в него модули выпуска (их теги в своих репозиториях).
// Для входа github: nix сам хранит rev, lastModified (время коммита) и narHash — sha256 архива NAR того,
// что лежит в тарболе GitHub, а тарбол GitHub — это `git archive` коммита. Его и сериализуем.
//
//   yami-dev nar-hash ПАПКА            sha256-… архива NAR папки (проверка: тот же хеш, что у nix)
//   yami-dev flake-lock [ПАПКА_МОДУЛЕЙ]  входы github:Trendorin/<модуль>/<тег> из flake.nix → flake.lock

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// строка NAR: длина u64 LE, байты, нули до кратного 8
fn put(w: &mut impl Write, b: &[u8]) -> std::io::Result<()> {
    w.write_all(&(b.len() as u64).to_le_bytes())?;
    w.write_all(b)?;
    w.write_all(&[0u8; 8][..(8 - b.len() % 8) % 8])
}

/// узел NAR: папка (имена по байтам), файл (бит исполнения), ссылка
fn node(w: &mut impl Write, p: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;
    let m = std::fs::symlink_metadata(p)?;
    put(w, b"(")?;
    put(w, b"type")?;
    if m.file_type().is_symlink() {
        put(w, b"symlink")?;
        put(w, b"target")?;
        put(w, std::fs::read_link(p)?.as_os_str().as_bytes())?;
    } else if m.is_dir() {
        put(w, b"directory")?;
        let mut names: Vec<_> = std::fs::read_dir(p)?.map(|e| e.map(|e| e.file_name())).collect::<Result<_, _>>()?;
        names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        for n in names {
            put(w, b"entry")?;
            put(w, b"(")?;
            put(w, b"name")?;
            put(w, n.as_bytes())?;
            put(w, b"node")?;
            node(w, &p.join(&n))?;
            put(w, b")")?;
        }
    } else {
        put(w, b"regular")?;
        if m.permissions().mode() & 0o100 != 0 {
            put(w, b"executable")?;
            put(w, b"")?;
        }
        put(w, b"contents")?;
        let len = m.len();
        w.write_all(&len.to_le_bytes())?;
        std::io::copy(&mut std::fs::File::open(p)?, w)?;
        w.write_all(&[0u8; 8][..((8 - len % 8) % 8) as usize])?;
    }
    put(w, b")")
}

fn b64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

/// sha256-… (SRI) архива NAR папки: байты идут прямо в sha256sum, в памяти ничего не копится
pub fn nar_hash(dir: &Path) -> Result<String, String> {
    let mut ch = Command::new("sha256sum").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().map_err(|e| format!("sha256sum: {e}"))?;
    {
        let mut w = std::io::BufWriter::with_capacity(1 << 20, ch.stdin.take().ok_or("sha256sum: no stdin")?);
        put(&mut w, b"nix-archive-1").and_then(|_| node(&mut w, dir)).and_then(|_| w.flush()).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let out = ch.wait_with_output().map_err(|e| e.to_string())?;
    let hex = String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or("").to_string();
    let raw: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()).collect();
    if raw.len() != 32 {
        return Err("sha256sum gave no hash".into());
    }
    Ok(format!("sha256-{}", b64(&raw)))
}

fn git(dir: &Path, a: &[&str]) -> Result<String, String> {
    let o = Command::new("git").arg("-C").arg(dir).args(a).output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("git {} in {}: {}", a.join(" "), dir.display(), String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// то, что отдаст тарбол GitHub этого коммита: `git archive` во временную папку. export-subst
/// GitHub разворачивает по-своему — такого репозитория здесь не считаем, хеш вышел бы не тот
fn archive_hash(repo: &Path, rev: &str) -> Result<String, String> {
    let attrs = git(repo, &["ls-tree", "-r", "--name-only", rev]).unwrap_or_default();
    for f in attrs.lines().filter(|l| l.ends_with(".gitattributes")) {
        if git(repo, &["show", &format!("{rev}:{f}")]).unwrap_or_default().contains("export-subst") {
            return Err(format!("{}: {f} uses export-subst, its GitHub tarball cannot be hashed here", repo.display()));
        }
    }
    let tmp = std::env::temp_dir().join(format!("yami-nar-{}-{rev}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let ok = Command::new("sh").arg("-c").arg("git -C \"$1\" archive --format=tar \"$2\" | tar -x -C \"$3\"").args(["sh"]).arg(repo).arg(rev).arg(&tmp).status().is_ok_and(|s| s.success());
    let r = if ok { nar_hash(&tmp) } else { Err(format!("git archive {rev} in {}", repo.display())) };
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

pub fn run_nar(a: &[String]) -> i32 {
    let Some(d) = a.first() else {
        eprintln!("yami-dev nar-hash DIR");
        return 2;
    };
    match nar_hash(Path::new(d)) {
        Ok(h) => {
            println!("{h}");
            0
        }
        Err(e) => {
            eprintln!("nar-hash: {e}");
            1
        }
    }
}

/// входы модулей из flake.nix: (имя, владелец, репозиторий, тег)
fn inputs(flake: &str) -> Vec<(String, String, String, String)> {
    let mut v = vec![];
    let mut name = String::new();
    for l in flake.lines().map(str::trim) {
        if let Some(n) = l.strip_suffix("= {") {
            name = n.trim().to_string();
        } else if let Some(u) = l.strip_prefix("url = \"github:").and_then(|u| u.strip_suffix("\";")) {
            let p: Vec<&str> = u.split('/').collect();
            if let [owner, repo, tag] = p[..] {
                if owner == "Trendorin" && !name.is_empty() {
                    v.push((name.clone(), owner.into(), repo.into(), tag.into()));
                }
            }
        }
    }
    v
}

pub fn run_lock(a: &[String]) -> i32 {
    let root = crate::root();
    let mods = a.first().map(PathBuf::from).or_else(|| std::env::var_os("YAMI_MODULES_DIR").map(PathBuf::from)).unwrap_or_else(|| root.join("../yami-modules"));
    let flake = std::fs::read_to_string(root.join("flake.nix")).unwrap_or_default();
    let lock_path = root.join("flake.lock");
    let Ok(mut lock) = std::fs::read_to_string(&lock_path).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<serde_json::Value>(&s).map_err(|e| e.to_string())) else {
        eprintln!("flake-lock: cannot read {}", lock_path.display());
        return 1;
    };
    let ins = inputs(&flake);
    if ins.is_empty() {
        eprintln!("flake-lock: no github:Trendorin/… inputs in flake.nix");
        return 1;
    }
    for (name, owner, repo, tag) in &ins {
        let dir = mods.join(repo);
        let got = git(&dir, &["rev-parse", &format!("refs/tags/{tag}^{{commit}}")]).and_then(|rev| {
            let at: u64 = git(&dir, &["log", "-1", "--format=%ct", &rev])?.parse().map_err(|_| "no commit time".to_string())?;
            Ok((archive_hash(&dir, &rev)?, rev, at))
        });
        let (nar, rev, at) = match got {
            Ok(x) => x,
            Err(e) => {
                eprintln!("flake-lock: {repo} {tag}: {e}");
                return 1;
            }
        };
        lock["nodes"][name] = serde_json::json!({
            "flake": false,
            "locked": { "lastModified": at, "narHash": nar, "owner": owner, "repo": repo, "rev": rev, "type": "github" },
            "original": { "owner": owner, "ref": tag, "repo": repo, "type": "github" }
        });
        lock["nodes"]["root"]["inputs"][name] = serde_json::Value::String(name.clone());
        println!("  {name}: {tag} {} {nar}", &rev[..12.min(rev.len())]);
    }
    let text = serde_json::to_string_pretty(&lock).unwrap_or_default() + "\n";
    let tmp = lock_path.with_extension("lock.new");
    if std::fs::write(&tmp, &text).and_then(|_| std::fs::rename(&tmp, &lock_path)).is_err() {
        eprintln!("flake-lock: cannot write {}", lock_path.display());
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_pads_like_nix() {
        assert_eq!(super::b64(b""), "");
        assert_eq!(super::b64(b"f"), "Zg==");
        assert_eq!(super::b64(b"fo"), "Zm8=");
        assert_eq!(super::b64(b"foo"), "Zm9v");
        assert_eq!(super::b64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn module_inputs() {
        let f = "inputs = {\n  nixpkgs.url = \"github:NixOS/nixpkgs/nixos-unstable\";\n  yami-files = {\n    url = \"github:Trendorin/yami-files/v3.0.0\";\n    flake = false;\n  };\n};";
        assert_eq!(super::inputs(f), vec![("yami-files".into(), "Trendorin".into(), "yami-files".into(), "v3.0.0".into())]);
    }
}
