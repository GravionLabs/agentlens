## Checks

- Run `check:changed` once when the work is done. It formats, lints, typechecks and tests the changed files and runs the ratchets.
- A pass is cached: an unchanged tree is not checked again. `--base=<ref>` sets the base for stacked work; `GATE_MAX_WORKERS` limits the workers.
- Never skip it and never commit with `--no-verify`.
- Run it in the foreground with a timeout. Failures are printed in full; success prints nothing.
