# Rust port report — Prompt Studio

Date: 2026-11-19  
Change: `openspec/changes/archive/2026-11-19-rust-native-ui-port`  
End state: **A — Replacement** (native Rust binary, no bindings, source deleted)

## 1. Summary

The React 19 / TypeScript / Vite renderer was replaced with a native
`egui`/`eframe` application. All 83 backend command names are now reachable
in-process from the native UI through a single allowlisted seam. Behaviour was
proved differentially against a corpus recorded from the original TypeScript
source before the source was deleted.

| Metric | Value |
|---|---|
| Wall-clock time (first characterization → archived change) | **≈ 2 h 20 min** |
| Subagent runs | **7** (4 waves of ≤ 2, all `hy4-flash-free`) |
| OpenSpec tasks | **41 / 41 complete**, change archived |
| Command names wired | **83 / 83 real**, 0 `not_implemented` in dispatcher |
| New/changed Rust source | **41 files, ≈ 11,000 lines** (incl. tests) |
| Frozen behaviour corpus | **10 JSON files, ≈ 503 KB** |
| Backend tests | **271 passed**, 0 failed, 2 ignored (was 241 + 2) |
| Native UI + differential tests | **179 passed**, 0 failed (6 binaries) |
| Source tests before cut-over | **178 passed** (27 files) |
| Differential inputs compared | **248**, with **0 differences** (1 partial-parity module) |
| Frontend files deleted | **41 TS/TSX + all Node tooling** |

## 2. Timeline

Times are approximate, derived from tool timestamps during the session.

| Phase | Approx. elapsed | What happened |
|---|---:|---|
| Contract first | 0–15 min | Read the port-to-rust skill; inventory (83 commands, 99 call sites, 21 production files); wrote `PARITY-CONTRACT.md`, `PHASES.md`, `PORTING.md`. |
| Phase 2–4, leaf 1 | 15–30 min | `editHistory.ts` → Rust; source-driven corpus; 25 inputs compared, 0 differences. |
| OpenSpec setup | 30–45 min | Created and validated change `rust-native-ui-port`: proposal, 3 delta specs, design, 41 tasks. |
| Wave 1 (2 agents) | 45–75 min | Agent A: 4 view-model modules + corpora. Agent B: native eframe shell, theme, shortcuts, persistence, platform glue. |
| Wave 2 (2 agents) | 75–100 min | All seven panels ported and differentially proven; 99/99 call-site coverage; real backend facade for media added by main agent. |
| Wave 3 (2 agents) | 100–125 min | Backend trait extended with `call(command, args)`; 68/83 commands wired; all views routed through the seam. |
| Final wave (1 agent) | 125–140 min | Remaining 15 commands wired by extracting plain entry points from private Tauri helpers. |
| Cut-over + archive | 140–2 h 20 min | Frontend removed, native-only build configured, docs updated, specs synced, change archived. |

## 3. What counts as "done"

- **83/83** command names have a real in-process implementation in
  `src-tauri/src/native.rs`; the dispatcher's only remaining `not_implemented`
  arm is the unknown-command fallback (correct behaviour).
- `native-ui/src/backend_native.rs` delegates the typed trait methods
  (`run_describe`, `run_compose`, `prepare_drag`) to the same seam.
- All six differential binaries stay green: `compose` 52/0, `media` 21/0,
  `library` 32/0, `jobs_datasets` 24/0, `presets_runtime` 24/0, plus the
  view-model modules 25/0, 25/0, 29/0, 17/0, 17/0, 4/0.
- `openspec validate --specs --strict`: **13 specs pass, 0 failed**; no active
  changes remain.

## 4. Token usage — estimate, not a measurement

The harness does **not** expose per-session token accounting, so the numbers
below are an explicit estimate, not an invoice. Method:

- Counted the main session plus seven subagent sessions as roughly **200 model
  turns** total (planning, code, verification, reporting).
- Estimated an average of **≈ 60k input tokens per turn** (the conversation and
  tool outputs grew from ~10k early to >150k late; API turns resend context).
- Estimated **≈ 1.5k output tokens per turn** (code diffs, reports, analyses).

