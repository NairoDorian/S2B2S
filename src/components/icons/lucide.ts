/**
 * Vendored Lucide icons — GENERATED, do not edit by hand.
 *
 * Generated from lucide-react@1.44.0 by a one-shot script (2026-09-13), then
 * the dependency was removed from package.json. Re-running it means restoring
 * that dev dependency first; the icon data below is the whole of what was
 * taken, so nothing else about the package is needed to keep these rendering.
 *
 * Why they are vendored rather than depended on: `lucide-react` is a React
 * component library, and Phase 1 of the Solid 2 migration (docs/PLAN_SOLIDJS_2.md)
 * removes every React-only dependency *before* the framework swap, so each
 * removal is a shippable improvement on its own. The app already hand-writes
 * the rest of its SVGs (components/icons/), so this is the same trade the
 * `ui/Dropdown` and `ui/Dialog` rewrites make: own the 60 lines you use
 * instead of the framework binding you do not.
 *
 * Rendering is deliberately data-driven — one `createElement` loop over the
 * node data — rather than 58 hand-generated JSX trees. It keeps this
 * file readable, keeps attribute-name conversion in one place, and ports to
 * Solid's `createElement` unchanged in Phase 3.
 *
 * Attribute names, defaults, the `lucide lucide-<name>` class and the
 * `aria-hidden` rule are copied from lucide-react's own renderer
 * (`Icon.mjs` / `buildLucideIconNode.mjs`) so the markup is identical to what
 * the package emitted.
 *
 * ---------------------------------------------------------------------------
 * The icons themselves are from Lucide (https://lucide.dev), ISC licensed:
 *
 *   Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as
 *   part of Feather (MIT). All other copyright (c) for Lucide are held by
 *   Lucide Contributors 2022.
 *
 *   Permission to use, copy, modify, and/or distribute this software for any
 *   purpose with or without fee is hereby granted, provided that the above
 *   copyright notice and this permission notice appear in all copies.
 *
 *   THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
 *   WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 *   MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
 *   ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 *   WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
 *   ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR
 *   IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
 */

import { createEffect } from "solid-js";
import type { JSX } from "@solidjs/web";

/** `<tag, attrs>` pairs, exactly as lucide stores them. */
type IconNode = readonly [string, Record<string, string | number>];

export type LucideIconProps = Omit<Record<string, string | number>, "ref">;

export type LucideIcon = ((props: LucideIconProps) => JSX.Element) & {
  displayName?: string;
};

/**
 * React DOM attribute names differ from the SVG ones lucide stores. Its
 * renderer passes this same map (`buildLucideIconForReact.mjs`); without it a
 * `stroke-width` passed straight through would be dropped by React.
 * No child node below uses a hyphenated name today, but the map stays so a
 * future re-vendor cannot silently lose one. */
/** React/lucide camelCase attribute names → the SVG ones. */
const SVG_ATTR: Record<string, string> = {
  className: "class",
  strokeWidth: "stroke-width",
  strokeLinecap: "stroke-linecap",
  strokeLinejoin: "stroke-linejoin",
  strokeDasharray: "stroke-dasharray",
  strokeDashoffset: "stroke-dashoffset",
  strokeMiterlimit: "stroke-miterlimit",
  fillRule: "fill-rule",
  clipRule: "clip-rule",
};

function createElement(
  tag: string,
  attrs: Record<string, string | number>,
  ...children: any[]
) {
  const el = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [k, v] of Object.entries(attrs)) {
    // `key` is React's row identity, never a DOM attribute.
    if (k === "key" || v === undefined || v === null) continue;
    el.setAttribute(SVG_ATTR[k] ?? k, String(v));
  }
  // `flat()` because the icon renderer passes its child nodes as one array.
  // Without it an array is neither a string nor a Node, so every glyph's paths
  // were dropped and each icon drew an empty <svg>.
  for (const child of children.flat()) {
    if (typeof child === "string")
      el.appendChild(document.createTextNode(child));
    else if (child instanceof Node) el.appendChild(child);
  }
  return el;
}

