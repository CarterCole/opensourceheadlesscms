import { Suspense } from 'react'
import { Link } from 'react-router-dom'
import { graphql, useLazyLoadQuery } from 'react-relay'
import { useAdminConfig } from '../lib/api'
import StatusBadge from '../components/StatusBadge'
import { Plus, FileText, Loader2 } from 'lucide-react'
import type { DashboardRecentContentQuery as DashboardRecentContentQueryType } from './__generated__/DashboardRecentContentQuery.graphql'

const RecentContentQuery = graphql`
  query DashboardRecentContentQuery($type: String!) {
    contentList(type: $type, page: 1, perPage: 5) {
      id
      status
      fields
    }
  }
`

function ContentTypeCardInner({ typeName, label }: { typeName: string; label: string }) {
  const data = useLazyLoadQuery<DashboardRecentContentQueryType>(RecentContentQuery, { type: typeName })
  const recent = data.contentList

  return (
    <div className="rounded-xl border border-gray-200 bg-white p-5 shadow-sm">
      <div className="mb-4 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <FileText size={16} className="text-gray-400" />
          <h3 className="font-semibold text-gray-800">{label}</h3>
        </div>
        <Link
          to={`/content/${typeName}/new`}
          className="flex items-center gap-1 rounded-md bg-brand-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-brand-700"
        >
          <Plus size={12} />
          New
        </Link>
      </div>

      {recent.length === 0 ? (
        <p className="text-sm text-gray-400">No content yet.</p>
      ) : (
        <ul className="divide-y divide-gray-100">
          {recent.map((item) => {
            const fields = (item.fields ?? {}) as Record<string, unknown>
            return (
              <li key={item.id} className="flex items-center justify-between py-2">
                <Link
                  to={`/content/${typeName}/${item.id}/edit`}
                  className="truncate text-sm text-gray-700 hover:text-brand-700"
                >
                  {String(fields.title ?? item.id)}
                </Link>
                <StatusBadge status={item.status} />
              </li>
            )
          })}
        </ul>
      )}

      <Link
        to={`/content/${typeName}`}
        className="mt-3 block text-center text-xs text-brand-600 hover:underline"
      >
        View all →
      </Link>
    </div>
  )
}

function ContentTypeCard(props: { typeName: string; label: string }) {
  return (
    <Suspense
      fallback={
        <div className="flex items-center gap-2 rounded-xl border border-gray-200 bg-white p-5 text-gray-400 shadow-sm">
          <Loader2 className="animate-spin" size={14} />
          Loading…
        </div>
      }
    >
      <ContentTypeCardInner {...props} />
    </Suspense>
  )
}

export default function Dashboard() {
  const { config, isLoading } = useAdminConfig()

  return (
    <div>
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900">Dashboard</h1>
        <p className="mt-1 text-sm text-gray-500">Overview of your content</p>
      </div>

      {isLoading && (
        <div className="text-sm text-gray-400">Loading content types…</div>
      )}

      {config && config.content_types.length === 0 && (
        <div className="rounded-xl border border-dashed border-gray-300 p-10 text-center">
          <p className="text-gray-500">No content types configured yet.</p>
          <p className="mt-1 text-sm text-gray-400">
            Add YAML files to <code className="font-mono text-xs">content-types/</code> and restart
            the server.
          </p>
        </div>
      )}

      {config && config.content_types.length > 0 && (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {config.content_types.map((ct) => (
            <ContentTypeCard key={ct.name} typeName={ct.name} label={ct.label} />
          ))}
        </div>
      )}
    </div>
  )
}
