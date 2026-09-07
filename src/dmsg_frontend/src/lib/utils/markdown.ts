import markdownit from 'markdown-it'

// commonmark mode
export const md = markdownit({
  html: false,
  breaks: true,
  linkify: true,
  typographer: true
})

md.linkify.set({ fuzzyEmail: false })
