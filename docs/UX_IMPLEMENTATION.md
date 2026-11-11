# GitKraken-inspired UX implementation

Implemented on 2026-07-22 after the research in [GITKRAKEN_UX_RESEARCH.md](GITKRAKEN_UX_RESEARCH.md). This file records shipped behavior, safety boundaries, and verification targets.

## Repository tabs and application menus

- The native WebView/browser context menu is suppressed throughout the app.
- Right-clicking a repository tab opens GitCat's menu: activate, move to no folder, move to an existing/new folder, rename the tab alias, copy its path, close it, close others, or close tabs to the right.
- Ungrouped repositories are first-class persisted workspace entries. Existing grouped workspaces migrate through serde defaults.
- `Ctrl+Tab`, `Ctrl+Shift+Tab`, and `Ctrl+1` through `Ctrl+9` use visual workspace order, wrap where applicable, and expand the destination group.
- Repository switching remains available while an overview is loading or a repository mutation is finishing, so rapid `Ctrl+Tab` presses keep browser-style behavior. Closing, dragging, and opening are disabled during an exclusive mutation. Repository-bound dialogs and menus close on a tab change, preventing an action captured for one repository from running against another.

## Commit graph and SHA interaction

- Graph headers, WIP, lanes, commit text, author, time, and SHA share one fixed column grid.
- Commit rows are 28 px high. Selection changes paint only; it does not change lane geometry or horizontal offsets.
- The WIP pseudo-node is above HEAD and uses the same graph X coordinate as the first lane.
- Initial avatars occupy graph nodes. Provider-hosted profile images remain out of scope until provider authentication/profile lookup exists.
- The abbreviated SHA is the copy control. Hover or keyboard focus opens a viewport-safe tooltip with the full object ID and “Click to copy”; the button uses a pointer cursor.
- The separate copy icon was removed from commit details.

## Changed files, diff, and WIP

- Historical changed files and working-tree files share the same `Path` / `Tree` switch.
- Tree mode builds folders from repository-relative paths, counts descendants, supports expand/collapse-all, reveals the selected file, and preserves the selection when switching layouts.
- Inline diff uses explicit old-line, new-line, and content columns. Split mode keeps the existing side-by-side layout.
- The working-tree panel has independent collapsible `Unstaged` and `Staged` sections, per-file actions, bulk stage/unstage, amend/sign-off options, and a commit editor.
- Conflicted files are never included in Stage All. They stay in the conflict workflow until explicitly resolved.
- Hiding either side panel collapses its grid column to zero without unmounting its content. Commit message, amend, and sign-off drafts are keyed by repository tab, so they survive hide/show, WIP/commit-detail navigation, and switching away and back.
- Fetch, Pull, Push, Branch, and Stash use the same command callbacks as their shortcuts and are centered against the full window through equal outer grid tracks.

## Keybinds

Preferences contains a single registry-backed Keybinds section. Every registered shortcut can be recorded, reset, or cleared with Backspace/Delete. Duplicate assignments are rejected. Bare navigation/activation keys and OS-reserved combinations such as `Alt+F4`, `Ctrl+Alt+Delete`, and `Cmd+Q` are rejected in both the UI and persisted-state validator.

| Action | Default |
| --- | --- |
| Next / previous repository | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Repository 1–9 | `Ctrl+1` … `Ctrl+9` |
| New repository tab / close repository | `Ctrl+T` / `Ctrl+W` |
| Open repository | `Ctrl+Shift+O` |
| Search commits / preferences / refresh | `Ctrl+F` / `Ctrl+,` / `F5` |
| Toggle left / right panel | `Ctrl+J` / `Ctrl+K` |
| Fetch / pull / push | `Ctrl+L` / `Ctrl+Alt+P` / `Ctrl+Shift+P` |
| Create branch / stash | `Ctrl+B` / `Ctrl+Alt+S` |
| Show WIP / graph | `Ctrl+Shift+W` / `Alt+Left` |
| Inline / split diff | `Alt+1` / `Alt+2` |
| Copy selected full SHA | `Ctrl+Shift+C` |
| Continue / abort active operation | `Ctrl+Alt+Enter` / `Ctrl+Shift+Backspace` |
| Stage all / unstage all | `Ctrl+Shift+S` / `Ctrl+Shift+U` |
| Focus commit message / commit | `Ctrl+Shift+M` / `Ctrl+Enter` |
| Reuse recorded conflict resolutions | `Ctrl+Alt+R` |

Modal dialogs and repository/commit context menus suppress global command dispatch. The commit shortcut uses the same eligibility guard as the button: non-empty message, staged content or amend, no unresolved conflicts, and no operation already running.

## Conflict preflight and resolution

The toolbar conflict control has two distinct modes:

