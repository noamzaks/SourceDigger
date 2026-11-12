import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import * as vscode from 'vscode'

import { historyHtml } from './history'
import { Api, describe, matchingSource } from './sourcedigger'

export function activate(context: vscode.ExtensionContext) {
  const digger = new SourceDigger()
  context.subscriptions.push(
    digger.log,
    vscode.languages.registerHoverProvider('c', digger),
    vscode.workspace.registerTextDocumentContentProvider(SourceDigger.SCHEME, digger),
    vscode.commands.registerCommand('sourcedigger.history', (name?: string) =>
      digger.run(() => digger.history(name)),
    ),
    vscode.commands.registerCommand('sourcedigger.compare', (name?: string) =>
      digger.run(() => digger.compare(name)),
    ),
    vscode.commands.registerCommand(
      'sourcedigger.diff',
      (...sides: Parameters<SourceDigger['diff']>) => digger.run(() => digger.diff(...sides)),
    ),
    vscode.workspace.onDidChangeConfiguration((change) => {
      if (change.affectsConfiguration('sourcedigger')) {
        digger.matched.clear()
      }
    }),
  )
}

class SourceDigger implements vscode.HoverProvider, vscode.TextDocumentContentProvider {
  static readonly SCHEME = 'sourcedigger'
  static readonly INCLUDE = /^\s*#\s*include\b/
  readonly log = vscode.window.createOutputChannel('SourceDigger', { log: true })
  readonly matched = new Map<string, Promise<string | undefined>>()

  async provideHover(document: vscode.TextDocument, position: vscode.Position) {
    const range = document.getWordRangeAtPosition(position)
    if (!range || SourceDigger.INCLUDE.test(document.lineAt(position.line).text)) {
      return undefined
    }
    const name = document.getText(range)
    try {
      const source = await this.matchedSource(document)
      if (!source) {
        return undefined
      }
      const events = await this.api().events(source, name)
      if (events.length === 0) {
        return undefined
      }
      const argument = encodeURIComponent(JSON.stringify([name]))
      const markdown = new vscode.MarkdownString(
        `${describe(name, events)}\n\n[History](command:sourcedigger.history?${argument}) · ` +
          `[Compare](command:sourcedigger.compare?${argument})`,
      )
      markdown.isTrusted = { enabledCommands: ['sourcedigger.history', 'sourcedigger.compare'] }
      return new vscode.Hover(markdown, range)
    } catch (error) {
      this.log.error(`hovering ${name}: ${(error as Error).message}`)
      return undefined
    }
  }

  async provideTextDocumentContent(uri: vscode.Uri): Promise<string> {
    const { source, name, version, path } = JSON.parse(uri.query)
    if (version === null) {
      return ''
    }
    const revision = await this.api().code(source, name, version, path)
    return revision.files[path] ?? ''
  }

  async run(action: () => Promise<void>) {
    try {
      await action()
    } catch (error) {
      vscode.window.showErrorMessage((error as Error).message)
    }
  }

  async history(name: string | undefined) {
    const symbol = await this.symbol(name)
    if (!symbol) {
      return
    }
    const [changes, events] = await Promise.all([
      this.api().changes(symbol.source, symbol.name),
      this.api().events(symbol.source, symbol.name),
    ])
    const panel = vscode.window.createWebviewPanel(
      'sourcedigger.history',
      `${symbol.name} history`,
      vscode.ViewColumn.Beside,
      { enableCommandUris: ['sourcedigger.diff'] },
    )
    panel.webview.html = historyHtml(symbol.name, changes, events, (change, file) => {
      const sides = [
        symbol.source,
        symbol.name,
        change.previous_version,
        file.moved_from ?? file.path,
        change.version,
        file.path,
      ]
      return `command:sourcedigger.diff?${encodeURIComponent(JSON.stringify(sides))}`
    })
  }

