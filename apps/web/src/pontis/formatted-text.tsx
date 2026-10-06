import type { ReactNode } from "react";

type InlinePart =
  | { readonly kind: "text"; readonly value: string }
  | { readonly kind: "bold" | "italic" | "code"; readonly value: string; readonly children?: readonly InlinePart[] }
  | { readonly kind: "link"; readonly label: string; readonly href: string; readonly host: string };

type Block =
  | { readonly kind: "paragraph"; readonly text: string }
  | { readonly kind: "code"; readonly language: string; readonly text: string }
  | { readonly kind: "list"; readonly ordered: boolean; readonly start: number; readonly items: readonly string[] };

const FENCE = /^```([\w-]*)\s*$/;
const BULLET = /^- (.*)$/;
const NUMBERED = /^(\d+)\. (.*)$/;

/**
 * Render the small reply grammar used by the chat prototype. This parser only
 * creates known React elements; markup-looking input remains text.
 */
export function FormattedText({ text }: { readonly text: string }) {
  const blocks = parseBlocks(text);
  return (
    <div className="agent-text break-words [overflow-wrap:anywhere]">
      {blocks.map((block, index) => renderBlock(block, index))}
    </div>
  );
}

function renderBlock(block: Block, key: number): ReactNode {
  switch (block.kind) {
    case "paragraph":
      return (
        <p key={key} className="whitespace-pre-wrap [overflow-wrap:anywhere]">
          {renderInline(parseInline(block.text))}
        </p>
      );
    case "code":
      return (
        <pre
          key={key}
          aria-label={block.language ? `${block.language} code` : undefined}
          className="max-w-full overflow-x-auto whitespace-pre-wrap [overflow-wrap:anywhere]"
        >
          <code>{block.text}</code>
        </pre>
      );
    case "list": {
      const List = block.ordered ? "ol" : "ul";
      return (
        <List key={key} start={block.ordered ? block.start : undefined} className="min-w-0 list-inside space-y-1 [overflow-wrap:anywhere]">
          {block.items.map((item, itemIndex) => (
            <li key={itemIndex} className="[overflow-wrap:anywhere]">
              {renderInline(parseInline(item))}
            </li>
          ))}
        </List>
      );
    }
  }
}

function parseBlocks(source: string): Block[] {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index] ?? "";
    if (line.trim().length === 0) {
      index += 1;
      continue;
    }

    const fence = line.match(FENCE);
    if (fence) {
      const code: string[] = [];
      index += 1;
      while (index < lines.length && !/^```\s*$/.test(lines[index] ?? "")) {
        code.push(lines[index] ?? "");
        index += 1;
      }
      if (index < lines.length) index += 1;
      blocks.push({ kind: "code", language: fence[1] ?? "", text: code.join("\n") });
      continue;
    }

    const listStart = listLine(line);
    if (listStart) {
      const items: string[] = [];
      const ordered = listStart.ordered;
      const start = listStart.start;
      while (index < lines.length) {
        const next = listLine(lines[index] ?? "");
        if (!next || next.ordered !== ordered) break;
        items.push(next.text);
        index += 1;
      }
      blocks.push({ kind: "list", ordered, start, items });
      continue;
    }

    const paragraph = [line];
    index += 1;
    while (index < lines.length) {
      const next = lines[index] ?? "";
      if (next.trim().length === 0 || FENCE.test(next) || listLine(next)) break;
      paragraph.push(next);
      index += 1;
    }
    blocks.push({ kind: "paragraph", text: paragraph.join("\n") });
  }

  return blocks;
}

function listLine(line: string): { readonly ordered: boolean; readonly start: number; readonly text: string } | null {
  const bullet = line.match(BULLET);
  if (bullet) return { ordered: false, start: 1, text: bullet[1] ?? "" };
  const numbered = line.match(NUMBERED);
  if (!numbered) return null;
  const start = Number(numbered[1]);
  return { ordered: true, start: Number.isSafeInteger(start) ? start : 1, text: numbered[2] ?? "" };
}

function parseInline(source: string, depth = 0): InlinePart[] {
  if (depth > 4) return [{ kind: "text", value: source }];

  const parts: InlinePart[] = [];
  let text = "";
  const flush = () => {
    if (!text) return;
    parts.push({ kind: "text", value: text });
    text = "";
  };

  for (let index = 0; index < source.length;) {
    if (source[index] === "<") {
      const end = source.indexOf(">", index + 1);
      if (end >= 0) {
        text += source.slice(index, end + 1);
        index = end + 1;
        continue;
      }
    }

    const image = source.slice(index).match(/^!\[[^\]\n]*\]\([^\n)]*\)/);
    if (image) {
      text += image[0];
      index += image[0].length;
      continue;
    }

    const marker = source.slice(index, index + 2);
    if (marker === "**") {
      const end = source.indexOf("**", index + 2);
      if (end > index + 2) {
        flush();
        parts.push({ kind: "bold", value: source.slice(index + 2, end), children: parseInline(source.slice(index + 2, end), depth + 1) });
        index = end + 2;
        continue;
      }
    }

    const char = source[index];
    if (char === "*") {
      const end = source.indexOf("*", index + 1);
      if (end > index + 1 && source[end + 1] !== "*") {
        flush();
        parts.push({ kind: "italic", value: source.slice(index + 1, end), children: parseInline(source.slice(index + 1, end), depth + 1) });
        index = end + 1;
        continue;
      }
    }

    if (char === "`") {
      const end = source.indexOf("`", index + 1);
      if (end > index + 1) {
        flush();
        parts.push({ kind: "code", value: source.slice(index + 1, end) });
        index = end + 1;
        continue;
      }
    }

    if (char === "[") {
      const link = source.slice(index).match(/^\[([^\]\n]+)\]\(([^\s)]+)\)/);
      if (link) {
        const href = safeWebUrl(link[2] ?? "");
        if (href !== null) {
          flush();
          parts.push({ kind: "link", label: link[1] ?? "", href: href.href, host: href.host });
        } else {
          text += link[0];
        }
        index += link[0].length;
        continue;
      }
    }

    text += char ?? "";
    index += 1;
  }

  flush();
  return parts;
}

function safeWebUrl(raw: string): URL | null {
  try {
    const url = new URL(raw);
    return (url.protocol === "http:" || url.protocol === "https:") && !url.username && !url.password ? url : null;
  } catch {
    return null;
  }
}

function renderInline(parts: readonly InlinePart[]): ReactNode[] {
  return parts.map((part, index) => {
    switch (part.kind) {
      case "text":
        return part.value;
      case "bold":
        return <strong key={index}>{renderInline(part.children ?? [])}</strong>;
      case "italic":
        return <em key={index}>{renderInline(part.children ?? [])}</em>;
      case "code":
        return <code key={index}>{part.value}</code>;
      case "link":
        return (
          <a key={index} href={part.href} target="_blank" rel="noopener noreferrer" aria-label={`${part.label} on ${part.host}, opens in a new tab`}>
            {part.label} · {part.host}
          </a>
        );
    }
  });
}
