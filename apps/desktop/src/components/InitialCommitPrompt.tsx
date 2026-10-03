import { Button, Spinner } from "./ui";

// Git has no branch until something is committed on it, so a repository with an
// unborn HEAD has nothing to draw and nothing to build on. The offer sits in a
// bar above the history rather than in the empty graph itself: it is a question
// about the repository rather than about a row. Staging files in the working
// tree panel and committing there is the other way out of this state, so the
// bar states the offer and leaves it at that.
//
// With a remote configured, "no commits" only describes what has been fetched.
// While the remote is being asked the bar says so and holds the offer back, and
// the offer itself names the remote check it will run first.
export function InitialCommitPrompt({
  busy,
  checkingRemote,
  onInitialize,
  remoteCount,
  remoteName,
  repositoryName,
  stagedCount,
}: {
  busy: boolean;
  checkingRemote: boolean;
  onInitialize: () => void;
  remoteCount: number;
  remoteName: string | null;
  repositoryName: string;
  stagedCount: number;
}) {
  const remoteLabel = remoteCount > 1 ? "the remotes" : remoteName ?? "the remote";
  const offer = stagedCount > 0
    ? `Do you want GitCat to commit the ${stagedCount} staged file${stagedCount === 1 ? "" : "s"} as that commit?`
    : "Do you want GitCat to make a commit for you? It writes a README.md named after the repository.";
  const remoteNote = remoteCount > 0
    ? ` GitCat checks ${remoteLabel} for existing commits first.`
    : "";

  return (
    <div
      className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-2 border-b border-border bg-[color-mix(in_srgb,var(--gc-accent)_10%,var(--gc-panel))] px-3.5 py-2"
      role="status"
    >
      {checkingRemote ? (
        <p className="flex min-w-0 flex-1 items-center gap-2 text-[12px] leading-[1.5]">
          <Spinner label={`Checking ${remoteLabel}`} />
          <span>
            Repository <span className="font-semibold">{repositoryName}</span> has no local commits. Checking {remoteLabel} for existing commits…
          </span>
        </p>
      ) : (
        <p className="min-w-0 flex-1 text-[12px] leading-[1.5]">
          Repository <span className="font-semibold">{repositoryName}</span> has no commits yet. {offer}{remoteNote}
        </p>
      )}
      <Button compact disabled={busy || checkingRemote} onClick={onInitialize} tone="accent">
        Initialize
      </Button>
    </div>
  );
}
