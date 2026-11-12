export const sourceLink = (source: string) => `/sources/${encodeURIComponent(source)}`

export const settingsLink = (source: string) => `${sourceLink(source)}/settings`

export const symbolLink = (source: string, name: string) =>
  `${sourceLink(source)}/symbols/${encodeURIComponent(name)}`

export const searchLink = (source: string, text: string) =>
  `${sourceLink(source)}?${new URLSearchParams({ q: text })}`
