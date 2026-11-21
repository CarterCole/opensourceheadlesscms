import { Link, useLocation } from 'react-router-dom'
import { useAdminConfig } from '../lib/api'
import { LayoutDashboard, FileText, Users, Loader2, SlidersHorizontal } from 'lucide-react'

export default function Sidebar() {
  const { pathname } = useLocation()
  const { config, isLoading } = useAdminConfig()

  const linkClass = (path: string) =>
    `flex items-center gap-2 rounded-md px-3 py-2 text-sm font-medium transition-colors ${
      pathname === path || pathname.startsWith(path + '/')
        ? 'bg-brand-100 text-brand-700'
        : 'text-gray-600 hover:bg-gray-100'
    }`

  return (
    <aside className="flex h-full w-56 flex-col border-r border-gray-200 bg-white">
      <div className="flex h-14 items-center border-b border-gray-200 px-4">
        <span className="text-lg font-bold text-brand-700">CMS Admin</span>
      </div>

      <nav className="flex-1 overflow-y-auto p-3 space-y-1">
        <Link to="/" className={linkClass('/')}>
          <LayoutDashboard size={16} />
          Dashboard
        </Link>

        {isLoading && (
          <div className="flex items-center gap-2 px-3 py-2 text-sm text-gray-400">
            <Loader2 size={14} className="animate-spin" />
            Loading…
          </div>
        )}

        {config && config.content_types.length > 0 && (
          <div className="pt-3">
            <p className="mb-1 px-3 text-xs font-semibold uppercase tracking-wider text-gray-400">
              Content
            </p>
            {config.content_types.map((ct) => (
              <Link
                key={ct.name}
                to={`/content/${ct.name}`}
                className={linkClass(`/content/${ct.name}`)}
              >
                <FileText size={16} />
                {ct.label || ct.name}
              </Link>
            ))}
          </div>
        )}

        {config && config.globals?.length > 0 && (
          <div className="pt-3">
            <p className="mb-1 px-3 text-xs font-semibold uppercase tracking-wider text-gray-400">
              Site
            </p>
            {config.globals.map((g) => (
              <Link key={g.slug} to={`/globals/${g.slug}`} className={linkClass(`/globals/${g.slug}`)}>
                <SlidersHorizontal size={16} />
                {g.label}
              </Link>
            ))}
          </div>
        )}

        <div className="pt-3">
          <p className="mb-1 px-3 text-xs font-semibold uppercase tracking-wider text-gray-400">
            Settings
          </p>
          <Link to="/users" className={linkClass('/users')}>
            <Users size={16} />
            Users
          </Link>
        </div>
      </nav>
    </aside>
  )
}
