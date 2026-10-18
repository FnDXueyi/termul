#!/usr/bin/env node
/**
 * Nightly audit lever for the termul-routines skill (Pass 3).
 *
 * Deterministic, dependency-free QA of the moving `nightly` prerelease:
 *   1. fetch `latest-nightly.json` and read the version + platform entry
 *   2. download the `termul-server` binary for this platform
 *   3. boot it on a scratch port with a scratch state dir
 *   4. probe /health, the /ws relay (authenticate, ping, store round-trip,
 *      list_persisted_sessions) and the /terminal/ws spawn/write/kill path
 *   5. kill the server, remove the scratch dir
 *
 * Output: human summary on stdout; `--json` emits ONLY the JSON report
 * (verdict: pass|fail) so the skill can archive it as evidence.
 *
 * Usage:
 *   node scripts/routine/nightly-audit.mjs            # human summary
 *   node scripts/routine/nightly-audit.mjs --json     # machine report
 *   ... --keep-server                                 # leave the server up for
 *                                                     # Pass 4 / UI QA; prints
 *                                                     # the port + scratch dir
 *                                                     # + pid on stderr
 */

import { spawn } from 'node:child_process'
import { createHash, randomBytes } from 'node:crypto'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { connect as netConnect } from 'node:net'
import { tmpdir } from 'node:os'
import { arch, platform } from 'node:process'

const RELEASE_BASE = 'https://github.com/gnoviawan/termul/releases/download/nightly'
const MANIFEST_URL = `${RELEASE_BASE}/latest-nightly.json`

// Must match `WS_PROTOCOL_VERSION` in src/shared/types/web-protocol.types.ts.
// The host refuses the authenticate handshake on any other value (AD-15).
const WS_PROTOCOL_VERSION = 7
const BOOT_TIMEOUT_MS = 30_000
const REQUEST_TIMEOUT_MS = 15_000

function parseArgs(argv) {
  const flags = new Set(argv.filter((a) => a.startsWith('--')))
  return { json: flags.has('--json'), keepServer: flags.has('--keep-server') }
}

function platformKey() {
  if (platform === 'linux' && arch === 'x64') return 'linux-x86_64-server'
  if (platform === 'linux' && arch === 'arm64') return 'linux-aarch64-server'
  return null
}

