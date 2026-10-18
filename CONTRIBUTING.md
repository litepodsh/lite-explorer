# Contributing to Lite Explorer

Thanks for helping out. Bug reports, fixes and feature ideas are all welcome.

Found a security problem? Don't open an issue. Follow [SECURITY.md](SECURITY.md) instead.

## Proposing a feature

Open an issue before you write the code. Describe:

- **The problem.** What are you trying to do, and what gets in the way today?
- **How other file managers handle it**, if any do (Finder, ForkLift, Path Finder, yazi…).
- **What it costs.** Does it add a dependency, slow down startup or listing, or need a
  new permission?

Check [ROADMAP.md](ROADMAP.md) first. **Listing speed is the top priority right now.** A
feature that makes opening or browsing big folders slower is unlikely to be merged, even
if it's useful. Put any trade-off like that in the issue.

Small, focused pull requests get reviewed faster than large ones.

## Setup

You need [Bun](https://bun.sh), a Rust toolchain and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
cd apps/desktop
just i      # install dependencies
just dev    # run the app with hot reload
just        # list every recipe
```

## Before opening a pull request

Run these from `apps/desktop`:

```bash
bun run check   # svelte-check / TypeScript
bun run test    # frontend tests
just lint       # oxlint
just clippy     # Rust lints, warnings are errors
just test       # Rust tests
just fmt        # format TypeScript and Rust
```

Add or update tests for the behavior you change. Pure logic belongs in plain `.ts` or
Rust modules where it can be tested without the app running.

## Code guidelines

- **Match the surrounding code**: its naming, comment density and idioms.
- **Comments explain why**, not what. A comment often explains a limit or a platform quirk.
- **Keep the UI thread and the startup bundle light.**
  - Heavy views and libraries load on demand with a dynamic `import()`, as the preview
    panel and Monaco do.
  - Long work runs in Rust off the main thread. Results stream back through a `Channel`,
    and the work can be cancelled.
- **Big folders are the normal case**: 100,000 entries locally, slow SMB shares remotely.
  - Avoid per-row `stat` calls, full re-listings and quadratic array updates.
  - Listings use `$state.raw`. Change rows by replacing the array.
- **Platform code** goes behind `#[cfg(target_os = "...")]`. Other platforms must still
  build and fail gracefully.

## Commits

Commits follow [Conventional Commits](https://www.conventionalcommits.org/)
(`feat:`, `fix:`, `perf:`, `refactor:`, `docs:`…). The changelog is generated from them
with git-cliff.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
