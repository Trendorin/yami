// Готовые шаблоны (#31): цвета палитры для программ, которых recolor сам не знает. Файлы лежат в
// config/templates и вшиты в бинарь — сеть не нужна, чужой код не скачивается. «Добавить» кладёт копию
// в ~/.config/yami/templates; дальше это обычный твой шаблон: переключатель, правка руками, перезапись.
// Шаблон пишет только свой файл рядом с конфигом программы; подключить его — одна строка в её
// конфиге (`hint`), её yami не трогает.

/// (имя файла шаблона, программа, что вписать в её конфиг, текст)
pub const ALL: &[(&str, &str, &str, &str)] = &[
    ("ghostty", "Ghostty", "theme = yami", include_str!("../../config/templates/ghostty")),
    ("alacritty", "Alacritty", "import = [\"~/.config/alacritty/yami.toml\"]", include_str!("../../config/templates/alacritty")),
    ("wezterm", "WezTerm", "config.color_scheme = \"yami\"", include_str!("../../config/templates/wezterm")),
    ("helix", "Helix", "theme = \"yami\"", include_str!("../../config/templates/helix")),
    ("neovim", "Neovim", ":colorscheme yami", include_str!("../../config/templates/neovim")),
    ("zathura", "zathura", "include yami", include_str!("../../config/templates/zathura")),
    ("tmux", "tmux", "source-file ~/.config/tmux/yami.conf", include_str!("../../config/templates/tmux")),
    ("fuzzel", "fuzzel", "include=~/.config/fuzzel/yami.ini", include_str!("../../config/templates/fuzzel")),
    ("vesktop", "Vesktop", "themes → yami.theme.css", include_str!("../../config/templates/vesktop")),
    ("equibop", "Equibop", "themes → yami.theme.css", include_str!("../../config/templates/equibop")),
];

pub fn dir() -> std::path::PathBuf {
    crate::config::config_dir().join("yami/templates")
}

/// положить копию; своего файла с тем же именем не трогает
pub fn add(i: usize) -> bool {
    let Some((file, _, _, body)) = ALL.get(i) else { return false };
    let p = dir().join(file);
    if p.exists() {
        return false;
    }
    let _ = std::fs::create_dir_all(dir());
    crate::config::write_atomic(&p, body).is_ok()
}

#[cfg(test)]
mod tests {
    /// каждый шаблон говорит, куда писать, и пишет внутрь ~/.config; цвета — только из палитры
    #[test]
    fn every_template_is_complete() {
        let known = ["accent", "bg", "border", "bright", "codeBg", "deep", "dim", "glow", "ok", "onAccent", "panel", "selection", "surface", "surfaceHi", "text", "textDim", "textFaint", "textHi", "warn"];
        for (file, _, hint, body) in super::ALL {
            assert!(!hint.is_empty(), "{file}");
            let out = body.lines().find_map(|l| l.split_once("yami-output:").map(|(_, r)| r.trim())).unwrap_or("");
            assert!(out.starts_with("~/.config/"), "{file}: {out}");
            let mut rest = *body;
            while let Some(i) = rest.find("{{") {
                let tail = &rest[i + 2..];
                let end = tail.find("}}").unwrap_or(0);
                let key = tail[..end].trim().split('.').next().unwrap_or("");
                let ansi = key.strip_prefix("color").and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| n < 16);
                assert!(known.contains(&key) || ansi, "{file}: {{{{{key}}}}}");
                rest = &tail[end..];
            }
        }
    }
}