1. With no active conflict, it optionally compares HEAD with a selected local/remote target using read-only `git merge-tree --write-tree --name-only -z --no-messages`. The backend resolves the target to a full OID first. Git versions without the required capability return `unavailable`; no worktree, index, ref, or HEAD mutation is attempted.
2. During merge, rebase, cherry-pick, or revert conflicts, the control shows the active unresolved count and opens the WIP resolver.

The default comparison target is the current branch upstream, then `origin/HEAD`, then another symbolic remote HEAD. Users can select another branch or explicitly disable preflight per repository tab. Preflight reruns when HEAD or the selected target OID changes, not for every worktree edit.

The built-in resolver exposes Base, index stage 2, index stage 3, and the current result. Labels are operation-aware so rebase does not misleadingly present raw “ours/theirs” semantics. Users can:

- take either available side, including binary and modify/delete cases;
- compose a text result line-by-line or take a complete text side;
- save and stage an edited result;
- stage the current working copy explicitly;
- delete the conflicted path explicitly;
- apply only exact resolutions already recorded by repository-local Git `rerere`.

Git already performs every clean three-way merge automatically. GitCat does not guess an ambiguous side. Stage All excludes conflicts, and automatic resolution means only `rerere` reuse.

Conflict mutations carry an expected snapshot containing all index-stage OID/mode identities plus the exact working-result presence, kind, size, SHA-256, line-ending, and Unix mode-bit identity. Any external content edit after opening the resolver returns `stale_snapshot`; on Unix, external permission-mode edits do too. Text saves use a same-directory temporary file, flush/sync it, preserve existing permissions, atomically replace the result, and stage only after replacement succeeds. New result files receive the selected Git file mode through normal creation permissions, so the operating-system umask remains effective. Worktree hashing and file I/O run off the async executor. Editor content is capped at 1 MiB; larger or binary content remains side-selectable but is not loaded into the text editor.

LF and CRLF results keep their original line endings when `preserve` is selected. A changed mixed-EOL result cannot be silently normalized: the UI requires explicit LF or CRLF selection.

### Working tree panel while files are conflicted

Modelled on GitKraken 12.6.2 (observations in `scripts/conflict-lab/FINDINGS.md`, plan in `docs/conflict-ux-plan.md`):

