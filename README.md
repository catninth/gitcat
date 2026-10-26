# GitCat — an open-source Git GUI for Windows and Linux

**GitCat by Cat Ninth** is a desktop Git client for reviewing changes, navigating commit history, managing branches, and resolving conflicts.

A focused GitKraken alternative, GitCat brings repository tabs, a visual commit graph, and syntax-highlighted diffs to your existing Git workflow. It uses your system Git, including your configuration, credential helpers, SSH agent, hooks, filters, and commit signing. Built with Tauri, Rust, and React. Licensed under MPL-2.0. No built-in AI features.

[Download the latest release](https://github.com/catninth/gitcat/releases/latest) · [Product overview](https://catninth.com/guides/gitcat/) · [User guide](https://catninth.com/guides/gitcat/first-repository/) · [Feature gallery](docs/gallery/README.md) · [FAQ](docs/FAQ.md)

Official project: [catninth/gitcat](https://github.com/catninth/gitcat). [Privacy notice](PRIVACY.md) · [Code signing policy](CODE_SIGNING.md) · [License](LICENSE)

## Download and platform support

Get the installer or package from [GitCat Releases](https://github.com/catninth/gitcat/releases/latest). As of October 26, 2026, the latest stable release is [v1.13.0](https://github.com/catninth/gitcat/releases/tag/v1.13.0), published October 23, 2026.

| Platform | Release package | Support and requirements |
| --- | --- | --- |
| Windows 10/11 x64 | `_x64-setup.exe` installer | Primary target. Install Git 2.31+; the installer can install WebView2 if needed. |
| Linux x86_64 | `.AppImage`, `.deb`, `.rpm` | Built, tested, and packaged; receives less manual UX testing than Windows. Requires Git and a compatible WebKitGTK 4.1 environment. |
| macOS | None | Not currently packaged or verified. |

Use your distribution's package installer for `.deb` or `.rpm` to resolve dependencies. Linux release builds use Ubuntu 24.04 as their build baseline; packages are not a promise of compatibility with every distribution. See the [installation guide](https://catninth.com/guides/gitcat/install/) for setup details. Rust and Node.js are needed only to [build from source](#development).

Windows packages are not Authenticode-signed; Windows may display an unknown-publisher or SmartScreen warning. Updater signatures are separate from Windows publisher verification. See the [code signing policy](CODE_SIGNING.md) before installing.

## Quick start

1. Install Git 2.31+ and the GitCat package for your platform. Keep your usual Git author identity and authentication configured.
2. Launch GitCat and choose **Open** to select an existing repository folder. Choose **Clone** for a remote URL or **Create** for a new repository.
3. Select **WIP** to review uncommitted changes. Open a file to inspect its diff, stage the files you want, enter a commit summary, and commit.
4. Use **Pull** and **Push** to sync with a configured remote. For private repositories, use your system Git credentials or connect a supported GitHub account under **Preferences → Integrations**.

Start with [your first repository](https://catninth.com/guides/gitcat/first-repository/), then follow the [changes and commits guide](https://catninth.com/guides/gitcat/changes-and-commits/). A GitCat account is not required for local repository work.

## Why GitCat?

- **Move between repositories without losing your place.** Open repositories in tabs, switch with `Ctrl+Tab`, and return to each repository's commit draft. [See the workspace screenshot](docs/gallery/workspace/repository-overview.png).
- **Review before you commit.** Inspect syntax-highlighted hunk, inline, or split diffs, then stage files in the working-tree panel. [See the split diff screenshot](docs/gallery/diffs/split-diff.png) and [staging screenshot](docs/gallery/working-tree/tree-layout.png).
- **Work through conflicts with context.** Check a prospective merge with read-only conflict preflight, then use the Base/Ours/Theirs/result editor when an operation stops on conflicts. Preflight requires Git 2.38+ with the needed `merge-tree` capability. [See the conflict preflight screenshot](docs/gallery/git-actions/conflict-preflight.png) and [conflict guide](https://catninth.com/guides/gitcat/conflicts/).

Considering a switch? Read [GitCat as a GitKraken alternative](docs/GITKRAKEN_ALTERNATIVE.md) for supported workflows, setup, and gaps to check before adopting it.

## Project status

GitCat has installable Windows and Linux releases for real repositories. Commit review, staging, branching, remote synchronization, and conflict resolution are implemented; platform coverage and feature gaps remain explicit. Windows is the primary target, and Linux receives less manual UX verification. See [known limitations](#known-limitations) before relying on a particular integration or workflow.

For development, `tauri dev` works with real repositories. The standalone Vite page uses a built-in demo data source and cannot access your repositories.

## Screenshots

<p align="center">
  <a href="docs/gallery/workspace/repository-overview.png">
    <img src="docs/gallery/workspace/repository-overview.png" alt="GitCat repository overview with branch graph and working tree" width="100%">
  </a>
</p>

<p align="center">
  <a href="docs/gallery/diffs/split-diff.png">
    <img src="docs/gallery/diffs/split-diff.png" alt="GitCat split diff viewer" width="49%">
  </a>
  <a href="docs/gallery/workspace/commit-details.png">
    <img src="docs/gallery/workspace/commit-details.png" alt="GitCat commit details" width="49%">
  </a>
</p>

<p align="center">
  <a href="docs/gallery/working-tree/tree-layout.png">
    <img src="docs/gallery/working-tree/tree-layout.png" alt="GitCat working tree" width="49%">
  </a>
  <a href="docs/gallery/preferences/theme-library.png">
    <img src="docs/gallery/preferences/theme-library.png" alt="GitCat theme library" width="49%">
  </a>
</p>

<p align="center">
  <a href="docs/gallery/README.md"><strong>Browse the full feature gallery →</strong></a>
</p>

## Key features

- Start page when no repository is open: searchable recent repositories with per-entry removal, plus Open, Clone, and Create actions. Create supports a target path, default branch name, and a `.gitignore` template. Connected GitHub.com and GitHub Enterprise accounts support hosted repository browsing, cloning, and creation.
- Repository tabs in a single row: app-owned context actions, aliases, browser-style switching, a searchable overview of every open tab, automatic restoration, and reopening the most recently closed tab.
- Tabs whose folder moved or is no longer a Git repository open a dedicated unavailable-repository page with retry, locate, and close actions instead of failing silently.
- LOCAL, REMOTE, and TAGS sidebar with filtering, a current-branch marker, `/`-segment folders for local branches, remote-grouped remote branches with owner avatars, and a right-click branch menu for pull, push, branch creation, rename, safe deletion, and name copy.
- Double-clicking a remote branch checks it out directly as a local tracking branch, with no intermediate name prompt.
- Compact, column-aligned commit DAG with colored lanes, relative time markers behind the lanes, WIP pseudo-node connected to HEAD, initials avatars, arrow-key stepping between the WIP row and history, and loading of older commits.
- Within a connected history, clean layout follows Git's visible row order and ancestry; checkout decoration alone does not reshuffle lanes. When the worktree has WIP, its current HEAD span is anchored in the primary lane. Stashes anchor to the branch they were created from, use the lowest available short-lived lane, and move right only when WIP or another active route occupies a lower lane.
- Ref labels, row highlight, and the WIP node take the color of the branch they belong to; the default palette is the ten-color GitLens lane set.
- Stash entries display Git's index-parent object as one dashed graph row while hiding the outer merge and untracked plumbing commits; their visible order follows Git's topo/date traversal rather than reflog selector order.
- Double-click a local branch label in the graph to check that branch out.
- `Ctrl+F` search across commit subjects and full description/body text; matches stay highlighted while the rest of the graph dims.
- Graph rows show the commit description under the subject; commit details list `Co-authored-by` trailers as separate authors.
- Commit details: click-to-copy SHA with full-object tooltip, author, timestamp, parents, statistics, Path/Tree changed files, and in-place commit message editing (reword).
- GitKraken-style structured diff with three view modes - hunk, inline, and split - persisted across restarts, plus line numbers and rename/mode/binary/truncation states. Split panes scroll independently and size to their own content. The diff opens in place of the graph and is closed from its header or with `Esc`; there is no separate view tab bar.
- Syntax highlighting in diffs (Shiki-based tokenization) and a change map next to whole-file diffs for jumping between modified regions. Highlighting survives silent background reloads without flicker.
- WIP/working tree: separate collapsible staged/unstaged trees, optimistic staging with rollback on failure, per-file or bulk actions, separate summary and description fields with a 72-character counter on the summary, amend, sign-off, and discard-all.
- Changed-file context menu: stage/unstage, discard, ignore the file, extension, or folder, stash a single file, copy the path, and save a patch.
- Push and pull quick actions carrying ahead/behind counts, autostash-backed pull for dirty worktrees, stash push and pop, and explicit pull modes: merge, fast-forward only, or rebase. Fetch and refresh are keybind-driven (`Ctrl+L`, `F5`).
- Repositories without a remote can add one by URL or create one on a connected GitHub service. The checked-out branch's graph menu includes **Set upstream**.
- Right-click a stash in the graph to apply, pop, or delete that entry. The commit graph menu also supports annotated tags with a tag message.
- Commit context menu: detached checkout, branch/tag creation, cherry-pick, revert, a reset submenu (soft, mixed, hard), and full SHA copy.
- Continue, skip, or abort an in-progress merge/rebase/cherry-pick/revert from an operation banner that names the commit the operation stopped on and shows the rebase position.
- Read-only merge-conflict preflight plus an operation-aware Base/Ours/Theirs/result editor, guarded side selection, delete/stage actions, and conservative Git `rerere` reuse.
- Bulk conflict resolution: mark every conflicted file resolved or take one side, for all conflicts or for a single folder.
- Automatic refresh: the active repository's worktree and `.git` metadata are watched on disk, so commits, checkouts, and editor saves made outside GitCat appear without a manual refresh.
- Auto-fetch when a repository is opened and then on a timer (1 minute by default, `0` disables, 60 maximum) keeps ahead/behind counts and remote branches current. It runs silently in the background: no toasts, no blocked toolbar, and failures stay quiet. Switching back to an already-fetched tab reuses the last result until the interval elapses.
- Resizable panels, a persisted Path/Tree changed-files view mode, and customizable semantic UI and diff colors plus all ten graph lane colors, each resettable on its own.
- Hideable side panels, window-centered repository actions, configurable command keybinds, and persistent footer build identity.
- Window size, position, and maximized state are restored on the next launch.
- The Windows installer offers to add an "Open with GitCat" entry to the Explorer context menu, both on a folder and on the background of an open folder. Opening a folder that way hands it to the window that is already running as a new tab rather than starting a second instance. The choice is remembered, so an update never turns it back on.

Keyboard shortcuts:

- `Ctrl+Tab` / `Ctrl+Shift+Tab`: next / previous repository
- `Ctrl+1` … `Ctrl+9`: select repository by visual order
- `Ctrl+T` / `Ctrl+W` / `Ctrl+Shift+T`: open a new repository tab / close active repository / reopen the last closed repository
- `Ctrl+Shift+O`: open repository
- `Alt+O`: open the active repository's folder in the file explorer
- `Ctrl+F`: search commits
- `Ctrl+,`: settings
- `F5` / `Ctrl+L`: refresh repository / fetch
- `Ctrl+J` / `Ctrl+K`: toggle left / right panel
- `Ctrl+Shift+S` / `Ctrl+Shift+U`: stage all / unstage all
- `Ctrl+Enter`: commit from the working-tree panel
- `Esc`: close the open diff, then any open context menu
- With the graph focused: `↑`, `↓`, `Home`, `End`, `Enter`, `Shift+F10`

All registered shortcuts, including network, branch, stash, diff, conflict, and operation commands, can be changed or cleared under Preferences → Keybinds. See [UX implementation notes](docs/UX_IMPLEMENTATION.md) for the complete default table.

## Architecture

```text
React 19 + TypeScript + Tailwind CSS 4
              |
       typed GitCatApi
              |
    Tauri v2 invoke commands
              |
         gitcat-core
    registry + repository locks
              |
       GitBackend trait
              |
      gitcat-git-cli
              |
          system Git
```

Workspace:

- `apps/desktop`: Vite/React UI, Tauri v2 adapter, and the single-repository filesystem watcher.
- `crates/gitcat-contracts`: Serde DTOs, enums, and stable API errors.
- `crates/gitcat-core`: repository registry, operation serialization, DAG layout, and persistent workspace/settings.
- `crates/gitcat-git-cli`: safe system Git runner and porcelain/plumbing parsers.

Using system Git is intentional: it preserves Git Credential Manager, SSH agents, hooks, signing, filters, and user Git configuration.

## Requirements

- Git 2.31+
- Rust 1.85+; Tauri Windows/MSVC prerequisites and WebView2 for native Windows builds
- On Linux, the Tauri v2 prerequisites: `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libssl-dev`, `libxdo-dev`, `build-essential`, plus `patchelf` for AppImage bundling
- Node.js 22 LTS and npm

Windows 10/11 is the primary target. Linux x86_64 is built, tested, and packaged as AppImage, `.deb`, and `.rpm` (WebKitGTK 4.1); it receives less manual UX verification than Windows. macOS is neither packaged nor verified.

## Development

Frontend dependencies:

```powershell
cd .\apps\desktop
npm.cmd install
```

Real native GitCat:

```powershell
npm.cmd run tauri dev
```

UI only, with demo data:

```powershell
npm.cmd run dev
```

The Vite page does not access the file system or real repositories. Native Git operations always require `tauri dev`.

## Build and verification

`scripts/build-all.ps1` runs the whole pipeline in order: frontend dependency install, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, TypeScript typecheck, and finally `npm run tauri build` (which also produces the Vite web build and the native installers). It stops at the first failing step and exits with that step's exit code.

```powershell
cd .\apps\desktop
npm.cmd run build:all
```

`scripts/build-all.sh` is the Linux/macOS counterpart with the same steps and the flags `--skip-install`, `--skip-verify`, and `--no-bundle`:

```bash
cd apps/desktop
npm run build:all:unix
```

Flags of the PowerShell script:

- `-SkipInstall`: do not run `npm ci` / `npm install` first.
- `-SkipVerify`: skip fmt, clippy, tests, and typecheck; build only.
- `-NoBundle`: build the release binary without MSI/NSIS installers (faster).

Outputs: `apps/desktop/dist` (web), `target/release/gitcat-desktop.exe` (`gitcat-desktop` on Linux), `target/release/bundle` (installers and packages).

The individual steps are also available separately.

Rust workspace:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Windows helper for the same checks:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1
```

Linux/macOS helper:

```bash
./scripts/verify.sh
```

Frontend:

```powershell
cd .\apps\desktop
npm.cmd run typecheck
npm.cmd run build
```

Native release and installer packages:

```powershell
cd .\apps\desktop
npm.cmd run tauri build
```

The web build is written to `apps/desktop/dist`; native artifacts are written under Cargo's `target/release` directory.

## Releases

`.github/workflows/release.yml` reads the version from the Cargo workspace, builds both platforms in parallel, and then publishes them together:


- `build-windows` on `windows-latest` enforces the `x86_64-pc-windows-msvc` host, runs fmt, clippy, tests, and typecheck, then bundles the NSIS installer. The build is verified to be self-contained: the job fails if `gitcat-desktop.exe` still imports `WebView2Loader.dll` or if a dynamic loader DLL is left in the release output, and it prints the installer's SHA-256.
- `build-linux` on `ubuntu-24.04` (pinned, because the AppImage inherits that image's glibc as its minimum baseline) enforces the `x86_64-unknown-linux-gnu` host, installs the Tauri prerequisites, runs the same checks, and bundles the AppImage, `.deb`, and `.rpm`. It fails unless exactly one of each package exists and the binary links `libwebkit2gtk-4.1`, and it prints their SHA-256 sums. The `.deb` and `.rpm` declare a dependency on `git`.
- `publish` merges the per-platform updater entries into one `latest.json` (`windows-x86_64`, `linux-x86_64`, and package-specific `linux-x86_64-appimage`, `linux-x86_64-deb`, and `linux-x86_64-rpm`) and creates the GitHub release with every package and signature.

The desktop app uses the shared [`catninth-updater`](https://github.com/catninth/updater)
Rust library to check stable releases four seconds after startup and every six hours.
Installation remains user-triggered. The native backend owns progress, signature
verification, and restart; see [the integration guide](docs/TAURI_INTEGRATION.md#auto-update).

Triggers:

- `workflow_dispatch`: a stable `v<version>` release. Artifacts are always uploaded; the release is created only with `publish: true`.
- Scheduled nightly publication is currently disabled in the workflow. The retained scheduled-run logic would publish a rolling `nightly` pre-release if the schedule were re-enabled; existing nightly pre-releases are excluded from the stable in-app update channel.

`apps/desktop/src-tauri/nsis/installer.nsi` is a vendored copy of the Tauri bundler's NSIS template for the pinned `@tauri-apps/cli` version, carrying one addition: the "Additional tasks" page that offers the Explorer context menu. Tauri's four `NSIS_HOOK_*` macros all run inside a section, which is too late to declare an installer page, so the page itself cannot live in `nsis/hooks.nsh` with the rest of the logic. Upgrading the CLI means re-fetching the template for the new version and reapplying that one insertion; see `AGENTS.md`.

## Security model

- Git runs directly as a `tokio::process::Command` process; there is no shell and no general-purpose `run_git(args)` IPC.
- The UI passes a path only when opening a repository. Subsequent calls use a registered, UUID-based repository ID.
- Ref names are validated by Git; revisions are resolved to full object IDs before mutations.
- Pathspec arguments are passed literally after `--`.
- Conflict-editor writes reject stale index stages or externally changed worktree content, preserve line endings, and atomically replace files before staging.
- External diff tools and textconv are disabled for read-only diffs.
- Allowed remote protocols: `file`, `git`, `http`, `https`, `ssh`; custom remote helpers are disabled.
- Git's interactive credential prompts are disabled. HTTPS operations can use a hosting-service credential connected in GitCat; otherwise Git uses your configured credential helpers or SSH agent.
- Auto-fetch runs `git fetch` on the active repository and can be disabled by setting its interval to `0`. Update checks, repository-location lookups, enabled author pictures, pull-request/check decorations, and remote-owner pictures can also make background network requests; see the [privacy notice](PRIVACY.md) for controls and destinations.
- `reset --hard`, discarding changes, forced branch deletion, and stash drop require explicit confirmation plus a current matching snapshot. The core rejects stale dialogs. Destructive branch deletion is confirmed inline in the top bar instead of a modal.
- Discarding a file that another process holds open fails with an explicit "file is in use" message instead of a generic Git error.
- Rewording a commit is guarded by the same snapshot check, so it is rejected if the branch moved underneath the panel.
- The filesystem watcher observes one repository - the active one - ignores `.git/objects` and `.git/lfs` churn, debounces bursts, and only asks the UI to reload. It never runs Git on its own.
- GitCat does not modify the global `safe.directory` value or delete `.git/index.lock`.
- The Tauri main window receives only core and directory-open permissions; CSP blocks external object/frame/form content.

<a id="mvp-limitations"></a>

## Known limitations

- Hosted repository browsing and creation support GitHub.com and GitHub Enterprise. Other listed providers currently offer links only; their hosted clone/create integration is disabled. Cloning a remote URL through system Git remains available.
- There is no general editor for existing remote URLs. Adding a remote when none exists and setting the checked-out branch's upstream are available.
- There is no standalone stash panel. Use the toolbar to push a stash or pop the latest entry, or a stash's graph context menu to apply, pop, or delete that entry.
- Clone, fetch, pull, and push do not expose transfer progress or cancellation in the UI; the Tauri adapter currently creates its own cancellation token for each call.
- Diffs are line-level: syntax highlighting is available, word-level diffs are not.
- There is no built-in terminal or interactive prompt for arbitrary Git credentials. Use supported hosting connections, system Git credential helpers, or your SSH agent.
- Author pictures can be fetched from GitHub without signing in for public repositories and GitHub alias addresses. Gravatar fallback is opt-in; unresolved authors use initials. See the [privacy notice](PRIVACY.md).

Detailed contracts: [Core API](docs/CORE_API.md), [Tauri integration](docs/TAURI_INTEGRATION.md), [GitKraken UX research](docs/GITKRAKEN_UX_RESEARCH.md), and [UX implementation notes](docs/UX_IMPLEMENTATION.md).

For maintainers: [project discoverability and measurement](docs/DISCOVERABILITY.md).

## Common questions

The [FAQ](docs/FAQ.md) covers the license, supported platforms, hosting accounts, AI features, and network activity. GitCat has no built-in usage analytics, but update checks, Git operations, hosting lookups, and pictures can make network requests; see the [privacy notice](PRIVACY.md) for the controls and remaining activity.

## Project background

GitCat by Cat Ninth was introduced as a "vibe-coded GitKraken alternative." That describes the project's development background. The feature list, source, verification commands, and known limitations above describe the application you can evaluate today.

## License

GitCat's own source code is licensed under the [Mozilla Public License 2.0](LICENSE), matching the Rust workspace and desktop package metadata. Third-party dependencies and vendored files retain their respective licenses and notices.
