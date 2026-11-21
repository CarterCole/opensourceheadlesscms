import { Suspense, useMemo, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { graphql, useLazyLoadQuery, useMutation, useSubscription } from 'react-relay'
import type { GraphQLSubscriptionConfig } from 'relay-runtime'
import { useAdminConfig } from '../lib/api'
import StatusBadge from '../components/StatusBadge'
import { Plus, Trash2, Pencil, Loader2 } from 'lucide-react'
import type { ContentListQuery as ContentListQueryType } from './__generated__/ContentListQuery.graphql'
import type { ContentListDeleteMutation as ContentListDeleteMutationType } from './__generated__/ContentListDeleteMutation.graphql'
import type { ContentListSubscription as ContentListSubscriptionType } from './__generated__/ContentListSubscription.graphql'

const STATUS_FILTERS = ['all', 'draft', 'in_review', 'approved', 'published', 'archived']

const ContentListQuery = graphql`
  query ContentListQuery($type: String!, $status: String) {
    contentList(type: $type, status: $status) {
      id
      status
      updatedAt
      fields
    }
  }
`

const DeleteMutation = graphql`
  mutation ContentListDeleteMutation($type: String!, $id: ID!) {
    deleteContent(type: $type, id: $id)
  }
`

const ContentChangedSubscription = graphql`
  subscription ContentListSubscription($type: String!) {
    contentChanged(type: $type) {
      mutationType
      id
      item {
        id
        status
        updatedAt
        fields
      }
    }
  }
`

function ContentListInner({ type, status }: { type: string; status: string }) {
  const navigate = useNavigate()
  const { config } = useAdminConfig()
  const typeDef = config?.content_types.find((ct) => ct.name === type)

  const listVariables = { type, status: status === 'all' ? null : status }
  const data = useLazyLoadQuery<ContentListQueryType>(ContentListQuery, listVariables)
  const items = data.contentList

  const [commitDelete, isDeleting] = useMutation<ContentListDeleteMutationType>(DeleteMutation)

  const subscriptionConfig = useMemo<GraphQLSubscriptionConfig<ContentListSubscriptionType>>(
    () => ({
      subscription: ContentChangedSubscription,
      variables: { type },
      updater: (store) => {
        const payload = store.getRootField('contentChanged')
        if (!payload) return
        const mutationType = payload.getValue('mutationType')
        const id = payload.getValue('id') as string
        const root = store.getRoot()
        const existing = root.getLinkedRecords('contentList', listVariables) ?? []

        if (mutationType === 'DELETED') {
          root.setLinkedRecords(
            existing.filter((r) => r?.getValue('id') !== id),
            'contentList',
            listVariables
          )
          return
        }

        const itemRecord = payload.getLinkedRecord('item')
        if (!itemRecord) return
        const alreadyPresent = existing.some((r) => r?.getValue('id') === id)
        if (mutationType === 'CREATED' && !alreadyPresent) {
          root.setLinkedRecords([itemRecord, ...existing], 'contentList', listVariables)
        }
        // UPDATED / TRANSITIONED: the item record is already refreshed in place by
        // normalization (matched by `id`), so list membership doesn't need to change.
      },
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [type, status]
  )
  useSubscription(subscriptionConfig)

  const handleDelete = (id: string) => {
    if (!confirm('Delete this item?')) return
    commitDelete({
      variables: { type, id },
      optimisticResponse: { deleteContent: true },
      updater: (store) => {
        const root = store.getRoot()
        const existing = root.getLinkedRecords('contentList', listVariables) ?? []
        root.setLinkedRecords(
          existing.filter((r) => r?.getValue('id') !== id),
          'contentList',
          listVariables
        )
      },
    })
  }

  return (
    <div>
      <div className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">{typeDef?.label ?? type}</h1>
          <p className="mt-0.5 text-sm text-gray-500">{items.length} items</p>
        </div>
        <Link
          to={`/content/${type}/new`}
          className="flex items-center gap-1.5 rounded-md bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700"
        >
          <Plus size={15} />
          New {typeDef?.label ?? type}
        </Link>
      </div>

      {items.length === 0 && (
        <div className="rounded-xl border border-dashed border-gray-300 p-10 text-center">
          <p className="text-gray-500">No {status === 'all' ? '' : status} {typeDef?.label ?? type} items yet.</p>
        </div>
      )}

      {items.length > 0 && (
        <div className="overflow-hidden rounded-xl border border-gray-200 bg-white shadow-sm">
          <table className="min-w-full divide-y divide-gray-200">
            <thead className="bg-gray-50">
              <tr>
                <th className="px-4 py-3 text-left text-xs font-semibold uppercase tracking-wider text-gray-500">Title / ID</th>
                <th className="px-4 py-3 text-left text-xs font-semibold uppercase tracking-wider text-gray-500">Status</th>
                <th className="px-4 py-3 text-left text-xs font-semibold uppercase tracking-wider text-gray-500">Updated</th>
                <th className="px-4 py-3" />
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-100">
              {items.map((item) => {
                const fields = (item.fields ?? {}) as Record<string, unknown>
                return (
                  <tr key={item.id} className="hover:bg-gray-50">
                    <td className="px-4 py-3">
                      <button
                        onClick={() => navigate(`/content/${type}/${item.id}/edit`)}
                        className="text-sm font-medium text-gray-800 hover:text-brand-700"
                      >
                        {String(fields.title ?? item.id)}
                      </button>
                      <p className="font-mono text-xs text-gray-400">{item.id}</p>
                    </td>
                    <td className="px-4 py-3">
                      <StatusBadge status={item.status} />
                    </td>
                    <td className="px-4 py-3 text-sm text-gray-500">
                      {new Date(item.updatedAt as string).toLocaleDateString()}
                    </td>
                    <td className="px-4 py-3">
                      <div className="flex items-center justify-end gap-1">
                        <Link
                          to={`/content/${type}/${item.id}/edit`}
                          className="rounded p-1.5 text-gray-400 hover:bg-gray-100 hover:text-gray-700"
                        >
                          <Pencil size={14} />
                        </Link>
                        <button
                          onClick={() => handleDelete(item.id)}
                          disabled={isDeleting}
                          className="rounded p-1.5 text-gray-400 hover:bg-red-50 hover:text-red-600 disabled:opacity-60"
                        >
                          <Trash2 size={14} />
                        </button>
                      </div>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

export default function ContentList() {
  const { type } = useParams<{ type: string }>()
  const [status, setStatus] = useState<string>('all')

  if (!type) return null

  return (
    <div>
      <div className="mb-4 flex gap-1 overflow-x-auto border-b border-gray-200">
        {STATUS_FILTERS.map((s) => (
          <button
            key={s}
            onClick={() => setStatus(s)}
            className={`px-3 py-2 text-sm font-medium whitespace-nowrap transition-colors ${
              status === s
                ? 'border-b-2 border-brand-600 text-brand-700'
                : 'text-gray-500 hover:text-gray-700'
            }`}
          >
            {s.replace('_', ' ')}
          </button>
        ))}
      </div>

      <Suspense
        fallback={
          <div className="flex items-center gap-2 text-gray-400">
            <Loader2 className="animate-spin" size={16} />
            Loading…
          </div>
        }
      >
        <ContentListInner type={type} status={status} />
      </Suspense>
    </div>
  )
}
