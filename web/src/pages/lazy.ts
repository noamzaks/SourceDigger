import { lazy } from 'react'

const loadSourceSettingsPage = () => import('./SourceSettingsPage')
const loadSymbolPage = () => import('./SymbolPage')
const loadSymbolCodePage = () => import('./SymbolCodePage')
const loadSiteSettingsPage = () => import('./SiteSettingsPage')
export const SourceSettingsPage = lazy(() =>
  loadSourceSettingsPage().then((module) => ({ default: module.SourceSettingsPage })),
)
export const SymbolPage = lazy(() =>
  loadSymbolPage().then((module) => ({ default: module.SymbolPage })),
)

export const SiteSettingsPage = lazy(() =>
  loadSiteSettingsPage().then((module) => ({ default: module.SiteSettingsPage })),
)

export const SymbolCodePage = lazy(() =>
  loadSymbolCodePage().then((module) => ({ default: module.SymbolCodePage })),
)

export function preloadPages() {
  requestIdleCallback(() => {
    loadSymbolPage()
    loadSymbolCodePage()
    loadSourceSettingsPage()
  })
}
