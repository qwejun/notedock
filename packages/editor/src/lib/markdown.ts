/**
 * TipTap document → Markdown, for the desktop export.
 *
 * Hand-written rather than pulled from a library because the schema is fixed and
 * small: StarterKit plus highlight, colour and images. A general converter would
 * carry rules for nodes this app cannot produce, and would still need overrides
 * for the two marks Markdown has no syntax for.
 */
import type { JSONContent } from "@tiptap/core";

/**
 * Narrow on purpose. Escaping every Markdown metacharacter turns a CJK note into
 * a field of backslashes, and CommonMark does not read intra-word `_` as
 * emphasis, so it is left alone.
 */
function escapeText(text: string): string {
  return text.replace(/([\\`*[\]])/g, "\\$1");
}

/** Text, images and hard breaks — everything that lives inside a block. */
function inline(node: JSONContent): string {
  if (node.type === "hardBreak") return "  \n";
  if (node.type === "image") {
    const src = typeof node.attrs?.src === "string" ? node.attrs.src : "";
    const alt = typeof node.attrs?.alt === "string" ? node.attrs.alt : "";
    return src ? `![${alt}](${src})` : "";
  }
  if (node.type !== "text" || !node.text) return "";

  const marks = node.marks ?? [];
  const has = (name: string): boolean => marks.some((mark) => mark.type === name);

  // Inside code the text is literal, so it must not be escaped: the backslash
  // would end up in the exported file verbatim.
  let out = has("code") ? `\`${node.text}\`` : escapeText(node.text);

  if (has("strike")) out = `~~${out}~~`;
  if (has("italic")) out = `*${out}*`;
  if (has("bold")) out = `**${out}**`;
  // `==` is the de-facto highlight syntax (Obsidian, Typora) and `<u>` is the
  // only way to say underline at all. Text colour is dropped: it is decoration,
  // and an inline `<span style>` would cost more readability than it buys.
  if (has("highlight")) out = `==${out}==`;
  if (has("underline")) out = `<u>${out}</u>`;

  const href = marks.find((mark) => mark.type === "link")?.attrs?.href;
  if (typeof href === "string" && href) out = `[${out}](${href})`;

  return out;
}

function children(node: JSONContent): string {
  return (node.content ?? []).map(inline).join("");
}

/** Blocks, one blank line between them. */
function blocks(content: JSONContent[] | undefined): string[] {
  const out: string[] = [];
  for (const node of content ?? []) {
    if (out.length > 0) out.push("");
    out.push(...block(node));
  }
  return out;
}

/**
 * Nesting is the parent's job, not a depth counter's: a list item indents every
 * line its children produced, so a nested list arrives already marked up and
 * only needs shifting right.
 */
function list(node: JSONContent): string[] {
  const ordered = node.type === "orderedList";
  const start = Number(node.attrs?.start ?? 1);
  const out: string[] = [];

  (node.content ?? []).forEach((item, index) => {
    const marker = ordered ? `${start + index}. ` : "- ";
    const indent = " ".repeat(marker.length);
    blocks(item.content).forEach((line, row) => {
      if (row === 0) out.push(marker + line);
      else out.push(line ? indent + line : "");
    });
  });

  return out;
}
function block(node: JSONContent): string[] {
  switch (node.type) {
    case "heading": {
      const level = Math.min(Math.max(Number(node.attrs?.level ?? 1), 1), 6);
      return [`${"#".repeat(level)} ${children(node)}`];
    }
    case "codeBlock": {
      const language =
        typeof node.attrs?.language === "string" ? node.attrs.language : "";
      const body = (node.content ?? []).map((child) => child.text ?? "").join("");
      return [`\`\`\`${language}`, ...body.split("\n"), "```"];
    }
    case "horizontalRule":
      return ["---"];
    case "blockquote":
      return blocks(node.content).map((line) => (line ? `> ${line}` : ">"));
    case "bulletList":
    case "orderedList":
      return list(node);
    case "image":
      return [inline(node)];
    // paragraph, and anything the schema grows later: keep the text rather than
    // dropping content on the floor.
    default:
      return [children(node)];
  }
}

/**
 * Serializes a note.
 *
 * The title leads the file as an H1. It is stored outside the body — a Y.Map
 * entry rather than part of the document — so without this the export would lose
 * the one piece of the note the user named themselves.
 */
export function toMarkdown(doc: JSONContent, title = ""): string {
  const heading = title.trim() ? [`# ${escapeText(title.trim())}`, ""] : [];
  const text = [...heading, ...blocks(doc.content)].join("\n");
  // Empty paragraphs are legitimate while editing but read as stray blank lines
  // in a file. One trailing newline, because every other tool expects it.
  return `${text.replace(/\n{3,}/g, "\n\n").trimEnd()}\n`;
}
