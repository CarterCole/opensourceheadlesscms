import {
  Environment,
  Network,
  Observable,
  RecordSource,
  Store,
  type FetchFunction,
  type SubscribeFunction,
} from 'relay-runtime'
import { createClient } from 'graphql-ws'
import { useAuthStore } from './store'

const fetchGraphQL: FetchFunction = async (request, variables) => {
  const res = await fetch('/graphql', {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      query: request.text,
      variables,
    }),
  })

  if (res.status === 401) {
    useAuthStore.getState().clearSession()
    window.location.href = `${import.meta.env.BASE_URL}login`
    throw new Error('Unauthorized')
  }

  return res.json()
}

const wsClient = createClient({
  url: `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}/graphql/ws`,
  // No connectionParams: the browser sends the HttpOnly session cookie on the upgrade request.
})

const subscribeGraphQL: SubscribeFunction = (request, variables) => {
  return Observable.create((sink) => {
    return wsClient.subscribe(
      { query: request.text ?? '', variables, operationName: request.name },
      {
        next: (data) => sink.next(data as Parameters<typeof sink.next>[0]),
        error: (err) => sink.error(err instanceof Error ? err : new Error(String(err))),
        complete: () => sink.complete(),
      }
    )
  })
}

export const relayEnvironment = new Environment({
  network: Network.create(fetchGraphQL, subscribeGraphQL),
  store: new Store(new RecordSource()),
})
