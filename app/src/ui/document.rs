use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

#[derive(Clone, Debug)]
pub struct Chapter {
    pub title: String,
    pub body: String,
}

pub fn load_chapters(path: &Path, format: &str) -> Result<Vec<Chapter>, String> {
    match format.to_ascii_uppercase().as_str() {
        "TXT" => {
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            Ok(vec![Chapter {
                title: "Text".into(),
                body: text,
            }])
        }
        "EPUB" => load_epub(path),
        other => Err(format!("{other} is not a text chapter format.")),
    }
}

fn load_epub(path: &Path) -> Result<Vec<Chapter>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let container = read_zip(&mut archive, "META-INF/container.xml")?;
    let opf_path = attr_value(&container, "full-path")
        .ok_or_else(|| "EPUB is missing a rootfile path.".to_string())?;
    let opf = read_zip(&mut archive, &opf_path)?;
    let base = opf_path
        .rsplit_once('/')
        .map(|(dir, _)| format!("{dir}/"))
        .unwrap_or_default();

    let manifest = manifest_hrefs(&opf);
    let spine = spine_ids(&opf);
    if spine.is_empty() {
        return Err("EPUB spine is empty.".into());
    }

    let mut chapters = Vec::new();
    for (index, id) in spine.iter().enumerate() {
        let Some(href) = manifest.get(id) else {
            continue;
        };
        let full = join_zip(&base, href);
        let html = match read_zip(&mut archive, &full) {
            Ok(html) => html,
            Err(_) => continue,
        };
        let body = html_to_text(&html);
        if body.trim().is_empty() {
            continue;
        }
        let title = html_title(&html).unwrap_or_else(|| format!("Chapter {}", index + 1));
        chapters.push(Chapter { title, body });
    }
    if chapters.is_empty() {
        return Err("No readable chapters in this EPUB.".into());
    }
    Ok(chapters)
}

fn read_zip(archive: &mut ZipArchive<File>, name: &str) -> Result<String, String> {
    let mut buf = String::new();
    if let Ok(mut file) = archive.by_name(name) {
        file.read_to_string(&mut buf).map_err(|e| e.to_string())?;
        return Ok(buf);
    }
    let alt = name.replace('\\', "/");
    let mut file = archive
        .by_name(&alt)
        .map_err(|_| format!("Missing {name} in the EPUB."))?;
    file.read_to_string(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

fn join_zip(base: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or(href);
    if href.starts_with('/') {
        return href.trim_start_matches('/').to_string();
    }
    format!("{base}{href}")
}

fn attr_value(xml: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = xml.find(&needle)? + needle.len();
    let rest = &xml[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn manifest_hrefs(opf: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for item in opf.split("<item").skip(1) {
        let chunk = item.split("</item>").next().unwrap_or(item);
        let chunk = chunk.split("/>").next().unwrap_or(chunk);
        if let (Some(id), Some(href)) = (attr_value(chunk, "id"), attr_value(chunk, "href")) {
            map.insert(id, href);
        }
    }
    map
}

fn spine_ids(opf: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for item in opf.split("<itemref").skip(1) {
        let chunk = item.split("/>").next().unwrap_or(item);
        if let Some(id) = attr_value(chunk, "idref") {
            ids.push(id);
        }
    }
    ids
}

fn html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<h1")?;
    let after = html[start..].find('>')? + start + 1;
    let end_rel = lower[after..].find("</h1")?;
    let text = html_to_text(&html[after..after + end_rel]);
    let text = text.trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

pub fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut chars = html.chars().peekable();
    let mut in_tag = false;
    let mut tag = String::new();
    while let Some(ch) = chars.next() {
        if ch == '<' {
            in_tag = true;
            tag.clear();
            continue;
        }
        if in_tag {
            if ch == '>' {
                in_tag = false;
                let name = tag
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_start_matches('/')
                    .to_ascii_lowercase();
                if matches!(
                    name.as_str(),
                    "p" | "div" | "br" | "h1" | "h2" | "h3" | "h4" | "li" | "tr" | "blockquote"
                ) {
                    if !out.ends_with("\n\n") {
                        out.push_str("\n\n");
                    }
                }
            } else {
                tag.push(ch);
            }
            continue;
        }
        if ch == '&' {
            let mut entity = String::new();
            while let Some(next) = chars.peek().copied() {
                if next == ';' || entity.len() > 12 {
                    break;
                }
                entity.push(next);
                chars.next();
            }
            if chars.peek() == Some(&';') {
                chars.next();
            }
            out.push_str(match entity.as_str() {
                "amp" => "&",
                "lt" => "<",
                "gt" => ">",
                "quot" => "\"",
                "apos" | "#39" => "'",
                "nbsp" => " ",
                _ => " ",
            });
            continue;
        }
        out.push(ch);
    }
    let mut collapsed = String::new();
    let mut blank = false;
    for line in out.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            if !blank && !collapsed.is_empty() {
                collapsed.push('\n');
            }
            blank = true;
        } else {
            if !collapsed.is_empty() && !collapsed.ends_with('\n') {
                collapsed.push('\n');
            }
            collapsed.push_str(&line);
            collapsed.push('\n');
            blank = false;
        }
    }
    collapsed.trim().to_string()
}
