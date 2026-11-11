// Статическая проверка переводов: у каждой русской строки в коде есть перевод. Шелл и окно настройки, либо
// папки исходников из аргументов (tools/check-modules.sh передаёт папку каждого модуля). Строка проходит, если
// она ключ PAIRS в i18n.rs, русская часть tf!("…", "…", "…") или стоит в строке с пометкой `// i18n-ok`. В файле с
// `// i18n: inline pairs` русская строка проходит и когда следующая в строке — её английский двойник. Португальский:
// у каждого английского значения есть строка в i18n_pt_br.rs, а tf! хранит те же {поля}. Ловит и дубли ключей,
// и кириллицу в английском или португальском. Живая половина — `shot --lang en` и `--lang pt-BR` каждой программы.

use regex::Regex;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const S: &str = r#""((?:[^"\\]|\\.)*)""#;
static CYR: LazyLock<Regex> = LazyLock::new(|| Regex::new("[а-яА-ЯёЁ]").unwrap());
static LIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(S).unwrap());
static PAIR: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"(?m)^\s*\({S}, {S}\),")).unwrap());
static TEMPLATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"tf!\(\s*{S}\s*,\s*{S}\s*,\s*{S}")).unwrap());
static TWO: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"^{S}\s*,\s*{S}")).unwrap());
static TF_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"tf!\(\s*$").unwrap());

fn native(s: &str) -> bool {
    s == "Русский"
}

