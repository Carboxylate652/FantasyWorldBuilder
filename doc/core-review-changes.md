# Core changes and verification

## Changes

- **Project persistence (`core/src/world.rs`)**: each file is written to a temporary
  file in its destination directory, flushed and atomically replaced. The manifest
  is written last. Missing optional edit files still receive defaults; malformed
  JSON and other I/O errors are now reported. Cached field paths and declared
  lengths are checked before loading. This is atomic per file, not a transaction
  across the whole project directory.
- **Status summaries (`world.rs`)**: large political tables are excluded before
  cloning metadata, avoiding copies that were immediately discarded.
- **Grid and Dijkstra (`grid.rs`, `graph.rs`, stage callers)**: directed edge
  lengths and reverse-edge indices are computed once and reused. Lengths remain
  `f64` to retain the original numerical precision. This trades additional grid
  memory for fewer repeated angle calculations.
- **Climate (`stages/climate.rs`)**: retain the most recent coarse-grid/transfer
  pair using `Arc`; replace per-cell transport allocations with CSR-aligned flat
  weights. A cache regression test checks repeated solves and grid-level changes.
  The polar latitude limit uses `FRAC_PI_2 - 1e-4` to avoid pole singularities and
  Clippy's `approx_constant` error.
- **Culture reach (`stages/cultures.rs`)**: reuse dense distance and heap scratch
  storage between searches; reset only touched province distances.
- **Undo/redo (`api.rs`)**: record only the affected edit component. Use a deque
  to discard the oldest of at most 500 entries without shifting the full history.
  A regression test verifies that undo leaves unrelated layers intact.
- **CLI tests (`cli/tests/smoke.sh`)**: add a repeatable end-to-end test with
  assertions and retained command logs.

## Verification

Validated on Linux with Rust/Clippy 1.99.0 on 2026-10-09:

```sh
cargo test -p worldcore -p worldgen -p fwm-update --all-targets
cargo test -p worldcore --test pipeline coarse_climate_cache_preserves_results_across_level_changes
cargo build -p worldgen
bash cli/tests/smoke.sh "$PWD/target/debug/worldgen" 5
cargo clippy -p worldcore --all-targets
git diff --check
```

The core/update regression suite passed 38 tests, followed by the additional
coarse-climate cache test. CLI checks use grid level 5, seed 7 and sweep seeds
11–12. Determinism and save/load checks compare all 72 generated fields. The
generated world passes validation with no errors or warnings. Exports use
1024×512 pixels and are checked through the province validator and import paths.

Clippy succeeds with 51 pre-existing warnings. Comparing the original `HEAD`
with the modified source under the same toolchain, including a complete run with
`--cap-lints warn`, shows no added warnings; the original `approx_constant` error
and one culture `unnecessary_map_or` warning are removed.

These checks cover the headless CLI and core behavior. They do not test the Tauri
desktop UI, real external AI providers, release downloads, Windows persistence,
or production-scale performance. The changes reduce identifiable allocation and
computation costs; no benchmark-derived speedup is claimed.
