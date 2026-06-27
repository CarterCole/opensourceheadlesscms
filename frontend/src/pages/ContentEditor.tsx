import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { getAdminConfig } from '../lib/api'
import { getContent, createContent, updateContent, transitionContent } from '../lib/queries'
import FieldEditor from '../components/FieldEditor'
import StatusBadge from '../components/StatusBadge'
import { Loader2, ChevronLeft, Save, ArrowRight } from 'lucide-react'

export default function ContentEditor() {
  const { type, id } = useParams<{ type: string; id?: string }>()
  const navigate = useNavigate()
  const qc = useQueryClient()
  const isNew = !id

  const { data: config } = useQuery({ queryKey: ['admin-config'], queryFn: getAdminConfig })
  const typeDef = config?.content_types.find((ct) => ct.name === type)
  const workflowDef = typeDef?.workflow ? config?.workflows[typeDef.workflow] : null

  const [fields, setFields] = useState<Record<string, unknown>>({})
  const [savedStatus, setSavedStatus] = useState<string>('draft')

  const { data: existing, isLoading } = useQuery({
    queryKey: ['content', type, id],
    queryFn: () => getContent(type!, id!),
    enabled: !isNew && !!type && !!id,
  })

  useEffect(() => {
    if (existing) {
      setSavedStatus(existing.status)
      setFields(existing as Record<string, unknown>)
    }
  }, [existing])

  const saveMutation = useMutation({
    mutationFn: () => {
      const input = { ...fields }
      if (isNew) return createContent(type!, input)
      return updateContent(type!, id!, input)
    },
    onSuccess: (item) => {
      qc.invalidateQueries({ queryKey: ['content', type] })
      if (isNew) navigate(`/content/${type}/${item.id}/edit`, { replace: true })
      else setSavedStatus(item.status)
    },
  })

  const transitionMutation = useMutation({
    mutationFn: (toState: string) => transitionContent(type!, id!, toState),
    onSuccess: (item) => {
      setSavedStatus(item.status)
      qc.invalidateQueries({ queryKey: ['content', type] })
    },
  })

  if (!type) return null
  if (!isNew && isLoading) {
    return (
      <div className="flex items-center gap-2 text-gray-400">
        <Loader2 className="animate-spin" size={16} />
        Loading…
      </div>
    )
  }

  const currentStates = workflowDef?.states ?? []
  const currentIdx = currentStates.indexOf(savedStatus)
  const canAdvance = currentIdx >= 0 && currentIdx < currentStates.length - 1
  const nextState = canAdvance ? currentStates[currentIdx + 1] : null

  return (
    <div className="mx-auto max-w-3xl">
      <div className="mb-6 flex items-center gap-3">
        <button
          onClick={() => navigate(`/content/${type}`)}
          className="rounded p-1.5 text-gray-400 hover:bg-gray-100"
        >
          <ChevronLeft size={18} />
        </button>
        <div className="flex-1">
          <h1 className="text-xl font-bold text-gray-900">
            {isNew ? `New ${typeDef?.label ?? type}` : `Edit ${typeDef?.label ?? type}`}
          </h1>
        </div>
        {!isNew && <StatusBadge status={savedStatus} />}
      </div>

      <div className="rounded-xl border border-gray-200 bg-white p-6 shadow-sm">
        {typeDef?.fields.map((field) => (
          <div key={field.name} className="mb-5">
            <FieldEditor
              field={field}
              value={fields[field.name] ?? ''}
              onChange={(val) => setFields((prev) => ({ ...prev, [field.name]: val }))}
            />
          </div>
        ))}

        {!typeDef && (
          <p className="text-sm text-gray-400">
            Content type <code className="font-mono">{type}</code> not found in config.
          </p>
        )}

        <div className="mt-6 flex items-center justify-between border-t border-gray-100 pt-4">
          <div className="flex gap-2">
            {nextState && !isNew && (
              <button
                onClick={() => transitionMutation.mutate(nextState)}
                disabled={transitionMutation.isPending}
                className="flex items-center gap-1.5 rounded-md border border-brand-300 bg-brand-50 px-3 py-2 text-sm font-medium text-brand-700 hover:bg-brand-100 disabled:opacity-60"
              >
                {transitionMutation.isPending ? (
                  <Loader2 size={13} className="animate-spin" />
                ) : (
                  <ArrowRight size={13} />
                )}
                Move to {nextState.replace('_', ' ')}
              </button>
            )}
          </div>

          <button
            onClick={() => saveMutation.mutate()}
            disabled={saveMutation.isPending}
            className="flex items-center gap-1.5 rounded-md bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-60"
          >
            {saveMutation.isPending ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <Save size={14} />
            )}
            Save
          </button>
        </div>

        {saveMutation.isError && (
          <p className="mt-2 text-sm text-red-600">
            Save failed: {(saveMutation.error as Error).message}
          </p>
        )}
      </div>
    </div>
  )
}
