# Services

A Hangar kit: a **Services** tab that answers « what is running on this computer, on which port, and whose
is it » without a terminal. Workers each get their own block of 20 ports, so leftovers pile up: a dev server
whose worker is long gone, a container nobody remembers.

## What it does

- **Ports.** Every TCP port something listens on, as a row: the port, the program, its pid, when it started,
  and whether it is reachable from the network or from this computer only. A port in the pool workers draw
  from (`HANGAR_PORT_BASE`, 21000 and up, 20 ports each) says so, and when the program was started in a
  worker's pane it names the worker, or says that its pane is closed: that is a leftover. A filter shows the
  worker ports alone.
- **Containers.** Every Docker container, running or not, with its state, image and published ports, and the
  port block those ports sit in. « Logs » opens the last 200 lines in a log block that opens on its first error.
- **Refresh.** On demand, and every 10 seconds while the tab is shown (not while it is behind another).
- **Ask an agent to stop it.** « Ask an agent to stop this » under a port or a container opens a question:
  you pick the pane, read the brief, and send it. The agent is told to check that the thing is its own before
  stopping it, and to say what it stopped. **The kit itself stops nothing**: no kill, no `docker stop`. It is not
  offered for a program of the system or of another account, nor when the kit cannot tell whose a program is.
- **States.** Docker not installed, Docker not running, nothing listening, no container, a command not accepted,
  Node or the system's listing tool missing: each says what it is and what to do.
- **A skill** telling agents to open the tab with `kit_tab_open` instead of guessing with `netstat`, and never
  to stop what they did not start.

