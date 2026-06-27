import { create } from 'zustand'
import { persist } from 'zustand/middleware'

interface AuthState {
  token: string | null
  userId: string | null
  roles: string[]
  setAuth: (token: string, userId: string, roles: string[]) => void
  logout: () => void
  isAdmin: () => boolean
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      token: null,
      userId: null,
      roles: [],
      setAuth: (token, userId, roles) => set({ token, userId, roles }),
      logout: () => set({ token: null, userId: null, roles: [] }),
      isAdmin: () => get().roles.includes('admin'),
    }),
    { name: 'cms_auth' }
  )
)
