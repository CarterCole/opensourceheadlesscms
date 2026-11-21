import { useEffect, useState } from 'react'
import { apiGet } from '../lib/api'
import { UserCircle, ShieldCheck } from 'lucide-react'

interface User {
  id: string
  email: string
  display_name: string
  roles: string[]
  created_at: string
}

interface UserListResponse {
  users: User[]
}

export default function Users() {
  const [users, setUsers] = useState<User[]>([])
  const [isLoading, setIsLoading] = useState(true)

  useEffect(() => {
    let cancelled = false
    apiGet<UserListResponse>('/api/admin/users')
      .then((data) => {
        if (!cancelled) setUsers(data.users)
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <div>
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900">Users</h1>
        <p className="mt-1 text-sm text-gray-500">
          User management via the gRPC UserService is coming soon. View only for now.
        </p>
      </div>

      {isLoading && <p className="text-sm text-gray-400">Loading…</p>}

      {!isLoading && users.length === 0 && (
        <div className="rounded-xl border border-dashed border-gray-300 p-10 text-center">
          <p className="text-gray-500">No users endpoint yet.</p>
          <p className="mt-1 text-sm text-gray-400">
            The <code className="font-mono text-xs">/api/admin/users</code> endpoint will be
            available after the gRPC UserService milestone.
          </p>
        </div>
      )}

      {users.length > 0 && (
        <div className="overflow-hidden rounded-xl border border-gray-200 bg-white shadow-sm">
          <table className="min-w-full divide-y divide-gray-200">
            <thead className="bg-gray-50">
              <tr>
                <th className="px-4 py-3 text-left text-xs font-semibold uppercase tracking-wider text-gray-500">User</th>
                <th className="px-4 py-3 text-left text-xs font-semibold uppercase tracking-wider text-gray-500">Roles</th>
                <th className="px-4 py-3 text-left text-xs font-semibold uppercase tracking-wider text-gray-500">Joined</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-100">
              {users.map((user) => (
                <tr key={user.id} className="hover:bg-gray-50">
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-3">
                      <UserCircle size={28} className="flex-shrink-0 text-gray-300" />
                      <div>
                        <p className="text-sm font-medium text-gray-800">{user.display_name}</p>
                        <p className="text-xs text-gray-400">{user.email}</p>
                      </div>
                    </div>
                  </td>
                  <td className="px-4 py-3">
                    <div className="flex flex-wrap gap-1">
                      {user.roles.map((r) => (
                        <span
                          key={r}
                          className="flex items-center gap-0.5 rounded-full bg-purple-100 px-2 py-0.5 text-xs font-medium text-purple-700"
                        >
                          {r === 'admin' && <ShieldCheck size={10} />}
                          {r}
                        </span>
                      ))}
                    </div>
                  </td>
                  <td className="px-4 py-3 text-sm text-gray-500">
                    {new Date(user.created_at).toLocaleDateString()}
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
