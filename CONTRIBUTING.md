# Contributing

Thanks for helping build MBL!

## Workflow
1. Pick a task from [IMPLEMENTATION.md](IMPLEMENTATION.md). Follow the rules in §0.1 and the conventions in §1.
2. Keep each change focused on one task.
3. Run `just check` before pushing. CI runs `just ci`.

## Commits
- Use [Conventional Commits](https://www.conventionalcommits.org/) and include the task ID, for example `feat(scanner): M1.6 incremental scan with quick hash`.
- Every commit must carry a DCO sign-off (`git commit -s`) certifying that you have the right to submit the work under the project license ([developercertificate.org](https://developercertificate.org/)).

## Code style
- Run `cargo fmt` and `cargo clippy -- -D warnings`.
- Library code has no `unwrap`/`expect`, uses `thiserror` errors, and logs with `tracing`.
- Every change needs tests.

## Architecture decisions
Larger decisions are recorded as ADRs in [docs/adr](docs/adr) using the template in IMPLEMENTATION.md Appendix D.
