use crate::error::{err, AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

const SEARCH_API: &str = "https://openlibrary.org/search.json";
const MAX_ATTEMPTS: u32 = 4;

#[derive(Debug, Clone, Serialize)]
pub struct CoverCandidate {
    pub id: String,
    pub volume_id: Option<String>,
    pub title: String,
    pub authors: String,
    pub preview_url: String,
    pub download_urls: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MetadataMatch {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub categories: Vec<String>,
    pub identifiers: Vec<(String, String)>,
    pub cover_url: Option<String>,
    pub cover_urls: Vec<String>,
    pub openlibrary_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    docs: Option<Vec<SearchDoc>>,
}

#[derive(Debug, Deserialize)]
struct SearchDoc {
    key: Option<String>,
    title: Option<String>,
    author_name: Option<Vec<String>>,
    subject: Option<Vec<String>>,
    cover_i: Option<i64>,
    isbn: Option<Vec<String>>,
    edition_key: Option<Vec<String>>,
    first_sentence: Option<FirstSentence>,
    editions: Option<EditionsBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FirstSentence {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct EditionsBlock {
    docs: Option<Vec<EditionDoc>>,
}

#[derive(Debug, Deserialize)]
struct EditionDoc {
    key: Option<String>,
    title: Option<String>,
    cover_i: Option<i64>,
    isbn: Option<Vec<String>>,
}

fn http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Grimoire/0.1 (ebook library manager; +https://github.com/BoxingOctopus/grimoire)")
        .build()
        .map_err(|e| err(format!("Could not create HTTP client: {e}")))
}

/// Prefer ISBN search, then title + author.
pub async fn lookup_book(
    title: &str,
    authors: &[String],
    isbn: Option<&str>,
) -> AppResult<Option<MetadataMatch>> {
    let matches = lookup_book_candidates(title, authors, isbn, 8).await?;
    Ok(pick_best(matches, title))
}

/// Return several Open Library editions/works for cover picking.
pub async fn lookup_book_candidates(
    title: &str,
    authors: &[String],
    isbn: Option<&str>,
    max_results: u32,
) -> AppResult<Vec<MetadataMatch>> {
    let client = http_client()?;
    let mut collected: Vec<MetadataMatch> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    let mut merge = |items: Vec<MetadataMatch>, collected: &mut Vec<MetadataMatch>| {
        for item in items {
            let key = item
                .openlibrary_key
                .clone()
                .or_else(|| item.cover_url.clone())
                .unwrap_or_else(|| {
                    format!(
                        "{}:{}",
                        item.title.as_deref().unwrap_or(""),
                        item.authors.join(",")
                    )
                });
            if seen_ids.insert(key) {
                collected.push(item);
            }
        }
    };

    if let Some(isbn) = isbn.map(str::trim).filter(|s| !s.is_empty()) {
        let digits: String = isbn
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
            .collect();
        if !digits.is_empty() {
            match search_docs(&client, &[("isbn", digits.as_str())], max_results).await {
                Ok(items) => merge(items, &mut collected),
                Err(e) => {
                    eprintln!("Grimoire Open Library ISBN lookup failed, trying title: {e}");
                }
            }
        }
    }

    let title = title.trim();
    if !title.is_empty() && !title.eq_ignore_ascii_case("untitled") {
        let mut params: Vec<(&str, String)> = vec![("title", title.to_string())];
        if let Some(author) = authors
            .iter()
            .map(|a| a.trim())
            .find(|a| !a.is_empty() && !a.eq_ignore_ascii_case("unknown"))
        {
            params.push(("author", author.to_string()));
        }
        let owned: Vec<(String, String)> = params
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let refs: Vec<(&str, &str)> = owned.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        match search_docs(&client, &refs, max_results).await {
            Ok(items) => merge(items, &mut collected),
            Err(e) if collected.is_empty() => return Err(e),
            Err(e) => eprintln!("Grimoire Open Library title lookup failed: {e}"),
        }

        if collected.len() < 3 {
            let mut loose = title.to_string();
            if let Some(author) = authors
                .iter()
                .map(|a| a.trim())
                .find(|a| !a.is_empty() && !a.eq_ignore_ascii_case("unknown"))
            {
                loose.push(' ');
                loose.push_str(author);
            }
            match search_docs(&client, &[("q", loose.as_str())], max_results).await {
                Ok(items) => merge(items, &mut collected),
                Err(e) if collected.is_empty() => return Err(e),
                Err(e) => eprintln!("Grimoire Open Library loose lookup failed: {e}"),
            }
        }
    }

    Ok(collected)
}

pub fn cover_candidates_from_matches(matches: &[MetadataMatch]) -> Vec<CoverCandidate> {
    let mut out = Vec::new();
    let mut seen_preview = HashSet::new();

    for item in matches {
        // Prefer distinct cover URL sets from the match itself.
        let Some(preview) = item
            .cover_urls
            .first()
            .cloned()
            .or_else(|| item.cover_url.clone())
        else {
            continue;
        };
        if !seen_preview.insert(preview.clone()) {
            continue;
        }
        let volume_id = item.openlibrary_key.clone();
        let id = volume_id.clone().unwrap_or_else(|| preview.clone());
        out.push(CoverCandidate {
            id,
            volume_id,
            title: item.title.clone().unwrap_or_else(|| "Untitled".into()),
            authors: item.authors.join(", "),
            preview_url: preview,
            download_urls: item.cover_urls.clone(),
        });

        // Also surface edition/ISBN-specific covers already embedded in cover_urls
        // via expand_doc; additional candidates come from separate matches.
    }
    out
}

async fn search_docs(
    client: &reqwest::Client,
    params: &[(&str, &str)],
    max_results: u32,
) -> AppResult<Vec<MetadataMatch>> {
    let max_results = max_results.clamp(1, 20);
    let mut url = reqwest::Url::parse(SEARCH_API).map_err(|e| err(e.to_string()))?;
    {
        let mut pairs = url.query_pairs_mut();
        for (k, v) in params {
            pairs.append_pair(k, v);
        }
        pairs.append_pair("limit", &max_results.to_string());
        pairs.append_pair(
            "fields",
            "key,title,author_name,subject,cover_i,isbn,edition_key,first_sentence,editions,editions.key,editions.title,editions.cover_i,editions.isbn",
        );
    }

    let mut last_error = err("Open Library request failed");
    for attempt in 0..MAX_ATTEMPTS {
        if attempt > 0 {
            let delay_ms = 400u64 * 2u64.pow(attempt - 1);
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        let response = match client.get(url.clone()).send().await {
            Ok(response) => response,
            Err(e) => {
                last_error = err(format!("Open Library request failed: {e}"));
                continue;
            }
        };

        let status = response.status();
        if status.is_success() {
            let payload: SearchResponse = response
                .json()
                .await
                .map_err(|e| err(format!("Could not parse Open Library response: {e}")))?;
            let docs = payload.docs.unwrap_or_default();
            let mut out = Vec::new();
            for doc in docs {
                out.extend(expand_doc(doc));
            }
            return Ok(out);
        }

        let body = response.text().await.unwrap_or_default();
        last_error = map_http_error(status.as_u16(), &body);
        if matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504) && attempt + 1 < MAX_ATTEMPTS {
            continue;
        }
        return Err(last_error);
    }
    Err(last_error)
}

fn expand_doc(doc: SearchDoc) -> Vec<MetadataMatch> {
    let work_title = doc
        .title
        .as_ref()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    let authors = doc
        .author_name
        .unwrap_or_default()
        .into_iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect::<Vec<_>>();
    let categories = doc
        .subject
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(12)
        .collect::<Vec<_>>();
    let description = match doc.first_sentence {
        Some(FirstSentence::One(s)) => {
            let t = s.trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        }
        Some(FirstSentence::Many(list)) => list
            .into_iter()
            .map(|s| s.trim().to_string())
            .find(|s| !s.is_empty()),
        None => None,
    };

    let mut out = Vec::new();

    // Work-level candidate.
    {
        let isbns = doc.isbn.clone().unwrap_or_default();
        let cover_urls = build_cover_urls(doc.cover_i, &isbns, doc.edition_key.as_deref());
        if work_title.is_some() || !authors.is_empty() || !cover_urls.is_empty() {
            let mut identifiers = Vec::new();
            if let Some(isbn) = pick_best_isbn(&isbns) {
                identifiers.push(("isbn".into(), isbn));
            }
            out.push(MetadataMatch {
                title: work_title.clone(),
                authors: authors.clone(),
                description: description.clone(),
                categories: categories.clone(),
                identifiers,
                cover_url: cover_urls.first().cloned(),
                cover_urls,
                openlibrary_key: doc.key.clone(),
            });
        }
    }

    // Edition-level candidates for the cover picker.
    if let Some(block) = doc.editions {
        for edition in block.docs.unwrap_or_default() {
            let edition_key = edition
                .key
                .as_deref()
                .map(|k| k.trim_start_matches("/books/").to_string())
                .filter(|k| !k.is_empty());
            let isbns = edition.isbn.unwrap_or_default();
            let cover_urls = build_cover_urls(
                edition.cover_i.or(doc.cover_i),
                &isbns,
                edition_key.as_ref().map(|s| vec![s.clone()]).as_deref(),
            );
            if cover_urls.is_empty() {
                continue;
            }
            let mut identifiers = Vec::new();
            if let Some(isbn) = pick_best_isbn(&isbns) {
                identifiers.push(("isbn".into(), isbn));
            }
            out.push(MetadataMatch {
                title: edition
                    .title
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .or_else(|| work_title.clone()),
                authors: authors.clone(),
                description: description.clone(),
                categories: categories.clone(),
                identifiers,
                cover_url: cover_urls.first().cloned(),
                cover_urls,
                openlibrary_key: edition_key.map(|k| format!("/books/{k}")),
            });
        }
    }

    // Extra ISBN-based cover candidates when editions block is thin.
    if out.len() < 2 {
        for isbn in doc.isbn.unwrap_or_default().into_iter().take(6) {
            let isbn = isbn.trim().to_string();
            if isbn.is_empty() {
                continue;
            }
            let cover_urls = cover_urls_for_isbn(&isbn);
            if cover_urls.is_empty() {
                continue;
            }
            out.push(MetadataMatch {
                title: work_title.clone(),
                authors: authors.clone(),
                description: description.clone(),
                categories: categories.clone(),
                identifiers: vec![("isbn".into(), isbn.clone())],
                cover_url: cover_urls.first().cloned(),
                cover_urls,
                openlibrary_key: Some(format!("isbn:{isbn}")),
            });
        }
    }

    out
}

fn pick_best_isbn(isbns: &[String]) -> Option<String> {
    let cleaned: Vec<String> = isbns
        .iter()
        .map(|s| {
            s.chars()
                .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
                .collect::<String>()
        })
        .filter(|s| s.len() == 10 || s.len() == 13)
        .collect();
    cleaned
        .iter()
        .find(|s| s.len() == 13)
        .cloned()
        .or_else(|| cleaned.first().cloned())
}

fn build_cover_urls(
    cover_i: Option<i64>,
    isbns: &[String],
    edition_keys: Option<&[String]>,
) -> Vec<String> {
    let mut urls = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |url: String| {
        if seen.insert(url.clone()) {
            urls.push(url);
        }
    };

    if let Some(id) = cover_i {
        push(format!(
            "https://covers.openlibrary.org/b/id/{id}-L.jpg?default=false"
        ));
        push(format!(
            "https://covers.openlibrary.org/b/id/{id}-M.jpg?default=false"
        ));
    }
    if let Some(isbn) = pick_best_isbn(isbns) {
        for u in cover_urls_for_isbn(&isbn) {
            push(u);
        }
    }
    if let Some(keys) = edition_keys {
        for key in keys.iter().take(3) {
            let olid = key.trim().trim_start_matches("/books/");
            if olid.is_empty() {
                continue;
            }
            push(format!(
                "https://covers.openlibrary.org/b/olid/{olid}-L.jpg?default=false"
            ));
            push(format!(
                "https://covers.openlibrary.org/b/olid/{olid}-M.jpg?default=false"
            ));
        }
    }
    urls
}

fn cover_urls_for_isbn(isbn: &str) -> Vec<String> {
    vec![
        format!("https://covers.openlibrary.org/b/isbn/{isbn}-L.jpg?default=false"),
        format!("https://covers.openlibrary.org/b/isbn/{isbn}-M.jpg?default=false"),
    ]
}

fn pick_best(mut items: Vec<MetadataMatch>, query: &str) -> Option<MetadataMatch> {
    if items.is_empty() {
        return None;
    }
    if items.len() == 1 {
        return items.pop();
    }
    let q = query.to_ascii_lowercase();
    items.sort_by_key(|item| {
        let title = item.title.as_deref().unwrap_or("").to_ascii_lowercase();
        let exact = if q.contains(&title) { 0 } else { 1 };
        let has_cover = if item.cover_url.is_some() { 0 } else { 1 };
        let has_isbn = if item.identifiers.iter().any(|(k, _)| k == "isbn") {
            0
        } else {
            1
        };
        (exact, has_cover, has_isbn)
    });
    items.into_iter().next()
}

fn map_http_error(status: u16, body: &str) -> AppError {
    let detail = body.chars().take(160).collect::<String>();
    let hint = match status {
        503 | 502 | 504 => " Open Library is temporarily unavailable. Wait a moment and try again.",
        429 => " Open Library rate-limited the request. Wait a moment and try again.",
        400 => " The search query was rejected. Try editing the title or ISBN, then fetch again.",
        _ => "",
    };
    err(format!(
        "Open Library returned {status}{}{}",
        if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        },
        hint
    ))
}

pub async fn download_cover_from_urls(urls: &[String]) -> AppResult<Vec<u8>> {
    if urls.is_empty() {
        return Err(err("No cover URLs to download"));
    }
    let client = http_client()?;
    let mut last_error = err("Cover download failed");

    for url in urls {
        match download_one_cover(&client, url).await {
            Ok(bytes) => return Ok(bytes),
            Err(e) => last_error = e,
        }
    }
    Err(last_error)
}

async fn download_one_cover(client: &reqwest::Client, url: &str) -> AppResult<Vec<u8>> {
    let mut last_error = err("Cover download failed");
    for attempt in 0..MAX_ATTEMPTS {
        if attempt > 0 {
            let delay_ms = 300u64 * 2u64.pow(attempt - 1);
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        let response = match client.get(url).send().await {
            Ok(response) => response,
            Err(e) => {
                last_error = err(format!("Cover download failed: {e}"));
                continue;
            }
        };

        if response.status().is_success() {
            let bytes = response
                .bytes()
                .await
                .map_err(|e| err(format!("Could not read cover bytes: {e}")))?;
            if bytes.len() < 800 {
                last_error = err("Cover image was too small");
                break;
            }
            return Ok(bytes.to_vec());
        }

        let status = response.status().as_u16();
        last_error = err(format!("Cover download returned HTTP {status}"));
        if matches!(status, 429 | 500 | 502 | 503 | 504) && attempt + 1 < MAX_ATTEMPTS {
            continue;
        }
        break;
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_isbn_cover_urls() {
        let urls = cover_urls_for_isbn("9780140328721");
        assert!(urls[0].contains("isbn/9780140328721-L"));
    }

    #[test]
    fn picks_isbn13() {
        let best = pick_best_isbn(&["0306406152".into(), "9780306406157".into()]);
        assert_eq!(best.as_deref(), Some("9780306406157"));
    }
}
