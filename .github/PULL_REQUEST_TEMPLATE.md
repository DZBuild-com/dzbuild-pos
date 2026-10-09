## What and why

<!-- What does this change, and why? Link the issue: "Closes #123". -->

## How it was tested

<!-- Commands you ran, screens you checked, hardware you used. For money, tax, stock or kridi logic, paste the failing-then-passing test output. -->

- [ ] `pnpm check` passes locally
- [ ] Tests added or updated for the change

## Checklist

- [ ] One topic in this pull request
- [ ] Commits follow [Conventional Commits](https://www.conventionalcommits.org) and are signed off (`git commit -s`, DCO)
- [ ] No hard-coded user-facing text; every new key exists in `ar`, `fr` and `en` (`pnpm i18n`)
- [ ] Layout uses logical CSS only (`ms-`, `pe-`, `start`, `end`) and was checked in Arabic (right to left)
- [ ] Amounts are integer centimes; no `unwrap()` or `expect()` in command paths
- [ ] Database changes are a new migration file (existing migrations untouched)
- [ ] Core flows still work with no network
- [ ] No secrets, personal data or real shop data in the code, tests or screenshots

## Screenshots

<!-- For interface changes: before and after, ideally in Arabic and in French or English. -->
