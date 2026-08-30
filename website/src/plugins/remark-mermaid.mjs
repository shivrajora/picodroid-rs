import { visit } from 'unist-util-visit';

/**
 * Turn ```mermaid fences into `<pre class="mermaid">` so the client-side
 * renderer in `MermaidScript.astro` can swap them for SVG.
 *
 * Without this the fence falls through to Expressive Code and ships as a
 * syntax-highlighted code block — the diagram source, verbatim, instead of a
 * diagram. Running at the remark stage means we replace the node before
 * Expressive Code ever sees it.
 *
 * The source is emitted HTML-escaped: mermaid reads it back with
 * `textContent`, so the browser undoes the escaping for us.
 */
const ESCAPES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' };

function escapeHtml(s) {
  return s.replace(/[&<>"]/g, (c) => ESCAPES[c]);
}

export function remarkMermaid() {
  return (tree) => {
    visit(tree, 'code', (node, index, parent) => {
      if (node.lang !== 'mermaid' || !parent || index === undefined) return;
      parent.children[index] = {
        type: 'html',
        value: `<pre class="mermaid" data-mermaid-source="${escapeHtml(
          node.value,
        )}">${escapeHtml(node.value)}</pre>`,
      };
    });
  };
}
