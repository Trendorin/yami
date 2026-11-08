// Помощники tools/readme-shots.sh: ответы niri (JSON), обрезка снимков, демо-папка с картинками палитр, правки
// настроек на время съёмки. Каждая подкоманда — один короткий шаг, его результат — в stdout.

use crate::recolor_data::KANJI;
use crate::recolor_math::{derive, mix, rgb, wallpaper_img};
use image::{imageops, DynamicImage, Rgb};
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;

fn stdin_json() -> Value {
    let mut t = String::new();
    let _ = std::io::stdin().read_to_string(&mut t);
    serde_json::from_str(&t).unwrap_or(Value::Null)
}

fn file_json(p: &str) -> Value {
    std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)
}

fn open(p: &str) -> Result<DynamicImage, String> {
    image::open(p).map_err(|e| format!("{p}: {e}"))
}

fn demo(dir: &str, root: &Path) -> Result<(), String> {
    let d = Path::new(dir);
    let choices = std::fs::read_to_string(root.join("installer/choices.sh")).map_err(|e| e.to_string())?;
    let list = |name: &str| -> Vec<String> {
        choices.lines().find_map(|l| l.strip_prefix(&format!("{name}=("))).map(|l| l.trim_end_matches(')').split_whitespace().map(String::from).collect()).unwrap_or_default()
    };
    let (ids, hexes) = (list("PAL_ID"), list("PAL_HEX"));
    let (kw, kh) = (KANJI[0].len() as u32, KANJI.len() as u32);
    let on = |x: i32, y: i32| y >= 0 && y < kh as i32 && x >= 0 && x < kw as i32 && KANJI[y as usize].as_bytes()[x as usize] == b'#';
    for (id, hx) in ids.iter().zip(hexes.iter()) {
        let p = derive(&format!("#{hx}"), 1.0, false);
        let mut im = wallpaper_img(&p, 480, 270);
        let bands = [p.g("bright").to_string(), mix(p.g("bright"), p.g("accent"), 0.5), p.g("accent").to_string(), mix(p.g("accent"), p.g("deep"), 0.5)];
        let shadow = mix(p.g("deep"), p.g("bg"), 0.45);
        let cell = 8;
        let (x0, y0) = ((480 - kw * cell) / 2, (270 - kh * cell) / 2);
        for y in 0..=kh {
            for x in 0..=kw {
                let (ix, iy) = (x as i32, y as i32);
                let c = if on(ix, iy) { Some(bands[((y * 4 / kh) as usize).min(3)].as_str()) } else if on(ix - 1, iy - 1) { Some(shadow.as_str()) } else { None };
                let Some(c) = c else { continue };
                let (r, g, b) = rgb(c);
                for yy in y0 + y * cell..y0 + (y + 1) * cell {
                    for xx in x0 + x * cell..x0 + (x + 1) * cell {
                        im.put_pixel(xx, yy, Rgb([r as u8, g as u8, b as u8]));
                    }
                }
            }
        }
        let big = imageops::resize(&im, 480 * 4, 270 * 4, imageops::FilterType::Nearest);
        big.save(d.join(format!("{id}.png"))).map_err(|e| e.to_string())?;
    }
    let w = |n: &str, b: Vec<u8>| std::fs::write(d.join(n), b).map_err(|e| e.to_string());
    w("notes.md", b"# yami\n\nPixel-art desktop for niri.\n".to_vec())?;
    w("todo.txt", b"ship 2.1.0\n".to_vec())?;
    let mut gz = vec![0x1f, 0x8b];
    gz.resize(4002, 0);
    w("Projects/yami.tar.gz", gz)?;
    w("Music/night.flac", vec![0; 9000])
}