  async compare(name: string | undefined) {
    const symbol = await this.symbol(name)
    if (!symbol) {
      return
    }
    const revisions = await this.api().revisions(symbol.source, symbol.name)
    const latest = revisions.at(-1)
    if (!latest || revisions.length === 1) {
      throw new Error(`${symbol.name} has no earlier code to compare`)
    }
    const picked = await vscode.window.showQuickPick(
      revisions
        .slice(0, -1)
        .reverse()
        .map((revision) => ({
          label: revision.version,
          description: revision.definitions.length === 0 ? 'removed' : undefined,
          revision,
        })),
      { placeHolder: `Compare ${symbol.name} in a version with ${latest.version}` },
    )
    if (!picked) {
      return
    }
    const paths = [
      ...new Set([...Object.keys(picked.revision.files), ...Object.keys(latest.files)]),
    ].filter((path) => picked.revision.files[path] !== latest.files[path])
    const path =
      paths.length === 1
        ? paths[0]
        : await vscode.window.showQuickPick(paths.sort(), {
            placeHolder: `${symbol.name} changed in ${paths.length} files; which one?`,
          })
    if (path === undefined) {
      return
    }
    await this.diff(symbol.source, symbol.name, picked.label, path, latest.version, path)
  }

  async diff(
    source: string,
    name: string,
    beforeVersion: string | null,
    beforePath: string,
    afterVersion: string,
    afterPath: string,
  ) {
    const uri = (version: string | null, path: string) =>
      vscode.Uri.from({
        scheme: SourceDigger.SCHEME,
        path: `/${path}`,
        query: JSON.stringify({ source, name, version, path }),
      })
    await vscode.commands.executeCommand(
      'vscode.diff',
      uri(beforeVersion, beforePath),
      uri(afterVersion, afterPath),
      `${name} · ${beforePath === afterPath ? afterPath : `${beforePath} → ${afterPath}`}  ` +
        `${beforeVersion ?? 'before'} ↔ ${afterVersion}`,
    )
  }

  private api(): Api {
    return new Api(vscode.workspace.getConfiguration('sourcedigger').get('url', ''))
  }

  private async symbol(name: string | undefined) {
    const editor = vscode.window.activeTextEditor
    if (!editor) {
      throw new Error('Open a C file first')
    }
    const range = editor.document.getWordRangeAtPosition(editor.selection.active)
    const symbol = name ?? (range && editor.document.getText(range))
    if (!symbol) {
      throw new Error('Put the cursor on a symbol first')
    }
    const source = await this.chosenSource(editor.document)
    return source && { source, name: symbol }
  }

  private matchedSource(document: vscode.TextDocument): Promise<string | undefined> {
    const folder = vscode.workspace.getWorkspaceFolder(document.uri)
    const key = folder?.uri.toString() ?? ''
    if (!this.matched.has(key)) {
      const found = this.findSource(folder)
      found.catch(() => this.matched.delete(key))
      this.matched.set(key, found)
    }
    return this.matched.get(key)!
  }

  private async findSource(folder: vscode.WorkspaceFolder | undefined) {
    const configured = vscode.workspace.getConfiguration('sourcedigger', folder).get('source', '')
    if (configured) {
      return configured
    }
    if (!folder) {
      return undefined
    }
    const sources = await this.api().sources()
    return matchingSource(sources, await this.remotes(folder))?.name
  }

  private async remotes(folder: vscode.WorkspaceFolder): Promise<string[]> {
    try {
      const { stdout } = await promisify(execFile)('git', ['-C', folder.uri.fsPath, 'remote', '-v'])
      return stdout.split('\n').map((line) => line.split(/\s+/)[1] ?? '')
    } catch (error) {
      this.log.info(`no git remotes in ${folder.uri.fsPath}: ${(error as Error).message}`)
      return []
    }
  }

  private async chosenSource(document: vscode.TextDocument): Promise<string | undefined> {
    const matched = await this.matchedSource(document)
    if (matched) {
      return matched
    }
    const sources = await this.api().sources()
    const picked = await vscode.window.showQuickPick(
      sources.map((source) => ({ label: source.name, description: source.url })),
      { placeHolder: 'No source matches this repository; which one is it?' },
    )
    if (!picked) {
      return undefined
    }
    const folder = vscode.workspace.getWorkspaceFolder(document.uri)
    await vscode.workspace
      .getConfiguration('sourcedigger', folder)
      .update(
        'source',
        picked.label,
        folder ? vscode.ConfigurationTarget.WorkspaceFolder : vscode.ConfigurationTarget.Global,
      )
    return picked.label
  }
}
