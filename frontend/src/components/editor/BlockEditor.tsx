import { useMemo, useState, type ReactNode } from 'react'
import { useEditor, EditorContent, type Editor } from '@tiptap/react'
import StarterKit from '@tiptap/starter-kit'
import { Placeholder } from '@tiptap/extensions'
import Image from '@tiptap/extension-image'
import {
  Bold,
  Italic,
  Strikethrough,
  List,
  ListOrdered,
  Heading1,
  Heading2,
  Heading3,
  Code,
  SquareCode,
  Quote,
  Minus,
  Link as LinkIcon,
  Unlink,
  Image as ImageIcon,
  Undo,
  Redo,
} from 'lucide-react'
import { htmlToMarkdown, markdownToHtml, sanitizeHtml } from './convert'

interface Props {
  value: string
  onChange: (val: unknown) => void
}

type Mode = 'visual' | 'markdown' | 'html'

const MODES: { id: Mode; label: string }[] = [
  { id: 'visual', label: 'Visual' },
  { id: 'markdown', label: 'Markdown' },
  { id: 'html', label: 'HTML' },
]

/** Accept absolute http(s)/mailto URLs and site-relative paths; reject javascript: and friends. */
function safeUrl(input: string, allowMailto: boolean): string | null {
  const url = input.trim()
  if (!url) return null
  if (url.startsWith('/') || url.startsWith('#')) return url
  const withScheme = /^[a-z][a-z0-9+.-]*:/i.test(url) ? url : `https://${url}`
  try {
    const parsed = new URL(withScheme)
    const ok = ['http:', 'https:', ...(allowMailto ? ['mailto:'] : [])]
    return ok.includes(parsed.protocol) ? withScheme : null
  } catch {
    return null
  }
}

export default function BlockEditor({ value, onChange }: Props) {
  const [mode, setMode] = useState<Mode>('visual')
  // Canonical HTML for the current content, kept in step with whichever mode is active.
  const [html, setHtml] = useState(value || '')
  const [markdown, setMarkdown] = useState('')

  const update = (next: string) => {
    setHtml(next)
    onChange(next)
  }

  const editor = useEditor({
    extensions: [
      // v3 StarterKit bundles Link, Underline and ListKeymap.
      StarterKit.configure({
        heading: { levels: [1, 2, 3] },
        underline: false,
        listKeymap: false,
        link: {
          openOnClick: false,
          autolink: true,
          defaultProtocol: 'https',
          protocols: ['http', 'https', 'mailto'],
        },
      }),
      Image.configure({ inline: false }),
      Placeholder.configure({ placeholder: 'Start writing…' }),
    ],
    content: value || '',
    // v3 no longer re-renders on every transaction; the toolbar's active/disabled state needs it.
    shouldRerenderOnTransaction: true,
    onUpdate: ({ editor }) => update(editor.getHTML()),
  })

  const switchMode = (next: Mode) => {
    if (next === mode || !editor) return
    if (next === 'markdown') setMarkdown(htmlToMarkdown(html))
    if (next === 'visual') editor.commands.setContent(html, { emitUpdate: false })
    setMode(next)
  }

  const preview = useMemo(
    () => (mode === 'markdown' ? markdownToHtml(markdown) : mode === 'html' ? sanitizeHtml(html) : ''),
    [mode, markdown, html],
  )

  if (!editor) return null

  return (
    <div className="rounded-md border border-gray-300 focus-within:border-brand-500 focus-within:ring-1 focus-within:ring-brand-500">
      <div className="flex items-center gap-1 border-b border-gray-200 bg-gray-50 px-2 pt-1.5" role="tablist">
        {MODES.map((m) => (
          <button
            key={m.id}
            type="button"
            role="tab"
            aria-selected={mode === m.id}
            onClick={() => switchMode(m.id)}
            className={`-mb-px rounded-t border px-3 py-1 text-xs font-medium ${
              mode === m.id
                ? 'border-gray-200 border-b-white bg-white text-gray-900'
                : 'border-transparent text-gray-500 hover:text-gray-800'
            }`}
          >
            {m.label}
          </button>
        ))}
      </div>

      <div className={mode === 'visual' ? '' : 'hidden'}>
        <Toolbar editor={editor} />
        <EditorContent editor={editor} className="rich-content max-w-none px-4 py-3" />
      </div>

      {mode === 'markdown' && (
        <SplitPane
          source={markdown}
          onSource={(md) => {
            setMarkdown(md)
            update(markdownToHtml(md))
          }}
          placeholder="Write Markdown…"
          previewHtml={preview}
        />
      )}

      {mode === 'html' && (
        <SplitPane source={html} onSource={update} placeholder="<p>Raw HTML…</p>" previewHtml={preview} />
      )}
    </div>
  )
}

