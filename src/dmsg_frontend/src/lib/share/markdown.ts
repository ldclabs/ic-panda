import markdownit from 'markdown-it'

// A dedicated renderer: no raw HTML or automatic requests for embedded images.
const md = markdownit({ html: false, linkify: true, breaks: true })
md.renderer.rules['image'] = (tokens, index) =>
  md.utils.escapeHtml(tokens[index]!.content)
md.renderer.rules['link_open'] = (tokens, index, options, env, renderer) => {
  tokens[index]!.attrSet('target', '_blank')
  tokens[index]!.attrSet('rel', 'noopener noreferrer')
  tokens[index]!.attrSet('referrerpolicy', 'no-referrer')
  return renderer.renderToken(tokens, index, options)
}

export function renderMarkdown(text: string): string {
  return md.render(text)
}
