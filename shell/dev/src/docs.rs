// Английские и русские документы — один текст на двух языках: README, CHANGELOG, docs/CONFIG, docs/SECURITY и
// их двойники *.ru.md. Зовут tools/check.sh (CI) и tools/release.sh. У пары один скелет: те же заголовки
// (заголовки версий и имена файлов дословно), абзацы, пункты и строки таблиц в том же порядке, тот же
// `код` в каждом блоке (<заполнитель> можно переводить), те же блоки кода (комментарии можно переводить),
// те же ссылки. Русская сторона — по-русски, английская — без русского; каждая ссылка и #якорь существуют,
// русские файлы ведут на русские, каждый файл ссылается на двойника в начале, новейший раздел CHANGELOG — VERSION.

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const PAIRS: [&str; 5] = ["README.md", "CHANGELOG.md", "docs/CONFIG.md", "docs/SECURITY.md", "docs/PACKAGING.md"];

macro_rules! re {
    ($n:ident, $p:expr) => {
        static $n: LazyLock<Regex> = LazyLock::new(|| Regex::new($p).unwrap());
    };
}
// заголовки-имена: одинаковы на обоих языках
re!(SAME, r"^(\d+\.\d+\.\d+|[\w.]*[.\-][\w.\-]*|yami|Nix)$");
re!(CYR, "[а-яёА-ЯЁ]");
re!(ITEM, r"^\s*([-*]|\d+\.) ");
re!(HEADING, r"^(#{1,6}) (.*)");
re!(SPAN, r"`([^`]+)`");
re!(PLACEHOLDER, r"<[^<>`]*>");
re!(LETTERS, r"[^\W\d_]+");
re!(LINK, r"\]\(([^)\s]+)\)");
re!(SRC, r#"(?:src|href)="([^"]+)""#);
re!(SCHEME, r"^[a-z]+:");
re!(LANG_FILE, r"-(en|ru)(\.\w+)$");
re!(CODE, r"`[^`]*`");
re!(IMG, r"!\[[^\]]*\]");
re!(TARGET, r"\]\([^)]*\)");
re!(TAG, r"<[^>]*>");
re!(HTTP, r"https?://\S+");
re!(WORD, r"\b[A-Za-z]{3,}\b");
re!(QUOTED, r"«[^»]*»");
re!(LATIN, r"[A-Za-z]+(?:-[A-Za-z]+)*");
re!(COMMENT, r"(^|\s)#.*$");
re!(VERSION_HEAD, r"(?m)^## (\d+\.\d+\.\d+)$");

// служебные слова английского: в названиях программ их не бывает, так что два таких слова в русском
// блоке — оставленная английская фраза; «…» (заголовки issue) не в счёт
const STOP: &str = "the and with from that when into this are its only every each not which while after before under over than then also both their there where what was by of to in on or it as at an you your";

#[derive(Clone)]
enum B {
    Code(usize, String, Vec<String>),
    Heading(usize, usize, String),
    Table(usize, Vec<String>),
    Item(usize, String),
    Para(usize, String),
}

impl B {
    fn line(&self) -> usize {
        match self {
            B::Code(l, ..) | B::Heading(l, ..) | B::Table(l, ..) | B::Item(l, ..) | B::Para(l, ..) => *l,
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            B::Code(..) => "code",
            B::Heading(..) => "heading",
            B::Table(..) => "table",
            B::Item(..) => "item",
            B::Para(..) => "para",
        }
    }
}

/// блоки markdown по порядку
fn blocks(text: &str) -> Vec<B> {
    let lines: Vec<&str> = text.split('\n').collect();
    let (mut out, mut i) = (vec![], 0);
    while i < lines.len() {
        let ln = lines[i];
        if ln.starts_with("```") {
            let mut j = i + 1;
            while j < lines.len() && !lines[j].starts_with("```") {
                j += 1;
            }
            out.push(B::Code(i + 1, ln[3..].trim().to_string(), lines[i + 1..j].iter().map(|s| s.to_string()).collect()));
            i = j + 1;
        } else if ln.trim().is_empty() {
            i += 1;
        } else if let Some(m) = HEADING.captures(ln) {
            out.push(B::Heading(i + 1, m[1].len(), m[2].trim().to_string()));
            i += 1;
        } else if ln.starts_with('|') {
            let start = i;
            let mut rows = vec![];
            while i < lines.len() && lines[i].starts_with('|') {
                rows.push(lines[i].to_string());
                i += 1;
            }
            out.push(B::Table(start + 1, rows));
        } else {
            let start = i;
            let mut buf = vec![ln];
            i += 1;
            let item = ITEM.is_match(ln);
            while i < lines.len() && !lines[i].trim().is_empty() && !lines[i].starts_with('#') && !lines[i].starts_with('|') && !lines[i].starts_with("```") && !ITEM.is_match(lines[i]) {
                buf.push(lines[i]);
                i += 1;
            }
            let text = buf.iter().map(|x| x.trim()).collect::<Vec<_>>().join(" ");
            out.push(if item { B::Item(start + 1, text) } else { B::Para(start + 1, text) });
        }
    }
    out
}

/// `код` блока; <заполнитель> внутри можно переводить
fn spans(t: &str) -> Vec<String> {
    let mut v: Vec<String> = SPAN.captures_iter(t).map(|m| PLACEHOLDER.replace_all(&m[1], "<>").into_owned()).collect();
    v.sort();
    v
}

fn skeleton(s: &str) -> String {
    LETTERS.replace_all(s, "a").into_owned()
}

/// `код`, который есть только с одной стороны. Русский `код` с кириллицей — переведённый пример: годится,
/// если у английского есть такой же по форме
fn span_diff(ta: &str, tb: &str) -> (Vec<String>, Vec<String>) {
    let (mut a, mut b) = (spans(ta), spans(tb));
    for x in a.clone() {
        if let Some(p) = b.iter().position(|y| *y == x) {
            b.remove(p);
            if let Some(q) = a.iter().position(|y| *y == x) {
                a.remove(q);
            }
        }
    }
    for y in b.clone().into_iter().filter(|y| CYR.is_match(y)) {
        if let Some(q) = a.iter().position(|x| skeleton(x) == skeleton(&y)) {
            a.remove(q);
            if let Some(p) = b.iter().position(|z| *z == y) {
                b.remove(p);
            }
        }
    }
    (a, b)
}

/// адреса ссылок и картинок по порядку; у своих — без языка и якоря
fn links(t: &str) -> Vec<String> {
    LINK.captures_iter(t).chain(SRC.captures_iter(t)).map(|m| {
        let u = &m[1];
        if SCHEME.is_match(u) {
            u.to_string()
        } else if u.starts_with('#') {
            "#".to_string()
        } else {
            // свой двойник и картинки на языке страницы (features-en.png / features-ru.png)
            let base = u.split('#').next().unwrap_or("");
            let base = base.strip_suffix(".ru.md").map(|b| format!("{b}.md")).unwrap_or_else(|| base.to_string());
            LANG_FILE.replace(&base, "-xx$2").into_owned()
        }
    }).collect()
}

/// текст без кода, адресов, подписей картинок и тегов
fn prose(t: &str) -> String {
    let t = CODE.replace_all(t, " ");
    let t = IMG.replace_all(&t, " ");
    let t = TARGET.replace_all(&t, "]");
    let t = TAG.replace_all(&t, " ");
    HTTP.replace_all(&t, " ").into_owned()
}

fn words(t: &str) -> usize {
    WORD.find_iter(&prose(t)).count()
}

fn english(t: &str) -> Vec<String> {
    let p = prose(t);
    let p = QUOTED.replace_all(&p, " ");
    let stop: HashSet<&str> = STOP.split(' ').collect();
    LATIN.find_iter(&p).map(|m| m.as_str().to_string()).filter(|w| stop.contains(w.to_lowercase().as_str())).collect()
}

/// строки кода без комментариев (их можно переводить)
fn uncomment(lines: &[String]) -> Vec<String> {
    lines.iter().map(|x| COMMENT.replace(x, "").trim_end().to_string()).collect()
}

/// якоря заголовков, как их делает GitHub
fn slugs(text: &str) -> HashSet<String> {
    let (mut seen, mut out): (HashMap<String, usize>, HashSet<String>) = (HashMap::new(), HashSet::new());
    for b in blocks(text) {
        let B::Heading(_, _, h) = b else { continue };
        let h = TARGET.replace_all(&h, "]").to_lowercase();
        let h: String = h.chars().filter(|c| c.is_alphanumeric() || " -_".contains(*c)).collect::<String>().replace(' ', "-");
        let n = seen.get(&h).copied().unwrap_or(0);
        seen.insert(h.clone(), n + 1);
        out.insert(if n == 0 { h } else { format!("{h}-{n}") });
    }
    out
}

fn repr(s: &str) -> String {
    let e = s.replace('\\', "\\\\");
    if e.contains('\'') && !e.contains('"') {
        format!("\"{e}\"")
    } else {
        format!("'{}'", e.replace('\'', "\\'"))
    }
}

fn list(v: &[String]) -> String {
    format!("[{}]", v.iter().map(|s| repr(s)).collect::<Vec<_>>().join(", "))
}

fn head90(s: &str) -> String {
    s.chars().take(90).collect()
}

struct Ck {
    root: PathBuf,
    errors: Vec<String>,
}

impl Ck {
    fn err(&mut self, at: &str, msg: String) {
        self.errors.push(format!("{at}: {msg}"));
    }

    /// один абзац, пункт или строка таблицы в двух языках
    fn one(&mut self, at: &str, ta: &str, tb: &str) {
        let (oa, ob) = span_diff(ta, tb);
        if !oa.is_empty() || !ob.is_empty() {
            self.err(at, format!("`code` differs: only English {}, only Russian {}", list(&oa), list(&ob)));
        }
        if links(ta) != links(tb) {
            self.err(at, format!("links differ: {} vs {}", list(&links(ta)), list(&links(tb))));
        }
        let en = english(tb);
        if words(ta) >= 3 && !CYR.is_match(&prose(tb)) {
            self.err(at, format!("not translated: {}", repr(&head90(tb))));
        } else if en.len() >= 2 {
            self.err(at, format!("English left in the Russian text ({}): {}", en.iter().take(6).cloned().collect::<Vec<_>>().join(" "), repr(&head90(tb))));
        }
    }

    fn compare(&mut self, en_rel: &str, ru_rel: &str, en: &str, ru: &str) {
        let (a, b) = (blocks(en), blocks(ru));
        for (x, y) in a.iter().zip(b.iter()) {
            let at = format!("{en_rel}:{} ↔ {ru_rel}:{}", x.line(), y.line());
            if x.kind() != y.kind() {
                self.err(&at, format!("{} in English, {} in Russian: the files part here (a paragraph, item or row added on one side?)", x.kind(), y.kind()));
                return;
            }
            match (x, y) {
                (B::Heading(_, lva, ha), B::Heading(_, lvb, hb)) => self.heading(&at, *lva, ha, *lvb, hb),
                (B::Code(_, la, ca), B::Code(_, lb, cb)) => {
                    if la != lb {
                        self.err(&at, format!("code block language {} vs {}", repr(la), repr(lb)));
                    }
                    let (ua, ub) = (uncomment(ca), uncomment(cb));
                    let same = |p: &str, q: &str| p == q || (CYR.is_match(q) && skeleton(p) == skeleton(q));
                    if ua.len() != ub.len() || !ua.iter().zip(ub.iter()).all(|(p, q)| same(p, q)) {
                        let d = ua.iter().zip(ub.iter()).position(|(p, q)| !same(p, q)).unwrap_or(ua.len().min(ub.len()));
                        let side = |v: &[String]| v.get(d).map_or("∅".to_string(), |s| s.clone());
                        self.err(&at, format!("code block differs (comments aside) at its line {}: {} vs {}", d + 1, repr(&side(&ua)), repr(&side(&ub))));
                    }
                }
                (B::Table(la, da), B::Table(lb, db)) => {
                    if da.len() != db.len() {
                        self.err(&at, format!("table has {} rows in English, {} in Russian", da.len(), db.len()));
                        continue;
                    }
                    for (r, (ra, rb)) in da.iter().zip(db.iter()).enumerate() {
                        let rat = format!("{en_rel}:{} ↔ {ru_rel}:{}", la + r, lb + r);
                        let cells = |s: &str| s.trim_matches('|').split(" | ").count();
                        if cells(ra) != cells(rb) {
                            self.err(&rat, format!("{} cells vs {}", cells(ra), cells(rb)));
                        }
                        self.one(&rat, ra, rb);
                    }
                }
                (B::Item(_, da), B::Item(_, db)) => {
                    let last = |s: &str| ITEM.captures(s).and_then(|m| m[1].chars().last());
                    if last(da) != last(db) {
                        self.err(&at, "bullet vs numbered item".into());
                    }
                    self.one(&at, da, db);
                }
                (B::Para(_, da), B::Para(_, db)) => self.one(&at, da, db),
                _ => {}
            }
        }
        if a.len() != b.len() {
            let (longer, blk) = if a.len() > b.len() { (en_rel, &a[b.len()]) } else { (ru_rel, &b[a.len()]) };
            self.err(&format!("{longer}:{}", blk.line()), format!("{} more block(s) than its twin from here", a.len().abs_diff(b.len())));
        }
    }

    fn heading(&mut self, at: &str, lva: usize, ha: &str, lvb: usize, hb: &str) {
        if lva != lvb {
            self.err(at, format!("heading level {lva} vs {lvb}"));
        }
        let head = (ha == "Unreleased").then_some("Не выпущено");
        if let (Some(h), true) = (head, Some(hb) != head) {
            self.err(at, format!("«{ha}» is «{h}» in Russian, not «{hb}»"));
        } else if SAME.is_match(ha) && ha != hb {
            self.err(at, format!("heading «{ha}» is a name and stays the same, not «{hb}»"));
        }
        let (oa, ob) = span_diff(ha, hb);
        if !oa.is_empty() || !ob.is_empty() {
            self.err(at, format!("code in heading differs: {} vs {}", list(&spans(ha)), list(&spans(hb))));
        }
        if !(SAME.is_match(ha) || head.is_some()) && words(ha) > 0 && !CYR.is_match(&prose(hb)) {
            self.err(at, format!("heading not translated: «{hb}»"));
        }
    }

    /// каждая относительная ссылка и якорь существуют; русские файлы ведут на русские двойники
    fn link_targets(&mut self, rel: &str, text: &str, own: &str) {
        let base = self.root.join(rel).parent().map(Path::to_path_buf).unwrap_or_default();
        let mut body = vec![];
        for b in blocks(text) {
            match b {
                B::Code(..) => {}
                B::Para(_, t) | B::Item(_, t) => body.push(t),
                B::Table(_, r) => body.extend(r),
                B::Heading(_, _, h) => body.push(h),
            }
        }
        let body = body.join("\n");
        let ru = rel.ends_with(".ru.md");
        let urls: Vec<String> = LINK.captures_iter(&body).chain(SRC.captures_iter(&body)).map(|m| m[1].to_string()).collect();
        for u in urls {
            if SCHEME.is_match(&u) {
                continue;
            }
            let (path, anchor) = u.split_once('#').unwrap_or((u.as_str(), ""));
            let tgt = if path.is_empty() { self.root.join(rel) } else { base.join(path) };
            let Ok(tgt) = tgt.canonicalize() else {
                self.err(rel, format!("link to a missing file: {u}"));
                continue;
            };
            let md = tgt.extension().is_some_and(|e| e == "md");
            if !anchor.is_empty() && md && !slugs(&std::fs::read_to_string(&tgt).unwrap_or_default()).contains(anchor) {
                self.err(rel, format!("link to a missing anchor: {u}"));
            }
            if path.is_empty() || !md {
                continue;
            }
            let trel = tgt.strip_prefix(&self.root).map(|p| p.display().to_string()).unwrap_or_default();
            if trel == own {
                continue;
            }
            if ru && PAIRS.contains(&trel.as_str()) {
                self.err(rel, format!("links the English {trel}; the Russian file links {}", twin(&trel)));
            }
            if !ru && trel.ends_with(".ru.md") {
                self.err(rel, format!("links the Russian {trel}; the English file links {}.md", &trel[..trel.len() - 6]));
            }
        }
    }
}

fn twin(rel: &str) -> String {
    format!("{}.ru.md", &rel[..rel.len() - 3])
}

fn walk_ru(d: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            if name != ".git" && name != "target" {
                walk_ru(&p, out);
            }
        } else if name.ends_with(".ru.md") {
            out.push(p);
        }
    }
}

