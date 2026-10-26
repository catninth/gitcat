# GitCat FAQ

GitCat by Cat Ninth is the open-source desktop Git client maintained at [catninth/gitcat](https://github.com/catninth/gitcat).

[Download](https://github.com/catninth/gitcat/releases/latest) · [Product overview](https://catninth.com/guides/gitcat/) · [User guide](https://catninth.com/guides/gitcat/first-repository/) · [README](../README.md)

## Is GitCat open source? What is the license?

GitCat's own source code is licensed under [MPL-2.0](../LICENSE). Third-party dependencies and vendored files retain their respective licenses and notices. The official source and release packages are in the [catninth/gitcat repository](https://github.com/catninth/gitcat).

## Which platforms are supported?

Windows 10/11 x64 is the primary target. Linux x86_64 packages are available as AppImage, `.deb`, and `.rpm`; Linux receives less manual UX verification than Windows. macOS is not packaged or verified.

Git 2.31+ is required. Conflict preflight needs Git 2.38+ with the required `merge-tree` support and reports when that support is unavailable. Linux builds use Ubuntu 24.04 as their build baseline and need a compatible WebKitGTK 4.1 environment. See the [installation guide](https://catninth.com/guides/gitcat/install/).

## Do I need an account or a GitHub login?

There is no GitCat cloud account required for local repository work. You can open an existing repository, review changes, and commit using your system Git configuration. Remote operations use your Git credential helpers or SSH setup; GitCat also offers optional hosting connections.

Connected GitHub.com and GitHub Enterprise accounts support hosted repository browsing, cloning, and creation. Other listed providers currently offer links only, with hosted clone/create integration disabled. Ordinary Git remote URLs remain usable through system Git. GitCat does not provide an interactive terminal credential prompt. See [hosting integrations](https://catninth.com/guides/gitcat/integrations/) for setup.

## Does GitCat include AI features?

No. GitCat has no built-in AI features. Its visual Git workflow uses system Git and does not replace Git with a different version-control model. Tools, hooks, filters, or services you configure separately have their own behavior.

## Does GitCat send analytics or access the network?

GitCat has no built-in usage analytics, advertising, or automatic crash-report upload service. Repository contents are processed on your computer; Git operations such as push send data to the remote you configured.

Network access still occurs. GitCat checks for updates automatically, auto-fetches the active repository by default, and can query hosting APIs or download author, owner, and account pictures. Public hosting lookups can run without a connected account. Gravatar fallback is off by default.

Preferences can disable auto-fetch, author pictures, Gravatar fallback, pull-request decorations, and check decorations. These switches do not provide a complete offline mode: update checks, repository-location requests, and separately displayed owner/account images remain possible. There is no in-app switch for update checks. The [privacy notice](../PRIVACY.md) documents destinations, data, defaults, and controls.

## Where are connected credentials stored?

Connected access tokens and OAuth refresh tokens use Windows Credential Manager or Linux Secret Service. System Git credentials are managed separately by your configured helpers or SSH agent. Disconnecting an account in GitCat removes its local credential; revoking provider authorization is a separate action. See [data on your computer](../PRIVACY.md#data-on-your-computer) and [controls and retention](../PRIVACY.md#controls-and-retention).

## Why does Windows show an unknown publisher?

The checked-in release workflow does not apply Windows Authenticode signatures. The Cat Ninth publisher label is package metadata. Updater signatures verify packages during in-app installation, but do not establish Windows publisher identity or remove SmartScreen warnings. Running a manually downloaded installer does not invoke the in-app updater verification. Read the [code signing policy](../CODE_SIGNING.md).

## Is GitCat a GitKraken replacement for my workflow?

Evaluate the tasks you use: repository tabs, graph navigation, staging, diffs, branches, Git remotes, and conflict resolution are available. Some integrations and controls are incomplete, and macOS is not packaged. The [GitKraken alternative guide](GITKRAKEN_ALTERNATIVE.md) walks through those decisions without assuming feature parity.

## Where can I report a problem?

Use the [GitCat issue tracker](https://github.com/catninth/gitcat/issues). Include the GitCat version, operating system, Git version, steps, and the relevant error text. Remove credentials and private repository contents before posting publicly. For setup errors, start with [troubleshooting](https://catninth.com/guides/gitcat/troubleshooting/).
