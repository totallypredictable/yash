# Modularization task list

Rule for every step: `cargo build && cargo test` must pass before you commit and move on. One module per commit. Mechanical moves first, design decisions last.

## Phase 0 — Safety net
- [ ] Commit current working state
- [ ] Run the codecrafters test suite once to confirm a green baseline
- [ ] Add 2–3 more unit tests for `tokenize` edge cases (quotes, redirects) — cheap insurance before moving code

## Phase 1 — Zero-design extractions (pure file moves)
- [ ] `trie.rs`: move `TrieNode` and its impl. It has no shell knowledge — nothing else should need to change except an import
- [ ] `tokenizer.rs`: move `tokenize`, `TokenizerState`, `Backslash`. Move the tokenize/tilde/backslash tests into this module
- [ ] `parser.rs`: move `Command`, `ParsedCommand`, `Redirect`, `FileMode`. Note what this forces: `FileMode` is currently used by `make_writer` too — decide which module owns it and why

## Phase 2 — The shared layer
- [ ] `path.rs` (or `env.rs`): move `resolve_path`, `build_exec_db`, and the `BUILTINS` const. Both execution and completion will depend on this module — that's the point
- [ ] Check: does `build_exec_db` really belong here, or with the trie/completion? Justify your choice in a code comment

## Phase 3 — Execution
- [ ] `exec.rs`: move `dispatch_command`, `run_program`, `make_writer`, `make_handle`
- [ ] While there: notice the repeated `make_writer` boilerplate at the top of every builtin arm. Is there one place it could happen instead of seven?

## Phase 4 — The design step (completion vs. line editor)
This is the only phase requiring real thought. Don't start it until 1–3 are committed.
- [ ] Decide the interface: what is the *minimal* question the line editor needs answered? (Something like: buffer in → candidates out. What exactly goes in? What comes out?)
- [ ] `completion.rs`: move `lcp`, `search_dir`, `run_completer_script`, the trie usage, `complete_db` handling, and the candidate-finding logic currently inlined in `read_input`'s tab branch
- [ ] `editor.rs`: move `read_input`, `prompt`, termios handling. After the split, this file should contain no calls to `tokenize`'s output logic beyond what the interface provides — if it still parses, the boundary is wrong
- [ ] Restore raw terminal mode on panic too, not just on clean exit (look up RAII / `Drop` for this — it's the idiomatic fix)

## Phase 5 — State consolidation
- [ ] Introduce a `Shell` struct owning `pathenv`, the trie (or completion engine), and `complete_db`
- [ ] Rewrite `main.rs` as: build `Shell`, loop { prompt → read → tokenize → parse → dispatch }. Target: main.rs under ~50 lines
- [ ] Thread state through methods instead of function parameters

## Phase 6 — Tighten and verify
- [ ] Reduce visibility: everything `pub(crate)` or private; only expose what `main.rs` and cross-module calls actually need
- [ ] `cargo clippy` — fix warnings
- [ ] Full codecrafters suite green
- [ ] Diff the module dependency graph against the plan: trie and path at the bottom, editor/exec at the top, no cycles
