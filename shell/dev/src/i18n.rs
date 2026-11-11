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
        if fname == "i18n.rs" || fname.starts_with("i18n_") {
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
    for d in &dirs {
        let (p, k) = check(d);
        problems.extend(p);
        n += k;
    }
    let (p, langs) = coverage(&dirs);
    problems.extend(p);
    for p in &problems {
        println!("{p}");
    }
    if !problems.is_empty() {
        eprintln!("check-i18n: {} problem(s). Add pairs to PAIRS in that crate's i18n.rs, use tf!(ru, en, pt, …), give each English text its line in i18n_pt_br.rs, or mark intentional Russian with // i18n-ok", problems.len());
        return 1;
    }
    println!("check-i18n: ok ({n} pairs){}", if langs.is_empty() { String::new() } else { format!("; {langs}") });
    0
}

/// языки, которые переводятся по частям: таблица i18n_<код>.rs от английского; строки без перевода
/// показываются по-английски, проверка только считает процент. Португальский — полный, выше.
const PARTIAL: &[&str] = &["es", "de", "fr", "uk", "pl"];

fn table_file(code: &str) -> String {
    format!("i18n_{}.rs", code.to_ascii_lowercase().replace('-', "_"))
}

fn sorted_fields(s: &str) -> Vec<String> {
    let mut f = fields(s);
    f.sort();
    f
}

/// английские строки, нужные языку: ключи португальской таблицы (в ней есть каждая надпись) и
/// английские шаблоны tf!
/// самоназвания языков кириллицей (Русский) не переводят
fn needed(src: &Path) -> BTreeSet<String> {
    let mut n: BTreeSet<String> = pairs_of(&std::fs::read_to_string(src.join("i18n_pt_br.rs")).unwrap_or_default()).into_iter().map(|p| p.0).filter(|k| !CYR.is_match(k)).collect();
    let files = std::fs::read_dir(src).map(|rd| rd.flatten().map(|e| e.path()).collect::<Vec<_>>()).unwrap_or_default();
    for f in files {
        let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if !name.ends_with(".rs") || name.starts_with("i18n") {
            continue;
        }
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        for m in TEMPLATE.captures_iter(&text) {
            n.insert(m[2].to_string());
        }
    }
    n
}

/// ошибки таблицы языка (дубли, другие {поля}, кириллица не в украинском) и сколько нужных строк в ней есть
fn partial(src: &Path, code: &str, need: &BTreeSet<String>) -> (Vec<String>, usize) {
    let file = table_file(code);
    let mut problems = vec![];
    let mut have: HashSet<String> = HashSet::new();
    for (en, tr) in pairs_of(&std::fs::read_to_string(src.join(&file)).unwrap_or_default()) {
        if have.contains(&en) {
            problems.push(format!("{}/{file}: duplicate key «{en}»", src.display()));
        }
        if sorted_fields(&en) != sorted_fields(&tr) {
            problems.push(format!("{}/{file}: placeholders differ for «{en}»", src.display()));
        }
        if code != "uk" && CYR.is_match(&tr) {
            problems.push(format!("{}/{file}: «{en}» is Cyrillic: «{tr}»", src.display()));
        }
        have.insert(en);
    }
    (problems, need.iter().filter(|e| have.contains(*e)).count())
}

fn dir_arg(args: &[String], i: usize) -> PathBuf {
    args.get(i).map(PathBuf::from).unwrap_or_else(|| crate::root().join("shell/src"))
}

/// строки языка, которых нет в его таблице: «номер<TAB>английский» (номера — для i18n-add)
fn missing(src: &Path, code: &str) -> Vec<String> {
    let have: HashSet<String> = pairs_of(&std::fs::read_to_string(src.join(table_file(code))).unwrap_or_default()).into_iter().map(|p| p.0).collect();
    needed(src).into_iter().filter(|e| !have.contains(e)).collect()
}

pub fn todo(args: &[String]) -> i32 {
    let Some(code) = args.first().filter(|c| PARTIAL.contains(&c.as_str())) else {
        eprintln!("i18n-todo: CODE is one of {}", PARTIAL.join(" "));
        return 2;
    };
    for (i, e) in missing(&dir_arg(args, 1), code).iter().enumerate() {
        println!("{}\t{e}", i + 1);
    }
    0
}

pub fn add(args: &[String]) -> i32 {
    let (Some(code), Some(file)) = (args.first().filter(|c| PARTIAL.contains(&c.as_str())), args.get(1)) else {
        eprintln!("i18n-add: CODE FILE [DIR], CODE is one of {}", PARTIAL.join(" "));
        return 2;
    };
    let src = dir_arg(args, 2);
    let list = missing(&src, code);
    let quote = Regex::new(r#"(^|[^\\])""#).unwrap();
    let (mut rows, mut bad) = (vec![], 0);
    for line in std::fs::read_to_string(file).unwrap_or_default().lines().filter(|l| !l.trim().is_empty()) {
        let Some((n, tr)) = line.split_once('\t') else {
            eprintln!("i18n-add: no tab: {line}");
            bad += 1;
            continue;
        };
        let Some(en) = n.trim().parse::<usize>().ok().and_then(|n| list.get(n.wrapping_sub(1))) else {
            eprintln!("i18n-add: no string number {n}");
            bad += 1;
            continue;
        };
        // пробелы по краям — часть надписи (как у английской), только конец строки файла — нет
        let tr = tr.trim_end_matches(['\r', '\n']);
        let why = if tr.trim().is_empty() {
            "empty"
        } else if quote.is_match(tr) {
            "a bare quote"
        } else if sorted_fields(en) != sorted_fields(tr) {
            "placeholders differ"
        } else if *code != "uk" && CYR.is_match(tr) {
            "Cyrillic"
        } else {
            ""
        };
        if !why.is_empty() {
            eprintln!("i18n-add: {n}: {why}: «{en}» → «{tr}»");
            bad += 1;
            continue;
        }
        rows.push(format!("    (\"{en}\", \"{tr}\"),\n"));
    }
    let path = src.join(table_file(code));
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let Some(end) = text.rfind("];") else {
        eprintln!("i18n-add: {} has no table", path.display());
        return 1;
    };
    let out = format!("{}{}{}", &text[..end], rows.concat(), &text[end..]);
    if std::fs::write(&path, out).is_err() {
        eprintln!("i18n-add: cannot write {}", path.display());
        return 1;
    }
    println!("i18n-add: {} added to {}, {bad} refused, {} left", rows.len(), path.display(), list.len() - rows.len());
    i32::from(bad > 0)
}

/// у частичных языков — только процент; ошибки в их таблицах — настоящие ошибки
fn coverage(dirs: &[PathBuf]) -> (Vec<String>, String) {
    let mut problems = vec![];
    let mut line = vec![];
    for code in PARTIAL {
        let (mut got, mut all) = (0, 0);
        for d in dirs {
            if !d.join(table_file(code)).exists() {
                continue;
            }
            let need = needed(d);
            let (p, k) = partial(d, code, &need);
            problems.extend(p);
            got += k;
            all += need.len();
        }
        if all > 0 {
            line.push(format!("{code} {}%", got * 100 / all));
        }
    }
    (problems, line.join(", "))
}
