import { create } from 'zustand'

// The session token lives in an HttpOnly cookie that page scripts cannot read, so there
// is nothing secret to keep here. Only who we are (for the UI) is held, in memory; on load
// the app asks the server (GET /api/auth/me) whether the cookie is still a valid session.
type SessionStatus = 'unknown' | 'authed' | 'anon'

interface AuthState {
  status: SessionStatus
  userId: string | null
  roles: string[]
  setSession: (userId: string, roles: string[]) => void
  clearSession: () => void
  isAdmin: () => boolean
}

// Earlier versions kept the bearer token in localStorage; remove any leftover copy.
try {
  localStorage.removeItem('cms_auth')
} catch {
  /* storage unavailable */
}

export const useAuthStore = create<AuthState>()((set, get) => ({
  status: 'unknown',
  userId: null,
  roles: [],
  setSession: (userId, roles) => set({ status: 'authed', userId, roles }),
  clearSession: () => set({ status: 'anon', userId: null, roles: [] }),
  isAdmin: () => get().roles.includes('admin'),
}))
