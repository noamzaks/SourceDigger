export type DigStatus = 'queued' | 'running' | 'succeeded' | 'failed'
export type Kind = 'function' | 'define' | 'variable'
export type Action = 'added' | 'removed' | 'modified'

export const KINDS: Kind[] = ['function', 'define', 'variable']
export const ACTIONS: Action[] = ['added', 'removed', 'modified']

export interface Dig {
  id: number
  source_id: number
  status: DigStatus
  queued_at: number
  started_at: number | null
  finished_at: number | null
  versions_done: number
  versions_total: number
  objects_received: number
  objects_total: number
  log: string
  error: string | null
}

export interface SourceSettings {
  name: string
  url: string
  tag_pattern: string
  file_pattern: string
  viewer_url: string
  refresh_hours: number | null
}

export interface Source extends SourceSettings {
  id: number
}

export interface SourceSummary extends Source {
  version_count: number
  first_version: string | null
  last_version: string | null
  last_dig: Dig | null
}

export interface Place {
  path: string
  line: number
  signature: string
  url: string
}

export interface SymbolEvent {
  id: number
  version: string
  position: number
  name: string
  kind: Kind
  action: Action
  definition: Place
  previous: Place | null
}

export interface EventPage {
  events: SymbolEvent[]
  next: string | null
}

export interface PreviewVersion {
  name: string
  tag: string
}

export interface EventQuery {
  q: string
  kinds: Kind[]
  actions: Action[]
  since: string | null
  until: string | null
  path: string | null
}

export interface Version {
  position: number
  name: string
  tag: string
  commit: string
  added: number
  removed: number
  modified: number
}

export interface Definition {
  kind: Kind
  path: string
  line: number
  end_line: number
  url: string
  code: string
}

export interface Revision {
  version: string
  position: number
  definitions: Definition[]
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, {
    method,
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  const result = await response.json()
  if (!response.ok) {
    throw new Error(result.error ?? `${method} ${path} failed with ${response.status}`)
  }
  return result
}

const sourcePath = (source: string) => `/api/sources/${encodeURIComponent(source)}`

export const api = {
  sources: () => request<SourceSummary[]>('GET', '/api/sources'),
  source: (source: string) => request<SourceSummary>('GET', sourcePath(source)),
  createSource: (settings: SourceSettings) => request<Source>('POST', '/api/sources', settings),
  updateSource: (source: string, settings: SourceSettings) =>
    request<Source>('PUT', sourcePath(source), settings),
  deleteSource: (source: string) => request<null>('DELETE', sourcePath(source)),
  names: (source: string, q: string) =>
    request<string[]>('GET', `${sourcePath(source)}/names?${new URLSearchParams({ q })}`),
  events: (source: string, query: EventQuery, after: string | null) => {
    const parameters = new URLSearchParams({
      q: query.q || '*',
      kinds: query.kinds.join(','),
      actions: query.actions.join(','),
    })
    for (const key of ['since', 'until', 'path'] as const) {
      const value = query[key]
      if (value) {
        parameters.set(key, value)
      }
    }
    if (after) {
      parameters.set('after', after)
    }
    return request<EventPage>('GET', `${sourcePath(source)}/events?${parameters}`)
  },
  symbol: (source: string, name: string) =>
    request<SymbolEvent[]>('GET', `${sourcePath(source)}/symbols/${encodeURIComponent(name)}`),
  code: (source: string, name: string, path: string | null) =>
    request<Revision>(
      'GET',
      `${sourcePath(source)}/symbols/${encodeURIComponent(name)}/code?${new URLSearchParams(path ? { path } : {})}`,
    ),
  versions: (source: string) => request<Version[]>('GET', `${sourcePath(source)}/versions`),
  revisions: (source: string, name: string, path: string | null) =>
    request<Revision[]>(
      'GET',
      `${sourcePath(source)}/symbols/${encodeURIComponent(name)}/revisions?${new URLSearchParams(path ? { path } : {})}`,
    ),
  digs: (source: string) => request<Dig[]>('GET', `${sourcePath(source)}/digs`),
  startDig: (source: string, from: 'new' | 'scratch' = 'new') =>
    request<Dig>('POST', `${sourcePath(source)}/digs?from=${from}`),
  preview: (url: string, tag_pattern: string) =>
    request<PreviewVersion[]>('POST', '/api/preview', { url, tag_pattern }),
}

export const isDigging = (dig: Dig | null) => dig?.status === 'queued' || dig?.status === 'running'
