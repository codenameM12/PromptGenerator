// Builds the offline docs wiki in docs/wiki from the Markdown guides.
// No dependencies: the renderer covers the constructs these docs use.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const output = join(root, "docs", "wiki");

const pages = [
  { source: "README.md", out: "index.html", title: "Overview", kicker: "Start here" },
  {
    source: "docs/user-guide.md",
    out: "user-guide.html",
    title: "User guide",
    kicker: "Day-to-day use",
  },
  { source: "docs/presets.md", out: "presets.html", title: "Presets", kicker: "Targets" },
  {
    source: "docs/troubleshooting.md",
    out: "troubleshooting.html",
    title: "Troubleshooting",
    kicker: "Fix problems",
  },
  {
    source: "docs/release-notes.md",
    out: "release-notes.html",
    title: "Release notes",
    kicker: "Verification",
  },
  { source: "docs/drag-out.md", out: "drag-out.html", title: "Drag out", kicker: "Files" },
  {
    source: "docs/ui-system.md",
    out: "ui-system.html",
    title: "Interface system",
    kicker: "Design",
  },
];

const bySource = new Map(pages.map((page) => [page.source, page]));

function escapeHtml(value) {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function slugify(value) {
  return value
    .toLowerCase()
    .replace(/[`*_]/g, "")
    .replace(/[^a-z0-9\u00c0-\uffff]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function resolveHref(href, fromSource) {
  if (/^(https?:|mailto:|#)/.test(href)) return href;
  const [path, hash] = href.split("#");
  const fromDir = dirname(fromSource);
  const targetSource = relative(root, join(root, fromDir, path)).replace(/\\/g, "/");
  const page = bySource.get(targetSource);
  if (page) return `${page.out}${hash ? `#${hash}` : ""}`;
  const relativePath = relative(join(root, "docs", "wiki"), join(root, fromDir, path)).replace(
    /\\/g,
    "/",
  );
  return `${relativePath}${hash ? `#${hash}` : ""}`;
}

function renderInline(text, fromSource, linked) {
  let value = escapeHtml(text);
  const codes = [];
  value = value.replace(/`([^`]+)`/g, (_, code) => {
    codes.push(`<code>${code}</code>`);
    return `\u0000${codes.length - 1}\u0000`;
  });
  value = value.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, label, href) => {
    const resolved = resolveHref(href, fromSource);
    if (/^[a-z0-9-]+\.html/.test(resolved)) linked.add(resolved.split("#")[0]);
    return `<a href="${resolved}">${label}</a>`;
  });
  value = value.replace(/&lt;(https?:\/\/[^&]+?)&gt;/g, (_, url) => {
    return `<a href="${url}">${url}</a>`;
  });
  value = value.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  value = value.replace(/(^|[^*])\*([^*\n]+)\*/g, "$1<em>$2</em>");
  value = value.replace(/\u0000(\d+)\u0000/g, (_, index) => codes[Number(index)]);
  return value;
}

function splitRow(line) {
  const trimmed = line.trim().replace(/^\|/, "").replace(/\|$/, "");
  return trimmed.split("|").map((cell) => cell.trim());
}

function renderMarkdown(source, fromSource, linked) {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const html = [];
  const toc = [];
  let paragraph = [];
  let list = null;
  let quote = [];
  let fence = null;

  const flushParagraph = () => {
    if (!paragraph.length) return;
    html.push(`<p>${renderInline(paragraph.join(" "), fromSource, linked)}</p>`);
    paragraph = [];
  };
  const flushList = () => {
    if (!list) return;
    html.push(
      `<${list.tag}>${list.items.map((item) => `<li>${item}</li>`).join("")}</${list.tag}>`,
    );
    list = null;
  };
  const flushQuote = () => {
    if (!quote.length) return;
    html.push(
      `<blockquote>${quote
        .map((line) => `<p>${renderInline(line, fromSource, linked)}</p>`)
        .join("")}</blockquote>`,
    );
    quote = [];
  };
  const flushAll = () => {
    flushParagraph();
    flushList();
    flushQuote();
  };

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];

    if (fence !== null) {
      if (line.trimStart().startsWith("```")) {
        html.push(`<pre class="code"><code>${escapeHtml(fence.lines.join("\n"))}</code></pre>`);
        fence = null;
      } else {
        fence.lines.push(line);
      }
      continue;
    }

    if (line.trimStart().startsWith("```")) {
      flushAll();
      fence = { lines: [] };
      continue;
    }

    if (!line.trim()) {
      flushAll();
      continue;
    }

    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    if (heading) {
      flushAll();
      const level = heading[1].length;
      const raw = heading[2];
      const id = slugify(raw);
      const text = renderInline(raw, fromSource, linked).replace(/<[^>]+>/g, "");
      html.push(`<h${level} id="${id}">${renderInline(raw, fromSource, linked)}</h${level}>`);
      if (level === 2 || level === 3) toc.push({ level, id, text });
      continue;
    }

    if (/^---+\s*$/.test(line)) {
      flushAll();
      html.push("<hr />");
      continue;
    }

    if (line.startsWith(">")) {
      flushParagraph();
      flushList();
      quote.push(line.replace(/^>\s?/, ""));
      continue;
    }
    flushQuote();

    if (line.includes("|") && /^\s*\|?[\s:|-]+\|[\s:|-]*$/.test(lines[index + 1] ?? "")) {
      flushParagraph();
      flushList();
      const headers = splitRow(line);
      const rows = [];
      index += 2;
      while (index < lines.length && lines[index].includes("|") && lines[index].trim()) {
        rows.push(splitRow(lines[index]));
        index += 1;
      }
      index -= 1;
      html.push(
        `<div class="table-wrap"><table><thead><tr>${headers
          .map((cell) => `<th>${renderInline(cell, fromSource, linked)}</th>`)
          .join("")}</tr></thead><tbody>${rows
          .map(
            (row) =>
              `<tr>${row
                .map((cell) => `<td>${renderInline(cell, fromSource, linked)}</td>`)
                .join("")}</tr>`,
          )
          .join("")}</tbody></table></div>`,
      );
      continue;
    }

    const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
    const ordered = /^\s*\d+\.\s+(.*)$/.exec(line);
    if (bullet || ordered) {
      flushParagraph();
      const tag = bullet ? "ul" : "ol";
      if (!list || list.tag !== tag) {
        flushList();
        list = { tag, items: [] };
      }
      list.items.push(renderInline((bullet ?? ordered)[1], fromSource, linked));
      continue;
    }
    flushList();

    paragraph.push(line.trim());
  }
  flushAll();
  if (fence !== null) {
    html.push(`<pre class="code"><code>${escapeHtml(fence.lines.join("\n"))}</code></pre>`);
  }
  return { html: html.join("\n"), toc };
}

