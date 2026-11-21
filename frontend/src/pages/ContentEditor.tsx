import { Suspense, useMemo, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { graphql, useLazyLoadQuery, useMutation, useSubscription } from 'react-relay'
import type { GraphQLSubscriptionConfig } from 'relay-runtime'
import { useAdminConfig } from '../lib/api'
import FieldEditor from '../components/FieldEditor'
import StatusBadge from '../components/StatusBadge'
import { Loader2, ChevronLeft, Save, ArrowRight } from 'lucide-react'
import type { ContentEditorQuery as ContentEditorQueryType } from './__generated__/ContentEditorQuery.graphql'
import type { ContentEditorCreateMutation as ContentEditorCreateMutationType } from './__generated__/ContentEditorCreateMutation.graphql'
import type { ContentEditorUpdateMutation as ContentEditorUpdateMutationType } from './__generated__/ContentEditorUpdateMutation.graphql'
import type { ContentEditorTransitionMutation as ContentEditorTransitionMutationType } from './__generated__/ContentEditorTransitionMutation.graphql'
import type { ContentEditorSubscription as ContentEditorSubscriptionType } from './__generated__/ContentEditorSubscription.graphql'

const ContentQuery = graphql`
  query ContentEditorQuery($type: String!, $id: ID!) {
    content(type: $type, id: $id) {
      id
      status
      fields
    }
  }
`

const CreateMutation = graphql`
  mutation ContentEditorCreateMutation($type: String!, $input: JSON!) {
    createContent(type: $type, input: $input) {
      id
      status
      fields
    }
  }
`

const UpdateMutation = graphql`
  mutation ContentEditorUpdateMutation($type: String!, $id: ID!, $input: JSON!) {
    updateContent(type: $type, id: $id, input: $input) {
      id
      status
      fields
    }
  }
`

const TransitionMutation = graphql`
  mutation ContentEditorTransitionMutation($type: String!, $id: ID!, $toState: String!) {
    transitionContent(type: $type, id: $id, toState: $toState) {
      id
      status
      fields
    }
  }
`

const ContentChangedSubscription = graphql`
  subscription ContentEditorSubscription($type: String!, $id: ID) {
    contentChanged(type: $type, id: $id) {
      mutationType
      item {
        id
        status
        fields
      }
    }
  }
`

function ExistingContentEditor({ type, id }: { type: string; id: string }) {
  const navigate = useNavigate()
  const { config } = useAdminConfig()
  const typeDef = config?.content_types.find((ct) => ct.name === type)
  const workflowDef = typeDef?.workflow ? config?.workflows[typeDef.workflow] : null

  const data = useLazyLoadQuery<ContentEditorQueryType>(ContentQuery, { type, id })
  const existing = data.content

  const [fields, setFields] = useState<Record<string, unknown>>((existing?.fields as Record<string, unknown>) ?? {})
  const [savedStatus, setSavedStatus] = useState<string>(existing?.status ?? 'draft')

  useSubscription(
    useMemo<GraphQLSubscriptionConfig<ContentEditorSubscriptionType>>(
      () => ({
        subscription: ContentChangedSubscription,
        variables: { type, id },
        onNext: (response) => {
          const item = response?.contentChanged.item
          if (!item) return
          setSavedStatus(item.status)
          setFields(item.fields as Record<string, unknown>)
        },
      }),
      [type, id]
    )
  )

  const [commitUpdate, isUpdating] = useMutation<ContentEditorUpdateMutationType>(UpdateMutation)
  const [commitTransition, isTransitioning] = useMutation<ContentEditorTransitionMutationType>(TransitionMutation)
  const [saveError, setSaveError] = useState<string | null>(null)

  const handleSave = () => {
    setSaveError(null)
    commitUpdate({
      variables: { type, id, input: fields },
      onCompleted: (response) => setSavedStatus(response.updateContent.status),
      onError: (err) => setSaveError(err.message),
    })
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
          <h1 className="text-xl font-bold text-gray-900">Edit {typeDef?.label ?? type}</h1>
        </div>
        <StatusBadge status={savedStatus} />
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
            {nextState && (
              <button
                onClick={() =>
                  commitTransition({
                    variables: { type, id, toState: nextState },
                    onCompleted: (response) => setSavedStatus(response.transitionContent.status),
                  })
                }
                disabled={isTransitioning}
                className="flex items-center gap-1.5 rounded-md border border-brand-300 bg-brand-50 px-3 py-2 text-sm font-medium text-brand-700 hover:bg-brand-100 disabled:opacity-60"
              >
                {isTransitioning ? <Loader2 size={13} className="animate-spin" /> : <ArrowRight size={13} />}
                Move to {nextState.replace('_', ' ')}
              </button>
            )}
          </div>

          <button
            onClick={handleSave}
            disabled={isUpdating}
            className="flex items-center gap-1.5 rounded-md bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-60"
          >
            {isUpdating ? <Loader2 size={14} className="animate-spin" /> : <Save size={14} />}
            Save
          </button>
        </div>

        {saveError && <p className="mt-2 text-sm text-red-600">Save failed: {saveError}</p>}
      </div>
    </div>
  )
}

function NewContentEditor({ type }: { type: string }) {
  const navigate = useNavigate()
  const { config } = useAdminConfig()
  const typeDef = config?.content_types.find((ct) => ct.name === type)

  const [fields, setFields] = useState<Record<string, unknown>>({})
  const [saveError, setSaveError] = useState<string | null>(null)
  const [commitCreate, isCreating] = useMutation<ContentEditorCreateMutationType>(CreateMutation)

  const handleSave = () => {
    setSaveError(null)
    commitCreate({
      variables: { type, input: fields },
      onCompleted: (response) => {
        void navigate(`/content/${type}/${response.createContent.id}/edit`, { replace: true })
      },
      onError: (err) => setSaveError(err.message),
    })
  }

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
          <h1 className="text-xl font-bold text-gray-900">New {typeDef?.label ?? type}</h1>
        </div>
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

        <div className="mt-6 flex items-center justify-end border-t border-gray-100 pt-4">
          <button
            onClick={handleSave}
            disabled={isCreating}
            className="flex items-center gap-1.5 rounded-md bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-60"
          >
            {isCreating ? <Loader2 size={14} className="animate-spin" /> : <Save size={14} />}
            Save
          </button>
        </div>

        {saveError && <p className="mt-2 text-sm text-red-600">Save failed: {saveError}</p>}
      </div>
    </div>
  )
}

export default function ContentEditor() {
  const { type, id } = useParams<{ type: string; id?: string }>()

  if (!type) return null

  return (
    <Suspense
      fallback={
        <div className="flex items-center gap-2 text-gray-400">
          <Loader2 className="animate-spin" size={16} />
          Loading…
        </div>
      }
    >
      {id ? <ExistingContentEditor type={type} id={id} /> : <NewContentEditor type={type} />}
    </Suspense>
  )
}
