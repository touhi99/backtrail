export type ItemType = 'decision' | 'step' | 'question' | 'turn'

export type ClaimEdge = {
  id: string
  title: string
  body: string
  state: string
  reason: string
}

export type Item = {
  id: string
  type: ItemType
  thread: string
  sub?: string
  title: string
  body: string
  state: string
  origin: string
  tag: string | null
  done_by: string | null
  answer: string | null
  stale: string | null
  review: string | null
  check_how: string | null
  quote: string
  created: string
  updated: string
  superseded_by: string | null
  source_id: number
  chunk_id: number | null
  source_ref: string
  source_title: string
  source_kind: string
  source_ts: string
  by_actor: string | null
  blocked_by?: ClaimEdge[]
  also?: ClaimEdge[]
}

export type ThreadStat = {
  name: string
  area: string
  state: string
  kind: string | null
  summary: string
  sources: number
  first: string
  last: string
  session_first: string | null
  session_last: string | null
  open_questions: number
  open_q: number
  open_s: number
  done_s: number
  parent: string | null
}

export type SourceStat = {
  id: number
  ref: string
  kind: string
  title: string
  project: string
  thread: string
  first_ts: string
  last_ts: string
  nchunks: number
  cited: number
  reviewed: number
}

export type ThreadClaim = {
  id: string
  type: string
  status: string
  speaker: string
  d: string | null
  text: string
  q: string | null
  newer: string | null
}

export type Decided = {
  id: string
  title: string
  review: string | null
  role: string
  source_id: number
  chunk_id: number | null
  day: string
  how: string
  sub?: string
  also?: ClaimEdge[]
}

export type ArtifactLink = {
  id: string
  title: string
  ts: string
  source_id: number | null
  about: string
  url: string
}

export type PullRequest = {
  repo: string
  number: number
  url: string
  ts: string
  source: string
  state: '' | 'MERGED' | 'CLOSED' | 'OPEN'
  merged_on: string
  title: string
  opened_here: number
}

export type ThreadDetail = {
  thread: ThreadStat | null
  children: ThreadStat[]
  items: Item[]
  sources: SourceStat[]
  claims: ThreadClaim[]
  decided: Decided[]
  artifacts: ArtifactLink[]
  prs: PullRequest[]
  agent: { name: string; start: string }
}

export type PendingSource = {
  id: number
  ref: string
  title: string
  thread: string
  last_ts: string
  nchunks: number
}

export type MovedSource = {
  id: number
  ref: string
  title: string
  kind: string
  thread: string
  last_ts: string
}

export type StaleWork = {
  id: string
  title: string
  type: string
  thread: string
  since: string
  why: string
}

export type Today = {
  todo: Item[]
  pending: PendingSource[]
  waiting: Item[]
  asks: Item[]
  suggested: Item[]
  looks: Item[]
  threads: ThreadStat[]
  moved: MovedSource[]
  stale: StaleWork[]
  totals: { sessions: number; notes: number; artifacts: number; chunks: number; items: number }
  date: string
}

export type FocusNote = {
  id: number
  kind: string
  title: string
  gist: string
  outline: { heading: string; chunk_id: number }[]
  cited: number
  last_ts: string
  passages: number
  links: number
}

export type Focus = {
  thread: (ThreadStat & { n: number }) | null
  items: Item[]
  notes: FocusNote[]
  note_edges: { a: number; b: number }[]
  wikilinks_resolved: number
  counts: { kind: string; n: number }[]
  sessions: { id: number; title: string; last_ts: string; nchunks: number }[]
  linked: { name: string; kind: string; w: number; dir: string }[]
  hidden_notes: number
}

export type Chunk = {
  id: number
  locator: string
  role: string
  ts: string
  text: string
}

export type SourcePage = {
  source: SourceStat | null
  chunks: Chunk[]
  items: Item[]
}

export type SourceRaw = {
  kind: string
  text: string
}

export type SearchHit = {
  chunk_id: number
  source_id: number
  ref: string
  kind: string
  title: string
  thread: string
  last_ts: string
  locator: string
  items: { id: string; type: string; title: string; state: string; dead: boolean }[]
  snip: string
  score: number
  thread_state: string
  why: string[]
}

export type SearchResult = {
  results: SearchHit[]
  items: { id: string; type: string; title: string; thread: string; state: string; stale: string | null; review: string | null }[]
  mode: string
  note: string
  embedded: number
  passages: number
}

export type LogEntry = {
  ts: string
  actor: string
  action: string
  target: string
  detail: string
}

export type GraphNode = {
  id: string
  name: string
  kind: string
  aliases: string | null
  claims: number
}

export type KgGraph = {
  nodes: GraphNode[]
  edges: { a: string; b: string; w: number }[]
  none: { n: number }[]
}

export type KgClaim = {
  id: string
  type: string
  text: string
  body: string
  speaker: string
  asserted_at: string
  status: string
  entities: string
}

export type KgEvidence = {
  claim_id: string
  quote: string
  stated: string
  ts: string
  source_id: number
  source: string
  source_title: string
}

export type KgClaims = {
  total: number
  claims: KgClaim[]
  evidence: KgEvidence[]
}

export type LineageClaim = {
  id: string
  type: string
  text: string
  speaker: string
  asserted_at: string
  status: string
  path: string
  quote: string | null
  stated: string
  source_id: number | null
  source_title: string | null
}

export type Suggestion = {
  term: string
  turns: number
  sessions: number
  examples: { date: string; text: string; source_id: number }[]
}

export type KgSuggest = {
  hidden: string[]
  aliases: { alias: string; entity: string; turns: number }[]
  entities: Suggestion[]
}

export type EntityAdded = {
  ok: boolean
  error?: string
  entity?: string
  linked?: number
  unread?: number
  calls_each?: number
}

export type Trail = {
  threads: { name: string; state: string; summary: string }[]
  sources: { id: number; thread: string; kind: string; title: string; d: string }[]
  items: { thread: string; type: string; title: string; d: string }[]
  links: { type: string; text: string; d: string }[]
}

export type Done = {
  ok: boolean
  error?: string
  id?: string
  thread?: string
}

export type DReason = { id: string; title: string; quote: string; source_id: number | null; chunk_id: number | null }
export type DDecision = {
  id: string
  title: string
  thread: string
  state: string
  quote: string
  source_id: number | null
  chunk_id: number | null
  superseded_by: string
  day: string
  subjects: string[]
  outside?: boolean
  because: DReason[]
}
export type DLink = { a: string; b: string; kind: 'changed' | 'restates'; reason: string }
export type DProject = { name: string; state: string; area: string | null; last: string | null; headline: { id: string; title: string; day: string }; turns: number; n: number }
export type DEdge = { a: string; b: string; kind: 'shared' | 'relation' | 'changed'; label: string; w: number; was?: string }
export type DOverview = { projects: DProject[]; edges: DEdge[] }
export type DProjectView = { name: string; decisions: DDecision[]; links: DLink[] }
export type DChain = { focus: string; decisions: DDecision[]; links: DLink[] }

export type Corrections = {
  weeks: { week: string; turns: number; corrections: number; kinds: Record<string, number> }[]
  kinds: { kind: string; n: number }[]
  threads: { thread: string; turns: number; corrections: number }[]
  untagged: number
}

export type CorrectionMessages = {
  n: number
  items: { chunk_id: number; source_id: number; thread: string; kind: string | null; ts: string; text: string }[]
}