function pageNav(current) {
  return pages
    .map(
      (page) =>
        `<a class="nav-link${page.out === current ? " is-current" : ""}" href="${page.out}" data-title="${page.title.toLowerCase()} ${page.kicker.toLowerCase()}"><span>${page.title}</span><small>${page.kicker}</small></a>`,
    )
    .join("\n");
}

function shell(page, body, toc, backlinks, index) {
  const previous = pages[index - 1];
  const next = pages[index + 1];
  const tocHtml = toc.length
    ? `<nav class="toc" aria-label="On this page"><p class="eyebrow">On this page</p>${toc
        .map(
          (item) =>
            `<a class="toc-link toc-${item.level}" href="#${item.id}">${escapeHtml(item.text)}</a>`,
        )
        .join("")}</nav>`
    : "";
  const backlinksHtml = backlinks.length
    ? `<section class="backlinks"><p class="eyebrow">Linked from</p><ul>${backlinks
        .map((target) => `<li><a href="${target.out}">${target.title}</a></li>`)
        .join("")}</ul></section>`
    : "";
  const pager =
    previous || next
      ? `<nav class="pager" aria-label="Page navigation">${
          previous
            ? `<a href="${previous.out}"><small>Previous</small><span>${previous.title}</span></a>`
            : "<span></span>"
        }${
          next
            ? `<a class="pager-next" href="${next.out}"><small>Next</small><span>${next.title}</span></a>`
            : "<span></span>"
        }</nav>`
      : "";

  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>${page.title} · Prompt Studio docs</title>
<meta name="description" content="Prompt Studio documentation: ${page.title}" />
<link rel="stylesheet" href="assets/wiki.css" />
</head>
<body data-page="${page.out}">
<a class="skip" href="#content">Skip to content</a>
<div class="shell">
  <aside class="sidebar">
    <a class="brand" href="index.html">
      <span class="brand-mark" aria-hidden="true">PS</span>
      <span><strong>Prompt Studio</strong><small>Documentation</small></span>
    </a>
    <label class="search">
      <span class="visually-hidden">Filter pages</span>
      <input id="nav-filter" type="search" placeholder="Filter pages…" autocomplete="off" />
    </label>
    <nav class="nav" aria-label="Documentation">${pageNav(page.out)}</nav>
    <p class="sidebar-foot">Offline wiki · built from the Markdown guides</p>
  </aside>
  <main id="content" class="content">
    <header class="page-head">
      <p class="eyebrow">${page.kicker}</p>
      <h1>${page.title}</h1>
    </header>
    <article class="prose">
${body}
    </article>
    ${backlinksHtml}
    ${pager}
  </main>
  <aside class="rail">${tocHtml}</aside>
</div>
<script src="assets/wiki.js"></script>
</body>
</html>`;
}

mkdirSync(join(output, "assets"), { recursive: true });

const rendered = new Map();
const outbound = new Map();

for (const page of pages) {
  const source = readFileSync(join(root, page.source), "utf8");
  const links = new Set();
  const result = renderMarkdown(source, page.source, links);
  rendered.set(page.out, { ...page, body: result.html, toc: result.toc });
  outbound.set(page.out, links);
}

for (const [out, result] of rendered) {
  const backlinks = pages.filter(
    (candidate) => candidate.out !== out && (outbound.get(candidate.out)?.has(out) ?? false),
  );
  writeFileSync(
    join(output, out),
    shell(
      result,
      result.body,
      result.toc,
      backlinks,
      pages.findIndex((p) => p.out === out),
    ),
    "utf8",
  );
}

console.log(`Built ${pages.length} wiki pages in docs/wiki`);
