use scraper::{Html, Selector};
use serde::Deserialize;

use crate::capability::SearchHit;
use crate::domain::DomainError;
use crate::ports::SearchPort;

#[derive(Clone, Debug, Default)]
pub struct HttpClient;

#[derive(Debug, Deserialize)]
struct DuckDuckGoResponse {
    #[serde(rename = "AbstractText")]
    abstract_text: String,
    #[serde(rename = "AbstractURL")]
    abstract_url: String,
    #[serde(rename = "Heading")]
    heading: String,
    #[serde(rename = "RelatedTopics", default)]
    related_topics: Vec<DuckDuckGoTopic>,
}

#[derive(Debug, Deserialize)]
struct DuckDuckGoTopic {
    #[serde(rename = "Text")]
    text: Option<String>,
    #[serde(rename = "FirstURL")]
    first_url: Option<String>,
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Topics", default)]
    topics: Vec<DuckDuckGoTopic>,
}

impl SearchPort for HttpClient {
    fn search(&self, query: &str) -> Result<Vec<SearchHit>, DomainError> {
        if query.trim().is_empty() {
            return Err(DomainError::InvalidInput(
                "web search query is empty".to_string(),
            ));
        }

        let client = reqwest::blocking::Client::builder()
            .user_agent("agent_core/1.0")
            .build()
            .map_err(|e| DomainError::PortError(format!("http client build failed: {e}")))?;

        let response = client
            .get("https://api.duckduckgo.com/")
            .query(&[
                ("q", query),
                ("format", "json"),
                ("no_html", "1"),
                ("skip_disambig", "1"),
            ])
            .send()
            .and_then(|res| res.error_for_status())
            .map_err(|e| DomainError::PortError(format!("web search request failed: {e}")))?;

        let payload: DuckDuckGoResponse = response
            .json()
            .map_err(|e| DomainError::PortError(format!("web search decode failed: {e}")))?;

        let mut hits = Vec::new();
        if !payload.abstract_text.trim().is_empty() {
            hits.push(SearchHit {
                title: if payload.heading.trim().is_empty() {
                    query.to_string()
                } else {
                    payload.heading.clone()
                },
                snippet: format!("{} {}", payload.abstract_text, payload.abstract_url)
                    .trim()
                    .to_string(),
            });
        }
        flatten_topics(&payload.related_topics, &mut hits);
        if hits.is_empty() {
            hits = fetch_html_results(&client, query)?;
        }
        Ok(hits)
    }
}

fn fetch_html_results(
    client: &reqwest::blocking::Client,
    query: &str,
) -> Result<Vec<SearchHit>, DomainError> {
    let response = client
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query)])
        .send()
        .and_then(|res| res.error_for_status())
        .map_err(|e| DomainError::PortError(format!("html web search request failed: {e}")))?;

    let html = response
        .text()
        .map_err(|e| DomainError::PortError(format!("html web search decode failed: {e}")))?;

    Ok(parse_html_results(&html))
}

fn flatten_topics(topics: &[DuckDuckGoTopic], hits: &mut Vec<SearchHit>) {
    for topic in topics {
        if let (Some(text), Some(url)) = (topic.text.as_ref(), topic.first_url.as_ref()) {
            let title = topic
                .name
                .clone()
                .unwrap_or_else(|| text.split(" - ").next().unwrap_or(text).to_string());
            hits.push(SearchHit {
                title,
                snippet: format!("{text} {url}"),
            });
        }
        if !topic.topics.is_empty() {
            flatten_topics(&topic.topics, hits);
        }
    }
}

fn parse_html_results(html: &str) -> Vec<SearchHit> {
    let document = Html::parse_document(html);
    let result_selector = Selector::parse(".result").expect("valid selector");
    let title_selector = Selector::parse(".result__title a, a.result__a").expect("valid selector");
    let snippet_selector =
        Selector::parse(".result__snippet, .result__extras__url").expect("valid selector");

    let mut hits = Vec::new();
    for result in document.select(&result_selector) {
        let Some(title_node) = result.select(&title_selector).next() else {
            continue;
        };

        let title = title_node
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if title.is_empty() {
            continue;
        }

        let url = title_node
            .value()
            .attr("href")
            .unwrap_or_default()
            .to_string();
        let snippet = result
            .select(&snippet_selector)
            .next()
            .map(|node| node.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();

        hits.push(SearchHit {
            title,
            snippet: format!("{snippet} {url}").trim().to_string(),
        });
    }

    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_is_invalid() {
        let client = HttpClient;
        let result = client.search("   ");
        assert!(matches!(result, Err(DomainError::InvalidInput(_))));
    }

    #[test]
    fn flatten_topics_collects_nested_hits() {
        let topics = vec![DuckDuckGoTopic {
            text: None,
            first_url: None,
            name: None,
            topics: vec![DuckDuckGoTopic {
                text: Some("NeoVim - terminal editor".to_string()),
                first_url: Some("https://example.com/neovim".to_string()),
                name: None,
                topics: vec![],
            }],
        }];
        let mut hits = Vec::new();
        flatten_topics(&topics, &mut hits);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("https://example.com/neovim"));
    }

    #[test]
    fn parse_html_results_extracts_standard_result_blocks() {
        let html = r#"
        <div class="result">
          <div class="result__title">
            <a class="result__a" href="https://example.com/neovim">NeoVim Architecture</a>
          </div>
          <a class="result__snippet">Terminal editor design notes</a>
        </div>
        "#;

        let hits = parse_html_results(html);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "NeoVim Architecture");
        assert!(hits[0].snippet.contains("https://example.com/neovim"));
    }
}
