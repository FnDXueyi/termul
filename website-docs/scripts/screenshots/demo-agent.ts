import { randomUUID } from 'node:crypto'

// A scripted ACP agent for documentation screenshots. Replies are canned so
// every capture run shows the same conversation. Prompt markers:
// [ASK] holds the turn on a permission request, [QUESTION] holds it on a
// form question, [BUSY] streams for a minute so later prompts queue,
// [SUBAGENT] hands part of the work to a subagent, [QUICK] gives the default
// reply without a plan.
//
// DEMO_AGENT_PROFILE picks extras without changing the default screenshots:
// "rich" adds more modes, an effort level, a fast mode switch, context usage
// and plain-language triggers for the markers (see PHRASES); "second" is
// another agent with its own model names that never sends a plan.

type Json = string | number | boolean | null | Json[] | { [key: string]: Json }
interface Message {
  id?: number | string
  method?: string
  params?: Record<string, Json>
  result?: Json
}

const write = (msg: Record<string, Json | undefined>): void => {
  process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', ...msg })}\n`)
}
const respond = (id: Message['id'], result: Json): void => write({ id, result })
const update = (sessionId: string, body: Json): void =>
  write({ method: 'session/update', params: { sessionId, update: body } })
const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms))

const promptText = (prompt: Json | undefined): string =>
  Array.isArray(prompt)
    ? prompt.map((b) => (b && typeof b === 'object' && 'text' in b ? String(b.text) : '')).join(' ')
    : ''

async function say(sessionId: string, text: string): Promise<void> {
  for (const piece of text.match(/[\s\S]{1,24}/g) ?? []) {
    update(sessionId, {
      sessionUpdate: 'agent_message_chunk',
      content: { type: 'text', text: piece }
    })
    await sleep(15)
  }
}

function tool(sessionId: string, call: Record<string, Json>): void {
  update(sessionId, { sessionUpdate: 'tool_call', status: 'completed', ...call })
}

const CART_BEFORE = `export function cartTotal(items: CartItem[]): number {
  return items.reduce((sum, item) => sum + item.price * item.quantity, 0)
}`
const CART_AFTER = `export function cartTotal(items: CartItem[], discountPercent = 0): number {
  const subtotal = items.reduce((sum, item) => sum + item.price * item.quantity, 0)
  return subtotal * (1 - discountPercent / 100)
}`

function sendPlan(sessionId: string, last: 'in_progress' | 'completed'): void {
  update(sessionId, {
    sessionUpdate: 'plan',
    entries: [
      { content: 'Read the cart module', priority: 'high', status: 'completed' },
      { content: 'Add a discount parameter to cartTotal', priority: 'high', status: 'completed' },
      { content: 'Add a unit test', priority: 'medium', status: last },
      {
        content: 'Update the roadmap',
        priority: 'low',
        status: last === 'completed' ? 'completed' : 'pending'
      }
    ]
  })
}

async function standardTurn(sessionId: string, withPlan: boolean): Promise<void> {
  update(sessionId, {
    sessionUpdate: 'agent_thought_chunk',
    content: { type: 'text', text: 'The cart total lives in src/cart.ts. I will read it first.' }
  })
  if (withPlan) sendPlan(sessionId, 'in_progress')
  tool(sessionId, {
    toolCallId: `read-${randomUUID().slice(0, 6)}`,
    title: 'Read src/cart.ts',
    kind: 'read',
    locations: [{ path: 'src/cart.ts' }]
  })
  tool(sessionId, {
    toolCallId: `search-${randomUUID().slice(0, 6)}`,
    title: 'Search for "cartTotal"',
    kind: 'search'
  })
  await say(
    sessionId,
    'I found `cartTotal` in `src/cart.ts`. It is used by `checkout.ts` too, so I kept the old call shape working.\n\n'
  )
  tool(sessionId, {
    toolCallId: `edit-${randomUUID().slice(0, 6)}`,
    title: 'Edit src/cart.ts',
    kind: 'edit',
    locations: [{ path: 'src/cart.ts' }],
    content: [{ type: 'diff', path: 'src/cart.ts', oldText: CART_BEFORE, newText: CART_AFTER }]
  })
  tool(sessionId, {
    toolCallId: `run-${randomUUID().slice(0, 6)}`,
    title: 'bun test',
    kind: 'execute',
    rawInput: { command: 'bun test' }
  })
  await say(
    sessionId,
    [
      'Done. Here is what changed:',
      '',
      '- `cartTotal` now takes an optional `discountPercent`.',
      '- Existing callers keep working because the default is `0`.',
      '- All tests pass.',
      '',
      '| File | Change |',
      '| --- | --- |',
      '| `src/cart.ts` | New `discountPercent` parameter |',
      '',
      'Want me to add a discount code field to checkout next?'
    ].join('\n')
  )
  if (withPlan) sendPlan(sessionId, 'completed')
}

const pending = new Map<string, { promptId: Message['id']; sessionId: string }>()

