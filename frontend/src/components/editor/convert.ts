import DOMPurify from 'dompurify'
import { marked } from 'marked'
import TurndownService from 'turndown'

/** Sanitise HTML before it is rendered into the DOM or stored from a preview. */
export function sanitizeHtml(html: string): string {
  return DOMPurify.sanitize(html, { USE_PROFILES: { html: true } })
}

const turndown = new TurndownService({
  headingStyle: 'atx',
  hr: '---',
  bulletListMarker: '-',
  codeBlockStyle: 'fenced',
  emDelimiter: '*',
  strongDelimiter: '**',
  linkStyle: 'inlined',
})

// TipTap wraps list item content in <p>; turndown's default rule would then emit
// blank lines between items ("loose" lists). Collapse them to keep lists tight.
turndown.addRule('tightListItemParagraph', {
  filter: (node) =>
    node.nodeName === 'P' &&
    node.parentNode?.nodeName === 'LI' &&
    node.parentNode.childNodes.length === 1,
  replacement: (content) => content,
})

// Use "- item" / "1. item" rather than turndown's default 3-space padding after the marker;
// continuation lines are indented to the content column so nested lists stay valid CommonMark.
turndown.addRule('listItem', {
  filter: 'li',
  replacement: (content, node) => {
    const parent = node.parentNode as HTMLElement | null
    let prefix = '- '
    if (parent?.nodeName === 'OL') {
      const start = Number(parent.getAttribute('start') ?? '1') || 1
      prefix = `${start + Array.prototype.indexOf.call(parent.children, node)}. `
    }
    const indent = ' '.repeat(prefix.length)
    const body = content
      .replace(/^\n+/, '')
      .replace(/\n+$/, '\n')
      .replace(/\n(?=.)/g, `\n${indent}`)
    return prefix + body + (node.nextSibling && !/\n$/.test(body) ? '\n' : '')
  },
})

// Strikethrough (TipTap's <s>) → GFM ~~text~~, which marked renders back to <del>.
turndown.addRule('strikethrough', {
  filter: ['del', 's'],
  replacement: (content) => `~~${content}~~`,
})

export function htmlToMarkdown(html: string): string {
  if (!html.trim() || html === '<p></p>') return ''
  return turndown.turndown(sanitizeHtml(html))
}

export function markdownToHtml(markdown: string): string {
  if (!markdown.trim()) return ''
  const raw = marked.parse(markdown, { async: false, gfm: true, breaks: false })
  return sanitizeHtml(raw)
}
