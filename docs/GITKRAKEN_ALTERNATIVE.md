# GitCat as a GitKraken alternative

**GitCat by Cat Ninth** is an open-source Git GUI for Windows and Linux. It suits developers who want to review a visual commit graph, stage changes, switch repositories, and resolve conflicts while keeping their existing system Git setup.

This guide describes GitCat's current capabilities and limits so you can assess it against your own workflow. It does not claim feature parity or a performance advantage over another client.

[Download GitCat](https://github.com/catninth/gitcat/releases/latest) · [Product overview](https://catninth.com/guides/gitcat/) · [Feature gallery](gallery/README.md) · [FAQ](FAQ.md)

## Keep your existing Git workflow

GitCat opens ordinary Git repository folders; no repository conversion is needed. It invokes system Git directly, preserving your Git configuration, credential helpers, SSH agent, hooks, filters, and commit signing. Configure your author identity and remote authentication as you would for Git itself.

There is no built-in AI feature or new version-control model to adopt. GitCat's own source is [MPL-2.0 licensed](../LICENSE), and the official project is [catninth/gitcat](https://github.com/catninth/gitcat).

## Three workflows to try

### Switch between repositories

Open two repositories and use `Ctrl+Tab` to move between their tabs. Aliases, a searchable tab overview, restored tabs, and repository-specific commit drafts help you return to work in progress. The graph places commits, refs, and WIP in the repository view.

[Workspace screenshot](gallery/workspace/repository-overview.png) · [Open your first repository](https://catninth.com/guides/gitcat/first-repository/)

### Review and commit changes

Select WIP, open a changed file, and choose hunk, inline, or split diff. Syntax highlighting and line numbers support code review. Stage individual files or all eligible changes, write a summary and description, then commit. Commits use system Git, including configured hooks and signing.

[Split diff screenshot](gallery/diffs/split-diff.png) · [Working-tree screenshot](gallery/working-tree/tree-layout.png) · [Changes and commits guide](https://catninth.com/guides/gitcat/changes-and-commits/)

### Resolve a stopped operation

Read-only conflict preflight can compare HEAD with another branch before merging when Git 2.38+ supports the required `merge-tree` capability. If a merge, rebase, cherry-pick, or revert stops on conflicts, the operation banner provides the relevant continuation or recovery controls. The conflict editor exposes Base, Ours, Theirs, and the result; writes reject stale index stages or externally changed worktree content.

[Conflict preflight screenshot](gallery/git-actions/conflict-preflight.png) · [Conflict resolution guide](https://catninth.com/guides/gitcat/conflicts/)

## Check these requirements before switching

| Your requirement | GitCat's current scope |
| --- | --- |
| Windows desktop installation | Windows 10/11 x64 is the primary target; releases include an NSIS installer. Windows Authenticode signing is not configured. |
| Linux desktop installation | x86_64 AppImage, `.deb`, and `.rpm` packages are published. Linux receives less manual UX verification than Windows. |
| macOS | No packaged or verified build. |
| Hosted repository browsing and creation | Supported for connected GitHub.com and GitHub Enterprise accounts. Other listed providers currently offer links only; ordinary Git URLs still work through system Git. |
| Remote and upstream configuration | Add a remote when none exists, or set the checked-out branch's upstream from its graph menu. No general editor for existing remote URLs. |
| Stash management | Toolbar supports stash push and popping the latest entry. A stash's graph menu supports applying, popping, or deleting that entry; there is no standalone stash panel. |
| Long Git network transfers | Clone, fetch, pull, and push do not expose transfer progress or cancellation in the UI. |
| Fine-grained diff review | Hunk, inline, and split views with syntax highlighting. Word-level diffs are not available. |
| Terminal or interactive credential prompt | No built-in terminal or terminal credential prompt. Configure authentication through supported hosting connections or system Git. |
| Fully offline operation | Local repository work is available, but automatic updates and other background features can still make network requests. Preferences are not a complete offline switch. |

Read [known limitations](../README.md#known-limitations), the [privacy notice](../PRIVACY.md), and the [code signing policy](../CODE_SIGNING.md) for details. These limitations describe the current implementation, not a delivery commitment for future features.

## Try GitCat with a familiar repository

1. Install Git 2.31+ and the appropriate [release package](https://github.com/catninth/gitcat/releases/latest). Follow the [installation guide](https://catninth.com/guides/gitcat/install/) for platform prerequisites.
2. Open a repository you know. Inspect history and diffs first, then try staging and committing on a branch used for evaluation.
3. Check your normal authentication, hooks, signing, and conflict workflow before adopting GitCat across projects.
4. Review **Preferences** for keybinds, auto-fetch, hosting connections, pictures, and decorations. Consult the [FAQ](FAQ.md) for defaults and network behavior.

The [user guides](https://catninth.com/guides/gitcat/) cover everyday use. The [README](../README.md#build-and-verification) contains build and verification commands for contributors.
