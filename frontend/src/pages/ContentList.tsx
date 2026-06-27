import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { getAdminConfig } from '../lib/api'
import { listContent, deleteContent } from '../lib/queries'
import StatusBadge from '../components/StatusBadge'
import { Plus, Trash2, Pencil } from 'lucide-react'

const STATUS_FILTERS = ['all', 'draft', 'in_review', 'approved', 'published', 'archived']

export default function ContentList() {
  const { type } = useParams<{ type: string }>()
  const navigate = useNavigate()
  const qc = useQueryClient()
  const [status, setStatus] = useState<string>('all')

  const { data: config } = useQuery({ queryKey: ['admin-config'], queryFn: getAdminConfig })
  const typeDef = config?.content_types.find((ct) => ct.name === type)

  const { data: items = [], isLoading } = useQuery({
    queryKey: ['content', type, status],
    queryFn: () => listContent(type!, status === 'all' ? undefined : status),
    enabled: !!type,
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteContent(type!, id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['content', type] }),
  })

  if (!type) return null

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

      {/* Status filter tabs */}
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

      {isLoading && <p className="text-sm text-gray-400">Loading…</p>}

      {!isLoading && items.length === 0 && (
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
              {items.map((item) => (
                <tr key={item.id} className="hover:bg-gray-50">
                  <td className="px-4 py-3">
                    <button
                      onClick={() => navigate(`/content/${type}/${item.id}/edit`)}
                      className="text-sm font-medium text-gray-800 hover:text-brand-700"
                    >
                      {String(item.title ?? item.id)}
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
                        onClick={() => {
                          if (confirm('Delete this item?')) deleteMutation.mutate(item.id)
                        }}
                        className="rounded p-1.5 text-gray-400 hover:bg-red-50 hover:text-red-600"
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
