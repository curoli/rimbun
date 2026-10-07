## Summary

- Describe the behavior changed by this pull request.

## Risks

- Note data migrations, compatibility concerns, or operational risks. Write `None` when absent.

## Verification

- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `npm run build` in `web/` when frontend code is affected
- [ ] Relevant manual or end-to-end behavior was checked

## Review

- [ ] The change is focused and documented
- [ ] New behavior has automated tests
- [ ] No credentials, generated runtime state, or local configuration are included