function askPermission(promptId: Message['id'], sessionId: string): void {
  const id = `perm-${randomUUID().slice(0, 6)}`
  pending.set(id, { promptId, sessionId })
  tool(sessionId, {
    toolCallId: `call-${id}`,
    title: 'git push origin feature/discounts',
    kind: 'execute',
    status: 'pending',
    rawInput: { command: 'git push origin feature/discounts' }
  })
  write({
    id,
    method: 'session/request_permission',
    params: {
      sessionId,
      toolCall: {
        toolCallId: `call-${id}`,
        title: 'git push origin feature/discounts',
        kind: 'execute',
        status: 'pending'
      },
      options: [
        { optionId: 'allow', name: 'Allow', kind: 'allow_once' },
        { optionId: 'always', name: 'Always allow', kind: 'allow_always' },
        { optionId: 'reject', name: 'Reject', kind: 'reject_once' }
      ]
    }
  })
}

function askQuestion(promptId: Message['id'], sessionId: string): void {
  const id = `ask-${randomUUID().slice(0, 6)}`
  pending.set(id, { promptId, sessionId })
  write({
    id,
    method: 'elicitation/create',
    params: {
      mode: 'form',
      sessionId,
      message: 'How should discount codes work?',
      requestedSchema: {
        type: 'object',
        required: ['kind'],
        properties: {
          kind: {
            type: 'string',
            title: 'Discount type',
            description: 'Which kind of discount should codes give?',
            oneOf: [
              { const: 'percent', title: 'Percentage off' },
              { const: 'fixed', title: 'Fixed amount off' }
            ]
          }
        }
      }
    }
  })
}

async function subagentTurn(sessionId: string): Promise<void> {
  await say(
    sessionId,
    'I will ask a subagent to review the checkout flow while I read the cart.\n\n'
  )
  tool(sessionId, {
    toolCallId: `task-${randomUUID().slice(0, 6)}`,
    title: 'Review the checkout flow',
    kind: 'other',
    rawInput: {
      subagent_type: 'reviewer',
      description: 'Review the checkout flow',
      prompt:
        'Read src/checkout.ts and list anything that breaks when the cart total has a discount.'
    },
    content: [
      {
        type: 'content',
        content: {
          type: 'text',
          text: '`checkout` sends the total as is, so a discounted total reaches the API unchanged. No other callers depend on the old signature.'
        }
      }
    ]
  })
  await say(
    sessionId,
    'The subagent found no blockers. The checkout already sends the discounted total.'
  )
}

const busy = new Map<string, Message['id']>()

async function busyTurn(id: Message['id'], sessionId: string): Promise<void> {
  busy.set(sessionId, id)
  await say(sessionId, 'Running the full test suite. This takes a while.\n\n')
  for (let i = 1; i <= 60 && busy.get(sessionId) === id; i++) {
    await say(sessionId, `Checked ${i * 4} of 240 tests...\n`)
    await sleep(1000)
  }
  if (busy.get(sessionId) === id) {
    busy.delete(sessionId)
    respond(id, { stopReason: 'end_turn' })
  }
}

const PROFILE = process.env.DEMO_AGENT_PROFILE ?? ''

const MODELS: Json[] =
  PROFILE === 'second'
    ? [
        { value: 'helper-pro', name: 'Helper Pro' },
        { value: 'helper-mini', name: 'Helper Mini' }
      ]
    : [
        { value: 'demo-large', name: 'Demo Large' },
        { value: 'demo-fast', name: 'Demo Fast' }
      ]

const values: Record<string, string> = {
  model: String((MODELS[0] as { value: string }).value),
  effort: 'medium',
  fast_mode: 'false'
}

function configOptions(): Json[] {
  const options: Json[] = [
    {
      id: 'model',
      name: 'Model',
      category: 'model',
      type: 'select',
      currentValue: values.model,
      options: MODELS
    }
  ]
  if (PROFILE !== 'rich') return options
  options.push(
    {
      id: 'effort',
      name: 'Effort',
      category: 'thought_level',
      type: 'select',
      currentValue: values.effort,
      options: [
        { value: 'low', name: 'Low' },
        { value: 'medium', name: 'Medium' },
        { value: 'high', name: 'High' }
      ]
    },
    {
      id: 'fast_mode',
      name: 'Fast mode',
      type: 'boolean',
      currentValue: values.fast_mode === 'true'
    }
  )
  return options
}

const MODES: Json[] =
  PROFILE === 'rich'
    ? [
        { id: 'default', name: 'Default', description: 'Ask before edits and commands' },
        { id: 'plan', name: 'Plan', description: 'Read and plan, no edits' },
        { id: 'acceptEdits', name: 'Accept edits', description: 'Edit files without asking' },
        {
          id: 'bypassPermissions',
          name: 'Bypass permissions',
          description: 'Run everything without asking'
        }
      ]
    : [
        { id: 'default', name: 'Default', description: 'Ask before risky actions' },
        { id: 'plan', name: 'Plan', description: 'Plan first, edit later' },
        { id: 'auto', name: 'Auto', description: 'Run without asking' }
      ]

