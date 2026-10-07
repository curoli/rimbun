# Contributing to Rimbun

Changes should be reviewed through pull requests rather than committed directly to `main`.

1. Start from an up-to-date `main` and create a focused branch such as `feature/short-name` or
   `fix/short-name`.
2. Keep the change scoped, add tests for new behavior, and update user-facing documentation.
3. Run `cargo test --workspace`, strict Clippy for affected crates, and relevant frontend tests or
   builds before pushing.
4. Push the branch and open a pull request against `main`. Describe behavior, migration or
   operational risks, and the verification performed.
5. Address review findings with additional commits. Merge only after approval and successful
   checks; do not force-push after review has started unless necessary and communicated.

Repository administrators should protect `main` in GitHub by requiring a pull request, at least
one approval, and successful required checks before merge.