pub fn run(args: &[String]) -> i32 {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let r: Result<(), String> = (|| {
        match a.as_slice() {
            // niri msg --json focused-output
            ["out-name"] => println!("{}", stdin_json()["name"].as_str().unwrap_or("")),
            ["out-size"] => {
                let v = stdin_json();
                println!("{}x{}", v["logical"]["width"].as_i64().unwrap_or(0), v["logical"]["height"].as_i64().unwrap_or(0));
            }
            // niri msg --json workspaces
            ["ws-focused"] => {
                let v = stdin_json();
                let i = v.as_array().and_then(|w| w.iter().find(|x| x["is_focused"] == true)).and_then(|x| x["idx"].as_i64());
                println!("{}", i.ok_or("no focused workspace")?);
            }
            ["ws-empty", out] => {
                let v = stdin_json();
                let i = v.as_array().and_then(|w| w.iter().filter(|x| x["output"] == *out && x["active_window_id"].is_null()).filter_map(|x| x["idx"].as_i64()).max());
                println!("{}", i.ok_or("no empty workspace")?);
            }
            // на пустом столе только наши окна
            ["guard", ws, wins, idx, out] => {
                let (ws, wins) = (file_json(ws), file_json(wins));
                let idx: i64 = idx.parse().map_err(|_| "idx")?;
                let w = ws.as_array().and_then(|l| l.iter().find(|x| x["idx"] == idx && x["output"] == *out)).ok_or("no workspace")?;
                let ours = wins.as_array().is_some_and(|l| l.iter().filter(|x| x["workspace_id"] == w["id"]).all(|x| matches!(x["app_id"].as_str(), Some("yami-files" | "yami-view"))));
                if w["is_focused"] != true || !ours {
                    return Err("the workspace changed".into());
                }
            }
            ["demo", dir] => demo(dir, &crate::root())?,
            ["files-no-devices", file] => {
                let mut d = file_json(file);
                if !d.is_object() {
                    d = json!({});
                }
                d["devices"] = json!(false);
                std::fs::write(file, d.to_string()).map_err(|e| e.to_string())?;
            }
            // pactl -f json list sinks → простые имена устройств
            ["audio-names", lang] => {
                let (mic, out) = if *lang == "ru" { ("USB-микрофон", "Наушники") } else { ("USB microphone", "Headphones") }; // i18n-ok
                let (mut names, mut n) = (serde_json::Map::new(), 0);
                for s in stdin_json().as_array().cloned().unwrap_or_default() {
                    let name = s["name"].as_str().unwrap_or("").to_string();
                    let t = format!("{name} {}", s["description"].as_str().unwrap_or("")).to_lowercase();
                    if t.contains("mic") {
                        names.insert(name, mic.into());
                    } else {
                        n += 1;
                        names.insert(name, if n == 1 { out.to_string() } else { format!("{out} {n}") }.into());
                    }
                }
                println!("{}", Value::Object(names));
            }
            ["widgets-no-net", json_text] => {
                let mut ws: Value = serde_json::from_str(json_text).map_err(|e| e.to_string())?;
                for w in ws.as_array_mut().ok_or("not a list")? {
                    if w["kind"] == "net" {
                        w["kind"] = json!("graph");
                        w["source"] = json!("temp");
                        w["style"] = json!("line");
                    }
                }
                println!("{ws}");
            }
            // панель по центру экрана с полями
            ["crop", src, w, h, dst] => {
                let im = open(src)?;
                let (w, h): (i64, i64) = (w.parse().map_err(|_| "w")?, h.parse().map_err(|_| "h")?);
                let m = 28;
                let (x, y) = ((im.width() as i64 - w) / 2 - m, (im.height() as i64 - h) / 2 - m);
                im.crop_imm(x.max(0) as u32, y.max(0) as u32, (w + 2 * m) as u32, (h + 2 * m) as u32).save(dst).map_err(|e| e.to_string())?;
            }
            ["crop-top-right", src, w, h, dst] => {
                let im = open(src)?;
                let (w, h): (u32, u32) = (w.parse().map_err(|_| "w")?, h.parse().map_err(|_| "h")?);
                im.crop_imm(im.width().saturating_sub(w), 0, w, h).save(dst).map_err(|e| e.to_string())?;
            }
            ["rgb", src, dst] => open(src)?.to_rgb8().save(dst).map_err(|e| e.to_string())?,
            _ => return Err("shots: unknown command".into()),
        }
        Ok(())
    })();
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("yami-dev shots: {e}");
            1
        }
    }
}
