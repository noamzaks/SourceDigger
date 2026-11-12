import { readFileSync, writeFileSync } from 'node:fs'
import { Resvg } from '@resvg/resvg-js'

const SIZE = 128
const svg = readFileSync(new URL('../web/public/favicon.svg', import.meta.url), 'utf8')
const png = new Resvg(svg, { fitTo: { mode: 'width', value: SIZE } }).render().asPng()
writeFileSync(new URL('out/icon.png', import.meta.url), png)