function SplitPane({
  source,
  onSource,
  placeholder,
  previewHtml,
}: {
  source: string
  onSource: (val: string) => void
  placeholder: string
  previewHtml: string
}) {
  return (
    <div className="grid divide-y divide-gray-200 md:grid-cols-2 md:divide-x md:divide-y-0">
      <textarea
        value={source}
        onChange={(e) => onSource(e.target.value)}
        placeholder={placeholder}
        spellCheck={false}
        className="min-h-[260px] w-full resize-y border-0 bg-white px-4 py-3 font-mono text-sm text-gray-800 focus:outline-none focus:ring-0"
      />
      <div className="min-h-[260px] overflow-auto px-4 py-3">
        <div className="mb-2 text-[10px] font-semibold uppercase tracking-wide text-gray-400">Preview</div>
        {/* previewHtml is sanitised with DOMPurify in convert.ts */}
        <div className="rich-content" dangerouslySetInnerHTML={{ __html: previewHtml }} />
      </div>
    </div>
  )
}

function Toolbar({ editor }: { editor: Editor }) {
  const setLink = () => {
    const prev = (editor.getAttributes('link').href as string | undefined) ?? ''
    const input = window.prompt('Link URL (leave empty to remove)', prev)
    if (input === null) return
    if (!input.trim()) {
      editor.chain().focus().extendMarkRange('link').unsetLink().run()
      return
    }
    const href = safeUrl(input, true)
    if (!href) {
      window.alert('Only http(s), mailto and site-relative links are allowed.')
      return
    }
    editor.chain().focus().extendMarkRange('link').setLink({ href }).run()
  }

  const addImage = () => {
    const input = window.prompt('Image URL')
    if (!input) return
    const src = safeUrl(input, false)
    if (!src) {
      window.alert('Only http(s) and site-relative image URLs are allowed.')
      return
    }
    const alt = window.prompt('Alt text (optional)') ?? ''
    editor.chain().focus().setImage({ src, alt }).run()
  }

  const c = () => editor.chain().focus()

  return (
    <div className="flex flex-wrap items-center gap-0.5 border-b border-gray-200 px-2 py-1.5">
      <Btn title="Heading 1" active={editor.isActive('heading', { level: 1 })} onClick={() => c().toggleHeading({ level: 1 }).run()}>
        <Heading1 size={14} />
      </Btn>
      <Btn title="Heading 2" active={editor.isActive('heading', { level: 2 })} onClick={() => c().toggleHeading({ level: 2 }).run()}>
        <Heading2 size={14} />
      </Btn>
      <Btn title="Heading 3" active={editor.isActive('heading', { level: 3 })} onClick={() => c().toggleHeading({ level: 3 }).run()}>
        <Heading3 size={14} />
      </Btn>
      <Sep />
      <Btn title="Bold" active={editor.isActive('bold')} onClick={() => c().toggleBold().run()}>
        <Bold size={14} />
      </Btn>
      <Btn title="Italic" active={editor.isActive('italic')} onClick={() => c().toggleItalic().run()}>
        <Italic size={14} />
      </Btn>
      <Btn title="Strikethrough" active={editor.isActive('strike')} onClick={() => c().toggleStrike().run()}>
        <Strikethrough size={14} />
      </Btn>
      <Btn title="Inline code" active={editor.isActive('code')} onClick={() => c().toggleCode().run()}>
        <Code size={14} />
      </Btn>
      <Btn title="Link" active={editor.isActive('link')} onClick={setLink}>
        <LinkIcon size={14} />
      </Btn>
      {editor.isActive('link') && (
        <Btn title="Remove link" onClick={() => c().extendMarkRange('link').unsetLink().run()}>
          <Unlink size={14} />
        </Btn>
      )}
      <Sep />
      <Btn title="Bullet list" active={editor.isActive('bulletList')} onClick={() => c().toggleBulletList().run()}>
        <List size={14} />
      </Btn>
      <Btn title="Numbered list" active={editor.isActive('orderedList')} onClick={() => c().toggleOrderedList().run()}>
        <ListOrdered size={14} />
      </Btn>
      <Btn title="Blockquote" active={editor.isActive('blockquote')} onClick={() => c().toggleBlockquote().run()}>
        <Quote size={14} />
      </Btn>
      <Btn title="Code block" active={editor.isActive('codeBlock')} onClick={() => c().toggleCodeBlock().run()}>
        <SquareCode size={14} />
      </Btn>
      <Btn title="Horizontal rule" onClick={() => c().setHorizontalRule().run()}>
        <Minus size={14} />
      </Btn>
      <Btn title="Image" onClick={addImage}>
        <ImageIcon size={14} />
      </Btn>
      <div className="ml-auto flex gap-0.5">
        <Btn title="Undo" onClick={() => c().undo().run()} disabled={!editor.can().undo()}>
          <Undo size={14} />
        </Btn>
        <Btn title="Redo" onClick={() => c().redo().run()} disabled={!editor.can().redo()}>
          <Redo size={14} />
        </Btn>
      </div>
    </div>
  )
}

function Btn({
  title,
  active = false,
  disabled = false,
  onClick,
  children,
}: {
  title: string
  active?: boolean
  disabled?: boolean
  onClick: () => void
  children: ReactNode
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      aria-pressed={active}
      onClick={onClick}
      disabled={disabled}
      className={`rounded p-1.5 text-gray-500 hover:bg-gray-100 disabled:opacity-40 ${active ? 'bg-gray-200 text-gray-900' : ''}`}
    >
      {children}
    </button>
  )
}

function Sep() {
  return <span className="mx-1 h-4 w-px bg-gray-200" />
}
