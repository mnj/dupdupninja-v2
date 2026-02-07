# Windows Version Parity TODO (Remaining)

Goal: finish parity for `windows/DupdupNinjaWinUI` vs GTK4 by tracking only work that is still open.

Last updated: 2026-02-07

## Remaining Gaps

- Windows runtime validation is still pending (`dotnet`/WinUI build/runtime was not verifiable in this environment).
- Similar matching heuristics need final tuning against desired parity behavior.
- Action safety/robustness needs hardening (batch error handling, edge cases, UX guardrails).
- Replace-with-link policy needs final Windows strategy and UX polish.
- Compare view currently uses snapshot metadata; visual snapshot preview rendering is still missing.
- Packaging and clean-machine verification are not yet complete.

## Open Milestones

## M3 (Finalize Similar Matching)

Scope:
- Finalize similar grouping/scoring behavior and thresholds:
  - verify pHash/dHash/aHash threshold semantics
  - validate confidence output against parity expectations
- Ensure mode UX is coherent when switching exact/similar and filters.

Exit criteria:
- Similar groups and confidence behavior are stable and accepted.
- No regressions when toggling modes/filters on large filesets.

## M4 (Harden Actions / Compare / Properties)

Scope:
- File actions hardening:
  - per-file failure reporting in batch actions
  - safer overwrite/conflict handling for copy/move
  - better constraints/fallback messaging for link replacement
- Compare improvements:
  - add actual snapshot preview rendering (not metadata-only)
  - improve side-by-side compare UX for more than 2 selected files
- Fileset properties UX polish and validation.

Exit criteria:
- Actions are reliable and user-facing failures are actionable.
- Compare view supports practical review with real snapshot previews.

## M5 (Packaging + Validation)

Scope:
- Validate WinUI app build/run on Windows with required toolchain.
- Publish validation for:
  - `win-x64`
  - `win-arm64`
- Verify native `dupdupninja_ffi.dll` resolution on clean machines.
- Add release regression checklist execution steps.

Exit criteria:
- Fresh-machine launch works without manual DLL placement.
- Publish outputs verified for target RIDs.

## Open Issue List

P1:
- P1-2 Replace-with-link strategy finalization (Windows-safe behavior + permission fallback UX).
- P1-3 Compare view visual snapshot preview rendering and multi-select UX polish.
- P1-1 Action robustness pass (batch error handling + conflict dialogs + cancellation semantics).

P2:
- P2-1 Packaging/RID validation (`win-x64`, `win-arm64`).
- P2-2 Drive selection UX hardening and Windows path edge cases.
- P2-3 Performance pass (virtualization, incremental refresh, large filesets).
- P2-4 End-to-end regression checklist and release gate.

## Immediate Next Actions

1. Validate WinUI project build/run on a Windows machine and capture blocking issues.
2. Implement snapshot image preview rendering in Compare view.
3. Harden file action workflows (conflict/error UX + robust DB sync semantics).
4. Complete publish/RID validation and document clean-machine install/run steps.
