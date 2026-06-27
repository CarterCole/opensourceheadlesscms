import { GraphQLClient } from 'graphql-request'
import { useAuthStore } from './store'

export const API_BASE = ''

export function getGqlClient() {
  const token = useAuthStore.getState().token
  return new GraphQLClient('/graphql', {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  })
}

export async function apiPost<T>(path: string, body: unknown): Promise<T> {
  const token = useAuthStore.getState().token
  const res = await fetch(`${API_BASE}${path}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify(body),
  })
  if (res.status === 401) {
    useAuthStore.getState().logout()
    window.location.href = '/login'
    throw new Error('Unauthorized')
  }
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}

export async function apiGet<T>(path: string): Promise<T> {
  const token = useAuthStore.getState().token
  const res = await fetch(`${API_BASE}${path}`, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  })
  if (res.status === 401) {
    useAuthStore.getState().logout()
    window.location.href = '/login'
    throw new Error('Unauthorized')
  }
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}

export interface LoginResponse {
  access_token: string
  user_id: string
  roles: string[]
}

export async function login(email: string, password: string): Promise<LoginResponse> {
  return apiPost<LoginResponse>('/api/auth/login', { email, password })
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
