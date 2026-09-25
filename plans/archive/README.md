# Planning Archive

Completed, superseded, or abandoned interim plans are retained here for traceability. Archive moves preserve original filenames and subsystem grouping wherever possible.

- `phase-records/` — the pre-transition flat `plans/*.md` phase/roadmap/corrective/closure records (125 files, moved verbatim via `git mv` under ADR-0001). These are historical provenance, not active handoffs.
- `phase-records-README-legacy.md` — the former `plans/README.md` (legacy flat index with the "Active registered handoff" section), preserved verbatim.

Canonical long-term documents (`plans/000`–`003`) and accepted ADRs MUST NOT be archived merely because their initial implementation completed.

Newly completed subsystem milestones move under `archive/<subsystem>/` retaining their `implementation/` or `closure/` relative grouping when they no longer represent active work. The registry records the move.