- While any path is conflicted -- in any operation, including a stash apply or a pull's autostash that has no state file -- the panel shows only **Conflicted Files (n)** and **Resolved Files (n)** (the staged list, same menu). Unstaged and untracked files are hidden and return the moment the last conflict is resolved. With no conflicts left but an operation still pending (`merge --no-commit`), the panel is the normal Unstaged/Staged view with the operation's buttons at the bottom.
- Header: "Rebase conflicts detected" for a rebase, "Merge conflicts detected" for everything else. A subtitle appears only for a merge ("Merging `feature` into `main`") and a rebase ("Rebasing `feature` onto `main`", the onto as a short hash when no ref names it). There is no stash count badge.
- The working-copy row says "A file conflict was found when attempting to merge into `main`" or "N file conflicts were found …"; during a rebase the target is `HEAD`. The count includes paths deleted on both sides, which GitKraken hides.
- A pending autostash (`MERGE_AUTOSTASH`, `rebase-*/autostash`, `RepositorySnapshot.operation_autostash`) gets one line under the header saying the local changes come back when the operation finishes or is aborted. GitKraken shows nothing.
- The commit message field stays visible during an operation and is filled once from `RepositorySnapshot.operation_message` (`MERGE_MSG`, or the replayed commit's message for a rebase, `#` lines removed) when the draft is empty. A draft the user typed is never overwritten; a prefilled message nobody touched is replaced at the next rebase step and cleared when the operation ends. A stash's message is never prefilled.
- Buttons: merge "Commit and Merge" (disabled while conflicted; "No Changes to Merge" while conflicted with nothing resolved) + "Abort Merge"; rebase "Skip Commit" while conflicted, then "Continue Rebase", + "Abort Rebase"; cherry-pick/revert "Continue …" + "Abort …" (GitKraken has no Abort there; GitCat keeps it); a stash apply stopped on conflicts gets the ordinary commit button and no Abort. Rebase progress is a line above the message ("Rebasing commit n out of m"), never part of it. Ctrl+Enter only ever continues; it never skips.
- Continue sends the edited message: merge, cherry-pick and revert rewrite `MERGE_MSG`; a merge-backend rebase rewrites `rebase-merge/message` (verified against real Git: `MERGE_MSG` is ignored on `rebase --continue`). The apply backend rebuilds its message from the patch, so there the field is effectively read-only.
- **Decision (Abort confirmation):** GitKraken aborts without asking. GitCat asks only when Resolved Files is non-empty, because only then does the abort throw away work.
- Toasts for a command that stops on conflicts use GitKraken's titles with "There are merge conflicts that need to be resolved": "Merge Failed", "Rebase Failed", "Rebase Skip/Continue Failed", "Cherry pick Failed", "Revert Failed", "Error Applying Stash"; a pull that stopped while restoring its autostash shows "Error Applying Stash" and "Pull Failed – Has conflicts". A completed merge says "Merged – `feature` into `main`". `MergeBranch` now passes the branch name (when it still resolves to the same commit) so Git writes "Merge branch 'feature'" rather than "Merge commit '<oid>'".

### Resolving one file

- Side labels (menus, bulk actions, decision bar): merge -- current branch / incoming branch; rebase -- onto / replayed branch; cherry-pick and revert -- current branch / short commit id; stash apply and pull autostash -- short HEAD id / "Applied patch or stash". GitKraken labels a pull's autostash by the rebase direction ("origin/main" / "main"); GitCat does not.
- Right-click on a conflicted file: Take current (…) · Take incoming (…) · Mark resolved · Open conflict… · Show in Explorer · Copy file path · Delete file. A side Git recorded nothing for is disabled. GitCat has no file history or external-editor command yet, so those GitKraken entries are absent. Taking one file's side from a menu is not confirmed; a folder or "for all" still is.
- `StatusEntry.conflict_kind` carries the porcelain `u XY` shape. A click on a modify/delete (UD/DU), a one-sided add from a rename/rename or file/directory conflict (AU/UA), or a both-deleted path (DD) opens a decision bar above the graph instead of the editor: "X has been modified on A, but deleted on B." Keep Modified Version · Delete The File · Keep Base Version · Cancel; "GitCat was unable to determine whether to keep X …" Keep New Version · Delete The File · Cancel; "X was deleted on both sides." Delete The File · Keep Base Version · Cancel. A `path~HEAD` copy next to a directory says it is Git's file/folder rename.
- `ConflictResolution::Base` stages index stage 1 (`update-index --cacheinfo`, then checkout for files) and fails when there is no base. The same stage-0 write takes a side of a submodule pointer, which `checkout --ours/--theirs` cannot.
- Take incoming on a binary conflict works (regression-tested); in GitKraken it silently does nothing.
- A submodule pointer conflict opens its own view: Current (what the submodule has checked out) / Ours / Theirs cards described from the submodule's own repository, with Use current (stages the checked-out commit), Use incoming (stages stage 3), and Open submodule. GitKraken has no "use incoming".
- Mark resolved and Mark All Resolved first ask the backend (`conflict_resolution_warnings`) whether any path still holds a complete `<<<<<<<`/`=======`/`>>>>>>>` block (text files within the 1 MiB editor limit) or is a `path~<side>` helper copy beside a directory. Only then is the user asked to confirm, with the paths listed. GitKraken commits both silently.

### Merge editor

- Pane titles "Commit `8c69e9a` on `main`" (A = stage 2) and "Commit `ffcec0a` on `feature`" (B = stage 3); during a rebase A is HEAD on the onto and B the replayed commit read from the conflict marker label; a stash apply's B is "Applied patch or stash". The header reads "`path` (n conflicts)", and the editor opens scrolled to the first conflict.
- The Output still starts from the working copy, so a file fixed by hand (lab case C07) is not reset to the base as GitKraken does.
- An add/add conflict has an empty base, not an error.
- **Decision (large files):** GitKraken opens a 2 MB file. GitCat keeps the 1 MiB editor cap, because the panes render every line without virtualisation; a larger file shows a message that points to Take current / Take incoming in the file's menu.

### Cherry-pick and revert

- Starting either asks first, in a bar above the graph: "Do you want to immediately commit the cherry picked changes?" (or "…the reverted changes?") Yes · No · Cancel. No runs with `--no-commit` and leaves the change staged.
- Not implemented (plan phase F): "Cherry-picking commit n out of m" -- Git's sequencer keeps only the remaining `todo` for a cherry-pick series, not the finished picks, so n is not derivable. A conflict inside a submodule is not surfaced in the parent repository.

## Build identity

The footer always displays `GitCat v<package-version> · <build-commit>`. Native builds inject Git's unambiguous abbreviated source commit (at least seven hexadecimal characters) at compile time and rerun the build script when HEAD/ref metadata changes. The build script requires a `.git` directory or worktree pointer at GitCat's own repository root, so a source archive or non-Git build reports `unknown` instead of inheriting an unrelated parent repository; the browser fixture reports `browser-demo` rather than pretending to be a native build.

## Explicit limitations

- GitHub/GitLab provider authentication and hosted profile-picture lookup are not implemented; initials are intentional fallback avatars.
- The resolver does not launch an external merge tool. Binary conflicts can be resolved by selecting an index side or deleting the path.
- Preflight predicts the merge between committed HEAD and the selected committed target. It is not a promise about later ref movement or unrelated uncommitted edits.
