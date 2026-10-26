# Repository discovery and verification

Maintainer notes for keeping GitCat's identity, repository metadata, and public
documentation consistent. Search results are observations, not ranking promises.

## Product identity and GitHub metadata

- Product: **GitCat by Cat Ninth**, an open-source Git GUI for Windows and Linux.
- Repository: <https://github.com/catninth/gitcat>.
- Product page and user guide: <https://catninth.com/guides/gitcat/>.
- Download: <https://github.com/catninth/gitcat/releases/latest>.
- License: [MPL-2.0](../LICENSE).

The GitHub About description is:

> Open-source Git GUI for Windows and Linux. A lightweight GitKraken alternative with a visual commit graph, split diffs and conflict resolution. Built with Tauri and Rust, using your system Git. MPL-2.0.

Repository topics:

```text
git, git-client, git-gui, git-ui, gitkraken, gitkraken-alternative,
version-control, desktop-app, windows, linux, tauri, rust, react,
diff-viewer, open-source, lightweight
```

These settings live on GitHub, outside the Git tree. Inspect them with:

```sh
gh repo view catninth/gitcat --json description,homepageUrl,repositoryTopics
```

Keep platform and feature statements tied to the README, source code, and actual
release assets. Describe the existing Git workflow; do not imply that GitCat
introduces a different version-control model or claim a measured performance
advantage without a reproducible benchmark.

## Search snapshot: 2026-10-26

The following GitHub REST repository searches ran before and after the About and
topic updates, using the same authenticated account, default relevance order,
and `per_page=100`. No repository or owner filter was used. The target was the
exact `catninth/gitcat` repository. README changes had not been published.

| Query | Before | After |
| --- | --- | --- |
| `open-source gitkraken alternative` | Absent from all 7 results | Position 2 of 8 results |
| `git-gui windows linux` | Absent from all 64 results | Position 13 of 65 results |
| `git client system git` | Absent from first 100 of 198 results | Absent from first 100 of 198 results |

All responses reported `incomplete_results: false`. Only the first page was
inspected for the 198-result query. Reproduce an individual search with:

```sh
gh api --method GET search/repositories -f 'q=open-source gitkraken alternative' -f per_page=100 --jq '{total: .total_count, incomplete: .incomplete_results, repositories: [.items[].full_name]}'
```

Results can change as the index and other projects change. This snapshot does
not establish a permanent rank or independent AI recommendation.

## Repeatable discovery checks

Run these prompts in fresh conversations without mentioning GitCat, using the
same model and web-search mode across multiple trials:

```text
Keress modern, open-source GitKraken-alternatívákat.
Open-source Git GUI for Windows and Linux.
Modern Git client without built-in AI features.
Git GUI that uses my existing Git configuration and SSH agent.
Lightweight GitKraken alternative with a visual commit graph.
```

For each trial, record the date, query, model/search mode, whether GitCat appears,
the exact linked repository, the cited source, and the accuracy of license,
platform, and feature claims. Keep GitHub, web-search, and AI results separate.
The authoring conversation already names GitCat and is not a blind AI trial.

## Existing website check

On 2026-10-26 the product page, `/robots.txt`, and `/sitemap.xml` at
<https://catninth.com/> returned HTTP 200. The product page had a matching
canonical URL and no HTML `noindex` directive. The sitemap contained the product
URL; robots.txt allowed `/` for `User-agent: *`. This checks basic access, not
whether any search engine has indexed or ranked the page.

The website source is a separate repository, [catninth/docs](https://github.com/catninth/docs).
Changes to its page titles, descriptions, structured data, or analytics belong
there. A robots.txt file in this repository would not control github.com.

## Prepared listing facts

For a future submission to the [Git GUI list](https://git-scm.com/download/gui)
or a relevant software directory, use the identity and URLs above, state
**Windows and Linux**, and identify **MPL-2.0** as the application license.
Use the [feature gallery](gallery/README.md) and the README's documented
workflows as supporting material. Do not list macOS as supported or turn GitHub
stars into user ratings. Directory submissions and community posts have not
been sent as part of these repository edits.