function assert(cond, message) {
  if (!cond) throw new Error(message)
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

async function fetchJson(url) {
  const res = await fetch(url, { redirect: 'follow' })
  if (!res.ok) throw new Error(`GET ${url} → HTTP ${res.status}`)
  return res.json()
}

/**
 * Minimal RFC6455 WebSocket client (text frames, client side). The runtime's
 * global WebSocket is not used on purpose: this lever must behave identically
 * under node and bun with zero dependencies, and owning the raw socket keeps
 * teardown deterministic.
 */
class WsClient {
  constructor() {
    this.buffer = Buffer.alloc(0)
    this.frames = []
    this.waiters = []
    this.closed = false
  }

  connect(url) {
    return new Promise((resolve, reject) => {
      const target = new URL(url)
      const key = randomBytes(16).toString('base64')
      const socket = netConnect({ host: target.hostname, port: Number(target.port) })
      this.socket = socket
      let handshake = Buffer.alloc(0)
      const onHandshake = (chunk) => {
        handshake = Buffer.concat([handshake, chunk])
        const idx = handshake.indexOf('\r\n\r\n')
        if (idx === -1) return
        const head = handshake.subarray(0, idx).toString()
        if (!/^HTTP\/1\.1 101/.test(head)) {
          socket.destroy()
          reject(new Error(`WS handshake rejected: ${head.split('\r\n')[0]}`))
          return
        }
        socket.off('data', onHandshake)
        socket.on('data', (c) => this.onData(c))
        const rest = handshake.subarray(idx + 4)
        if (rest.length > 0) this.onData(rest)
        resolve()
      }
      socket.on('data', onHandshake)
      socket.once('error', (e) => reject(e))
      socket.write(
        `GET ${target.pathname}${target.search} HTTP/1.1\r\n` +
          `Host: ${target.host}\r\n` +
          'Upgrade: websocket\r\nConnection: Upgrade\r\n' +
          `Sec-WebSocket-Key: ${key}\r\n` +
          'Sec-WebSocket-Version: 13\r\n\r\n'
      )
    })
  }

  onData(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk])
    for (;;) {
      const frame = this.readFrame()
      if (!frame) return
      if (frame.opcode === 0x1) {
        this.frames.push(frame.payload.toString('utf8'))
        this.notify()
      } else if (frame.opcode === 0x8) {
        this.closed = true
        this.socket.destroy()
        this.notify()
        return
      } else if (frame.opcode === 0x9) {
        this.writeFrame(0xa, frame.payload)
      }
    }
  }

  readFrame() {
    const b = this.buffer
    if (b.length < 2) return null
    const len = b[1] & 0x7f
    let offset = 2
    let payloadLen = len
    if (len === 126) {
      if (b.length < 4) return null
      payloadLen = b.readUInt16BE(2)
      offset = 4
    } else if (len === 127) {
      if (b.length < 10) return null
      payloadLen = Number(b.readBigUInt64BE(2))
      offset = 10
    }
    const masked = (b[1] & 0x80) !== 0
    const maskLen = masked ? 4 : 0
    if (b.length < offset + maskLen + payloadLen) return null
    let payload = b.subarray(offset + maskLen, offset + maskLen + payloadLen)
    if (masked) {
      const mask = b.subarray(offset, offset + 4)
      const out = Buffer.allocUnsafe(payloadLen)
      for (let i = 0; i < payloadLen; i += 1) out[i] = payload[i] ^ mask[i & 3]
      payload = out
    }
    this.buffer = b.subarray(offset + maskLen + payloadLen)
    return { opcode: b[0] & 0x0f, payload }
  }

  writeFrame(opcode, payload) {
    const mask = randomBytes(4)
    const len = payload.length
    let header
    if (len < 126) {
      header = Buffer.alloc(2)
      header[1] = 0x80 | len
    } else if (len < 65536) {
      header = Buffer.alloc(4)
      header[1] = 0x80 | 126
      header.writeUInt16BE(len, 2)
    } else {
      header = Buffer.alloc(10)
      header[1] = 0x80 | 127
      header.writeBigUInt64BE(BigInt(len), 2)
    }
    header[0] = 0x80 | opcode
    const masked = Buffer.from(payload)
    for (let i = 0; i < masked.length; i += 1) masked[i] ^= mask[i & 3]
    this.socket.write(Buffer.concat([header, mask, masked]))
  }

  sendText(text) {
    this.writeFrame(0x1, Buffer.from(text, 'utf8'))
  }

  notify() {
    const waiters = this.waiters
    this.waiters = []
    for (const w of waiters) w()
  }

  /** Resolve once a new frame has arrived (or immediately if one just did). */
  frameArrival() {
    return new Promise((resolve) => {
      this.waiters.push(resolve)
    })
  }

  /** Send a request and await the reply frame with the same id. */
  async request(type, payload, { timeoutMs = REQUEST_TIMEOUT_MS, commandId } = {}) {
    const id = `audit-${Math.random().toString(36).slice(2, 10)}`
    this.sendText(JSON.stringify({ id, type, payload, ...(commandId ? { commandId } : {}) }))
    const deadline = Date.now() + timeoutMs
    for (;;) {
      const idx = this.frames.findIndex((f) => {
        try {
          return JSON.parse(f).id === id
        } catch {
          return false
        }
      })
      if (idx !== -1) return JSON.parse(this.frames.splice(idx, 1)[0])
      if (this.closed) throw new Error(`WS closed while awaiting reply to ${type}`)
      if (Date.now() > deadline) throw new Error(`timeout awaiting reply to ${type}`)
      await Promise.race([this.frameArrival(), sleep(250)])
    }
  }

  /** Await the first frame matching pred. Frames it does not match stay. */
  async awaitEvent(pred, { timeoutMs = REQUEST_TIMEOUT_MS } = {}) {
    const deadline = Date.now() + timeoutMs
    for (;;) {
      const idx = this.frames.findIndex((f) => {
        try {
          return pred(JSON.parse(f))
        } catch {
          return false
        }
      })
      if (idx !== -1) return JSON.parse(this.frames.splice(idx, 1)[0])
      if (this.closed) throw new Error('WS closed while awaiting event')
      if (Date.now() > deadline) throw new Error('timeout awaiting event')
      await Promise.race([this.frameArrival(), sleep(250)])
    }
  }

  close() {
    if (!this.closed && this.socket) {
      try {
        this.writeFrame(0x8, Buffer.alloc(0))
      } catch {
        /* socket already gone */
      }
      this.socket.destroy()
      this.closed = true
    }
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2))
  const report = {
    script: 'scripts/routine/nightly-audit.mjs',
    startedAt: new Date().toISOString(),
    checks: [],
    errors: []
  }
  const check = (name, detail) => {
    report.checks.push({ name, ok: true, detail })
    if (!args.json) console.log(`ok    ${name} — ${detail}`)
  }
  const fail = (name, error) => {
    const message = String(error?.message ?? error)
    report.checks.push({ name, ok: false, detail: message })
    report.errors.push({ name, message })
    if (!args.json) console.error(`FAIL  ${name} — ${message}`)
  }

  let server = null
  let scratchDir = null
  const port = 18000 + Math.floor(Math.random() * 2000)

  try {
    const manifest = await fetchJson(MANIFEST_URL)
    const version = manifest.version
    assert(
      typeof version === 'string' && /\d+\.\d+\.\d+-nightly\./.test(version),
      `manifest version is not a nightly: ${JSON.stringify(version)}`
    )
    const key = platformKey()
    assert(key, `unsupported audit platform: ${platform}-${arch}`)
    const entry = manifest.platforms?.[key]
    assert(entry?.url, `manifest has no ${key} platform entry`)
    report.manifest = { version, platformKey: key, pubDate: manifest.pub_date }
    check('manifest', `${version} → ${key}`)

    const res = await fetch(entry.url, { redirect: 'follow' })
    assert(res.ok, `GET ${entry.url} → HTTP ${res.status}`)
    const binBuf = Buffer.from(await res.arrayBuffer())
    assert(binBuf.length > 1_000_000, `server binary implausibly small: ${binBuf.length} bytes`)
    const sha256 = createHash('sha256').update(binBuf).digest('hex')
    report.binary = { url: entry.url, bytes: binBuf.length, sha256 }
    check('binary-download', `${binBuf.length} bytes, sha256 ${sha256.slice(0, 16)}…`)

    scratchDir = await mkdtemp(`${tmpdir()}/termul-nightly-audit-`)
    const binPath = `${scratchDir}/termul-server`
    await writeFile(binPath, binBuf, { mode: 0o755 })
    server = spawn(
      binPath,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(port),
        '--state-dir',
        scratchDir,
        '--sessions-dir',
        `${scratchDir}/sessions`,
        '--projects-file',
        `${scratchDir}/projects.json`,
        '--store-file',
        `${scratchDir}/store.json`
      ],
      { stdio: ['ignore', 'pipe', 'pipe'] }
    )
    let serverLog = ''
    server.stdout.on('data', (d) => {
      serverLog += d.toString()
    })
    server.stderr.on('data', (d) => {
      serverLog += d.toString()
    })
    const base = `http://127.0.0.1:${port}`

    let health = null
    const bootDeadline = Date.now() + BOOT_TIMEOUT_MS
    while (health === null && Date.now() < bootDeadline) {
      await sleep(500)
      try {
        const r = await fetch(`${base}/health`)
        if (r.ok) health = await r.json()
      } catch {
        /* not up yet */
      }
    }
    assert(health !== null, `server never became healthy on ${base}\n${serverLog.slice(-2000)}`)
    report.health = health
    check('server-boot', `/health ok on port ${port}: ${JSON.stringify(health).slice(0, 80)}`)

    const ws = new WsClient()
    await ws.connect(`ws://127.0.0.1:${port}/ws`)
    const auth = await ws.request('authenticate', {
      token: 'nightly-audit',
      protocolVersion: WS_PROTOCOL_VERSION,
      deviceId: 'nightly-audit-device',
      clientInstanceId: `nightly-audit-${Date.now()}`
    })
    assert(auth.ok === true, `authenticate reply: ${JSON.stringify(auth)}`)
    check('ws-authenticate', 'ok:true')

    const ping = await ws.request('ping', {})
    assert(ping.ok === true, `ping reply: ${JSON.stringify(ping)}`)
    check('ws-ping', 'ok:true')

    const storeKey = `device/nightly-audit-device/routine:audit:${Date.now()}`
    const storeVal = { probe: true, at: Date.now() }
    const write = await ws.request('store_write', {
      key: storeKey,
      value: storeVal,
      commandId: `audit-store-${Date.now()}`
    })
    assert(write.ok === true, `store_write reply: ${JSON.stringify(write)}`)
    const read = await ws.request('store_read', { key: storeKey })
    assert(read.ok === true, `store_read reply: ${JSON.stringify(read)}`)
    assert(
      JSON.stringify(read.payload?.value) === JSON.stringify(storeVal),
      `store round-trip mismatch: ${JSON.stringify(read.payload)}`
    )
    check('ws-store-roundtrip', `${storeKey} written and read back verbatim`)

    const persisted = await ws.request('list_persisted_sessions', {})
    assert(persisted.ok === true, `list_persisted_sessions reply: ${JSON.stringify(persisted)}`)
    assert(
      Array.isArray(persisted.payload),
      `unexpected list_persisted_sessions payload: ${JSON.stringify(persisted.payload)}`
    )
    check('ws-list-persisted-sessions', `ok, ${persisted.payload.length} session(s)`)

    const term = new WsClient()
    await term.connect(`ws://127.0.0.1:${port}/terminal/ws`)
    // /terminal/ws gates every non-authenticate request behind its own
    // authenticate handshake (same identity fields as /ws).
    const termAuth = await term.request('authenticate', {
      token: 'nightly-audit',
      protocolVersion: WS_PROTOCOL_VERSION,
      deviceId: 'nightly-audit-device',
      clientInstanceId: `nightly-audit-term-${Date.now()}`
    })
    assert(termAuth.success === true, `terminal authenticate reply: ${JSON.stringify(termAuth)}`)
    const MARKER = `AUDIT-${Date.now()}`
    const projectId = `audit-project-${Date.now()}`
    const addProject = await ws.request('add_project', {
      id: projectId,
      name: 'nightly-audit',
      path: scratchDir,
      color: '#2b6cb0'
    })
    assert(addProject.ok === true, `add_project reply: ${JSON.stringify(addProject)}`)
    const spawnReq = await term.request(
      'spawn',
      {
        projectId,
        cwd: scratchDir,
        shell: '/bin/bash'
      },
      { commandId: `audit-term-${Date.now()}` }
    )
    assert(spawnReq.success === true, `terminal spawn reply: ${JSON.stringify(spawnReq)}`)
    const termId = spawnReq.data?.id
    assert(
      termId !== undefined,
      `spawn reply carries no terminal id: ${JSON.stringify(spawnReq.data)}`
    )
    // Output only streams after attach with the claim the spawn issued.
    const attach = await term.request('attach', {
      terminalId: termId,
      claim: spawnReq.data?.claim,
      lastSeq: 0
    })
    assert(attach.success === true, `terminal attach reply: ${JSON.stringify(attach)}`)
    const echo = await term.request(
      'write',
      {
        terminalId: termId,
        data: `echo ${MARKER}\r`
      },
      { commandId: `audit-write-${Date.now()}` }
    )
    assert(echo.success === true, `terminal write reply: ${JSON.stringify(echo)}`)
    // Output frames are {type:'data', terminalId, seq, data:[byte values]}.
    const markerFrame = await term.awaitEvent(
      (f) => Array.isArray(f.data) && String.fromCharCode(...f.data).includes(MARKER),
      { timeoutMs: 10_000 }
    )
    assert(markerFrame !== undefined, 'echo marker never streamed back')
    check('terminal-roundtrip', `echo ${MARKER} streamed back`)
    const kill = await term.request(
      'kill',
      {
        terminalId: termId
      },
      { commandId: `audit-kill-${Date.now()}` }
    )
    assert(kill.success === true, `terminal kill reply: ${JSON.stringify(kill)}`)
    term.close()
    ws.close()
  } catch (e) {
    fail('audit', e)
  } finally {
    if (server !== null && !args.keepServer) {
      server.kill('SIGKILL')
      await sleep(300)
    }
    if (scratchDir !== undefined && !args.keepServer) {
      await rm(scratchDir, { recursive: true, force: true }).catch(() => {})
    }
    if (args.keepServer && server !== null) {
      process.stderr.write(`keep-server: pid=${server.pid} port=${port} scratch=${scratchDir}\n`)
    }
  }

  report.finishedAt = new Date().toISOString()
  report.verdict = report.errors.length === 0 ? 'pass' : 'fail'
  if (args.json) {
    console.log(JSON.stringify(report, null, 2))
  } else {
    const okCount = report.checks.filter((c) => c.ok).length
    console.log(`\nverdict: ${report.verdict} (${okCount}/${report.checks.length} checks ok)`)
  }
  process.exitCode = report.verdict === 'pass' ? 0 : 1
}

main().catch((e) => {
  console.error(e)
  process.exitCode = 1
})
