import type { ContentTypeField } from '../lib/api'
import BlockEditor from './editor/BlockEditor'

interface Props {
  field: ContentTypeField
  value: unknown
  onChange: (val: unknown) => void
}

export default function FieldEditor({ field, value, onChange }: Props) {
  const label = field.label || field.name

  const inputClass =
    'block w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500'

  const fieldType = field.type.toLowerCase()

  if (fieldType.includes('richtext') || fieldType.includes('rich_text')) {
    return (
      <div>
        <label className="mb-1 block text-sm font-medium text-gray-700">{label}</label>
        <BlockEditor
          value={typeof value === 'string' ? value : ''}
          onChange={onChange}
        />
      </div>
    )
  }

  if (fieldType.includes('bool')) {
    return (
      <label className="flex items-center gap-2 text-sm font-medium text-gray-700">
        <input
          type="checkbox"
          checked={Boolean(value)}
          onChange={(e) => onChange(e.target.checked)}
          className="h-4 w-4 rounded border-gray-300 text-brand-600 focus:ring-brand-500"
        />
        {label}
      </label>
    )
  }

  if (fieldType.includes('number')) {
    return (
      <div>
        <label className="mb-1 block text-sm font-medium text-gray-700">{label}</label>
        <input
          type="number"
          value={String(value ?? '')}
          onChange={(e) => onChange(Number(e.target.value))}
          className={inputClass}
        />
      </div>
    )
  }

  if (fieldType.includes('date')) {
    return (
      <div>
        <label className="mb-1 block text-sm font-medium text-gray-700">{label}</label>
        <input
          type="datetime-local"
          value={String(value ?? '')}
          onChange={(e) => onChange(e.target.value)}
          className={inputClass}
        />
      </div>
    )
  }

  // Default: text / slug / relation IDs / list (comma-separated)
  return (
    <div>
      <label className="mb-1 block text-sm font-medium text-gray-700">
        {label}
        {field.required && <span className="ml-1 text-red-500">*</span>}
      </label>
      <input
        type="text"
        value={String(value ?? '')}
        onChange={(e) => onChange(e.target.value)}
        className={inputClass}
        placeholder={fieldType.includes('slug') ? 'auto-generated from title' : ''}
      />
    </div>
  )
}
