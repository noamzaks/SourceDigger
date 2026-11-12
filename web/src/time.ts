const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
]

const relative = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto', style: 'narrow' })

export function ago(timestamp: number): string {
  const elapsed = timestamp - Date.now() / 1000
  for (const [unit, seconds] of UNITS) {
    if (Math.abs(elapsed) >= seconds) {
      return relative.format(Math.round(elapsed / seconds), unit)
    }
  }
  return 'just now'
}

export function date(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString()
}

export function duration(seconds: number): string {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const parts = [
    hours > 0 ? `${hours}h` : '',
    minutes > 0 ? `${minutes}m` : '',
    hours === 0 ? `${seconds % 60}s` : '',
  ]
  return parts.filter(Boolean).join(' ')
}