The tab is **native**: `tabs/services.wasm`, a Rust module that describes its interface and that Hangar draws
itself, in your theme, language (English and French) and interface size, and that holds at the side panel's
narrowest. No browser engine runs for it; it needs Hangar 2.0 or later. What it does not do, and why, is under
[What the native tab does not do](#what-the-native-tab-does-not-do).

## What it runs

Only these three, declared in `hangar-kit.json` and shown to you verbatim before you enable the kit. All of
them only read. The tab runs them with your own account; only `{id}` changes, and it must be 12 to 64 lowercase
hexadecimal characters.

| Name          | Command                                  |
| ------------- | ---------------------------------------- |
| `ports`       | `node {kitDir}/tools/ports.js`           |
| `docker-list` | `docker ps -a --format json`             |
| `docker-logs` | `docker logs --tail 200 {id}`            |

`tools/ports.js` is this kit's own script, and is the one thing to read before you accept. It asks the
system for the listening ports and nothing else:

| System  | What it runs                                                                                           |
| ------- | ------------------------------------------------------------------------------------------------------ |
| Windows | one `powershell.exe` call: `Get-NetTCPConnection -State Listen` and `Get-CimInstance Win32_Process` (name, start time) |
| macOS   | `lsof -nP -iTCP -sTCP:LISTEN -F pcn`, `ps -o pid=,lstart=`, and `ps eww -p <pid>` for each program       |
| Linux   | `ss -H -ltnp` (or the same `lsof`), `ps -o pid=,lstart=`, and `/proc/<pid>/environ` for each program    |

**Whose it is.** To name a worker, `ports.js` reads the environment of each listening program on macOS and
Linux (`ps eww`, `/proc/<pid>/environ`), and keeps **only** `HANGAR_PORT_BASE` and `HANGAR_PANE_NAME`, the two
variables Hangar gives a pane. Nothing else of an environment is kept or printed, and a program of another
account is not readable anyway. **Windows does not allow that without reading another program's memory**, which
this kit does not do: there a port in a worker's block is shown as « Port block … » with its range, without
the worker's name, until Hangar tells tabs which block is whose.

The tab also asks Hangar for the workspace's panes (`panes.read`), to tell whether a worker's pane is still
open and to offer them in the question, and sends a message (`panes.send`) only when you press « Send ».
Nothing leaves the machine.

## Install

You need Hangar 2.0.0 or later and `node` on your PATH. `docker` is optional: without it, the containers
part says so and the ports still show.

1. In Hangar, Settings › Kits, add the GitHub link:
   `https://github.com/hangar-ai/kit-services`
   (or link a local copy of this folder).
2. Enable it on a workspace, and read what you accept: the three commands and the two pane permissions.
3. Restart the workspace's Claude Code panes so they load the skill.

There is nothing to build: Hangar runs the module the kit ships (`tabs/services.wasm`, committed) and never builds one.

## How it is built

```
services/
├── hangar-kit.json            the kit: id, hangar >=2.0.0, schema 2, the three commands, the tab (entry: the module)
├── icon.svg
├── .claude-plugin/plugin.json the Claude Code plugin (name = the kit's id): a skill, nothing that runs by itself
├── skills/services/SKILL.md
├── tools/
│   ├── ports.js               the `ports` command: runs the system's tool, prints one line of JSON
│   └── parse.js               pure readers of PowerShell's JSON, ss, lsof, ps and an environment
├── tabs/services.wasm         the native tab, built from native/ and committed: what Hangar runs
├── native/                    the native tab: a Rust crate (cdylib + rlib) on hangar-kit-sdk
│   ├── Cargo.toml, Cargo.lock, rust-toolchain.toml   the SDK from git, the lockfile, the compiler that is pinned
│   ├── src/logic.rs           pure: the tools' text → rows, whose a port is, the brief, the words
│   ├── src/lib.rs             the state, the three reads, the page
│   └── tests/                 cargo test, on the SDK's harness; they read test/fixtures/
├── scripts/build-native.sh    builds tabs/services.wasm (--check: fails if the committed one is not that build)
├── test/                      node --test, no dependency: the tools and the manifest; fixtures/ are the tools' output, scrubbed
└── package.json, run-tests.js let `node --test kits/services/` find the tests on any Node
```

- **A redraw changes only what changed.** The tab draws its whole view each time, and what is open is part of its
  state, which a read leaves as it is; Hangar keeps the scroll of a log and the focus of a field.
- **Declared commands, not a shell.** Each is an argv. `{id}` is the only placeholder, with a strict rule,
  and Hangar refuses a value starting with `-`.
- **The kit cannot stop anything.** The commands are reads, `ports.js` has no way to kill, and a test fails if
  the manifest or the script gains one.

### The native tab

`native/` is a Rust crate that builds to `wasm32-unknown-unknown` (never `wasm32-wasip1`, no wasm-bindgen) and depends
on [`hangar-kit-sdk`](https://github.com/hangar-ai/hangar/tree/feat/native-rust/crates/hangar-kit-sdk). The tab makes
these host calls and holds these permissions, and nothing else: `command.run` of the three declared commands, `panes.list`
and `panes.send`, and, for the log's Copy button, `ui.copy` and a toast, which ask for no permission. A read is three
calls that go out together; the page changes once they are all back, and the next read is armed 10 seconds after,
only while the tab is in front. Every string is in `native/src/logic.rs`, in English and in French.

Hangar never builds a kit, so the module is committed with the source that made it:

```
rustup target add wasm32-unknown-unknown      # once, unless rustup does it for you (native/rust-toolchain.toml)
scripts/build-native.sh                       # native/ → tabs/services.wasm
scripts/build-native.sh --check               # build, and fail if tabs/services.wasm is not that build
```

The compiler is pinned in `native/rust-toolchain.toml`, the lockfile is held to, and the build names neither the
kit's folder nor Cargo's: the same source gives the same module in any folder, with any `CARGO_HOME`, which is what
lets CI tell a stale one. Change the source or the compiler, rebuild, commit both. To look at it: link this folder in
Hangar (Settings › Kits) and reload the tab from its bar after each build, or, without Hangar,
`cargo run -p hangar-kit-host --example preview -- tabs/services.wasm` from the Hangar repository (it does not run
commands, so the tab shows its refusals there).

### What the native tab does not do

What the native tab does differently from a web page, or cannot do, and why:

- **Text cannot be selected with the pointer** (iced's text has no selection). The log has a Copy button, in
  its bar; the brief is shown to be read and is sent as it is.
- **No accessibility roles** (`role=alert`, `aria-*`): iced has no accessibility tree yet. Icons and glyphs
  have tooltips naming them.
- **A line of a log is not wrapped.** A log keeps its lines whole so that a long one stays cheap; it scrolls
  sideways for a longer line, and its height is fixed at 320 px.
- **No client-side timeout on a call.** Only a command's own limit applies (30 s for `ports`, 20 s for Docker), and it says `timedOut`, which is shown as "It took too long".
- **Send waits for a pane.** The button is disabled until one is picked.
- **A filter with nothing to filter by shows everything.** After the last worker port goes, the list is not left
  empty under a "Worker ports" filter that is no longer offered.
- **The brief is plain text.** Hangar's typography (a no-break space between a number and its unit, a narrow one
  before a French colon) is left out of what an agent is sent.

## Tests

```
cargo test --manifest-path native/Cargo.toml      # the native tab
node --test kits/services/                        # the tools and the manifest
```

The Rust tests run on the SDK's `testing::Harness` (the module's own runtime, on the host, answering the tab's calls
by hand), on each system's output as recorded (`test/fixtures/README.md` says which is recorded and which is written
from the tool's documented format): `tests/logic.rs`, `tests/controller.rs` (what is read, when, what each refusal
means, the question, which calls are made and that each is granted), `tests/view.rs` (what each state draws, in both
languages) and `tests/manifest.rs` (`entry` names an existing `.wasm` inside the kit, and it imports Hangar's `log`
and `now` and nothing else).

The Node tests have no install step. They cover the `ports` script with a stand-in for the system so no real process
is asked for anything, and the manifest against the rules in
[docs/kits.md](https://github.com/hangar-ai/hangar/blob/main/docs/kits.md) and
[docs/native/kits-native.md](https://github.com/hangar-ai/hangar/blob/main/docs/native/kits-native.md), whose validator is Rust.

## This repository

`node --test` runs the tests, with Node alone. Hangar's own docs are not in this repository. CI takes them from the Hangar tag the
manifest's `hangar` names, and `node test/hangar-ui.js --get` does the same here. Without them, this test
skips and says so:

- the permissions and methods used are in the docs, when the docs are around

CI calls [kit-check](https://github.com/hangar-ai/.github/blob/main/.github/workflows/kit-check.yml), and has a job of
its own for the native tab: format, lints, the Rust tests, and `scripts/build-native.sh --check`, which fails when the
committed `tabs/services.wasm` is not what `native/` builds. A release is
a tag, `v` and the manifest's version, and it reaches Hangar's gallery once
[the catalog](https://github.com/hangar-ai/catalog) is updated and reviewed.
