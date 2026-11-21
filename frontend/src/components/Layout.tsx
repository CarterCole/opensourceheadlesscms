import { Outlet } from 'react-router-dom'
import { logout } from '../lib/api'
import Sidebar from './Sidebar'
import { LogOut } from 'lucide-react'

export default function Layout() {
  return (
    <div className="flex h-screen overflow-hidden bg-gray-50">
      <Sidebar />

      <div className="flex flex-1 flex-col overflow-hidden">
        <header className="flex h-14 items-center justify-end border-b border-gray-200 bg-white px-6">
          <button
            onClick={() => void logout()}
            className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm text-gray-500 hover:bg-gray-100"
          >
            <LogOut size={14} />
            Sign out
          </button>
        </header>

        <main className="flex-1 overflow-y-auto p-6">
          <Outlet />
        </main>
      </div>
    </div>
  )
}
