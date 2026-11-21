import { useEffect, useState } from 'react'
import { useAuthStore } from './store'

export const API_BASE = ''

function sessionExpired(): never {
  useAuthStore.getState().clearSession()
  window.location.href = '/login'
  throw new Error('Unauthorized')
}

// Authentication is the HttpOnly session cookie, sent automatically on same-origin requests.
export async function apiPost<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status === 401) sessionExpired()
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}

export async function apiGet<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, { credentials: 'same-origin' })
  if (res.status === 401) sessionExpired()
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}

export interface LoginResponse {
  access_token: string // also set as an HttpOnly cookie; the UI does not keep it
  user_id: string
  roles: string[]
}

export async function login(email: string, password: string): Promise<LoginResponse> {
  // Not apiPost: a 401 here means "wrong password", not "session expired".
  const res = await fetch(`${API_BASE}/api/auth/login`, {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  })
  if (res.status === 429) throw new Error('Too many attempts. Try again later.')
  if (!res.ok) throw new Error('Invalid email or password')
  return res.json()
}

export interface SsoConfig {
  enabled: boolean
  name?: string
}

/** Whether "Sign in with …" (OpenID Connect, e.g. Kith) is configured on the server. */
export async function ssoConfig(): Promise<SsoConfig> {
  try {
    const res = await fetch(`${API_BASE}/api/auth/oidc/config`, { credentials: 'same-origin' })
    if (res.ok) return (await res.json()) as SsoConfig
  } catch {
    /* network error: no SSO button */
  }
  return { enabled: false }
}

/** Ask the server whether the session cookie is valid; fills the auth store either way. */
export async function restoreSession(): Promise<void> {
  try {
    const res = await fetch(`${API_BASE}/api/auth/me`, { credentials: 'same-origin' })
    if (res.ok) {
      const me = (await res.json()) as { user_id: string; roles: string[] }
      useAuthStore.getState().setSession(me.user_id, me.roles)
      return
    }
  } catch {
    /* network error: treat as signed out */
  }
  useAuthStore.getState().clearSession()
}

/** Revoke the session on the server (invalidates the token everywhere), then clear local state. */
export async function logout(): Promise<void> {
  try {
    await fetch(`${API_BASE}/api/auth/logout`, { method: 'POST', credentials: 'same-origin' })
  } finally {
    useAuthStore.getState().clearSession()
  }
}

export interface ContentTypeField {
  name: string
  type: string
  required: boolean
  label: string | null
}

export interface ContentTypeDef {
  name: string
  label: string
  fields: ContentTypeField[]
  workflow: string | null
}

export interface AdminConfig {
  content_types: ContentTypeDef[]
  workflows: Record<string, { states: string[] }>
}

export async function getAdminConfig(): Promise<AdminConfig> {
  return apiGet<AdminConfig>('/api/admin/config')
}

/// Static per-request-session config shared across the sidebar/dashboard/editor -
/// fetched once and cached at module scope since it rarely changes.
let adminConfigPromise: Promise<AdminConfig> | null = null

export function useAdminConfig(): { config: AdminConfig | null; isLoading: boolean } {
  const [config, setConfig] = useState<AdminConfig | null>(null)
  const [isLoading, setIsLoading] = useState(!config)

  useEffect(() => {
    let cancelled = false
    if (!adminConfigPromise) adminConfigPromise = getAdminConfig()
    adminConfigPromise.then((c) => {
      if (!cancelled) {
        setConfig(c)
        setIsLoading(false)
      }
    })
    return () => {
      cancelled = true
    }
  }, [])

  return { config, isLoading }
}
