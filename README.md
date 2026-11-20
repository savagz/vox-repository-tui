# VOXRepositoryTUI

A terminal user interface (TUI) for managing git repositories, in the spirit of lazygit, with the industrial control-panel look of the other VOX TUIs. Built in Rust with [ratatui](https://ratatui.rs).

It has two levels:

- **Workspace**: every repository found under the configured root folders, with branch, ahead/behind, pending changes, stashes and last commit. Fetch or pull one repository or all of them at once.
- **Repository panel**: open a repository to stage, commit, browse history, rewrite commits, manage branches, remotes, stashes, tags, worktrees and the reflog.

All git work goes through the `git` command line client, so hooks, credential helpers, signing and your own configuration behave exactly as in a shell.

## Features

- Workspace scan with configurable roots, depth and ignored folders. Sort by name, changes or recent activity, filter by name, show only dirty repositories.
- Status with file, hunk and line staging (including untracked files), discarding of files, hunks or lines, and a staged/unstaged diff toggle.
- Commit editor with summary and multi-line body, amend, or commit with your `$EDITOR`.
- Commit log with graph, per-commit file list and diff, all-branches view, and the log of any branch or tag.
- History rewriting: reword, squash, fixup, drop and move commits, fold staged changes into an older commit, and a full interactive rebase planner (pick, reword, edit, squash, fixup, drop, reorder). git never opens an editor in the middle of a rebase: the plan is handed over complete.
- Conflict resolver: choose ours, theirs or both per conflict block, or take a whole side for the file. A banner shows any merge, rebase, cherry-pick, revert or bisect in progress, with continue, abort and skip.
- Branches: checkout (local or remote with tracking), create, delete, force delete, rename, merge, rebase onto, set or unset upstream, fast-forward.
- Remotes, stashes (all, staged only, keep index; apply, pop, drop, branch), tags (lightweight or annotated, push, delete local or remote), worktrees (add, open, remove, prune) and reflog (checkout, reset, branch).
- Cherry-pick, revert, reset (soft, mixed, hard), checkout of any commit.
- Push with automatic upstream setup and an offer to `--force-with-lease` when rejected.
- Free-form git command (`:`), shell in the repository (`!`) and a command log (`L`) with the output of every operation.
- Overview with working tree meters, inventory, 30-day commit activity and top authors.
- System check for git version, identity, credential helper, editor, SSH agent, clipboard and workspace roots.
- Spanish and English interface, switchable at runtime.

## Requirements

- `git` 2.28 or newer in your `PATH` (2.35 or newer recommended for staged-only stashes).
- For building: a Rust toolchain with support for the 2024 edition (1.88 or newer).

git runs without a terminal, so it cannot ask for passwords or passphrases. Use an SSH agent or a credential helper (`credential.helper`) for remotes that need authentication; otherwise the operation fails with git's error instead of hanging.

## Installation

Prebuilt binaries are published for Linux x86_64 (static musl build) and macOS (Apple Silicon and Intel).

```sh
curl -fsSL https://raw.githubusercontent.com/savagz/vox-repository-tui/main/install.sh | sh
```

Without `curl`, use `wget`:

```sh
wget -qO- https://raw.githubusercontent.com/savagz/vox-repository-tui/main/install.sh | sh
```

The script detects the platform, downloads the latest release, verifies its SHA-256 checksum and installs `voxrepository-tui` into `/usr/local/bin` (using `sudo` only if needed). On macOS it also clears the quarantine attribute of the unsigned binary. Options can be passed after `sh -s --`:

```sh
# Pin a version ("v0.1.0" and "0.1.0" are both accepted)
curl -fsSL https://raw.githubusercontent.com/savagz/vox-repository-tui/main/install.sh | sh -s -- --version v0.1.0

# Install without root (the directory is created if missing)
curl -fsSL https://raw.githubusercontent.com/savagz/vox-repository-tui/main/install.sh | sh -s -- --prefix ~/.local/bin

# Uninstall (pass the same --prefix used to install)
curl -fsSL https://raw.githubusercontent.com/savagz/vox-repository-tui/main/install.sh | sh -s -- --uninstall
```

The version can also be set with the `VOXREPOSITORY_VERSION` environment variable.

### Manual installation

Download `voxrepository-tui-<tag>-<platform>.tar.gz` and its `.sha256` file from the [releases page](https://github.com/savagz/vox-repository-tui/releases), where `<platform>` is `x86_64-linux`, `aarch64-macos` or `x86_64-macos`, then:

```sh
sha256sum -c voxrepository-tui-<tag>-<platform>.tar.gz.sha256   # macOS: shasum -a 256 -c ...
tar -xzf voxrepository-tui-<tag>-<platform>.tar.gz
sudo install -m 0755 voxrepository-tui-<tag>-<platform>/voxrepository-tui /usr/local/bin/
```

### Publishing a release

1. Bump `version` in `Cargo.toml` and commit.
2. `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. The `release` workflow creates the release, then builds, tests and uploads the Linux and macOS packages. It can also be run manually (`workflow_dispatch`) to build without publishing.

## Build and run

```sh
git clone https://github.com/savagz/vox-repository-tui
cd vox-repository-tui
cargo run --release
```

### Command line

```
voxrepository-tui [OPTIONS] [PATH]

    PATH             Open this repository
    -w, --workspace  Start in the workspace even inside a repository
    -V, --version    Print the version and exit
    -h, --help       Print help and exit
```

Without `PATH`, the repository containing the current directory is opened (if any) and the workspace is scanned in the background.

## Sections

| Key | Section      |
|-----|--------------|
| `w` | Workspace    |
| `1` | Overview     |
| `2` | Status       |
| `3` | Commits      |
| `4` | Branches     |
| `5` | Remotes      |
| `6` | Stash        |
| `7` | Tags         |
| `8` | Worktrees    |
| `9` | Reflog       |
| `0` | System check |

`Tab` and `Shift+Tab` cycle through the sections. Sections 1 to 9 need an open repository.

## Key bindings

Press `h` or `?` for the full in-app reference. The bottom bar always shows the keys of the current section.

### Global

| Key                   | Action                                        |
|-----------------------|-----------------------------------------------|
| `q`, `Ctrl+C`         | Quit                                          |
| `j`/`k`, arrows       | Move                                          |
| `g`/`G`, `Home`/`End` | Top / bottom                                  |
| `PgUp`/`PgDn`         | Scroll the detail panel                       |
| `/`                   | Filter the current list                       |
| `p` / `P` / `f`       | Pull / push / fetch the open repository       |
| `m`                   | Continue, abort or skip a merge/rebase        |
| `:`                   | Run a git command                             |
| `!`                   | Shell in the repository                       |
| `L`                   | Command log                                   |
| `F5`, `Ctrl+R`        | Refresh                                       |
| `F2`                  | Settings                                      |
| `Esc`                 | Back                                          |

### Workspace

`Enter` open, `f`/`F` fetch one/all, `p`/`P` pull `--ff-only` one/all, `s` sort, `d` dirty only, `y` copy path.

### Status

| Key           | Action                                                |
|---------------|-------------------------------------------------------|
| `Space`       | Stage / unstage the file                              |
| `a`           | Stage all / unstage all                               |
| `Enter`       | Focus the diff (or open the conflict resolver)        |
| `c` / `C`     | Commit / commit with `$EDITOR`                        |
| `A`           | Amend the last commit                                 |
| `d` / `D`     | Discard the file / discard and reset menu             |
| `s`           | Stash menu                                            |
| `i` / `e`     | Add to `.gitignore` / open in the editor              |
| `x`           | Toggle staged and unstaged diff                       |
| `O` / `T`     | Resolve a conflicted file with ours / theirs          |

With the diff focused: `Space` stages or unstages the current line (or the `v` range), `a` the whole hunk, `[`/`]` jump between hunks, `d` discards the selection from the working tree.

In the conflict resolver: `o` ours, `t` theirs, `b` both, `u` unresolved, `Enter` write the file (staged automatically once every block is resolved), `e` open the editor.

### Commits

| Key           | Action                                                |
|---------------|-------------------------------------------------------|
| `Enter`       | Files of the commit and their diff                    |
| `Space`       | Checkout the commit                                   |
| `r`           | Reword                                                |
| `S` / `F`     | Squash / fixup into the previous commit               |
| `d`           | Drop                                                  |
| `J` / `K`     | Move the commit down / up                             |
| `i`           | Interactive rebase from the selected commit           |
| `A`           | Fold the staged changes into the selected commit      |
| `c` / `V`     | Cherry-pick / revert                                  |
| `x`           | Reset menu (soft, mixed, hard)                        |
| `n` / `t`     | New branch / tag at the commit                        |
| `a`           | Toggle the all-branches graph                         |
| `y`           | Copy the hash                                         |

In the rebase planner: `p r e s f d` set the action, `J`/`K` reorder, `Enter` run.

### Branches, remotes, stash, tags, worktrees, reflog

- Branches: `Space` checkout, `Enter` log, `n` new, `d`/`D` delete/force, `R` rename, `M` merge, `r` rebase onto, `u`/`U` set/unset upstream, `F` fast-forward.
- Remotes: `n` add, `f` fetch, `e` edit URL, `R` rename, `d` remove.
- Stash: `Space` apply, `g` pop, `n` branch, `s` new stash, `d` drop.
- Tags: `n` new, `Space` checkout, `Enter` log, `P` push, `d`/`D` delete local/remote.
- Worktrees: `Enter` open, `n` add, `d`/`D` remove/force, `x` prune.
- Reflog: `Space` checkout, `n` branch, `x` reset menu.

Irreversible actions (hard reset, cleaning untracked files, force deleting a branch, deleting remote branches or tags) ask you to type a confirmation word.

## Configuration

Settings are edited with `F2` and saved to the user configuration directory (`~/Library/Application Support/voxrepository-tui/config.toml` on macOS, `~/.config/voxrepository-tui/config.toml` on Linux). See [`config/voxrepository.example.toml`](config/voxrepository.example.toml) for every option. Development builds also read `config/voxrepository.toml` from the current directory.

Logs are written to the user data directory (`voxrepository-tui.log`), never to the terminal. Set `RUST_LOG=debug` for more detail.
