# GitCat code signing policy

Last updated: 2026-10-26.

This policy covers the build and signing process in the [official GitCat repository](https://github.com/catninth/gitcat) and its [GitHub releases](https://github.com/catninth/gitcat/releases). GitCat uses Tauri updater signatures. The current release workflow does not apply Windows Authenticode signatures.

## Windows installer trust

The Windows executable and NSIS installer produced by the checked-in workflow are not signed with a Windows code-signing certificate. The configured publisher label, **Cat Ninth**, is package metadata, not a certificate-verified identity. Windows may show an unknown-publisher or SmartScreen warning.

A Tauri `.sig` file does not establish Windows publisher identity or remove SmartScreen warnings.

## Update signatures

The [release workflow](.github/workflows/release.yml) supplies `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` from GitHub Actions secrets to the build/signing steps. These are updater-signing credentials, not an Authenticode certificate. The workflow requires detached `.sig` files for the Windows installer and Linux AppImage, `.deb`, and `.rpm` packages before publishing their updater entries.

The corresponding public key is embedded in [the Tauri configuration](apps/desktop/src-tauri/tauri.conf.json). The native updater uses the selected release's `latest.json`, checks that its version matches, and delegates package signature verification to Tauri before installation. A package that cannot be verified against the embedded public key is not accepted by the updater. See [Tauri's updater signing documentation](https://v2.tauri.app/plugin/updater/#signing-updates).

These signatures protect the updater's package-authenticity check. The manifest and release page rely on GitHub's HTTPS delivery and repository access controls; they are not separately signed by this workflow. Launching an installer downloaded manually does not invoke GitCat's updater verification.

## Build and publication

The workflow builds from the GitHub Actions run's checked-out source revision on GitHub-hosted Windows and Linux runners. It runs Rust formatting, Clippy, workspace tests, TypeScript checks, and platform packaging checks before publication. It prints package SHA-256 hashes in the build logs and publishes packages, updater signatures, and `latest.json` together after both platform jobs succeed. A checksum can detect a mismatch; it is not a substitute for a trusted signature.

Stable releases are started manually and publish only when the workflow's `publish` input is enabled. The nightly schedule is currently disabled. The retained scheduled-run logic would publish a rolling `nightly` pre-release from the default branch if the schedule were re-enabled. The stable in-app updater excludes pre-releases.

Repository maintainers with the necessary GitHub permissions control workflow execution, source changes, release publication, and signing-secret configuration. The checked-in workflow does not define a separate signing approver, protected-environment approval, or two-person release gate. External branch-protection and account settings are managed on GitHub and are not asserted by this policy.

## Reporting and future changes

Download official packages from the [GitCat releases page](https://github.com/catninth/gitcat/releases). Report unexpected artifacts or differences in the documented signing process through the [project issue tracker](https://github.com/catninth/gitcat/issues), including the release tag and asset name. Never publish private signing keys, tokens, or other secrets in an issue.

Changes to signing providers, keys, or approval rules must be reflected in this policy and the release configuration. Until such a change is implemented, the current process above applies; this document does not enable Authenticode signing by itself.
