use std::collections::{HashMap, HashSet};

use ammonia::Builder;

fn rich_html_builder() -> Builder<'static> {
    let tags: HashSet<&str> = [
        "p", "br", "strong", "em", "u", "s", "ul", "ol", "li", "blockquote", "code", "pre",
        "h1", "h2", "h3", "h4", "a", "span",
    ]
    .into_iter()
    .collect();

    let mut tag_attributes: HashMap<&str, HashSet<&str>> = HashMap::new();
    tag_attributes.insert(
        "a",
        ["href", "title", "target", "rel"].into_iter().collect(),
    );
    tag_attributes.insert("span", ["class"].into_iter().collect());

    let url_schemes: HashSet<&str> = ["http", "https", "mailto"].into_iter().collect();

    let mut builder = Builder::new();
    builder
        .tags(tags)
        .tag_attributes(tag_attributes)
        .url_schemes(url_schemes)
        .link_rel(Some("noopener noreferrer nofollow"));

    builder
}

pub fn sanitize_rich_html(input: &str) -> String {
    rich_html_builder().clean(input).to_string()
}

pub fn sanitize_description_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(sanitize_rich_html(s)),
        serde_json::Value::Null => serde_json::Value::Null,
        _ => serde_json::Value::Null,
    }
}