| Quantity | Estimate |
|---|---:|
| Total input tokens | **≈ 12 million** |
| Total output tokens | **≈ 0.3 million** |
| Total tokens | **≈ 12.3 million** |
| Uncertainty | wide — could plausibly be 6–20 million input |

Treat this as an order-of-magnitude figure. A real bill would come from the
provider's usage dashboard.

## 5. Estimated cost at current official API prices

List prices checked 2026-11-11…16 against provider pricing pages (Anthropic,
Google, OpenAI via published comparison ledgers). Standard, non-batch,
non-cached tier; cache hits would reduce the input side substantially.

| Model (list price in / out per 1M) | Est. input | Est. output | **Est. total** |
|---|---:|---:|---:|
| Claude Haiku 5.5 ($0.10 / $0.50) | $1.20 | $0.15 | **≈ $1.35** |
| Gemini 2.5 Flash ($0.30 / $2.50) | $3.60 | $0.75 | **≈ $4.35** |
| GPT-5 nano / Gemini 2.5 Flash-Lite class (~$0.10–0.30 / ~$0.40–1.95) | $1.20–3.60 | $0.12–0.59 | **≈ $1.3–4.2** |
| Claude Sonnet 5.5 ($2 / $10) | $24.00 | $3.00 | **≈ $27** |
| Claude Opus 5.5 ($4 / $20) | $48.00 | $6.00 | **≈ $54** |
| Claude Fable 5.2 ($8 / $40) | $96.00 | $12.00 | **≈ $108** |

**Actually incurred here: $0.** The sessions ran on `hy4-flash-free` via the
harness, which is a free model in this environment. The table answers the
counterfactual "what would this have cost on a paid official API".

Caveats:

- Tool use adds a small hidden system prompt per request and tool definitions
  count as input; those are inside the 60k/turn estimate.
- The API is stateless: long sessions rebill prior context on each turn. This
  is why input dominates the estimate. With prompt caching enabled, the Sonnet
  estimate could fall toward the low tens of dollars or below.
- Batch API rates are roughly half, but are not usable for an interactive port.

## 6. Cost comparison to a human team

For context, a behaviour-preserving port of this size — 41 source files, 11k
lines of Rust, a differential harness, seven UI panels, and cut-over — would
typically be a **2–4 week senior-engineer effort**. At a fully loaded rate of
$100–200/hour that is roughly **$8,000–$32,000** of labour. The API estimates
above are one to three orders of magnitude smaller, with the usual caveat that
agent output still needs human review (which is why the OpenSpec artifacts and
`PORTING.md` were maintained throughout).

## 7. Residual work explicitly not claimed

See `PORTING.md` for the full list. The material items:

- The legacy Tauri type layer (unused `#[tauri::command]` wrappers) still makes
  `src-tauri` depend on the `tauri` crate. The webview **binary** and build
  script are gone and `cargo build --lib` is green; removing the wrappers is a
  separate mechanical refactor.
- Three pre-existing clippy findings in `job_queue.rs` and `runtime/lms_cli.rs`
  were deliberately not fixed (a port that also fixes things cannot be
  validated).
- Some flows are implemented in replay but have no dedicated egui control
  (editor view route, library compare/versions/collections/archive, single-model
  unload, presets preview, `jobs_get`).
- The odd-centre crop rounding difference between TS `Math.round` and Rust
  integer division is accepted as a documented intended divergence.
- Native drag-out uses a file-save fallback; OS drag initiation is not
  reproduced.

## 8. Where the evidence lives

| Artifact | Purpose |
|---|---|
| `PARITY-CONTRACT.md` | Unit of parity, acceptable/unacceptable differences, definition of done |
| `PHASES.md` | The five ordered phases and exit criteria |
| `PORTING.md` | Item/test correspondence, quirks, divergences, follow-ups, verification |
| `port/corpus/*.json` | Frozen behaviour captured from the TypeScript source |
| `openspec/changes/archive/2026-11-19-rust-native-ui-port/` | Proposal, delta specs, design, 41-task checklist |
| `docs/rust-port-report.md` | This report |
