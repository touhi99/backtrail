import { useMemo } from 'react'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

// Renders a note's markdown: frontmatter removed, [[wikilinks]] turned into search links, output sanitised.
export default function Markdown({ text }: { text: string }) {
  const html = useMemo(() => {
    const body = text.replace(/^---\n[\s\S]*?\n---\n/, '')
    const linked = body.replace(/\[\[([^\]|#]+)(?:[#|][^\]]*)?\]\]/g, (_, n) => `[${n.trim()}](/search?q=${encodeURIComponent(n.trim())})`)
    return DOMPurify.sanitize(marked.parse(linked, { gfm: true, async: false }) as string)
  }, [text])
  return <div className="md" dangerouslySetInnerHTML={{ __html: html }} />
}
