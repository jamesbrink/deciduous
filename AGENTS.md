# Deciduous - Decision Graph Tooling

Decision graph tooling for AI-assisted development. Track every goal, decision, and outcome. Survive context loss. Query your reasoning.

---

## MANDATORY: Decision Graph Workflow

**THIS IS NOT OPTIONAL. The decision graph is watched live by the user. Every step must be logged IN REAL-TIME, not retroactively.**

### The Core Rule

```
BEFORE you do something → Log what you're ABOUT to do
AFTER it succeeds/fails → Log the outcome
ALWAYS → Sync frequently so the live graph updates
```

### Behavioral Triggers - MUST LOG WHEN:

| Trigger | Log Type | Example |
|---------|----------|---------|
| User asks for a new feature | `goal` | "Add dark mode to UI" |
| You're choosing between approaches | `decision` | "Choose state management approach" |
| You identify multiple ways to do something | `option` (for each) | "Option A: Redux", "Option B: Context" |
| You're about to write/edit code | `action` | "Implementing Redux store" |
| You notice something interesting | `observation` | "Existing code uses hooks pattern" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| You complete a git commit | `action` with `--commit` | Include the commit hash |

### The Loop - Follow This EVERY Time

```
1. USER REQUEST RECEIVED
   ↓
   Log: goal or decision (what are we trying to do?)

2. BEFORE WRITING ANY CODE
   ↓
   Log: action "About to implement X"
   Link: Connect action to its parent goal/decision IMMEDIATELY

3. AFTER EACH SIGNIFICANT CHANGE
   ↓
   Log: outcome "X completed" or observation "Found Y"
   Link: Connect outcome back to its action/goal IMMEDIATELY

4. AUDIT CONNECTIONS
   ↓
   Ask: Does every outcome link to what caused it?
   Ask: Does every action link to why I did it?
   Fix: Any missing connections before continuing

5. BEFORE EVERY GIT PUSH
   ↓
   Run: deciduous sync
   Commit: Include graph-data.json

6. REPEAT - The user is watching the graph live
```

### Quick Commands

```bash
# Log nodes (use -c/--confidence 0-100)
deciduous add goal "Title" -c 90
deciduous add decision "Title" -c 75
deciduous add action "Title" -c 85
deciduous add observation "Title" -c 70
deciduous add outcome "Title" -c 95

# Link nodes
deciduous link FROM_ID TO_ID -r "Reason for connection"
deciduous link 1 2 --edge-type chosen -r "Selected this approach"

# View graph
deciduous nodes           # List all nodes
deciduous nodes -b main   # Filter by branch
deciduous edges           # List all edges
deciduous graph           # Full graph as JSON

# Sync and export
deciduous sync            # Export to .deciduous/web/graph-data.json
```

### Confidence Levels

- **90-100**: Certain, proven, tested
- **70-89**: High confidence, standard approach
- **50-69**: Moderate confidence, some unknowns
- **30-49**: Experimental, might change
- **0-29**: Speculative, likely to revisit

---

## Session Start Checklist

Every new session or after context recovery:

```bash
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected?
deciduous commands        # What happened recently?
git log --oneline -10     # Recent commits
git status                # Current state
```

---

## Multi-User Sync

**Problem**: Multiple users work on the same codebase, each with a local `.deciduous/deciduous.db` (gitignored). How to share decisions?

**Solution**: jj-inspired dual-ID model. Each node has:
- `id` (integer): Local database primary key, different per machine
- `change_id` (UUID): Globally unique, stable across all databases

### Export/Apply Workflow

```bash
# Export your branch's decisions as a patch
deciduous diff export --branch feature-x -o .deciduous/patches/alice-feature.json

# Export specific node IDs
deciduous diff export --nodes 172-188 -o .deciduous/patches/feature.json --author alice

# Apply patches from teammates (idempotent - safe to re-apply)
deciduous diff apply .deciduous/patches/*.json

# Preview what would change
deciduous diff apply --dry-run .deciduous/patches/bob-refactor.json

# Check patch status
deciduous diff status
```

### PR Workflow

1. Create nodes locally while working
2. Export: `deciduous diff export --branch my-feature -o .deciduous/patches/my-feature.json`
3. Commit the patch file (NOT the database)
4. Open PR with patch file included
5. Teammates pull and apply: `deciduous diff apply .deciduous/patches/my-feature.json`
6. **Idempotent**: Same patch applied twice = no duplicates

---

## Graph Integrity - CRITICAL

**Every node MUST be logically connected.** Floating nodes break the graph's value.

### Connection Rules

| Node Type | MUST connect to | Valid orphan? |
|-----------|----------------|---------------|
| `outcome` | The action/goal it resolves | NO |
| `action` | The decision/goal that spawned it | NO |
| `option` | Its parent decision | NO |
| `observation` | Related goal/action/decision | Usually no |
| `decision` | Parent goal (if any) | Sometimes |
| `goal` | Can be a root | YES |

### Find Disconnected Nodes

```bash
deciduous edges | cut -d'>' -f2 | cut -d' ' -f2 | sort -u > /tmp/has_parent.txt
deciduous nodes | tail -n+3 | awk '{print $1}' | while read id; do
  grep -q "^$id$" /tmp/has_parent.txt || echo "CHECK: $id"
done
```

---

## CLI Commands

| Command | Description |
|---------|-------------|
| `deciduous init` | Initialize deciduous in current directory |
| `deciduous add <type> "title"` | Add a node (goal/decision/option/action/outcome/observation) |
| `deciduous link <from> <to>` | Create edge between nodes |
| `deciduous nodes` | List all nodes |
| `deciduous edges` | List all edges |
| `deciduous graph` | Output full graph as JSON |
| `deciduous sync` | Export graph to JSON file |
| `deciduous diff export` | Export nodes as a shareable patch |
| `deciduous diff apply` | Apply patches from teammates |
| `deciduous diff status` | List available patches |

---

## Database Rules

**CRITICAL: NEVER delete the SQLite database (`.deciduous/deciduous.db`)**

The database contains the decision graph. If you need to clear data:
1. `deciduous backup` first
2. Ask the user before any destructive operation

---

## Development Rules

### Pre-Commit Checklist

```bash
cargo test              # All tests pass?
cargo build --release   # Compiles cleanly?
cargo clippy            # No warnings?
```

Only commit if ALL pass.

---

**Live graph**: https://notactuallytreyanastasio.github.io/deciduous/
