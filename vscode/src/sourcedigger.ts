export interface Source {
  name: string
  url: string
}

export interface Place {
  path: string
  line: number
  signature: string
}

export interface Commit {
  hash: string
  summary: string
}

export interface SymbolEvent {
  version: string
  commit: Commit
  kind: string
  action: 'added' | 'removed' | 'modified' | 'signature'
  definition: Place
  previous: Place | null
}

export interface Definition {
  kind: string
  path: string
  line: number
  code: string
}

export interface Revision {
  version: string
  definitions: Definition[]
  files: Record<string, string>
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
  action: 'added' | 'removed' | 'modified'
  files: FileChange[]
}

export class Api {
  readonly base: string

  constructor(base: string) {
    this.base = base.replace(/\/+$/, '')
  }

  sources(): Promise<Source[]> {
    return this.get('/api/sources')
  }

  events(source: string, name: string): Promise<SymbolEvent[]> {
    return this.get(this.symbolPath(source, name))
  }

  revisions(source: string, name: string): Promise<Revision[]> {
    return this.get(`${this.symbolPath(source, name)}/revisions`)
  }

  changes(source: string, name: string): Promise<Change[]> {
    return this.get(`${this.symbolPath(source, name)}/changes`)
  }

  code(source: string, name: string, version: string, path: string): Promise<Revision> {
    const query = new URLSearchParams({ version, path })
    return this.get(`${this.symbolPath(source, name)}/code?${query}`)
  }

  private symbolPath(source: string, name: string): string {
    return `/api/sources/${encodeURIComponent(source)}/symbols/${encodeURIComponent(name)}`
  }

  private async get<T>(path: string): Promise<T> {
    const response = await fetch(`${this.base}${path}`)
    const body = await response.json()
    if (!response.ok) {
      throw new Error(body.error ?? `GET ${path} failed with ${response.status}`)
    }
    return body
  }
}

export function repositoryKey(url: string): string {
  return url
    .trim()
    .toLowerCase()
    .replace(/^[a-z+]+:\/\//, '')
    .replace(/^[^@/]+@/, '')
    .replace(/^([^/:]+):(?!\d)/, '$1/')
    .replace(/\/+$/, '')
    .replace(/\.git$/, '')
}

export function matchingSource(sources: Source[], remotes: string[]): Source | undefined {
  const keys = new Set(remotes.map(repositoryKey))
  return sources.find((source) => keys.has(repositoryKey(source.url)))
}

export function describe(name: string, events: SymbolEvent[]): string {
  const oldestFirst = [...events].reverse()
  const live = new Map<string, SymbolEvent>()
  for (const event of oldestFirst) {
    if (event.previous) {
      live.delete(`${event.kind} ${event.previous.path}`)
    }
    const key = `${event.kind} ${event.definition.path}`
    if (event.action === 'removed') {
      live.delete(key)
    } else {
      live.set(key, event)
    }
  }

  const changed = new Set(
    events
      .filter((event) => event.action === 'modified' || event.action === 'signature')
      .map((event) => event.version),
  )
  const kinds = new Set(events.map((event) => event.kind))
  const facts = [`added in ${oldestFirst[0].version}`, `changed in ${changed.size} versions`]
  if (live.size === 0) {
    facts.push(`removed in ${events[0].version}`)
  }
  const inFileOrder = [...live.values()].sort(
    (left, right) =>
      left.definition.path.localeCompare(right.definition.path) ||
      left.definition.line - right.definition.line,
  )
  const signatures = new Set(inFileOrder.map((event) => event.definition.signature))
  const lines = [`**${name}** · ${[...kinds].join(', ')}`, '', facts.join(' · ')]
  if (signatures.size > 0) {
    lines.push('', '```c', ...signatures, '```')
  }
  return lines.join('\n')
}
