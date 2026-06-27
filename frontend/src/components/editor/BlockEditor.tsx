import { useEditor, EditorContent } from '@tiptap/react'
import StarterKit from '@tiptap/starter-kit'
import Placeholder from '@tiptap/extension-placeholder'
import { Bold, Italic, List, ListOrdered, Heading2, Code, Undo, Redo } from 'lucide-react'

interface Props {
  value: string
  onChange: (val: unknown) => void
}

export default function BlockEditor({ value, onChange }: Props) {
  const editor = useEditor({
    extensions: [
      StarterKit,
      Placeholder.configure({ placeholder: 'Start writing…' }),
    ],
    content: value || '',
    onUpdate: ({ editor }) => {
      onChange(editor.getHTML())
    },
  })

  if (!editor) return null

  const btn = (active: boolean) =>
    `rounded p-1.5 text-gray-500 hover:bg-gray-100 ${active ? 'bg-gray-200 text-gray-900' : ''}`

  return (
    <div className="rounded-md border border-gray-300 focus-within:border-brand-500 focus-within:ring-1 focus-within:ring-brand-500">
      <div className="flex flex-wrap gap-0.5 border-b border-gray-200 px-2 py-1.5">
        <button type="button" onClick={() => editor.chain().focus().toggleBold().run()} className={btn(editor.isActive('bold'))}>
          <Bold size={14} />
        </button>
        <button type="button" onClick={() => editor.chain().focus().toggleItalic().run()} className={btn(editor.isActive('italic'))}>
          <Italic size={14} />
        </button>
        <button type="button" onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()} className={btn(editor.isActive('heading', { level: 2 }))}>
          <Heading2 size={14} />
        </button>
        <button type="button" onClick={() => editor.chain().focus().toggleBulletList().run()} className={btn(editor.isActive('bulletList'))}>
          <List size={14} />
        </button>
        <button type="button" onClick={() => editor.chain().focus().toggleOrderedList().run()} className={btn(editor.isActive('orderedList'))}>
          <ListOrdered size={14} />
        </button>
        <button type="button" onClick={() => editor.chain().focus().toggleCode().run()} className={btn(editor.isActive('code'))}>
          <Code size={14} />
        </button>
        <div className="ml-auto flex gap-0.5">
          <button type="button" onClick={() => editor.chain().focus().undo().run()} className={btn(false)} disabled={!editor.can().undo()}>
            <Undo size={14} />
          </button>
          <button type="button" onClick={() => editor.chain().focus().redo().run()} className={btn(false)} disabled={!editor.can().redo()}>
            <Redo size={14} />
          </button>
        </div>
      </div>
      <EditorContent editor={editor} className="prose prose-sm max-w-none px-4 py-3" />
    </div>
  )
}
