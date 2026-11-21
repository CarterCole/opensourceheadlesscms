import { useEffect, useState } from 'react'
import { ArrowDown, ArrowUp, Plus, X } from 'lucide-react'
import type { GlobalItem, GlobalKind } from '../lib/api'

/**
 * Editor for a global's value.
 *  - single: one item
 *  - list:   ordered items, reorderable, duplicates allowed
 *  - set:    unique items, no ordering (the server stores them sorted)
 *  - map:    ordered label → item entries, reorderable
 * Items are text, URLs, or JSON (edited as text and parsed).
 *
 * `onValidityChange` reports whether every JSON item currently parses, so the parent can
 * disable saving.
 */
interface Props {
  kind: GlobalKind
  item: GlobalItem
  value: unknown
  onChange: (value: unknown) => void
  onValidityChange?: (valid: boolean) => void
  maxItems?: number | null
  disabled?: boolean
}

const inputClass =
  'block w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500 disabled:bg-gray-50 disabled:text-gray-500'
const iconBtn =
  'rounded p-1.5 text-gray-400 hover:bg-gray-100 hover:text-gray-700 disabled:opacity-30 disabled:hover:bg-transparent'

function emptyItem(item: GlobalItem): unknown {
  return item === 'json' ? {} : ''
}

/** One item: a text/url input, or a JSON textarea with its own draft text. */
function ItemInput({
  item,
  value,
  onChange,
  onValidityChange,
  disabled,
  placeholder,
  autoFocus,
}: {
  item: GlobalItem
  value: unknown
  onChange: (v: unknown) => void
  onValidityChange: (valid: boolean) => void
  disabled?: boolean
  placeholder?: string
  autoFocus?: boolean
}) {
  const [draft, setDraft] = useState(() => JSON.stringify(value ?? {}, null, 2))
  const [error, setError] = useState<string | null>(null)

  // Pick up external changes (e.g. after a save normalises the value).
  useEffect(() => {
    if (item !== 'json') return
    try {
      if (JSON.stringify(JSON.parse(draft)) === JSON.stringify(value)) return
    } catch {
      /* draft is mid-edit and invalid; still replace it with the new value */
    }
    setDraft(JSON.stringify(value ?? {}, null, 2))
    setError(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value, item])

  if (item === 'json') {
    return (
      <div className="w-full">
        <textarea
          value={draft}
          autoFocus={autoFocus}
          disabled={disabled}
          rows={Math.min(12, Math.max(3, draft.split('\n').length))}
          spellCheck={false}
          onChange={(e) => {
            setDraft(e.target.value)
            try {
              const parsed = JSON.parse(e.target.value)
              if (parsed === null) throw new Error('null is not allowed')
              setError(null)
              onValidityChange(true)
              onChange(parsed)
            } catch (err) {
              setError(err instanceof Error ? err.message : 'invalid JSON')
              onValidityChange(false)
            }
          }}
          className={`${inputClass} font-mono text-xs ${error ? 'border-red-400' : ''}`}
        />
        {error && <p className="mt-1 text-xs text-red-600">{error}</p>}
      </div>
    )
  }
  return (
    <input
      type={item === 'url' ? 'url' : 'text'}
      autoFocus={autoFocus}
      value={typeof value === 'string' ? value : ''}
      disabled={disabled}
      placeholder={placeholder ?? (item === 'url' ? 'https://… or /path' : '')}
      onChange={(e) => onChange(e.target.value)}
      className={inputClass}
    />
  )
}

export default function GlobalValueEditor({
  kind,
  item,
  value,
  onChange,
  onValidityChange,
  maxItems,
  disabled,
}: Props) {
  // Track validity per row so one bad JSON item disables saving until fixed.
  const [invalid, setInvalid] = useState<Set<number>>(new Set())
  const markValid = (i: number) => (valid: boolean) =>
    setInvalid((prev) => {
      const next = new Set(prev)
      if (valid) next.delete(i)
      else next.add(i)
      return next
    })
  // Map rows live here rather than being derived from `value`: while editing, a row can
  // have an empty or duplicate label, which a JSON object can't represent.
  const [rows, setRows] = useState<[string, unknown][]>(() => toEntries(value))
  useEffect(() => {
    if (kind !== 'map') return
    if (JSON.stringify(Object.fromEntries(rows)) !== JSON.stringify(value)) setRows(toEntries(value))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value, kind])
  const keys = rows.map(([k]) => k.trim())
  const badKeys = kind === 'map' && keys.some((k, i) => !k || keys.indexOf(k) !== i)

  useEffect(
    () => onValidityChange?.(invalid.size === 0 && !badKeys),
    [invalid, badKeys, onValidityChange],
  )
  const resetValidity = () => setInvalid(new Set())
  // Row just added with the Add button; it gets keyboard focus.
  const [focusRow, setFocusRow] = useState<number | null>(null)

  if (kind === 'single') {
    return (
      <ItemInput
        item={item}
        value={value}
        onChange={onChange}
        onValidityChange={markValid(0)}
        disabled={disabled}
      />
    )
  }

  const full = maxItems != null && (kind === 'map' ? rows.length : rowCount(kind, value)) >= maxItems

  if (kind === 'map') {
    const entries = rows
    const commit = (next: [string, unknown][]) => {
      setRows(next)
      onChange(Object.fromEntries(next))
    }
    return (
      <div className="space-y-2">
        {entries.map(([k, v], i) => {
          const dup = !!k.trim() && keys.indexOf(k.trim()) !== i
          return (
            <div key={i} className="flex items-start gap-2">
              <div className="w-1/3 shrink-0">
                <input
                  type="text"
                  value={k}
                  autoFocus={i === focusRow}
                  disabled={disabled}
                  placeholder="Label"
                  onChange={(e) => {
                    const next = [...entries]
                    next[i] = [e.target.value, v]
                    commit(next)
                  }}
                  className={`${inputClass} ${dup || !k.trim() ? 'border-red-400' : ''}`}
                />
                {dup && <p className="mt-1 text-xs text-red-600">Duplicate label</p>}
                {!k.trim() && <p className="mt-1 text-xs text-red-600">Label required</p>}
              </div>
              <ItemInput
                item={item}
                value={v}
                disabled={disabled}
                onChange={(nv) => {
                  const next = [...entries]
                  next[i] = [k, nv]
                  commit(next)
                }}
                onValidityChange={markValid(i)}
              />
              <RowButtons
                disabled={disabled}
                first={i === 0}
                last={i === entries.length - 1}
                onMove={(d) => {
                  resetValidity()
                  commit(move(entries, i, d))
                }}
                onRemove={() => {
                  resetValidity()
                  commit(entries.filter((_, j) => j !== i))
                }}
              />
            </div>
          )
        })}
        <AddButton
          disabled={disabled || full}
          label="Add entry"
          onClick={() => {
            setFocusRow(entries.length)
            commit([...entries, ['', emptyItem(item)]])
          }}
        />
      </div>
    )
  }

  // list / set
  const items: unknown[] = Array.isArray(value) ? value : []
  const ordered = kind === 'list'
  const seen = items.map((v) => JSON.stringify(v))
  return (
    <div className="space-y-2">
      {!ordered && (
        <p className="text-xs text-gray-500">Unordered and unique — saved sorted, duplicates removed.</p>
      )}
      {items.map((v, i) => {
        const dup = !ordered && seen.indexOf(seen[i]) !== i
        return (
          <div key={i} className="flex items-start gap-2">
            <div className="w-full">
              <ItemInput
                item={item}
                value={v}
                disabled={disabled}
                onChange={(nv) => {
                  const next = [...items]
                  next[i] = nv
                  onChange(next)
                }}
                onValidityChange={markValid(i)}
                autoFocus={i === focusRow}
              />
              {dup && <p className="mt-1 text-xs text-amber-600">Duplicate — will be dropped on save</p>}
            </div>
            <RowButtons
              disabled={disabled}
              ordered={ordered}
              first={i === 0}
              last={i === items.length - 1}
              onMove={(d) => {
                resetValidity()
                onChange(move(items, i, d))
              }}
              onRemove={() => {
                resetValidity()
                onChange(items.filter((_, j) => j !== i))
              }}
            />
          </div>
        )
      })}
      <AddButton
        disabled={disabled || full}
        label="Add item"
        onClick={() => {
          setFocusRow(items.length)
          onChange([...items, emptyItem(item)])
        }}
      />
    </div>
  )
}

function toEntries(value: unknown): [string, unknown][] {
  return value && typeof value === 'object' && !Array.isArray(value) ? Object.entries(value) : []
}

function rowCount(kind: GlobalKind, value: unknown): number {
  if (kind === 'map') return value && typeof value === 'object' ? Object.keys(value).length : 0
  return Array.isArray(value) ? value.length : 0
}

function move<T>(arr: T[], i: number, delta: -1 | 1): T[] {
  const j = i + delta
  if (j < 0 || j >= arr.length) return arr
  const next = [...arr]
  ;[next[i], next[j]] = [next[j], next[i]]
  return next
}

function RowButtons({
  ordered = true,
  first,
  last,
  disabled,
  onMove,
  onRemove,
}: {
  ordered?: boolean
  first: boolean
  last: boolean
  disabled?: boolean
  onMove: (delta: -1 | 1) => void
  onRemove: () => void
}) {
  return (
    <div className="flex shrink-0 items-center pt-1">
      {ordered && (
        <>
          <button type="button" className={iconBtn} disabled={disabled || first} onClick={() => onMove(-1)} title="Move up">
            <ArrowUp size={14} />
          </button>
          <button type="button" className={iconBtn} disabled={disabled || last} onClick={() => onMove(1)} title="Move down">
            <ArrowDown size={14} />
          </button>
        </>
      )}
      <button type="button" className={iconBtn} disabled={disabled} onClick={onRemove} title="Remove">
        <X size={14} />
      </button>
    </div>
  )
}

function AddButton({ label, onClick, disabled }: { label: string; onClick: () => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="inline-flex items-center gap-1 rounded-md border border-dashed border-gray-300 px-3 py-1.5 text-sm text-gray-600 hover:border-brand-500 hover:text-brand-700 disabled:opacity-40"
    >
      <Plus size={14} />
      {label}
    </button>
  )
}
