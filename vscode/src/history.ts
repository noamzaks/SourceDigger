import { diffLines } from 'diff'

import type { Change, FileChange, SymbolEvent } from './sourcedigger'

const CONTEXT_LINES = 3
const TITLES: Record<Change['action'], string> = {
  added: 'Added in',
  removed: 'Removed in',
  modified: 'Changed in',
}

const escape = (text: string) =>
  text.replace(/[&<>"]/g, (character) => `&#${character.charCodeAt(0)};`)

function lines(text: string): string[] {
  return text === '' ? [] : text.replace(/\n$/, '').split('\n')
}

export function diffHtml(before: string, after: string): string {
  const rows: string[] = []
  for (const part of diffLines(before, after)) {
    const kind = part.added ? 'added' : part.removed ? 'removed' : 'same'
    const sign = part.added ? '+' : part.removed ? '-' : ' '
    let shown = lines(part.value)
    if (kind === 'same' && shown.length > 2 * CONTEXT_LINES + 1) {
      const hidden = shown.length - 2 * CONTEXT_LINES
      rows.push(...shown.slice(0, CONTEXT_LINES).map((line) => `<div> ${escape(line)}</div>`))
      rows.push(`<div class="hidden">⋯ ${hidden} unchanged lines</div>`)
      shown = shown.slice(-CONTEXT_LINES)
    }
    rows.push(...shown.map((line) => `<div class="${kind}">${sign}${escape(line)}</div>`))
  }
  return `<pre>${rows.join('')}</pre>`
}

export function historyHtml(
  name: string,
  changes: Change[],
  events: SymbolEvent[],
  diffLink: (change: Change, file: FileChange) => string,
): string {
  const sections = changes.map((change) => {
    const commits = new Map(
      events
        .filter((event) => event.version === change.version)
        .map((event) => [event.commit.hash, event.commit]),
    )
    const commitLines = [...commits.values()].map(
      (commit) =>
        `<p class="commit"><code>${commit.hash.slice(0, 12)}</code> ${escape(commit.summary)}</p>`,
    )
    const files = change.files.map((file) => {
      const path = file.moved_from ? `${file.moved_from} → ${file.path}` : file.path
      return (
        `<p class="file"><code>${escape(path)}</code> <a href="${escape(diffLink(change, file))}">Diff</a></p>` +
        diffHtml(file.before, file.after)
      )
    })
    return `<h2>${TITLES[change.action]} ${escape(change.version)}</h2>${commitLines.join('')}${files.join('')}`
  })
  return `<!doctype html><html><head><meta charset="utf-8"><style>${STYLE}</style></head>
<body><h1>${escape(name)}</h1>${sections.join('')}</body></html>`
}

const STYLE = `
body { font-family: var(--vscode-font-family); color: var(--vscode-foreground); }
h2 { font-size: 1.1em; margin: 1.6em 0 0.4em; }
p { margin: 0.3em 0; }
.commit { color: var(--vscode-descriptionForeground); }
.file code { color: var(--vscode-textLink-foreground); }
pre { font-family: var(--vscode-editor-font-family); font-size: var(--vscode-editor-font-size);
  background: var(--vscode-textCodeBlock-background); padding: 6px 0; overflow-x: auto; }
pre div { padding: 0 8px; white-space: pre; }
.added { background: var(--vscode-diffEditor-insertedLineBackground); }
.removed { background: var(--vscode-diffEditor-removedLineBackground); }
.hidden { color: var(--vscode-descriptionForeground); }
`
