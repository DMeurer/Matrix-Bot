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

    Ok(value.trim().to_string())
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
}
