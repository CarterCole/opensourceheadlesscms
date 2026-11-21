import { useCallback, useEffect, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { Check, Globe, Lock, Loader2 } from 'lucide-react'
import { getGlobals, saveGlobal, type GlobalDef } from '../lib/api'
import GlobalValueEditor from '../components/GlobalValueEditor'

const KIND_LABEL: Record<GlobalDef['kind'], string> = {
  single: 'Single value',
  list: 'Ordered list',
  set: 'Set',
  map: 'Map',
}

/** Site settings: every global, or just one when reached as /globals/:slug. */
export default function Globals() {
  const { slug } = useParams()
  const [globals, setGlobals] = useState<GlobalDef[] | null>(null)
  const [canEdit, setCanEdit] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    getGlobals()
      .then((r) => {
        if (cancelled) return
        setGlobals(r.globals)
        setCanEdit(r.can_edit)
      })
      .catch((e) => !cancelled && setError(String(e.message ?? e)))
    return () => {
      cancelled = true
    }
  }, [])

  const shown = globals?.filter((g) => !slug || g.slug === slug) ?? []

  return (
    <div className="max-w-3xl">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900">
          {slug && shown[0] ? shown[0].label : 'Site settings'}
        </h1>
        <p className="mt-1 text-sm text-gray-500">
          Site-wide values served at <code className="font-mono text-xs">/api/v1/globals/&lt;slug&gt;</code>{' '}
          and as an ES module at <code className="font-mono text-xs">/api/v1/globals.js</code>.
          {slug && (
            <>
              {' '}
              <Link to="/globals" className="text-brand-700 hover:underline">
                All settings
              </Link>
            </>
          )}
        </p>
        {!canEdit && globals && (
          <p className="mt-2 text-sm text-amber-700">Read-only: your role can't change site settings.</p>
        )}
      </div>

      {error && <p className="text-sm text-red-600">{error}</p>}
      {!globals && !error && <p className="text-sm text-gray-400">Loading…</p>}
      {globals && shown.length === 0 && (
        <div className="rounded-xl border border-dashed border-gray-300 p-10 text-center text-gray-500">
          {slug ? `No setting "${slug}".` : 'No globals are defined. Add YAML files to the globals/ directory.'}
        </div>
      )}

      <div className="space-y-6">
        {shown.map((g) => (
          <GlobalCard
            key={g.slug}
            def={g}
            canEdit={canEdit}
            onSaved={(saved) => setGlobals((all) => all?.map((x) => (x.slug === saved.slug ? saved : x)) ?? null)}
          />
        ))}
      </div>
    </div>
  )
}

function GlobalCard({
  def,
  canEdit,
  onSaved,
}: {
  def: GlobalDef
  canEdit: boolean
  onSaved: (g: GlobalDef) => void
}) {
  const [value, setValue] = useState<unknown>(def.value)
  const [valid, setValid] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [savedAt, setSavedAt] = useState<number | null>(null)

  useEffect(() => setValue(def.value), [def.value])
  const dirty = JSON.stringify(value) !== JSON.stringify(def.value)
  const onValidityChange = useCallback((v: boolean) => setValid(v), [])

  async function save() {
    setSaving(true)
    setError(null)
    try {
      const saved = await saveGlobal(def.slug, value)
      onSaved(saved)
      setValue(saved.value)
      setSavedAt(Date.now())
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setSaving(false)
    }
  }

  return (
    <section className="rounded-xl border border-gray-200 bg-white p-5 shadow-sm">
      <header className="mb-4 flex flex-wrap items-start justify-between gap-2">
        <div>
          <h2 className="font-semibold text-gray-900">{def.label}</h2>
          {def.description && <p className="mt-0.5 text-sm text-gray-500">{def.description}</p>}
          <p className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-gray-400">
            <code className="font-mono">{def.slug}</code>
            <span>
              {KIND_LABEL[def.kind]}
              {def.kind !== 'single' || def.item !== 'text' ? ` of ${def.item}` : ''}
            </span>
            <span>stored in {def.storage}</span>
            <span className="inline-flex items-center gap-1">
              {def.public ? <Globe size={12} /> : <Lock size={12} />}
              {def.public ? 'public' : 'signed-in only'}
            </span>
          </p>
        </div>
        {def.updated_at && (
          <p className="text-xs text-gray-400">Updated {new Date(def.updated_at).toLocaleString()}</p>
        )}
      </header>

      <GlobalValueEditor
        kind={def.kind}
        item={def.item}
        value={value}
        onChange={setValue}
        onValidityChange={onValidityChange}
        maxItems={def.max_items}
        disabled={!canEdit || saving}
      />

      {canEdit && (
        <div className="mt-4 flex items-center gap-3">
          <button
            type="button"
            onClick={save}
            disabled={!dirty || !valid || saving}
            className="inline-flex items-center gap-2 rounded-md bg-brand-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-brand-700 disabled:opacity-40"
          >
            {saving && <Loader2 size={14} className="animate-spin" />}
            Save
          </button>
          {dirty && (
            <button
              type="button"
              onClick={() => setValue(def.value)}
              className="text-sm text-gray-500 hover:text-gray-800"
            >
              Discard changes
            </button>
          )}
          {!dirty && savedAt && (
            <span className="inline-flex items-center gap-1 text-sm text-green-700">
              <Check size={14} /> Saved
            </span>
          )}
          {error && <span className="text-sm text-red-600">{error}</span>}
        </div>
      )}
    </section>
  )
}
