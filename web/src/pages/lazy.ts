import { lazy } from 'react'

const loadSourceSettingsPage = () => import('./SourceSettingsPage')
const loadSymbolPage = () => import('./SymbolPage')
export const SourceSettingsPage = lazy(() =>
  loadSourceSettingsPage().then((module) => ({ default: module.SourceSettingsPage })),
)
export const SymbolPage = lazy(() =>
  loadSymbolPage().then((module) => ({ default: module.SymbolPage })),
)

export function preloadPages() {
  requestIdleCallback(() => {
    loadSymbolPage()
    loadSourceSettingsPage()
  })
}