// Rich-profile prompts that act like a marker, so screenshots don't show it.
const PHRASES: Array<[RegExp, string]> =
  PROFILE === 'rich'
    ? [
        [/^push\b/i, '[ASK]'],
        [/^add discount codes/i, '[QUESTION]'],
        [/full test suite/i, '[BUSY]'],
        [/second opinion/i, '[SUBAGENT]'],
        [/^(fix|try)\b/i, '[QUICK]']
      ]
    : []

const withMarkers = (text: string): string =>
  PHRASES.reduce((out, [phrase, marker]) => (phrase.test(text) ? `${out} ${marker}` : out), text)

const COMMANDS: Json[] = [
  { name: 'review', description: 'Review the uncommitted changes' },
  {
    name: 'test',
    description: 'Run the test suite and fix failures',
    input: { hint: 'test name' }
  },
  {
    name: 'explain',
    description: 'Explain a file or a function',
    input: { hint: 'file or symbol' }
  }
]

function sessionReply(sessionId: string): Json {
  return {
    sessionId,
    modes: { currentModeId: 'default', availableModes: MODES },
    configOptions: configOptions()
  }
}

// The first report is the agent's own startup context; the app measures the
// conversation as growth over it.
const usage = new Map<string, number>()

function reportUsage(sessionId: string, grow: number): void {
  if (PROFILE !== 'rich') return
  const used = (usage.get(sessionId) ?? 18_000) + grow
  usage.set(sessionId, used)
  update(sessionId, {
    sessionUpdate: 'usage_update',
    used,
    size: 200_000,
    cost: { amount: used / 250_000, currency: 'USD' }
  })
}

async function handle(msg: Message): Promise<void> {
  const { id, method } = msg
  const p = msg.params ?? {}
  if (method === undefined) {
    const held = id === undefined ? undefined : pending.get(String(id))
    if (!held) return
    pending.delete(String(id))
    await say(held.sessionId, 'Thanks, carrying on.')
    respond(held.promptId, { stopReason: 'end_turn' })
    return
  }
  switch (method) {
    case 'initialize':
      respond(id, {
        protocolVersion: 1,
        agentCapabilities: { loadSession: true, promptCapabilities: { image: true } },
        authMethods: []
      })
      return
    case 'session/new':
      respond(id, sessionReply(`sess-${randomUUID().slice(0, 8)}`))
      return
    case 'session/load':
    case 'session/resume':
      respond(id, sessionReply(String(p.sessionId)))
      return
    case 'session/set_config_option':
      values[String(p.configId)] = String(p.value)
      respond(id, { configOptions: configOptions() })
      return
    case 'session/set_mode':
      respond(id, {})
      return
    case 'session/prompt': {
      const sessionId = String(p.sessionId)
      const text = withMarkers(promptText(p.prompt))
      // Sent per prompt: a notification sent with the session/new reply can
      // arrive before the client knows the session and get dropped.
      update(sessionId, { sessionUpdate: 'available_commands_update', availableCommands: COMMANDS })
      if (!usage.has(sessionId)) reportUsage(sessionId, 0)
      if (text.includes('[ASK]')) {
        await say(sessionId, 'The branch is ready. I need your OK before pushing.\n\n')
        askPermission(id, sessionId)
        return
      }
      if (text.includes('[QUESTION]')) {
        await say(sessionId, 'One quick question before I start.\n\n')
        askQuestion(id, sessionId)
        return
      }
      if (text.includes('[BUSY]')) {
        void busyTurn(id, sessionId)
        return
      }
      if (text.includes('[SUBAGENT]')) await subagentTurn(sessionId)
      else await standardTurn(sessionId, PROFILE !== 'second' && !text.includes('[QUICK]'))
      reportUsage(sessionId, 52_000)
      respond(id, { stopReason: 'end_turn' })
      return
    }
    case 'session/cancel': {
      const sessionId = String(p.sessionId)
      const turn = busy.get(sessionId)
      if (turn !== undefined) {
        busy.delete(sessionId)
        respond(turn, { stopReason: 'cancelled' })
      }
      for (const [requestId, held] of pending) {
        if (held.sessionId !== sessionId) continue
        pending.delete(requestId)
        respond(held.promptId, { stopReason: 'cancelled' })
      }
      if (id !== undefined) respond(id, {})
      return
    }
    default:
      if (id !== undefined) respond(id, {})
  }
}

let buffer = ''
process.stdin.setEncoding('utf8')
process.stdin.on('data', (chunk: string) => {
  buffer += chunk
  for (let nl = buffer.indexOf('\n'); nl >= 0; nl = buffer.indexOf('\n')) {
    const line = buffer.slice(0, nl).trim()
    buffer = buffer.slice(nl + 1)
    if (line) void handle(JSON.parse(line))
  }
})
// The host owns this process: once its pipes close, nothing can reach the
// agent, so exit instead of letting a busy turn keep writing into the void.
process.stdin.on('end', () => process.exit(0))
process.stdout.on('error', () => process.exit(0))
