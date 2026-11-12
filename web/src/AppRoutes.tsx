import { Route, Routes } from 'react-router'

import { Layout } from './components/Layout'
import { SourceSettingsPage, SymbolPage } from './pages/lazy'
import { Home } from './pages/Home'
import { NotFound } from './pages/NotFound'
import { Search } from './pages/Search'

export function AppRoutes() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Home />} />
        <Route path="add" element={<SourceSettingsPage />} />
        <Route path="sources/:source" element={<Search />} />
        <Route path="sources/:source/symbols/:symbol" element={<SymbolPage />} />
        <Route path="sources/:source/settings" element={<SourceSettingsPage />} />
        <Route path="*" element={<NotFound />} />
      </Route>
    </Routes>
  )
}
