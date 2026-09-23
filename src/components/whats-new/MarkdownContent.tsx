import { openUrl } from "@tauri-apps/plugin-opener";
import { parseMarkdown, type Block, type Inline } from "./markdown";
import type { JSX } from "@solidjs/web";

interface MarkdownContentProps {
  markdown: string;
}

const isSafeUrl = (url: string) => {
  try {
    const parsed = new URL(url);
    return ["http:", "https:", "mailto:"].includes(parsed.protocol);
  } catch {
    return false;
  }
};

const openSafeUrl = async (url: string) => {
  if (!isSafeUrl(url)) return;

  try {
    await openUrl(url);
  } catch (error) {
    console.error("Failed to open release note link:", error);
  }
};

const isSafeImageSrc = (src: string) => {
  if (!src.startsWith("/release-notes/")) return false;
  if (src.includes("\\") || src.includes("..")) return false;

  return true;
};

const HEADING_CLASS = {
  1: "text-base font-semibold leading-snug text-text",
  2: "text-[15px] font-semibold leading-snug text-text",
  3: "text-sm font-semibold leading-snug text-text",
} as const;

const LIST_CLASS =
  "list-disc space-y-1 ps-5 text-sm leading-relaxed text-text/80";
const ORDERED_LIST_CLASS =
  "list-decimal space-y-1 ps-5 text-sm leading-relaxed text-text/80";

function renderInline(nodes: Inline[]): (string | JSX.Element)[] {
  return nodes.map((node): string | JSX.Element => {
    switch (node.type) {
      case "text":
        return node.value;
      case "break":
        return <br />;
      case "strong":
        return <strong>{renderInline(node.children)}</strong>;
      case "em":
        return <em>{renderInline(node.children)}</em>;
      case "code":
        return (
          <code class="rounded bg-mid-gray/10 px-1 py-0.5 font-mono text-[0.85em]">
            {node.value}
          </code>
        );
      case "image":
        if (!node.src || !isSafeImageSrc(node.src)) return null;

        return (
          <img
            src={node.src}
            alt={node.alt}
            loading="lazy"
            decoding="async"
            class="mx-auto block max-h-72 max-w-full object-contain"
          />
        );
      case "link":
        if (!node.href || !isSafeUrl(node.href)) {
          return <>{renderInline(node.children)}</>;
        }

        return (
          <a
            href={node.href}
            rel="noreferrer"
            onClick={(event) => {
              event.preventDefault();
              void openSafeUrl(node.href);
            }}
            class="text-accent underline decoration-accent/40 underline-offset-2 hover:decoration-accent"
          >
            {renderInline(node.children)}
          </a>
        );
    }
  });
}

function renderBlocks(blocks: Block[]): (JSX.Element | string)[] {
  return blocks.map((block): JSX.Element | string => {
    switch (block.type) {
      case "heading":
        return (
          <h3 class={HEADING_CLASS[block.level]}>
            {renderInline(block.children)}
          </h3>
        );
      case "paragraph":
        return (
          <p class="text-sm leading-relaxed text-text/80">
            {renderInline(block.children)}
          </p>
        );
      case "list": {
        const items = block.items.map((item) => (
          <li class="pl-1 marker:text-text/50">{renderInline(item)}</li>
        ));

        return block.ordered ? (
          <ol class={ORDERED_LIST_CLASS}>{items}</ol>
        ) : (
          <ul class={LIST_CLASS}>{items}</ul>
        );
      }
      case "blockquote":
        return (
          <blockquote class="border-s-2 border-accent/50 ps-3 text-sm leading-relaxed text-text/70">
            {renderInline(block.children)}
          </blockquote>
        );
      case "code":
        return (
          <pre class="overflow-x-auto rounded-md bg-mid-gray/10 p-3 text-xs leading-relaxed text-text/80">
            <code class="block whitespace-pre font-mono text-xs">
              {block.value}
            </code>
          </pre>
        );
      case "hr":
        return <hr class="border-mid-gray/20" />;
    }
  });
}

export const MarkdownContent = (props: MarkdownContentProps) => {
  return (
    <div class="space-y-3">{renderBlocks(parseMarkdown(props.markdown))}</div>
  );
};
