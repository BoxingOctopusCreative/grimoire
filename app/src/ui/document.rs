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
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
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

/// One screen of reading. A chapter or section can span many pages, and a short
/// section continues on the same page as the text that follows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageBlock {
    pub heading: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub blocks: Vec<PageBlock>,
    /// Chapter index and body offset where this page starts.
    pub chapter: usize,
    pub offset: usize,
}

/// Pack chapter text into pages of `lines_per_page` wrapped lines.
pub fn paginate(chapters: &[Chapter], lines_per_page: usize, chars_per_line: usize) -> Vec<Page> {
    let lines_per_page = lines_per_page.max(1);
    let chars_per_line = chars_per_line.max(1);
    let mut pages = Vec::new();
    let mut draft = Draft::new(0, 0);

    for (chapter_index, chapter) in chapters.iter().enumerate() {
        let heading_lines = 2.min(lines_per_page);
        if draft.lines_used > 0 && draft.lines_used + heading_lines + 1 > lines_per_page {
            pages.push(draft.finish());
            draft = Draft::new(chapter_index, 0);
        }
        if draft.blocks.is_empty() {
            draft.chapter = chapter_index;
            draft.offset = 0;
        }
        draft.blocks.push(PageBlock {
            heading: Some(chapter.title.clone()),
            text: String::new(),
        });
        draft.lines_used += heading_lines;

        let mut offset = 0usize;
        for (paragraph_index, paragraph) in chapter.body.split('\n').enumerate() {
            if paragraph_index > 0 {
                offset += 1;
            }
            if paragraph.is_empty() {
                if draft.lines_used > 0 && draft.lines_used < lines_per_page {
                    push_line(&mut draft.blocks, "");
                    draft.lines_used += 1;
                }
                continue;
            }
            let wrapped = wrap_words(paragraph, chars_per_line);
            let mut consumed = 0usize;
            for line in wrapped {
                if draft.lines_used >= lines_per_page {
                    pages.push(draft.finish());
                    draft = Draft::new(chapter_index, offset + consumed);
                    draft.blocks.push(PageBlock {
                        heading: None,
                        text: String::new(),
                    });
                }
                push_line(&mut draft.blocks, &line);
                draft.lines_used += 1;
                consumed += line.chars().count();
                if consumed < paragraph.chars().count() {
                    consumed += 1;
                }
            }
            offset += paragraph.chars().count();
        }
    }
    if draft.lines_used > 0 {
        pages.push(draft.finish());
    }
    pages
}

/// Page whose start is at or just before `chapter` / `offset`.
pub fn page_index_at(pages: &[Page], chapter: usize, offset: usize) -> usize {
    let mut found = 0;
    for (index, page) in pages.iter().enumerate() {
        if page.chapter < chapter || (page.chapter == chapter && page.offset <= offset) {
            found = index;
        } else {
            break;
        }
    }
    found
}

struct Draft {
    blocks: Vec<PageBlock>,
    lines_used: usize,
    chapter: usize,
    offset: usize,
}

impl Draft {
    fn new(chapter: usize, offset: usize) -> Self {
        Self {
            blocks: Vec::new(),
            lines_used: 0,
            chapter,
            offset,
        }
    }

    fn finish(self) -> Page {
        Page {
            blocks: self.blocks,
            chapter: self.chapter,
            offset: self.offset,
        }
    }
}

fn push_line(blocks: &mut [PageBlock], line: &str) {
    let Some(block) = blocks.last_mut() else {
        return;
    };
    if !block.text.is_empty() {
        block.text.push('\n');
    }
    block.text.push_str(line);
}

fn wrap_words(paragraph: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in paragraph.split_whitespace() {
        let word_len = word.chars().count();
        if current.is_empty() {
            if word_len <= width {
                current = word.to_string();
            } else {
                let mut rest = word;
                while rest.chars().count() > width {
                    let (head, tail) = split_chars(rest, width);
                    lines.push(head.to_string());
                    rest = tail;
                }
                current = rest.to_string();
            }
            continue;
        }
        if current.chars().count() + 1 + word_len <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn split_chars(text: &str, count: usize) -> (&str, &str) {
    let mut end = text.len();
    for (index, (byte, _)) in text.char_indices().enumerate() {
        if index == count {
            end = byte;
            break;
        }
    }
    text.split_at(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(title: &str, body: &str) -> Chapter {
        Chapter {
            title: title.into(),
            body: body.into(),
        }
    }

    #[test]
    fn long_chapter_turns_one_screen_at_a_time() {
        let body = (0..40)
            .map(|n| format!("sentence {n} of the same chapter"))
            .collect::<Vec<_>>()
            .join(" ");
        let pages = paginate(&[chapter("One", &body)], 4, 24);
        assert!(
            pages.len() > 1,
            "a long chapter should be more than one page"
        );
        assert!(pages.iter().all(|page| page.chapter == 0));
        let first = &pages[0].blocks[0].text;
        let second = &pages[1].blocks[0].text;
        assert!(!first.is_empty());
        assert!(!second.is_empty());
        assert_ne!(first, second);
        assert!(pages[0].blocks[0].heading.as_deref() == Some("One"));
        assert!(pages[1].blocks[0].heading.is_none());
    }

    #[test]
    fn short_sections_share_a_page() {
        let pages = paginate(
            &[chapter("Alpha", "alpha text"), chapter("Beta", "beta text")],
            12,
            40,
        );
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].blocks.len(), 2);
        assert_eq!(pages[0].blocks[1].heading.as_deref(), Some("Beta"));
    }

    #[test]
    fn page_index_stays_inside_a_split_chapter() {
        let body = (0..30)
            .map(|n| format!("word{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        let pages = paginate(&[chapter("One", &body), chapter("Two", "end")], 3, 12);
        let middle = pages.len() / 2;
        let page = &pages[middle];
        assert_eq!(page_index_at(&pages, page.chapter, page.offset), middle);
    }
}
