// Инструменты разработки репозитория (в установку не входят): tools/check.sh, tools/release.sh и
// tools/readme-shots.sh зовут их как `yami-dev <команда>`.
//
//   yami-dev check-docs [-v]            английские и русские документы — один текст
//   yami-dev check-i18n [ПАПКА …]       у каждой русской строки есть перевод
//   yami-dev i18n-todo КОД [ПАПКА]       непереведённые строки языка, по номерам
//   yami-dev i18n-add КОД ФАЙЛ [ПАПКА]   вписать переводы «номер<TAB>текст» в таблицу языка
//   yami-dev readme-art [#акцент]       картинки README
//   yami-dev changelog ВЕРСИЯ           «Unreleased» становится выпуском
//   yami-dev shots …                    помощники tools/readme-shots.sh

mod art;
mod docs;
mod i18n;
mod shots;
#[path = "../../src/recolor_data.rs"]
#[allow(dead_code)]
mod recolor_data;
#[path = "../../src/recolor_math.rs"]
#[allow(dead_code)]
mod recolor_math;

use std::path::PathBuf;

/// корень репозитория: ближайшая папка выше бинаря или текущей с install.sh
pub fn root() -> PathBuf {
    let from = |p: PathBuf| p.ancestors().find(|d| d.join("install.sh").is_file() && d.join("shell/Cargo.toml").is_file()).map(PathBuf::from);
    std::env::current_exe().ok().and_then(from).or_else(|| std::env::current_dir().ok().and_then(from)).unwrap_or_else(|| PathBuf::from("."))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or(&[]);
    std::process::exit(match args.first().map(String::as_str) {
        Some("check-docs") => docs::run(rest),
        Some("check-i18n") => i18n::run(rest),
        Some("i18n-todo") => i18n::todo(rest),
        Some("i18n-add") => i18n::add(rest),
        Some("readme-art") => art::run(rest),
        Some("changelog") => docs::changelog(rest),
        Some("shots") => shots::run(rest),
        _ => {
            eprintln!("yami-dev: check-docs [-v] | check-i18n [DIR…] | i18n-todo CODE [DIR] | i18n-add CODE FILE [DIR] | readme-art [#accent] | changelog VERSION | shots …");
            2
        }
    });
}
