# Modularization task list

Rule for every step: `cargo build && cargo test` must pass before you commit and move on. One module per commit. Mechanical moves first, design decisions last.

Verify with `codecrafters test --previous` (not `submit`) — it re-runs every stage you've already passed, which is exactly the regression net a refactor needs.

## Phase 0 — Safety net
- [x] Commit current working state
- [x] Add more unit tests for `tokenize` edge cases — cheap insurance before moving code
- [ ] Confirm a green baseline with `codecrafters test --previous`

## Phase 1 — Zero-design extractions (pure file moves)
- [x] `trie.rs`: `TrieNode` and its impl
- [x] `tokenizer.rs`: `tokenize`, `TokenizerState`, `Backslash` + their tests
- [x] `parser.rs`: `Command`, `ParsedCommand`, `Redirect`, `FileMode`

## Phase 2 — The shared layer
- [x] `env.rs`: `resolve_path`, `BUILTINS`
- [x] `build_exec_db` moved into the engine's constructor (single-phase init — no invalid half-built state)
- [ ] Dedupe: `resolve_path` and `build_exec_db` both contain the same "is a file AND `mode & 0o111`" check. Extract one predicate into `env.rs`
- [ ] Kill the `.clone()` in the constructor by initializing the borrowing field before the owning one

## Phase 3 — Execution
- [x] `exec.rs`: `dispatch_command`, `run_program`, `make_writer`, `make_handle`
- [ ] The `make_writer` boilerplate repeats at the top of every builtin arm. Is there one place it could happen instead of seven?

---

## Phase 4 — Completion engine + line editor

### Decisions already made (don't relitigate)

**The seam is `main.rs:97`, `results.sort()`.** Everything above it computes (→ engine); everything below it renders (→ editor). Lines 41–97 move to `completion.rs`; 99–136 collapse into a three-arm `match` in the editor.

**The engine returns a digested enum, not raw candidates.** Each variant names an editor *action* and carries exactly the data that action consumes:

```
Nothing                                        // bell; no payload — nothing to know
ExpandBuffer { suffix: String, append_space: bool }
ListCandidates { candidates: Vec<String> }
```

- Payload is the **suffix** (candidate minus what's typed), never the full candidate — that's what keeps `completion_prefix` from crossing the boundary. Today the editor slices with `completion_prefix.len()` at lines 105/106/117/118/119; all five uses disappear.
- Single-match and lcp-extends **merge into `ExpandBuffer`**: the editor's action is identical (append text, consult flag, flush). Merge criterion is "does the consumer branch differently," not "are these conceptually distinct."
- `append_space` means *"this token is complete"*, not *"this isn't a directory"* — the directory rule is one input to that judgment. Engine owns the policy; editor obeys blindly.

**Stateless engine.** Pure function of the buffer — no caching candidates across keystrokes. Shipping a candidate list early would go stale the moment the user types a non-tab character, and you'd have invented cache invalidation. The `tab` flag stays in the editor: it's interaction state, not completion state.

**`lcp` is the discriminator**, not just a helper — comparing lcp against the typed prefix is what chooses `ExpandBuffer` vs `ListCandidates` in the multi-candidate case.

**Method vs free function:** only the top-level entry point takes `&self` (it needs the trie). `lcp`, `search_dir`, `run_completer_script` never touch `self` → they stay private free functions.

**Engine internals are two phases:** *gather* (branchy — trie / completer script / `search_dir`, three paths, all producing `Vec<String>`) then *decide* (shared — sort, count, lcp, pick variant). The convergence on one type is why there's no duplication.

### Steps
- [x] Design the result enum
- [ ] Rename `SearchDb` → `Completer` (it holds *sources*, not results — the name caused real confusion)
- [ ] Rename enum fields: `target` means two different things across variants → `suffix` / `candidates`. `MultMatch` → `ListCandidates` (consistent action-naming register)
- [ ] Write the engine method signature: `&self` + two params (the buffer text; the completer registry) → enum
- [ ] Move lines 41–97 into that method
- [ ] `line_editor.rs`: move `read_input`, `prompt`, termios. Rewrite the tab branch as a `match` on the enum
- [ ] Decide `Buffer`'s home. Note: passing `&Buffer` does **not** avoid a module cycle — borrowing changes runtime ownership, not type dependency. Only passing `&str` does. `Buffer` is a per-line local (dies at Enter), so it is *not* `Shell` state
- [ ] Restore raw terminal mode on panic via `Drop` (RAII guard) — it belongs in `line_editor.rs`
- [ ] Boundary check: after the move, `line_editor.rs` must not mention `lcp`, `search_dir`, `run_completer_script`, or `TrieNode`. `main.rs` must not import from `completion` internals

## Phase 5 — State consolidation
- [ ] `Shell` struct owns `complete_db` — neither `exec` nor `completion` can claim it (it's *both* completion config and builtin state), which is the signal that a third party should own it and lend it out
- [ ] Plain `&` / `&mut` are sufficient — `exec` mutates it during dispatch, completion reads it at tab time, never overlapping. Reaching for `Rc<RefCell<_>>` means the ownership design is wrong
- [ ] Rewrite `main.rs` as: build `Shell`, loop { prompt → read → tokenize → parse → dispatch }. Target: under ~50 lines
- [ ] Decide how `exec` gets `pathenv` now that the engine owns it

## Phase 6 — Tighten and verify
- [ ] Make `Completer`'s fields private; `lcp` / `search_dir` / `run_completer_script` private. If anything outside the module still needs them, the boundary didn't land
- [ ] Fix `test_tilde` — it hardcodes `/Users/nashjr/...` and will fail on any machine that isn't yours (including CI)
- [ ] `cargo clippy`
- [ ] `codecrafters test --previous` green
- [ ] Check the dependency graph against the plan: `trie`/`env` at the bottom, `line_editor`/`exec` at the top, no cycles
