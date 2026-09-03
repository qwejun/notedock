//! Materializing a note's Yjs document into text the server can index.
//!
//! `y-prosemirror` stores a ProseMirror document as a Yjs XML tree: the root is
//! an `XmlFragment`, block nodes are `XmlElement`s tagged with the node name, and
//! the leaves are `XmlText`. Walking it here — rather than asking clients to send
//! a derived title — keeps the "title is the first line" rule in one place, and
//! keeps it true even for an edit made by a client that has never heard of it.

use notedock_api::text;
use yrs::{
    types::xml::{XmlElementPrelim, XmlFragment, XmlOut, XmlTextPrelim},
    updates::decoder::Decode,
    Doc, GetString, ReadTxn, Text, Transact, Update, XmlFragmentRef,
};

/// Root name `y-prosemirror` uses, and therefore what TipTap's Collaboration
/// extension writes into by default.
pub const ROOT: &str = "default";
pub const TITLE: &str = "title";

/// Builds a document by replaying stored updates in order.
pub fn replay(updates: &[Vec<u8>]) -> anyhow::Result<Doc> {
    let doc = Doc::new();
    {
        let mut txn = doc.transact_mut();
        for bytes in updates {
            let update = Update::decode_v1(bytes)?;
            txn.apply_update(update)?;
        }
    }
    Ok(doc)
}

/// Rebuilds a minimal collaborative document for notes created by the legacy
/// JSON-backed server. The Yjs migration keeps `content_text` as a derived
/// fallback, so opening an older note can restore its plain text into the new
/// document instead of showing only its title.
pub fn from_legacy_text(body: &str, title: &str) -> Doc {
    let doc = Doc::new();
    {
        let root = doc.get_or_insert_xml_fragment(ROOT);
        let mut txn = doc.transact_mut();
        for line in body.split('\n') {
            let paragraph = root.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
            if !line.is_empty() {
                paragraph.push_back(&mut txn, XmlTextPrelim::new(line));
            }
        }
        if !title.trim().is_empty() {
            let text = doc.get_or_insert_text(TITLE);
            text.insert(&mut txn, 0, title.trim());
        }
    }
    doc
}

/// Newline-separated plain text, matching what [`text::plain_text`] produces for
/// the equivalent JSON document.
pub fn plain_text(doc: &Doc) -> String {
    let root = doc.get_or_insert_xml_fragment(ROOT);
    let txn = doc.transact();
    let mut out = String::new();
    walk_children(&txn, &root, &mut out);
    text::tidy(&out)
}

pub fn title(doc: &Doc) -> String {
    let value = doc.get_or_insert_text(TITLE);
    value.get_string(&doc.transact()).trim().to_owned()
}

fn walk_children<T: ReadTxn>(txn: &T, fragment: &XmlFragmentRef, out: &mut String) {
    for node in fragment.children(txn) {
        walk(txn, &node, out);
    }
}

fn walk<T: ReadTxn>(txn: &T, node: &XmlOut, out: &mut String) {
    match node {
        XmlOut::Text(value) => out.push_str(&value.get_string(txn)),
        XmlOut::Element(element) => {
            let tag = element.tag().to_string();
            if tag == "hardBreak" {
                out.push('\n');
                return;
            }
            for child in element.children(txn) {
                walk(txn, &child, out);
            }
            if text::is_block(&tag) {
                out.push('\n');
            }
        }
        XmlOut::Fragment(inner) => walk_children(txn, inner, out),
    }
}
