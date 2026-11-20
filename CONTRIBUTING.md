# Contributing to agentiscaler

agentiscaler is the agentiscale fork of [herdr](https://github.com/herdrdev/herdr).

- Report agentiscaler problems, and anything about the agentiscale network
  features, in this repository's issues.
- A bug that also happens in herdr belongs upstream. Report it there; the fix
  reaches agentiscaler with the next upstream merge.
- Pull requests target the `agentiscale` branch. `master` mirrors upstream
  and never takes changes here. PR titles follow Conventional Commits
  (`feat: ...`, `fix: ...`); CI checks this.
- Keep fork changes small and in new files where possible; see the fork rules
  at the top of [AGENTS.md](AGENTS.md).

## Engineering rules (from upstream herdr)

### Understand your code

You must be able to explain what every change does, how it behaves at the edges, what the tests prove, and how it fits Herdr's existing design. Using AI to write code is fine. Submitting code you do not understand is not.

### Align before changing the product

Focused bug fixes that preserve the existing design are good pull request candidates. Features and larger changes to behavior, UI, interaction patterns, persistence, architecture, or product direction require discussion and maintainer approval first.

Herdr is opinionated. Its interaction patterns, layout, mouse behavior, terminology, and technical boundaries are deliberate. A working implementation is not enough when it moves the product in a direction maintainers did not choose.

### Keep the change focused

One pull request should solve one accepted problem. Do not bundle opportunistic cleanup, unrelated refactors, generated documentation, or speculative fixes. Do not bypass failing checks.

Use a lowercase conventional pull request title such as `fix: handle pane focus`.

When the work relates to an issue, put `refs #<issue-number>` in the commit body:

```text
fix: handle pane focus

refs #128
```

Do not use GitHub closing keywords such as `fixes`, `closes`, or `resolves`. Herdr closes released issues after the release is published, not when unreleased code reaches `master`.

### Test the change

Install the repository hook once:

```bash
just install-hooks
```

Before opening or updating a pull request, run:

```bash
just ci
```

The checks must pass. Make sure the tests exercise the reported failure and would fail without the fix.

Maintainers also run `just check`, which includes Windows cross-compilation on
Linux/macOS. Set up that target once:

```bash
cargo install xwin --locked
just setup-windows-cross
```

`xwin` downloads Microsoft's Windows SDK and CRT directly and prompts for license
acceptance. No Windows machine is needed. For noninteractive setup, pass
`just setup-windows-cross --accept-license` only after accepting that license.
The SDK is stored in `~/.local/share/herdr/windows-cross/` and reused across
worktrees; subsequent `just check` and `just windows-lint` runs use it automatically.
An existing SDK can be selected with `LIBGHOSTTY_VT_WINDOWS_LIBC`, pointing to its
Zig libc configuration file. Ordinary native Linux/macOS builds do not need this
setup. Native Windows builds use the SDK installed with Visual Studio Build Tools.

### Handle documentation correctly

For normal code changes, do not edit the root `README.md`, root `CHANGELOG.md`, `docs/preview/`, `docs/versions/`, or release payloads under `distribution/`.

When a user-facing change needs documentation, update the unreleased draft under `docs/next/` or explain what documentation will be needed. Maintainers prepare the next changelog during release review.

## Questions

Open a GitHub Discussion.
