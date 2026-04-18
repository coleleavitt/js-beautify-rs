# Memory System Plan for Oh-My-OpenCode

## TL;DR

Port Claude Code v105's persistent file-based memory system to OMO. Agents save/delete memories across sessions — user preferences, feedback corrections, project context, external references — and relevant memories are injected at session start.

**Deliverables**: Memory store, 3 tools (save/delete/list), injection hook, agent prompt guidance
**Effort**: Medium | **Waves**: 3 | **Critical Path**: Types → Store → Tools → Wire → Tests

## Architecture

```
~/.config/opencode/oh-my-opencode/memory/<sanitized-cwd>/
├── prefers-bun-over-npm.md
├── compliance-driven-rewrite.md
└── bugs-tracked-in-linear.md
```

Each memory file has YAML frontmatter (name, description, type) and a body paragraph. Files are immutable — delete + recreate, never edit. Per-project isolation via sanitized CWD directory name.

## 10 Tasks, 3 Waves

### Wave 1 — Foundation (parallel, start immediately)

- [x] 1. **Types + schema** — `src/features/memory/types.ts`
  MemoryType enum (user/feedback/project/reference), MemoryFile interface, Zod schemas for validation.
  Category: `quick` | Blocks: 2-9 | Blocked by: none

- [x] 2. **Frontmatter parser** — `src/features/memory/parser.ts`
  Parse `---\nyaml\n---\ncontent` from markdown. Extract name/description/type. Handle missing frontmatter, empty body, invalid YAML. Simple regex — no YAML library.
  Category: `quick` | Blocks: 3 | Blocked by: none

- [x] 3. **File store CRUD** — `src/features/memory/store.ts`
  `getMemoryDir(cwd)` → `~/.config/opencode/oh-my-opencode/memory/<sanitized-cwd>/`
  `saveMemory(cwd, name, description, type, content)` → write file with frontmatter
  `deleteMemory(cwd, filename)` → remove file
  `listMemories(cwd)` → parse all `.md` files, return MemoryFile[]
  `loadMemories(cwd)` → return all content for injection
  Atomic writes (write temp, rename). Auto-create directories.
  Category: `unspecified-high` | Blocks: 4-7 | Blocked by: 1, 2

  Commit: `feat(memory): add types, schema, frontmatter parser, and file store`

### Wave 2 — Tools + Hook (parallel, after Wave 1)

- [x] 4. **SaveMemory tool** — `src/tools/memory-save.ts`
  Input: `{ name, description, type, content }`. Calls `saveMemory()`. Returns created filename. Warns on duplicate name.
  Category: `quick` | Blocks: 8 | Blocked by: 1, 3
  Ref: `src/tools/background-task/create-task.ts` for tool pattern

- [x] 5. **DeleteMemory tool** — `src/tools/memory-delete.ts`
  Input: `{ filename }`. Calls `deleteMemory()`. Returns "deleted" or "not found". Graceful if missing.
  Category: `quick` | Blocks: 8 | Blocked by: 1, 3

- [x] 6. **ListMemories tool** — `src/tools/memory-list.ts`
  Input: `{}`. Calls `listMemories()`. Returns formatted table: filename, name, type, description. Shows count.
  Category: `quick` | Blocks: 8 | Blocked by: 1, 3

  Commit: `feat(memory): add SaveMemory, DeleteMemory, ListMemories tools`

- [x] 7. **Memory injection hook** — `src/hooks/memory-injection.ts`
  On `messageHookBefore` (first message per session), load all memories, inject as `<memories><memory name="..." type="...">content</memory></memories>` XML block. Track injected sessions. Export from `src/hooks/index.ts`.
  Category: `unspecified-high` | Blocks: 8 | Blocked by: 1, 3
  Ref: `src/hooks/away-summary.ts` for hook pattern with per-session tracking

  Commit: `feat(memory): add session-start memory injection hook`

### Wave 3 — Integration (after Wave 2)

- [x] 8. **Wire tools into agents**
  Register SaveMemory, DeleteMemory, ListMemories in tool registry. Available to Sisyphus + Sisyphus-Junior. Not gated.
  Category: `quick` | Blocks: 10 | Blocked by: 4, 5, 6, 7

- [x] 9. **Memory guidance prompt** — `buildMemoryGuidanceSection()` in `dynamic-agent-safety-sections.ts`
  When to save (corrections, confirmations, preferences). Memory types. What NOT to save (code patterns, git history, ephemera). Immutability rule. ~15 lines.
  Category: `quick` | Blocks: 10 | Blocked by: 1
  Ref: `~/VulnerabilityResearch/anthropic/claude-code-v105-system-prompts.md` §36

  Commit: `feat(memory): wire tools into agents and add guidance prompt`

- [x] 10. **Tests + build**
  Parser tests (valid/invalid/edge). Store tests (save/delete/list/sanitize-cwd). Full build verification.
  Category: `unspecified-high` | Blocks: none | Blocked by: 8, 9

  Commit: `test(memory): add parser and store tests`

## Key Design Decisions

- **No LLM selection in v1** — load all memories for the project. Selection model is v2.
- **Private only** — no team/shared scope in v1.
- **Immutable files** — delete + recreate, never edit in place.
- **No database** — pure filesystem, no SQLite, no JSON index.
- **Per-project isolation** — sanitized CWD directory prevents cross-project leakage.

## Memory Types

| Type | What | When to save |
|------|------|--------------|
| user | Role, preferences, knowledge | "I'm a data scientist" |
| feedback | Corrections AND confirmations | "don't mock the database", "yes bundled PR was right" |
| project | Work, goals, deadlines | "merge freeze after Thursday" |
| reference | External system pointers | "bugs tracked in Linear INGEST" |

## What NOT to Save
- Code patterns, architecture, file paths (derivable from code)
- Git history (git log/blame authoritative)
- Debugging solutions (fix is in the code)
- Anything in CLAUDE.md/AGENTS.md
- Ephemeral task details

## Source
- `~/VulnerabilityResearch/anthropic/claude-code-v105-system-prompts.md` §36
- v105 bundle lines 426680-426870 (memory tool prompt)
- v105 bundle line 432777 (memory selection prompt)

## Success Criteria
- [x] Agent can save a memory → file on disk
- [x] Agent can delete/list memories
- [x] Next session → memories injected into system prompt
- [x] Per-project isolation works
- [x] `bun test` passes, `bun run build` clean
