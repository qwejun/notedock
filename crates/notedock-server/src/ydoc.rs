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
    Doc, GetString, Map, ReadTxn, Transact, Update, XmlFragmentRef,
};

/// Root name `y-prosemirror` uses, and therefore what TipTap's Collaboration
/// extension writes into by default.
pub const ROOT: &str = "default";

/// Map of the note's own metadata, and the key inside it holding the title.
///
/// A map entry rather than a shared string, and not for brevity: concurrent
/// inserts into a `Y.Text` are *both* kept, which is right for prose and wrong for
/// a name — two clients each writing "blender学习" produced "blender学习" twice. As
/// a map entry the same race resolves to one of the two values.
pub const META: &str = "meta";
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
        // Both roots are resolved before the transaction opens: `get_or_insert_*`
        // takes the document's write lock itself, so reaching for one while a
        // `TransactionMut` is alive deadlocks against it.
        let root = doc.get_or_insert_xml_fragment(ROOT);
        let meta = doc.get_or_insert_map(META);
        let mut txn = doc.transact_mut();
        for line in body.split('\n') {
            let paragraph = root.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
            if !line.is_empty() {
                paragraph.push_back(&mut txn, XmlTextPrelim::new(line));
            }
        }
        if !title.trim().is_empty() {
            meta.insert(&mut txn, TITLE, title.trim());
        }
    }
    doc
}

/// Newline-separated plain text, matching what [`text::plain_text`] produces for
/// the equivalent JSON document.
pub fn plain_text(doc: &Doc) -> String {
    let txn = doc.transact();
    let mut out = String::new();
    // Absent until a client writes into it, which is the normal state of a note
    // created from the palette and not yet typed into.
    if let Some(root) = txn.get_xml_fragment(ROOT) {
        walk_children(&txn, &root, &mut out);
    }
    text::tidy(&out)
}

/// The title a client set, or empty when it never set one — in which case the
/// caller falls back to deriving it from the first line of the body.
pub fn title(doc: &Doc) -> String {
    let txn = doc.transact();
    txn.get_map(META)
        .and_then(|meta| meta.get(&txn, TITLE))
        .and_then(|value| value.cast::<String>().ok())
        .map(|value| value.trim().to_owned())
        .unwrap_or_default()
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
