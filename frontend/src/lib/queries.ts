import { gql } from 'graphql-request'
import { getGqlClient } from './api'

// Pluralize a content type name (simple: append 's')
const plural = (name: string) => `${name}s`
const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

export interface ContentItem {
  id: string
  status: string
  createdAt: string
  updatedAt: string
  createdBy: string
  [key: string]: unknown
}

export interface ListResult {
  items: ContentItem[]
  total: number
  page: number
  perPage: number
}

export async function listContent(
  type: string,
  status?: string,
  page = 1,
  perPage = 20
): Promise<ContentItem[]> {
  const client = getGqlClient()
  const query = gql`
    query List($status: String, $page: Int, $perPage: Int) {
      ${plural(type)}(status: $status, page: $page, perPage: $perPage)
    }
  `
  const data = await client.request<Record<string, ContentItem[]>>(query, { status, page, perPage })
  return data[plural(type)] ?? []
}

export async function getContent(type: string, id: string): Promise<ContentItem> {
  const client = getGqlClient()
  const query = gql`
    query Get($id: ID) {
      ${type}(id: $id)
    }
  `
  const data = await client.request<Record<string, ContentItem>>(query, { id })
  return data[type]
}

export async function createContent(
  type: string,
  input: Record<string, unknown>
): Promise<ContentItem> {
  const client = getGqlClient()
  const mutation = gql`
    mutation Create($input: ${cap(type)}Input!) {
      create${cap(type)}(input: $input)
    }
  `
  const data = await client.request<Record<string, ContentItem>>(mutation, { input })
  return data[`create${cap(type)}`]
}

export async function updateContent(
  type: string,
  id: string,
  input: Record<string, unknown>
): Promise<ContentItem> {
  const client = getGqlClient()
  const mutation = gql`
    mutation Update($id: ID!, $input: ${cap(type)}Input!) {
      update${cap(type)}(id: $id, input: $input)
    }
  `
  const data = await client.request<Record<string, ContentItem>>(mutation, { id, input })
  return data[`update${cap(type)}`]
}

export async function transitionContent(
  type: string,
  id: string,
  toState: string
): Promise<ContentItem> {
  const client = getGqlClient()
  const mutation = gql`
    mutation Transition($id: ID!, $toState: String!) {
      transition${cap(type)}(id: $id, toState: $toState)
    }
  `
  const data = await client.request<Record<string, ContentItem>>(mutation, { id, toState })
  return data[`transition${cap(type)}`]
}

export async function deleteContent(type: string, id: string): Promise<boolean> {
  const client = getGqlClient()
  const mutation = gql`
    mutation Delete($id: ID!) {
      delete${cap(type)}(id: $id)
    }
  `
  const data = await client.request<Record<string, boolean>>(mutation, { id })
  return data[`delete${cap(type)}`]
}

export async function searchContent(
  query: string,
  contentType?: string
): Promise<ContentItem[]> {
  const client = getGqlClient()
  const gqlQuery = gql`
    query Search($query: String!, $contentType: String) {
      search(query: $query, contentType: $contentType) {
        id
        contentType
        status
      }
    }
  `
  const data = await client.request<{ search: ContentItem[] }>(gqlQuery, { query, contentType })
  return data.search
}