/** Lucide's defaults, from `defaultAttributes.mjs`. */
const BASE = {
  xmlns: "http://www.w3.org/2000/svg",
  width: 24,
  height: 24,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

/**
 * Lucide suppresses an icon `aria-hidden="true"` unless the caller supplied
 * something that names it — an `aria-*` prop, `role` or `title`. Copied so a
 * future `aria-label` on one of these does not end up hidden from screen
 * readers. Callers that pass `aria-hidden` explicitly still win: it arrives in
 * `rest`, which spreads last.
 */
function hasA11yProp(props: Record<string, unknown>): boolean {
  for (const p in props)
    if (p.startsWith("aria-") || p === "role" || p === "title") return true;
  return false;
}

const pascal = (kebabName: string) =>
  kebabName
    .split("-")
    .map((p) => p.charAt(0).toUpperCase() + p.slice(1))
    .join("");

function createIcon(
  name: string,
  aliases: readonly string[],
  node: readonly IconNode[],
): LucideIcon {
  const Icon: LucideIcon = (props) => {
    const { class: callerClass, width, height, ...rest } = props;
    // `lucide lucide-<name> lucide-<alias>… <caller's>`, exactly as lucide's
    // mergeClasses builds it. The alias classes look redundant and are not:
    // they are part of the package's public styling contract, so a stylesheet
    // written against `lucide-alert-circle` keeps working.
    const classes = [
      "lucide",
      `lucide-${name}`,
      ...aliases.map((a) => `lucide-${a}`),
      callerClass,
    ]
      .filter(Boolean)
      .join(" ");
    const el = createElement(
      "svg",
      {
        ...BASE,
        width: width ?? BASE.width,
        height: height ?? BASE.height,
        class: classes,
        ...(hasA11yProp(rest) ? {} : { "aria-hidden": "true" }),
        ...rest,
      },
      node.map(([tag, attrs]) =>
        createElement(
          tag,
          Object.fromEntries(Object.entries(attrs).map(([k, v]) => [k, v])),
        ),
      ),
    );
    // A Solid component body runs once, so the imperative build above pins
    // whatever `props.class` (and size) held at mount — and callers do pass
    // dynamic values (`animate-spin` while fetching). Reading the props
    // getters in the compute keeps the attributes live.
    createEffect(
      () => [props.class, props.width, props.height] as const,
      ([cls, w, h]) => {
        const live = [
          "lucide",
          `lucide-${name}`,
          ...aliases.map((a) => `lucide-${a}`),
          cls,
        ]
          .filter(Boolean)
          .join(" ");
        el.setAttribute("class", live);
        if (w !== undefined) el.setAttribute("width", String(w));
        if (h !== undefined) el.setAttribute("height", String(h));
      },
    );
    return el;
  };
  Icon.displayName = pascal(name);
  return Icon;
}

/** `<canonical name, aliases, node data>`, one entry per distinct glyph. */
const ICONS: readonly (readonly [
  string,
  readonly string[],
  readonly IconNode[],
])[] = [
  [
    "activity",
    [],
    [
      [
        "path",
        {
          d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2",
          key: "169zse",
        },
      ],
    ],
  ],
  [
    "audio-lines",
    [],
    [
      ["path", { d: "M2 10v3", key: "1fnikh" }],
      ["path", { d: "M6 6v11", key: "11sgs0" }],
      ["path", { d: "M10 3v18", key: "yhl04a" }],
      ["path", { d: "M14 8v7", key: "3a1oy3" }],
      ["path", { d: "M18 5v13", key: "123xd1" }],
      ["path", { d: "M22 10v3", key: "154ddg" }],
    ],
  ],
  [
    "blocks",
    [],
    [
      [
        "path",
        {
          d: "M10 22V7a1 1 0 0 0-1-1H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-5a1 1 0 0 0-1-1H2",
          key: "1ah6g2",
        },
      ],
      [
        "rect",
        { x: "14", y: "2", width: "8", height: "8", rx: "1", key: "88lufb" },
      ],
    ],
  ],
  [
    "brain-circuit",
    [],
    [
      [
        "path",
        {
          d: "M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-2.526 5.77 4 4 0 0 0 .556 6.588A4 4 0 1 0 12 18Z",
          key: "l5xja",
        },
      ],
      ["path", { d: "M9 13a4.5 4.5 0 0 0 3-4", key: "10igwf" }],
      ["path", { d: "M6.003 5.125A3 3 0 0 0 6.401 6.5", key: "105sqy" }],
      ["path", { d: "M3.477 10.896a4 4 0 0 1 .585-.396", key: "ql3yin" }],
      ["path", { d: "M6 18a4 4 0 0 1-1.967-.516", key: "2e4loj" }],
      ["path", { d: "M12 13h4", key: "1ku699" }],
      ["path", { d: "M12 18h6a2 2 0 0 1 2 2v1", key: "105ag5" }],
      ["path", { d: "M12 8h8", key: "1lhi5i" }],
      ["path", { d: "M16 8V5a2 2 0 0 1 2-2", key: "u6izg6" }],
      ["circle", { cx: "16", cy: "13", r: ".5", key: "ry7gng" }],
      ["circle", { cx: "18", cy: "3", r: ".5", key: "1aiba7" }],
      ["circle", { cx: "20", cy: "21", r: ".5", key: "yhc1fs" }],
      ["circle", { cx: "20", cy: "8", r: ".5", key: "1e43v0" }],
    ],
  ],
  [
    "chart-column",
    ["bar-chart-3"],
    [
      ["path", { d: "M3 3v16a2 2 0 0 0 2 2h16", key: "c24i48" }],
      ["path", { d: "M18 17V9", key: "2bz60n" }],
      ["path", { d: "M13 17V5", key: "1frdt8" }],
      ["path", { d: "M8 17v-3", key: "17ska0" }],
    ],
  ],
  ["check", [], [["path", { d: "M20 6 9 17l-5-5", key: "1gmf2c" }]]],
  ["chevron-down", [], [["path", { d: "m6 9 6 6 6-6", key: "qrunsl" }]]],
  ["chevron-up", [], [["path", { d: "m18 15-6-6-6 6", key: "153udz" }]]],
  [
    "circle-alert",
    ["alert-circle"],
    [
      ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
      ["line", { x1: "12", x2: "12", y1: "8", y2: "12", key: "1pkeuh" }],
      ["line", { x1: "12", x2: "12.01", y1: "16", y2: "16", key: "4dfq90" }],
    ],
  ],
  [
    "circle-check-big",
    ["check-circle"],
    [
      ["path", { d: "M21.801 10A10 10 0 1 1 17 3.335", key: "yps3ct" }],
      ["path", { d: "m9 11 3 3L22 4", key: "1pflzl" }],
    ],
  ],
  [
    "circle-question-mark",
    ["help-circle", "circle-help"],
    [
      ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
      ["path", { d: "M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3", key: "1u773s" }],
      ["path", { d: "M12 17h.01", key: "p32p05" }],
    ],
  ],
  [
    "cog",
    [],
    [
      ["path", { d: "M11 10.27 7 3.34", key: "16pf9h" }],
      ["path", { d: "m11 13.73-4 6.93", key: "794ttg" }],
      ["path", { d: "M12 22v-2", key: "1osdcq" }],
      ["path", { d: "M12 2v2", key: "tus03m" }],
      ["path", { d: "M14 12h8", key: "4f43i9" }],
      ["path", { d: "m17 20.66-1-1.73", key: "eq3orb" }],
      ["path", { d: "m17 3.34-1 1.73", key: "2wel8s" }],
      ["path", { d: "M2 12h2", key: "1t8f8n" }],
      ["path", { d: "m20.66 17-1.73-1", key: "sg0v6f" }],
      ["path", { d: "m20.66 7-1.73 1", key: "1ow05n" }],
      ["path", { d: "m3.34 17 1.73-1", key: "nuk764" }],
      ["path", { d: "m3.34 7 1.73 1", key: "1ulond" }],
      ["circle", { cx: "12", cy: "12", r: "2", key: "1c9p78" }],
      ["circle", { cx: "12", cy: "12", r: "8", key: "46899m" }],
    ],
  ],
  [
    "copy",
    [],
    [
      [
        "rect",
        {
          width: "14",
          height: "14",
          x: "8",
          y: "8",
          rx: "2",
          ry: "2",
          key: "17jyea",
        },
      ],
      [
        "path",
        {
          d: "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2",
          key: "zix9uf",
        },
      ],
    ],
  ],
  [
    "cpu",
    [],
    [
      ["path", { d: "M12 20v2", key: "1lh1kg" }],
      ["path", { d: "M12 2v2", key: "tus03m" }],
      ["path", { d: "M17 20v2", key: "1rnc9c" }],
      ["path", { d: "M17 2v2", key: "11trls" }],
      ["path", { d: "M2 12h2", key: "1t8f8n" }],
      ["path", { d: "M2 17h2", key: "7oei6x" }],
      ["path", { d: "M2 7h2", key: "asdhe0" }],
      ["path", { d: "M20 12h2", key: "1q8mjw" }],
      ["path", { d: "M20 17h2", key: "1fpfkl" }],
      ["path", { d: "M20 7h2", key: "1o8tra" }],
      ["path", { d: "M7 20v2", key: "4gnj0m" }],
      ["path", { d: "M7 2v2", key: "1i4yhu" }],
      [
        "rect",
        { x: "4", y: "4", width: "16", height: "16", rx: "2", key: "1vbyd7" },
      ],
      [
        "rect",
        { x: "8", y: "8", width: "8", height: "8", rx: "1", key: "z9xiuo" },
      ],
    ],
  ],
  [
    "download",
    [],
    [
      ["path", { d: "M12 15V3", key: "m9g1x1" }],
      [
        "path",
        { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", key: "ih7n3h" },
      ],
      ["path", { d: "m7 10 5 5 5-5", key: "brsn70" }],
    ],
  ],
  [
    "external-link",
    [],
    [
      ["path", { d: "M15 3h6v6", key: "1q9fwt" }],
      ["path", { d: "M10 14 21 3", key: "gplh6r" }],
      [
        "path",
        {
          d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6",
          key: "a6xqqp",
        },
      ],
    ],
  ],
  [
    "file-headphone",
    ["file-audio", "file-audio-2"],
    [
      [
        "path",
        {
          d: "M4 6.835V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.706.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2h-.343",
          key: "1vfytu",
        },
      ],
      ["path", { d: "M14 2v5a1 1 0 0 0 1 1h5", key: "wfsgrz" }],
      [
        "path",
        {
          d: "M2 19a2 2 0 0 1 4 0v1a2 2 0 0 1-4 0v-4a6 6 0 0 1 12 0v4a2 2 0 0 1-4 0v-1a2 2 0 0 1 4 0",
          key: "1etmh7",
        },
      ],
    ],
  ],
  [
    "file-plus",
    [],
    [
      [
        "path",
        {
          d: "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z",
          key: "1oefj6",
        },
      ],
      ["path", { d: "M14 2v5a1 1 0 0 0 1 1h5", key: "wfsgrz" }],
      ["path", { d: "M9 15h6", key: "cctwl0" }],
      ["path", { d: "M12 18v-6", key: "17g6i2" }],
    ],
  ],
  [
    "file-text",
    [],
    [
      [
        "path",
        {
          d: "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z",
          key: "1oefj6",
        },
      ],
      ["path", { d: "M14 2v5a1 1 0 0 0 1 1h5", key: "wfsgrz" }],
      ["path", { d: "M10 9H8", key: "b1mrlr" }],
      ["path", { d: "M16 13H8", key: "t4e002" }],
      ["path", { d: "M16 17H8", key: "z1uh3a" }],
    ],
  ],
  [
    "flame",
    [],
    [
      [
        "path",
        {
          d: "M12 3q1 4 4 6.5t3 5.5a1 1 0 0 1-14 0 5 5 0 0 1 1-3 1 1 0 0 0 5 0c0-2-1.5-3-1.5-5q0-2 2.5-4",
          key: "1slcih",
        },
      ],
    ],
  ],
  [
    "flask-conical",
    [],
    [
      [
        "path",
        {
          d: "M14 2v6a2 2 0 0 0 .245.96l5.51 10.08A2 2 0 0 1 18 22H6a2 2 0 0 1-1.755-2.96l5.51-10.08A2 2 0 0 0 10 8V2",
          key: "18mbvz",
        },
      ],
      ["path", { d: "M6.453 15h11.094", key: "3shlmq" }],
      ["path", { d: "M8.5 2h7", key: "csnxdl" }],
    ],
  ],
  [
    "folder-open",
    [],
    [
      [
        "path",
        {
          d: "m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2",
          key: "usdka0",
        },
      ],
    ],
  ],
  [
    "gauge",
    [],
    [
      ["path", { d: "m12 14 4-4", key: "9kzdfg" }],
      ["path", { d: "M3.34 19a10 10 0 1 1 17.32 0", key: "19p75a" }],
    ],
  ],
  [
    "globe",
    [],
    [
      ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
      [
        "path",
        { d: "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20", key: "13o1zl" },
      ],
      ["path", { d: "M2 12h20", key: "9i4pu4" }],
    ],
  ],
  [
    "hard-drive",
    [],
    [
      ["path", { d: "M10 16h.01", key: "1bzywj" }],
      [
        "path",
        {
          d: "M2.212 11.577a2 2 0 0 0-.212.896V18a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-5.527a2 2 0 0 0-.212-.896L18.55 5.11A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
          key: "18tbho",
        },
      ],
      ["path", { d: "M21.946 12.013H2.054", key: "zqlbp7" }],
      ["path", { d: "M6 16h.01", key: "1pmjb7" }],
    ],
  ],
  [
    "info",
    [],
    [
      ["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
      ["path", { d: "M12 16v-4", key: "1dtifu" }],
      ["path", { d: "M12 8h.01", key: "e9boi3" }],
    ],
  ],
  [
    "keyboard",
    [],
    [
      ["path", { d: "M10 8h.01", key: "1r9ogq" }],
      ["path", { d: "M12 12h.01", key: "1mp3jc" }],
      ["path", { d: "M14 8h.01", key: "1primd" }],
      ["path", { d: "M16 12h.01", key: "1l6xoz" }],
      ["path", { d: "M18 8h.01", key: "emo2bl" }],
      ["path", { d: "M6 8h.01", key: "x9i8wu" }],
      ["path", { d: "M7 16h10", key: "wp8him" }],
      ["path", { d: "M8 12h.01", key: "czm47f" }],
      [
        "rect",
        { width: "20", height: "16", x: "2", y: "4", rx: "2", key: "18n3k1" },
      ],
    ],
  ],
  [
    "languages",
    [],
    [
      ["path", { d: "m5 8 6 6", key: "1wu5hv" }],
      ["path", { d: "m4 14 6-6 2-3", key: "1k1g8d" }],
      ["path", { d: "M2 5h12", key: "or177f" }],
      ["path", { d: "M7 2h1", key: "1t2jsx" }],
      ["path", { d: "m22 22-5-10-5 10", key: "don7ne" }],
      ["path", { d: "M14 18h6", key: "1m8k6r" }],
    ],
  ],
  [
    "layers",
    ["layers-3"],
    [
      [
        "path",
        {
          d: "M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z",
          key: "zw3jo",
        },
      ],
      [
        "path",
        {
          d: "M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12",
          key: "1wduqc",
        },
      ],
      [
        "path",
        {
          d: "M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17",
          key: "kqbvx6",
        },
      ],
    ],
  ],
  [
    "loader-circle",
    ["loader-2"],
    [["path", { d: "M21 12a9 9 0 1 1-6.219-8.56", key: "13zald" }]],
  ],
  [
    "mic",
    [],
    [
      ["path", { d: "M12 19v3", key: "npa21l" }],
      ["path", { d: "M19 10v2a7 7 0 0 1-14 0v-2", key: "1vc78b" }],
      [
        "rect",
        { x: "9", y: "2", width: "6", height: "13", rx: "3", key: "s6n7sd" },
      ],
    ],
  ],
  [
    "mic-vocal",
    ["mic-2"],
    [
      [
        "path",
        {
          d: "m11 7.601-5.994 8.19a1 1 0 0 0 .1 1.298l.817.818a1 1 0 0 0 1.314.087L15.09 12",
          key: "80a601",
        },
      ],
      [
        "path",
        {
          d: "M16.5 21.174C15.5 20.5 14.372 20 13 20c-2.058 0-3.928 2.356-6 2-2.072-.356-2.775-3.369-1.5-4.5",
          key: "j0ngtp",
        },
      ],
      ["circle", { cx: "16", cy: "7", r: "5", key: "d08jfb" }],
    ],
  ],
  [
    "panel-left-close",
    ["sidebar-close"],
    [
      [
        "rect",
        { width: "18", height: "18", x: "3", y: "3", rx: "2", key: "afitv7" },
      ],
      ["path", { d: "M9 3v18", key: "fh3hqa" }],
      ["path", { d: "m16 15-3-3 3-3", key: "14y99z" }],
    ],
  ],
  [
    "panel-left-open",
    ["sidebar-open"],
    [
      [
        "rect",
        { width: "18", height: "18", x: "3", y: "3", rx: "2", key: "afitv7" },
      ],
      ["path", { d: "M9 3v18", key: "fh3hqa" }],
      ["path", { d: "m14 9 3 3-3 3", key: "8010ee" }],
    ],
  ],
  [
    "pause",
    [],
    [
      [
        "rect",
        { x: "14", y: "3", width: "5", height: "18", rx: "1", key: "kaeet6" },
      ],
      [
        "rect",
        { x: "5", y: "3", width: "5", height: "18", rx: "1", key: "1wsw3u" },
      ],
    ],
  ],
  [
    "picture-in-picture-2",
    [],
    [
      [
        "path",
        {
          d: "M21 9V6a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v10c0 1.1.9 2 2 2h4",
          key: "daa4of",
        },
      ],
      [
        "rect",
        { width: "10", height: "7", x: "12", y: "13", rx: "2", key: "1nb8gs" },
      ],
    ],
  ],
  [
    "pipette",
    [],
    [
      [
        "path",
        {
          d: "m12 9-8.414 8.414A2 2 0 0 0 3 18.828v1.344a2 2 0 0 1-.586 1.414A2 2 0 0 1 3.828 21h1.344a2 2 0 0 0 1.414-.586L15 12",
          key: "1y3wsu",
        },
      ],
      [
        "path",
        {
          d: "m18 9 .4.4a1 1 0 1 1-3 3l-3.8-3.8a1 1 0 1 1 3-3l.4.4 3.4-3.4a1 1 0 1 1 3 3z",
          key: "110lr1",
        },
      ],
      ["path", { d: "m2 22 .414-.414", key: "jhxm08" }],
    ],
  ],
  [
    "play",
    [],
    [
      [
        "path",
        {
          d: "M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z",
          key: "10ikf1",
        },
      ],
    ],
  ],
  [
    "radio",
    [],
    [
      ["path", { d: "M16.247 7.761a6 6 0 0 1 0 8.478", key: "1fwjs5" }],
      ["path", { d: "M19.075 4.933a10 10 0 0 1 0 14.134", key: "ehdyv1" }],
      ["path", { d: "M4.925 19.067a10 10 0 0 1 0-14.134", key: "1q22gi" }],
      ["path", { d: "M7.753 16.239a6 6 0 0 1 0-8.478", key: "r2q7qm" }],
      ["circle", { cx: "12", cy: "12", r: "2", key: "1c9p78" }],
    ],
  ],
  [
    "refresh-ccw",
    [],
    [
      [
        "path",
        {
          d: "M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8",
          key: "14sxne",
        },
      ],
      ["path", { d: "M3 3v5h5", key: "1xhq8a" }],
      [
        "path",
        {
          d: "M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16",
          key: "1hlbsb",
        },
      ],
      ["path", { d: "M16 16h5v5", key: "ccwih5" }],
    ],
  ],
  [
    "refresh-cw",
    [],
    [
      [
        "path",
        {
          d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8",
          key: "v9h5vc",
        },
      ],
      ["path", { d: "M21 3v5h-5", key: "1q7to0" }],
      [
        "path",
        {
          d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16",
          key: "3uifl3",
        },
      ],
      ["path", { d: "M8 16H3v5", key: "1cv678" }],
    ],
  ],
  [
    "rotate-ccw",
    [],
    [
      [
        "path",
        {
          d: "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8",
          key: "1357e3",
        },
      ],
      ["path", { d: "M3 3v5h5", key: "1xhq8a" }],
    ],
  ],
  [
    "rotate-ccw-clock",
    ["history"],
    [
      [
        "path",
        {
          d: "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8",
          key: "1357e3",
        },
      ],
      ["path", { d: "M3 3v5h5", key: "1xhq8a" }],
      ["path", { d: "M12 7v5l4 2", key: "1fdv2h" }],
    ],
  ],
  [
    "rotate-cw",
    [],
    [
      [
        "path",
        {
          d: "M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8",
          key: "1p45f6",
        },
      ],
      ["path", { d: "M21 3v5h-5", key: "1q7to0" }],
    ],
  ],
  [
    "search",
    [],
    [
      ["path", { d: "m21 21-4.34-4.34", key: "14j7rj" }],
      ["circle", { cx: "11", cy: "11", r: "8", key: "4ej97u" }],
    ],
  ],
  [
    "settings-2",
    [],
    [
      ["path", { d: "M14 17H5", key: "gfn3mx" }],
      ["path", { d: "M19 7h-9", key: "6i9tg" }],
      ["circle", { cx: "17", cy: "17", r: "3", key: "18b49y" }],
      ["circle", { cx: "7", cy: "7", r: "3", key: "dfmy0x" }],
    ],
  ],
  [
    "sparkles",
    ["stars"],
    [
      [
        "path",
        {
          d: "M11.017 2.814a1 1 0 0 1 1.966 0l1.051 5.558a2 2 0 0 0 1.594 1.594l5.558 1.051a1 1 0 0 1 0 1.966l-5.558 1.051a2 2 0 0 0-1.594 1.594l-1.051 5.558a1 1 0 0 1-1.966 0l-1.051-5.558a2 2 0 0 0-1.594-1.594l-5.558-1.051a1 1 0 0 1 0-1.966l5.558-1.051a2 2 0 0 0 1.594-1.594z",
          key: "1s2grr",
        },
      ],
      ["path", { d: "M20 2v4", key: "1rf3ol" }],
      ["path", { d: "M22 4h-4", key: "gwowj6" }],
      ["circle", { cx: "4", cy: "20", r: "2", key: "6kqj1y" }],
    ],
  ],
  [
    "square",
    [],
    [
      [
        "rect",
        { width: "18", height: "18", x: "3", y: "3", rx: "2", key: "afitv7" },
      ],
    ],
  ],
  [
    "star",
    [],
    [
      [
        "path",
        {
          d: "M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z",
          key: "r04s7s",
        },
      ],
    ],
  ],
  [
    "trash",
    ["trash-2"],
    [
      ["path", { d: "M10 11v6", key: "nco0om" }],
      ["path", { d: "M14 11v6", key: "outv1u" }],
      [
        "path",
        { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6", key: "miytrc" },
      ],
      ["path", { d: "M3 6h18", key: "d0wm0j" }],
      ["path", { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2", key: "e791ji" }],
    ],
  ],
  [
    "triangle-alert",
    ["alert-triangle"],
    [
      [
        "path",
        {
          d: "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3",
          key: "wmoenq",
        },
      ],
      ["path", { d: "M12 9v4", key: "juzpu7" }],
      ["path", { d: "M12 17h.01", key: "p32p05" }],
    ],
  ],
  [
    "type",
    [],
    [
      ["path", { d: "M12 4v16", key: "1654pz" }],
      ["path", { d: "M4 7V5a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v2", key: "e0r10z" }],
      ["path", { d: "M9 20h6", key: "s66wpe" }],
    ],
  ],
  [
    "upload",
    [],
    [
      ["path", { d: "M12 3v12", key: "1x0j5s" }],
      ["path", { d: "m17 8-5-5-5 5", key: "7q97r8" }],
      [
        "path",
        { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", key: "ih7n3h" },
      ],
    ],
  ],
  [
    "volume-2",
    [],
    [
      [
        "path",
        {
          d: "M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z",
          key: "uqj9uw",
        },
      ],
      ["path", { d: "M16 9a5 5 0 0 1 0 6", key: "1q6k2b" }],
      ["path", { d: "M19.364 18.364a9 9 0 0 0 0-12.728", key: "ijwkga" }],
    ],
  ],
  [
    "wand-sparkles",
    ["wand-2"],
    [
      [
        "path",
        {
          d: "m21.64 3.64-1.28-1.28a1.21 1.21 0 0 0-1.72 0L2.36 18.64a1.21 1.21 0 0 0 0 1.72l1.28 1.28a1.2 1.2 0 0 0 1.72 0L21.64 5.36a1.2 1.2 0 0 0 0-1.72",
          key: "ul74o6",
        },
      ],
      ["path", { d: "m14 7 3 3", key: "1r5n42" }],
      ["path", { d: "M5 6v4", key: "ilb8ba" }],
      ["path", { d: "M19 14v4", key: "blhpug" }],
      ["path", { d: "M10 2v2", key: "7u0qdc" }],
      ["path", { d: "M7 8H3", key: "zfb6yr" }],
      ["path", { d: "M21 16h-4", key: "1cnmox" }],
      ["path", { d: "M11 3H9", key: "1obp7u" }],
    ],
  ],
  [
    "wrench",
    [],
    [
      [
        "path",
        {
          d: "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.106-3.105c.32-.322.863-.22.983.218a6 6 0 0 1-8.259 7.057l-7.91 7.91a1 1 0 0 1-2.999-3l7.91-7.91a6 6 0 0 1 7.057-8.259c.438.12.54.662.219.984z",
          key: "1ngwbx",
        },
      ],
    ],
  ],
  [
    "x",
    [],
    [
      ["path", { d: "M18 6 6 18", key: "1bl5f8" }],
      ["path", { d: "m6 6 12 12", key: "d8bk6v" }],
    ],
  ],
  [
    "zap",
    [],
    [
      [
        "path",
        {
          d: "M15.914 4a1.5 1.5 0 00-2.474-1.561l-9 9A1.5 1.5 0 005.5 14h4.002a.5.5 0 01.471.666L8.086 20a1.5 1.5 0 002.475 1.56l9-9A1.5 1.5 0 0018.5 10h-3.997a.5.5 0 01-.472-.667z",
          key: "1v7up4",
        },
      ],
    ],
  ],
];

const built = new Map<string, LucideIcon>();
for (const [name, aliases, node] of ICONS)
  built.set(name, createIcon(name, aliases, node));

/** The name a call site would import from lucide-react -> the glyph it resolves to. */
const ALIASES: Record<string, string> = {
  Activity: "activity",
  AudioLines: "audio-lines",
  Blocks: "blocks",
  BrainCircuit: "brain-circuit",
  BarChart3: "chart-column",
  Check: "check",
  ChevronDown: "chevron-down",
  ChevronUp: "chevron-up",
  AlertCircle: "circle-alert",
  CheckCircle: "circle-check-big",
  CircleHelp: "circle-question-mark",
  Cog: "cog",
  Copy: "copy",
  Cpu: "cpu",
  Download: "download",
  ExternalLink: "external-link",
  FileAudio: "file-headphone",
  FilePlus: "file-plus",
  FileText: "file-text",
  Flame: "flame",
  FlaskConical: "flask-conical",
  FolderOpen: "folder-open",
  Gauge: "gauge",
  Globe: "globe",
  HardDrive: "hard-drive",
  Info: "info",
  Keyboard: "keyboard",
  Languages: "languages",
  Layers: "layers",
  Loader2: "loader-circle",
  LoaderCircle: "loader-circle",
  Mic: "mic",
  Mic2: "mic-vocal",
  PanelLeftClose: "panel-left-close",
  PanelLeftOpen: "panel-left-open",
  Pause: "pause",
  PictureInPicture2: "picture-in-picture-2",
  Pipette: "pipette",
  Play: "play",
  PlayIcon: "play",
  Radio: "radio",
  RefreshCcw: "refresh-ccw",
  RefreshCw: "refresh-cw",
  RotateCcw: "rotate-ccw",
  History: "rotate-ccw-clock",
  RotateCw: "rotate-cw",
  Search: "search",
  Settings2: "settings-2",
  Sparkles: "sparkles",
  Square: "square",
  Star: "star",
  Trash2: "trash",
  AlertTriangle: "triangle-alert",
  TriangleAlert: "triangle-alert",
  Type: "type",
  Upload: "upload",
  Volume2: "volume-2",
  WandSparkles: "wand-sparkles",
  Wrench: "wrench",
  X: "x",
  Zap: "zap",
};

/**
 * Builds each exported component from its glyph. Written as a loop rather than
 * 61 lines of `export const X = createIcon(...)` so the alias table above
 * stays the single place a name is bound.
 */
function icon(name: string): LucideIcon {
  const i = built.get(ALIASES[name]);
  if (!i) throw new Error("No vendored lucide glyph for " + name);
  return i;
}

export const Activity: LucideIcon = /* @__PURE__ */ icon("Activity");
export const AlertCircle: LucideIcon = /* @__PURE__ */ icon("AlertCircle");
export const AlertTriangle: LucideIcon = /* @__PURE__ */ icon("AlertTriangle");
export const AudioLines: LucideIcon = /* @__PURE__ */ icon("AudioLines");
export const BarChart3: LucideIcon = /* @__PURE__ */ icon("BarChart3");
export const Blocks: LucideIcon = /* @__PURE__ */ icon("Blocks");
export const BrainCircuit: LucideIcon = /* @__PURE__ */ icon("BrainCircuit");
export const Check: LucideIcon = /* @__PURE__ */ icon("Check");
export const CheckCircle: LucideIcon = /* @__PURE__ */ icon("CheckCircle");
export const ChevronDown: LucideIcon = /* @__PURE__ */ icon("ChevronDown");
export const ChevronUp: LucideIcon = /* @__PURE__ */ icon("ChevronUp");
export const CircleHelp: LucideIcon = /* @__PURE__ */ icon("CircleHelp");
export const Cog: LucideIcon = /* @__PURE__ */ icon("Cog");
export const Copy: LucideIcon = /* @__PURE__ */ icon("Copy");
export const Cpu: LucideIcon = /* @__PURE__ */ icon("Cpu");
export const Download: LucideIcon = /* @__PURE__ */ icon("Download");
export const ExternalLink: LucideIcon = /* @__PURE__ */ icon("ExternalLink");
export const FileAudio: LucideIcon = /* @__PURE__ */ icon("FileAudio");
export const FilePlus: LucideIcon = /* @__PURE__ */ icon("FilePlus");
export const FileText: LucideIcon = /* @__PURE__ */ icon("FileText");
export const Flame: LucideIcon = /* @__PURE__ */ icon("Flame");
export const FlaskConical: LucideIcon = /* @__PURE__ */ icon("FlaskConical");
export const FolderOpen: LucideIcon = /* @__PURE__ */ icon("FolderOpen");
export const Gauge: LucideIcon = /* @__PURE__ */ icon("Gauge");
export const Globe: LucideIcon = /* @__PURE__ */ icon("Globe");
export const HardDrive: LucideIcon = /* @__PURE__ */ icon("HardDrive");
export const History: LucideIcon = /* @__PURE__ */ icon("History");
export const Info: LucideIcon = /* @__PURE__ */ icon("Info");
export const Keyboard: LucideIcon = /* @__PURE__ */ icon("Keyboard");
export const Languages: LucideIcon = /* @__PURE__ */ icon("Languages");
export const Layers: LucideIcon = /* @__PURE__ */ icon("Layers");
export const Loader2: LucideIcon = /* @__PURE__ */ icon("Loader2");
export const LoaderCircle: LucideIcon = /* @__PURE__ */ icon("LoaderCircle");
export const Mic: LucideIcon = /* @__PURE__ */ icon("Mic");
export const Mic2: LucideIcon = /* @__PURE__ */ icon("Mic2");
export const PanelLeftClose: LucideIcon =
  /* @__PURE__ */ icon("PanelLeftClose");
export const PanelLeftOpen: LucideIcon = /* @__PURE__ */ icon("PanelLeftOpen");
export const Pause: LucideIcon = /* @__PURE__ */ icon("Pause");
export const PictureInPicture2: LucideIcon =
  /* @__PURE__ */ icon("PictureInPicture2");
export const Pipette: LucideIcon = /* @__PURE__ */ icon("Pipette");
export const Play: LucideIcon = /* @__PURE__ */ icon("Play");
export const PlayIcon: LucideIcon = /* @__PURE__ */ icon("PlayIcon");
export const Radio: LucideIcon = /* @__PURE__ */ icon("Radio");
export const RefreshCcw: LucideIcon = /* @__PURE__ */ icon("RefreshCcw");
export const RefreshCw: LucideIcon = /* @__PURE__ */ icon("RefreshCw");
export const RotateCcw: LucideIcon = /* @__PURE__ */ icon("RotateCcw");
export const RotateCw: LucideIcon = /* @__PURE__ */ icon("RotateCw");
export const Search: LucideIcon = /* @__PURE__ */ icon("Search");
export const Settings2: LucideIcon = /* @__PURE__ */ icon("Settings2");
export const Sparkles: LucideIcon = /* @__PURE__ */ icon("Sparkles");
export const Square: LucideIcon = /* @__PURE__ */ icon("Square");
export const Star: LucideIcon = /* @__PURE__ */ icon("Star");
export const Trash2: LucideIcon = /* @__PURE__ */ icon("Trash2");
export const TriangleAlert: LucideIcon = /* @__PURE__ */ icon("TriangleAlert");
export const Type: LucideIcon = /* @__PURE__ */ icon("Type");
export const Upload: LucideIcon = /* @__PURE__ */ icon("Upload");
export const Volume2: LucideIcon = /* @__PURE__ */ icon("Volume2");
export const WandSparkles: LucideIcon = /* @__PURE__ */ icon("WandSparkles");
export const Wrench: LucideIcon = /* @__PURE__ */ icon("Wrench");
export const X: LucideIcon = /* @__PURE__ */ icon("X");
export const Zap: LucideIcon = /* @__PURE__ */ icon("Zap");