pub fn run(args: &[String]) -> i32 {
    let verbose = args.iter().any(|a| a == "-v");
    let root = crate::root().canonicalize().unwrap_or_else(|_| crate::root());
    let mut ck = Ck { root: root.clone(), errors: vec![] };
    for en_rel in PAIRS {
        let ru_rel = twin(en_rel);
        let (Ok(en), Ok(ru)) = (std::fs::read_to_string(root.join(en_rel)), std::fs::read_to_string(root.join(&ru_rel))) else {
            ck.err(en_rel, format!("the pair {en_rel} / {ru_rel} is incomplete"));
            continue;
        };
        ck.compare(en_rel, &ru_rel, &en, &ru);
        for (rel, text, other) in [(en_rel, &en, ru_rel.as_str()), (ru_rel.as_str(), &ru, en_rel)] {
            let name = Path::new(other).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let head = text.split('\n').take(20).collect::<Vec<_>>().join("\n");
            if !head.contains(&format!("]({name})")) {
                ck.err(rel, format!("no link to {name} in its first 20 lines"));
            }
            ck.link_targets(rel, text, other);
        }
        // английский файл без русского текста (кроме `кода` и подписи ссылки на двойник)
        let ru_name = Path::new(&ru_rel).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let own = Regex::new(&format!(r"\[[^\]]*\]\({}[^)]*\)", regex::escape(&ru_name))).unwrap();
        for b in blocks(&en) {
            let (ln, t) = match &b {
                B::Para(l, t) | B::Item(l, t) => (*l, t.clone()),
                B::Table(l, r) => (*l, r.join("\n")),
                _ => continue,
            };
            let t = own.replace_all(&t, " ");
            let p = prose(&t);
            if CYR.is_match(&p) {
                ck.err(&format!("{en_rel}:{ln}"), format!("Russian text in the English file: {}", repr(&head90(p.trim()))));
            }
        }
        if verbose {
            println!("  {en_rel} ↔ {ru_rel}: {} blocks", blocks(&en).len());
        }
    }
    // русские файлы без пары
    let mut extra = vec![];
    walk_ru(&root, &mut extra);
    extra.sort();
    for p in extra {
        let rel = p.strip_prefix(&root).map(|x| x.display().to_string()).unwrap_or_default();
        if !PAIRS.contains(&format!("{}.md", &rel[..rel.len() - 6]).as_str()) {
            ck.err(&rel, "a Russian document outside the checked pairs (add its English twin to PAIRS)".into());
        }
    }
    // самый новый выпуск в CHANGELOG — это VERSION
    let ver = std::fs::read_to_string(root.join("VERSION")).unwrap_or_default().trim().to_string();
    for rel in ["CHANGELOG.md", "CHANGELOG.ru.md"] {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else { continue };
        let vs: Vec<String> = VERSION_HEAD.captures_iter(&text).map(|m| m[1].to_string()).collect();
        if vs.first().is_some_and(|v| *v != ver) {
            ck.err(rel, format!("the newest section is {}, VERSION is {ver}: new lines go under «Unreleased», tools/release.sh makes the section", vs[0]));
        }
        let num = |v: &String| v.split('.').map(|x| x.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
        let mut sorted = vs.clone();
        sorted.sort_by_key(|v| std::cmp::Reverse(num(v)));
        if vs != sorted || vs.iter().collect::<HashSet<_>>().len() != vs.len() {
            ck.err(rel, "release sections are not newest first, or one repeats".into());
        }
    }
    for e in &ck.errors {
        println!("docs: {e}");
    }
    if !ck.errors.is_empty() {
        println!("docs: {} problem(s); the pairs are {} and their .ru.md twins", ck.errors.len(), PAIRS.join(", "));
        return 1;
    }
    println!("docs: {} pairs in step", PAIRS.len());
    0
}

/// `yami-dev changelog ВЕРСИЯ`: «Unreleased» («Не выпущено») становится выпуском в обоих файлах. Ничего не
/// меняется, пока в обоих нет строк: заметки пишутся по ходу, на двух языках
pub fn changelog(args: &[String]) -> i32 {
    let Some(ver) = args.first() else { return 2 };
    let root = crate::root();
    let mut todo = vec![];
    for (name, head) in [("CHANGELOG.md", "Unreleased"), ("CHANGELOG.ru.md", "Не выпущено")] {
        let p = root.join(name);
        let Ok(s) = std::fs::read_to_string(&p) else { return 3 };
        if s.contains(&format!("\n## {ver}\n")) {
            continue;
        }
        let tag = format!("\n## {head}\n");
        let Some(at) = s.find(&tag) else { return 3 };
        let body = &s[at + tag.len()..];
        let end = body.find("\n## ").unwrap_or(body.len());
        let text = body[..end].trim();
        if text.is_empty() {
            return 3;
        }
        todo.push((p, format!("{}\n## {head}\n\n## {ver}\n\n{text}\n{}", &s[..at], &body[end..])));
    }
    for (p, s) in todo {
        if std::fs::write(p, s).is_err() {
            return 1;
        }
    }
    0
}
