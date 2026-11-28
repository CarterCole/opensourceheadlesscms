import { useEffect } from 'react'
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom'
import { useAuthStore } from './lib/store'
import { restoreSession } from './lib/api'
import Login from './pages/Login'
import Layout from './components/Layout'
import Dashboard from './pages/Dashboard'
import ContentList from './pages/ContentList'
import ContentEditor from './pages/ContentEditor'
import Users from './pages/Users'
import Globals from './pages/Globals'

function AuthGuard({ children }: { children: React.ReactNode }) {
  const status = useAuthStore((s) => s.status)
  useEffect(() => {
    if (status === 'unknown') void restoreSession()
  }, [status])
  if (status === 'unknown') return null // checking the session cookie with the server
  if (status === 'anon') return <Navigate to="/login" replace />
  return <>{children}</>
}

export default function App() {
  return (
    <BrowserRouter basename={import.meta.env.BASE_URL}>
      <Routes>
        <Route path="/login" element={<Login />} />
        <Route
          path="/"
          element={
            <AuthGuard>
              <Layout />
            </AuthGuard>
          }
        >
          <Route index element={<Dashboard />} />
          <Route path="content/:type" element={<ContentList />} />
          <Route path="content/:type/new" element={<ContentEditor />} />
          <Route path="content/:type/:id/edit" element={<ContentEditor />} />
          <Route path="users" element={<Users />} />
          <Route path="globals" element={<Globals />} />
          <Route path="globals/:slug" element={<Globals />} />
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  )
}
