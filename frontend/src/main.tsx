import { StrictMode, Suspense } from 'react'
import { createRoot } from 'react-dom/client'
import { RelayEnvironmentProvider } from 'react-relay'
import { relayEnvironment } from './lib/relayEnvironment'
import './index.css'
import App from './App'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <RelayEnvironmentProvider environment={relayEnvironment}>
      <Suspense fallback={null}>
        <App />
      </Suspense>
    </RelayEnvironmentProvider>
  </StrictMode>
)
