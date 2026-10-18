#!/usr/bin/env node
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { cargoFigcraftMcpTag, readPin } from './figcraft-pin.mjs'

const root = resolve(import.meta.dirname, '..', '..')
const pinPath = process.argv[2] ?? resolve(root, 'canvas', 'figcraft.pin')
const cargoPath = process.argv[3] ?? resolve(root, 'src-tauri', 'Cargo.toml')

const pin = readPin(pinPath)
const cargoTag = cargoFigcraftMcpTag(readFileSync(cargoPath, 'utf8'))

if (cargoTag === null) {
  console.log(
    `figcraft pin check skipped: ${cargoPath} has no figcraft-mcp dependency yet (pin tag ${pin.tag})`
  )
} else if (cargoTag !== pin.tag) {
  console.error(
    `figcraft pin mismatch: canvas/figcraft.pin tag is ${pin.tag}, Cargo.toml figcraft-mcp tag is ${cargoTag}`
  )
  process.exit(1)
} else {
  console.log(`figcraft pin matches Cargo.toml figcraft-mcp tag ${pin.tag}`)
}
