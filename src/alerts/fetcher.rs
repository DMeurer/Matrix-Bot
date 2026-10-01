use anyhow::{anyhow, Result};
use scraper::{Html, Selector};
use std::time::Duration;
use tokio::time::sleep;

const TIMEOUT_SECS: u64 = 2;
const RETRY_DELAY_SECS: u64 = 10;
const MAX_ATTEMPTS: u8 = 5;

/// Fetch the value of `property` from the first element matching `css` at `url`.
/// Retries up to 5 times with a 10-second delay and a 2-second HTTP timeout.
pub async fn fetch_value(url: &str, css: &str, property: &str) -> Result<String> {
    let mut last_err = anyhow!("No attempts made");

    for attempt in 1..=MAX_ATTEMPTS {
        match try_fetch(url, css, property).await {
            Ok(value) => return Ok(value),
            Err(e) => {
                last_err = e;
                if attempt < MAX_ATTEMPTS {
                    tracing::warn!(
                        "Fetch attempt {attempt}/{MAX_ATTEMPTS} failed for {url}: {last_err} — retrying in {RETRY_DELAY_SECS}s"
                    );
                    sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                } else {
                    tracing::error!(
                        "Fetch failed for {url} after {MAX_ATTEMPTS} attempts: {last_err}"
                    );
                }
            }
        }
    }

    Err(anyhow!(
        "Fetch failed after {MAX_ATTEMPTS} attempts: {last_err}"
    ))
}

async fn try_fetch(url: &str, css: &str, property: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
        .build()?;

    let response = client.get(url).send().await?;
    let status = response.status();
    let html = response.text().await?;

    tracing::debug!(
        "Fetched {url} — HTTP {status}, {} bytes",
        html.len()
    );

    if !status.is_success() {
        return Err(anyhow!("HTTP {status} for {url}"));
    }

    extract_value(&html, css, property)
}

pub fn extract_value(html: &str, css: &str, property: &str) -> Result<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse(css)
        .map_err(|e| anyhow!("Invalid CSS selector \"{css}\": {e:?}"))?;

    let element = document
        .select(&selector)
        .next()
        .ok_or_else(|| anyhow!("No element matches selector \"{css}\""))?;

    let value = match property {
        "innerHTML" => element.inner_html(),
        "innerText" | "text" => element.text().collect::<String>(),
        other => element
            .value()
            .attr(other)
            .ok_or_else(|| anyhow!("Attribute \"{other}\" not found on element"))?
            .to_string(),
    };

    Ok(clean_text(&value))
}

/// Normalise whitespace so stored and transmitted values stay compact:
/// - each line is trimmed,
/// - a run of spaces becomes one space; a run containing a tab becomes one tab,
/// - at most two consecutive line breaks (one empty line),
/// - no leading or trailing empty lines.
pub fn clean_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_break = false;

    for line in text.lines() {
        let line = collapse_line(line);
        if line.is_empty() {
            pending_break = true;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if pending_break { "\n\n" } else { "\n" });
        }
        out.push_str(&line);
        pending_break = false;
    }

    out
}

/// Collapse whitespace in a single line:
fn collapse_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_gap = false;
    let mut gap_has_tab = false;

    for c in line.chars() {
        if c.is_whitespace() {
            in_gap = true;
            gap_has_tab |= c == '\t';
            continue;
        }
        if in_gap && !out.is_empty() {
            out.push(if gap_has_tab { '\t' } else { ' ' });
        }
        in_gap = false;
        gap_has_tab = false;
        out.push(c);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"<html><body>
        <span class="price">29,99 €</span>
        <a class="link" href="/product/123">Buy</a>
    </body></html>"#;

    #[test]
    fn extract_inner_text() {
        assert_eq!(
            extract_value(HTML, ".price", "innerText").unwrap(),
            "29,99 €"
        );
    }

    #[test]
    fn extract_attribute() {
        assert_eq!(
            extract_value(HTML, ".link", "href").unwrap(),
            "/product/123"
        );
    }

    #[test]
    fn missing_selector_errors() {
        assert!(extract_value(HTML, ".nonexistent", "innerText").is_err());
    }

    #[test]
    fn missing_attribute_errors() {
        assert!(extract_value(HTML, ".price", "href").is_err());
    }

    #[test]
    fn clean_collapses_spaces() {
        assert_eq!(clean_text("a    b  c"), "a b c");
    }

    #[test]
    fn clean_collapses_tabs_and_mixed() {
        assert_eq!(clean_text("a\t\t\tb"), "a\tb");
        assert_eq!(clean_text("a  \t  b"), "a\tb");
    }

    #[test]
    fn clean_trims_lines() {
        assert_eq!(clean_text("  a  \n\t b\t"), "a\nb");
    }

    #[test]
    fn clean_limits_line_breaks() {
        assert_eq!(clean_text("a\n\n\n\n\nb"), "a\n\nb");
        assert_eq!(clean_text("a\n  \n\t\n \nb"), "a\n\nb");
        assert_eq!(clean_text("a\r\n\r\n\r\nb"), "a\n\nb");
        assert_eq!(clean_text("a\nb"), "a\nb");
    }

    #[test]
    fn clean_strips_outer_empty_lines() {
        assert_eq!(clean_text("\n\n  \n a \n\n\n"), "a");
    }

    #[test]
    fn extract_cleans_inner_text() {
        let html = "<div class=\"x\">\n   Line one   \n\n\n\n   Line\t\ttwo  \n</div>";
        assert_eq!(
            extract_value(html, ".x", "innerText").unwrap(),
            "Line one\n\nLine\ttwo"
        );
    }
}
