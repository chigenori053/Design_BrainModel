use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

use crate::commands::knowledge_layer::save_temporary_web_hits;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WebSearchHit {
    pub title: String,
    pub snippet: String,
    pub url: String,
}

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

pub fn run(query: &str, limit: usize, format: &str) -> Result<(), String> {
    let hits = search(query, limit)?;
    let saved = save_temporary_web_hits(query, &hits)?;

    match format {
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&hits)
                    .map_err(|e| format!("json serialization failed: {e}"))?
            );
        }
        "text" => {
            if hits.is_empty() {
                println!("No web search results for \"{query}\".");
            } else {
                println!("Web Search Results");
                println!("{}", "─".repeat(55));
                println!("Query: {query}\n");
                for (i, hit) in hits.iter().enumerate() {
                    println!("{}. {}", i + 1, hit.title);
                    println!("   {}", hit.snippet);
                    println!("   {}", hit.url);
                }
                if !saved.is_empty() {
                    println!();
                    println!(
                        "Saved {} result(s) to temporary design knowledge: {}",
                        saved.len(),
                        crate::commands::knowledge_layer::temporary_knowledge_path().display()
                    );
                }
            }
        }
        other => {
            return Err(format!(
                "unknown web search format '{other}'; expected: text | json"
            ));
        }
    }

    Ok(())
}

pub fn search(query: &str, limit: usize) -> Result<Vec<WebSearchHit>, String> {
    if query.trim().is_empty() {
        return Err("web search query is empty".to_string());
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("arch_gen/0.1")
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))?;

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
        .map_err(|e| format!("web search request failed: {e}"))?;

    let payload: DuckDuckGoResponse = response
        .json()
        .map_err(|e| format!("failed to decode web search response: {e}"))?;

    let mut hits = Vec::new();

    if !payload.abstract_text.trim().is_empty() {
        hits.push(WebSearchHit {
            title: if payload.heading.trim().is_empty() {
                query.to_string()
            } else {
                payload.heading.clone()
            },
            snippet: payload.abstract_text.clone(),
            url: payload.abstract_url.clone(),
        });
    }

    flatten_topics(&payload.related_topics, &mut hits);
    if hits.is_empty() {
        hits = fetch_html_results(&client, query, limit)?;
    }
    hits.truncate(limit.max(1));
    Ok(hits)
}

fn fetch_html_results(
    client: &reqwest::blocking::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<WebSearchHit>, String> {
    let response = client
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query)])
        .send()
        .and_then(|res| res.error_for_status())
        .map_err(|e| format!("html web search request failed: {e}"))?;

    let html = response
        .text()
        .map_err(|e| format!("failed to read html web search response: {e}"))?;

    Ok(parse_html_results(&html, limit))
}

fn flatten_topics(topics: &[DuckDuckGoTopic], hits: &mut Vec<WebSearchHit>) {
    for topic in topics {
        if let (Some(text), Some(url)) = (topic.text.as_ref(), topic.first_url.as_ref()) {
            let title = topic
                .name
                .clone()
                .unwrap_or_else(|| text.split(" - ").next().unwrap_or(text).to_string());
            hits.push(WebSearchHit {
                title,
                snippet: text.clone(),
                url: url.clone(),
            });
        }
        if !topic.topics.is_empty() {
            flatten_topics(&topic.topics, hits);
        }
    }
}

fn parse_html_results(html: &str, limit: usize) -> Vec<WebSearchHit> {
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

        hits.push(WebSearchHit {
            title,
            snippet,
            url,
        });
        if hits.len() >= limit.max(1) {
            break;
        }
    }

    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_topics_collects_nested_entries() {
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
        assert_eq!(hits[0].title, "NeoVim");
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

        let hits = parse_html_results(html, 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "NeoVim Architecture");
        assert_eq!(hits[0].url, "https://example.com/neovim");
    }
}
