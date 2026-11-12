export type DigStatus = 'queued' | 'running' | 'succeeded' | 'failed'
export type Kind =
  | 'function'
  | 'define'
  | 'variable'
  | 'struct'
  | 'union'
  | 'enum'
  | 'enumerator'
  | 'typedef'
  | 'invocation'
export type Action = 'added' | 'removed' | 'modified' | 'signature'

export const KINDS: Kind[] = [
  'function',
  'define',
  'variable',
  'struct',
  'union',
  'enum',
  'enumerator',
  'typedef',
  'invocation',
]
export const ACTIONS: Action[] = ['added', 'removed', 'modified', 'signature']
export type Field = 'name' | 'kind' | 'action' | 'version' | 'after' | 'before' | 'path' | 'commit'

export interface Term {
  field: Field
  value: string
}

export type Query = { term: Term } | { not: Query } | { and: Query[] } | { or: Query[] }

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

export interface Commit {
  hash: string
  summary: string
  author: string
  time: number
}

export interface SymbolEvent {
  id: number
  version: string
  position: number
  name: string
  kind: Kind
  action: Action
  commit: Commit
  definition: Place
  previous: Place | null
}

export interface EventPage {
  query: Query
  events: SymbolEvent[]
  next: string | null
}

export interface SiteOptions {
  database?: string
  repositories?: string
  threads?: number
  listen?: string
  web_directory?: string
}

export interface Site {
  running: Required<Omit<SiteOptions, 'web_directory'>> & {
    web_directory: string | null
    file: string
    overridden: SiteOptions
  }
  saved: SiteOptions
}

export interface PreviewVersion {
  name: string
  tag: string
}

export interface Version {
  position: number
  name: string
  tag: string
  commit: string
  added: number
  removed: number
  modified: number
  signature: number
}

export interface Definition {
  kind: Kind
  path: string
  line: number
  end_line: number
  url: string
  code: string
}

export interface FileChange {
  path: string
  moved_from: string | null
  before: string
  after: string
}

export interface Change {
  version: string
  previous_version: string | null
  action: Action
  files: FileChange[]
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
const symbolPath = (source: string, name: string) =>
  `${sourcePath(source)}/symbols/${encodeURIComponent(name)}`
const pathParameters = (path: string | null) => new URLSearchParams(path ? { path } : {})

export const api = {
  sources: () => request<SourceSummary[]>('GET', '/api/sources'),
  source: (source: string) => request<SourceSummary>('GET', sourcePath(source)),
  createSource: (settings: SourceSettings) => request<Source>('POST', '/api/sources', settings),
  updateSource: (source: string, settings: SourceSettings) =>
    request<Source>('PUT', sourcePath(source), settings),
  deleteSource: (source: string) => request<null>('DELETE', sourcePath(source)),
  names: (source: string, q: string) =>
    request<string[]>('GET', `${sourcePath(source)}/names?${new URLSearchParams({ q })}`),
  events: (source: string, q: string, cursor: string | null) => {
    const parameters = new URLSearchParams({ q })
    if (cursor) {
      parameters.set('cursor', cursor)
    }
    return request<EventPage>('GET', `${sourcePath(source)}/events?${parameters}`)
  },
  symbol: (source: string, name: string) => request<SymbolEvent[]>('GET', symbolPath(source, name)),
  versions: (source: string) => request<Version[]>('GET', `${sourcePath(source)}/versions`),
  changes: (source: string, name: string, path: string | null) =>
    request<Change[]>('GET', `${symbolPath(source, name)}/changes?${pathParameters(path)}`),
  revisions: (source: string, name: string, path: string | null) =>
    request<Revision[]>('GET', `${symbolPath(source, name)}/revisions?${pathParameters(path)}`),
  digs: (source: string) => request<Dig[]>('GET', `${sourcePath(source)}/digs`),
  startDig: (source: string, from: 'new' | 'scratch' = 'new') =>
    request<Dig>('POST', `${sourcePath(source)}/digs?from=${from}`),
  site: () => request<Site>('GET', '/api/site'),
  saveSite: (options: SiteOptions) => request<Site>('PUT', '/api/site', options),
  preview: (url: string, tag_pattern: string) =>
    request<PreviewVersion[]>('POST', '/api/preview', { url, tag_pattern }),
}

export const isDigging = (dig: Dig | null) => dig?.status === 'queued' || dig?.status === 'running'
