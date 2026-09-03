//! Deriving plain text from a note's body.
//!
//! The server never renders notes, but it does need their text: for the list
//! preview, for the command palette, and as the column a future FTS5 index will
//! sit on. Doing it here rather than on the client means every write, from any
//! client, produces a consistent index.
//!
//! Two shapes of input arrive at the same place. [`plain_text`] walks a TipTap
//! JSON document; the server's Yjs materializer walks an XML tree. Both share
//! [`is_block`] and [`tidy`] so the two cannot disagree about where lines end.

use serde_json::Value as Json;

/// Node types that end a line of text.
const BLOCK_TYPES: &[&str] = &[
    "paragraph",
    "heading",
    "blockquote",
    "codeBlock",
    "listItem",
    "bulletList",
    "orderedList",
    "taskItem",
    "horizontalRule",
];

pub fn is_block(node_type: &str) -> bool {
    BLOCK_TYPES.contains(&node_type)
}

/// Collapses the blank lines that nested block nodes inevitably produce.
pub fn tidy(raw: &str) -> String {
    raw.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Flattens a ProseMirror JSON document into newline-separated plain text.
pub fn plain_text(doc: &Json) -> String {
    let mut out = String::new();
    walk(doc, &mut out);
    tidy(&out)
}

fn walk(node: &Json, out: &mut String) {
    match node {
        Json::Array(items) => {
            for item in items {
                walk(item, out);
            }
        }
        Json::Object(obj) => {
            let node_type = obj.get("type").and_then(Json::as_str).unwrap_or("");

            if node_type == "hardBreak" {
                out.push('\n');
                return;
            }
            if let Some(text) = obj.get("text").and_then(Json::as_str) {
                out.push_str(text);
            }
            if let Some(content) = obj.get("content") {
                walk(content, out);
            }
            if is_block(node_type) {
                out.push('\n');
            }
        }
        _ => {}
    }
}

/// First non-empty line, for notes the user never titled.
pub fn derive_title(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| truncate_chars(line, 80))
        .unwrap_or_default()
}

/// Leading text for the note list. Newlines become spaces so the preview stays
/// on one line in the UI.
pub fn preview(text: &str, max_chars: usize) -> String {
    let flattened = text.replace('\n', " ");
    truncate_chars(flattened.trim(), max_chars)
}

/// Truncates on a character boundary, never mid-codepoint — the notes are
/// mostly Chinese, where every character is multi-byte.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc() -> Json {
        json!({
            "type": "doc",
            "content": [
                { "type": "heading", "attrs": { "level": 1 },
                  "content": [{ "type": "text", "text": "会议记录" }] },
                { "type": "paragraph",
                  "content": [
                      { "type": "text", "text": "第一点" },
                      { "type": "hardBreak" },
                      { "type": "text", "text": "第二点" }
                  ] }
            ]
        })
    }

    #[test]
    fn flattens_blocks_and_breaks() {
        assert_eq!(plain_text(&doc()), "会议记录\n第一点\n第二点");
    }

    #[test]
    fn title_is_the_first_line() {
        assert_eq!(derive_title(&plain_text(&doc())), "会议记录");
    }

    #[test]
    fn empty_doc_yields_nothing() {
        assert_eq!(plain_text(&crate::empty_doc()), "");
        assert_eq!(derive_title(""), "");
    }

    #[test]
    fn preview_is_single_line_and_truncated_on_char_boundary() {
        let text = "一二三四五\n六七八九十";
        assert_eq!(preview(text, 6), "一二三四五 …");
        assert_eq!(preview(text, 99), "一二三四五 六七八九十");
    }
}
