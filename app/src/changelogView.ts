// A changelog entry as something the screen can draw without `v-html`: a short list of blocks with a few
// kinds of text in them. Only what a changelog uses is understood (headings, bullet lists, paragraphs,
// bold, code, links); anything else shows as plain text. Pure, so it is tested without a window.

export type Span =
  | { kind: "text"; text: string }
  | { kind: "bold"; text: string }
  | { kind: "code"; text: string }
  | { kind: "link"; text: string; url: string };

export type Block =
  | { kind: "heading"; spans: Span[] }
  | { kind: "paragraph"; spans: Span[] }
  | { kind: "list"; items: Span[][] };

/** Links are only followed when they lead to the project's own pages. */
const FOLLOWED = ["https://github.com/oxicut/", "https://oxicut.github.io/"];

export const isFollowable = (url: string): boolean => FOLLOWED.some((prefix) => url.startsWith(prefix));

const INLINE = /\*\*([^*]+)\*\*|`([^`]+)`|\[([^\]]+)\]\(([^)\s]+)\)/g;

export function spansOf(text: string): Span[] {
  const spans: Span[] = [];
  let last = 0;
  for (const m of text.matchAll(INLINE)) {
    const at = m.index ?? 0;
    if (at > last) spans.push({ kind: "text", text: text.slice(last, at) });
    if (m[1] !== undefined) spans.push({ kind: "bold", text: m[1] });
    else if (m[2] !== undefined) spans.push({ kind: "code", text: m[2] });
    else if (isFollowable(m[4])) spans.push({ kind: "link", text: m[3], url: m[4] });
    else spans.push({ kind: "text", text: m[3] });
    last = at + m[0].length;
  }
  if (last < text.length) spans.push({ kind: "text", text: text.slice(last) });
  return spans;
}

export function blocksOf(markdown: string): Block[] {
  const blocks: Block[] = [];
  let paragraph: string[] = [];
  let list: Span[][] | null = null;
  let item: string[] | null = null;

  const endItem = () => {
    if (item && list) list.push(spansOf(item.join(" ")));
    item = null;
  };
  const endList = () => {
    endItem();
    if (list) blocks.push({ kind: "list", items: list });
    list = null;
  };
  const endParagraph = () => {
    if (paragraph.length) blocks.push({ kind: "paragraph", spans: spansOf(paragraph.join(" ")) });
    paragraph = [];
  };

  for (const raw of markdown.replace(/\r\n?/g, "\n").split("\n")) {
    const line = raw.trimEnd();
    const heading = /^#{1,6}\s+(.*)$/.exec(line);
    const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
    if (!line.trim()) {
      endParagraph();
      endList();
    } else if (heading) {
      endParagraph();
      endList();
      blocks.push({ kind: "heading", spans: spansOf(heading[1]) });
    } else if (bullet) {
      endParagraph();
      endItem();
      list ??= [];
      item = [bullet[1]];
    } else if (item) {
      item.push(line.trim()); // a bullet that runs onto the next line
    } else {
      endList();
      paragraph.push(line.trim());
    }
  }
  endParagraph();
  endList();
  return blocks;
}
