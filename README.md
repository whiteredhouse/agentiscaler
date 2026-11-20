# agentiscaler

A terminal workspace for coding agents that knows your agentiscale network.

agentiscaler is a fork of [herdr](https://github.com/herdrdev/herdr). It keeps
everything herdr does (panes, tabs and workspaces in one terminal, detach and
reattach, a combined agent list with working, blocked and idle marks, saved
SSH machines, a socket API and plugins) and adds the agents of your
[agentiscale](https://agentiscale.net) network next to the agents in your
panes.

agentiscaler is not affiliated with or endorsed by the herdr project. See
[NOTICE](NOTICE).

## Status

Early. The fork has its own `agentiscaler` binary, configuration and state
directories, and update checks that never reach herdr's servers.

- **Network agents in the agent list (done).** Under "network", below the
  agents in your panes, the sidebar lists every agent in your network, on any
  machine: whether it is idle, busy, waiting for an answer or offline, for how
  long, and which machine offers it. It updates as soon as the node sees a
  change.
- **Tasks from the terminal (in progress).** Give a network agent a task,
  follow it in a pane, answer its questions and cancel it.

Both need [agentiscaled](https://github.com/whiteredhouse/agentiscaled) running
and joined to a network on the same machine. agentiscaler watches it on its
local socket (`agent.sock` in agentiscaled's data directory) and holds no
network credentials itself: the node reads the list from the control service
with its own key. Without agentiscaled the network group stays hidden. For a
node in a data directory other than agentiscaled's default, set
`node_data_dir` under `[agentiscale]` in the configuration file. The network
group is not available on Windows yet.

## Build from source

You need the Rust toolchain named in `rust-toolchain.toml` and
[Zig](https://ziglang.org) 0.16 on your `PATH` (it builds the vendored
terminal emulator).

```bash
cargo build --release
./target/release/agentiscaler
```

## Differences from herdr

- The binary is `agentiscaler`. Configuration lives in
  `~/.config/agentiscaler` and state in `~/.local/state/agentiscaler` (or the
  platform's equivalent), so agentiscaler runs beside a herdr install without
  sharing sessions.
- Update checks look at this repository's releases only, and are off until
  agentiscaler publishes releases. Agent detection rules ship in the binary;
  remote rule updates are off by default.
- Pane environment variables (`HERDR_*`), the socket API and the agent
  integrations are unchanged, so tools and hooks written for herdr work in
  agentiscaler panes.
- Saved SSH machines run a herdr server on the remote side, installed and
  updated the way herdr does it.

Everything else follows upstream. The upstream documentation applies:
[herdr docs](https://github.com/herdrdev/herdr/tree/master/docs/next/website/src/content/docs).

## How this fork tracks upstream

- `master` mirrors herdr's `master` and is never changed here.
- Our changes live on `agentiscale`, the default branch, and land through
  pull requests.
- Upstream releases are merged into `agentiscale`, never rebased.

## License

Apache License 2.0, like herdr. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