/// {поля} формата: одиночные скобки, не {{ и не }}
fn fields(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' && (i == 0 || b[i - 1] != b'{') && b.get(i + 1) != Some(&b'{') {
            if let Some(j) = (i + 1..b.len()).find(|&j| b[j] == b'{' || b[j] == b'}') {
                if b[j] == b'}' && b.get(j + 1) != Some(&b'}') {
                    out.push(s[i..=j].to_string());
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// строка без её // комментария (содержимое строк остаётся)
fn code_part(line: &str) -> String {
    let (mut out, mut instr, chars) = (String::new(), false, line.char_indices().collect::<Vec<_>>());
    for (k, (at, c)) in chars.iter().enumerate() {
        if !instr && line[*at..].starts_with("//") {
            break;
        }
        if *c == '"' && (k == 0 || chars[k - 1].1 != '\\') {
            instr = !instr;
        }
        out.push(*c);
    }
    out
}

fn pairs_of(text: &str) -> Vec<(String, String)> {
    PAIR.captures_iter(text).map(|m| (m[1].to_string(), m[2].to_string())).collect()
}

fn check(src: &Path) -> (Vec<String>, usize) {
    // без таблицы (окно настройки: всё парами прямо в коде) — только проверка строк
    let has_table = src.join("i18n.rs").exists();
    let read = |n: &str| if has_table { std::fs::read_to_string(src.join(n)).unwrap_or_default() } else { String::new() };
    let (mut problems, mut seen): (Vec<String>, HashMap<String, String>) = (vec![], HashMap::new());
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    for (ru, en) in pairs_of(&read("i18n.rs")) {
        if let Some(old) = seen.get(&ru) {
            // тот же перевод дважды — мусор в таблице, другой — неясно, какой победит
            problems.push(format!("{name}/i18n.rs: duplicate key «{ru}»: «{old}» / «{en}»"));
        }
        if CYR.is_match(&en) && !native(&en) {
            problems.push(format!("i18n.rs: English value of «{ru}» is Cyrillic: «{en}»"));
        }
        seen.insert(ru, en);
    }
    let parent2 = src.parent().and_then(Path::parent).unwrap_or(src);
    let rel = src.strip_prefix(parent2).map(|p| p.display().to_string()).unwrap_or_default();
    let mut portuguese: HashSet<String> = HashSet::new();
    for (en, pt) in pairs_of(&read("i18n_pt_br.rs")) {
        if portuguese.contains(&en) {
            problems.push(format!("{rel}/i18n_pt_br.rs: duplicate key «{en}»"));
        }
        if CYR.is_match(&pt) && !native(&pt) {
            problems.push(format!("{rel}/i18n_pt_br.rs: Portuguese of «{en}» is Cyrillic: «{pt}»"));
        }
        if fields(&en) != fields(&pt) {
            problems.push(format!("{rel}/i18n_pt_br.rs: placeholders differ for «{en}»"));
        }
        portuguese.insert(en);
    }
    for en in seen.values().collect::<BTreeSet<_>>() {
        if !portuguese.contains(en) {
            problems.push(format!("{rel}/i18n_pt_br.rs: no Portuguese for «{en}»"));
        }
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(src).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "rs")).collect()).unwrap_or_default();
    files.sort();
    for f in files {
        let fname = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if fname == "i18n.rs" || fname == "i18n_pt_br.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        let inline = text.contains("// i18n: inline pairs");
        for (idx, line) in text.lines().enumerate() {
            let n = idx + 1;
            if line.contains("i18n-ok") {
                continue;
            }
            let code = code_part(line);
            if inline {
                // tf! несёт свой португальский шаблон: каталог нужен только остальным парам
                let own: HashSet<(String, String)> = TEMPLATE.captures_iter(&code).map(|m| (m[1].to_string(), m[2].to_string())).collect();
                for (p, _) in code.match_indices('"') {
                    let Some(m) = TWO.captures(&code[p..]) else { continue };
                    let (ru, en) = (m[1].to_string(), m[2].to_string());
                    if has_table && CYR.is_match(&ru) && !en.is_empty() && !CYR.is_match(&en) && !portuguese.contains(&en) && !own.contains(&(ru, en.clone())) {
                        problems.push(format!("{rel}/{fname}:{n}: no Portuguese for the inline «{en}»"));
                    }
                }
            }
            for m in TEMPLATE.captures_iter(&code) {
                let (en, pt) = (&m[2], &m[3]);
                if fields(en) != fields(pt) {
                    problems.push(format!("{rel}/{fname}:{n}: tf! placeholders differ between English and Portuguese"));
                }
                if CYR.is_match(pt) {
                    problems.push(format!("{rel}/{fname}:{n}: the Portuguese tf! template is Cyrillic"));
                }
            }
            let lits: Vec<_> = LIT.captures_iter(&code).collect();
            for (j, m) in lits.iter().enumerate() {
                let s = &m[1];
                if !CYR.is_match(s) || seen.contains_key(s) {
                    continue;
                }
                let whole = m.get(0).map_or(0..0, |x| x.range());
                if inline {
                    if let Some(next) = lits.get(j + 1) {
                        let nw = next.get(0).map_or(0, |x| x.start());
                        if !CYR.is_match(&next[1]) && code[whole.end..nw].trim() == "," {
                            continue;
                        }
                    }
                }
                if TF_END.is_match(&code[..whole.start]) {
                    continue;
                }
                problems.push(format!("{rel}/{fname}:{n}: no translation for «{s}»"));
            }
        }
    }
    (problems, seen.len())
}

pub fn run(args: &[String]) -> i32 {
    let root = crate::root().join("shell");
    let dirs: Vec<PathBuf> = if args.is_empty() { vec![root.join("src"), root.join("agents/src")] } else { args.iter().map(|a| Path::new(a).canonicalize().unwrap_or_else(|_| PathBuf::from(a))).collect() };
    let (mut problems, mut n) = (vec![], 0);
    for d in dirs {
        let (p, k) = check(&d);
        problems.extend(p);
        n += k;
    }
    for p in &problems {
        println!("{p}");
    }
    if !problems.is_empty() {
        eprintln!("check-i18n: {} problem(s). Add pairs to PAIRS in that crate's i18n.rs, use tf!(ru, en, pt, …), give each English text its line in i18n_pt_br.rs, or mark intentional Russian with // i18n-ok", problems.len());
        return 1;
    }
    println!("check-i18n: ok ({n} pairs)");
    0
}
