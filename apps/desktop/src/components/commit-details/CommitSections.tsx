import { useState } from "react";
import type { ReactNode } from "react";

import { cx, identityInitials } from "../../lib";
import type { Identity } from "../../lib/types";
import { ShaChip } from "./ShaBar";

export function IdentityRow({ children }: { children: ReactNode }) {
  return <div className="flex gap-2.5 px-3 pb-3 pt-0.75">{children}</div>;
}

// Renders the shimmer placeholder when no initials are known yet, and the
// author's picture once the hosting service has named them. A picture that
// fails to decode falls back to the initials, so a stale cache entry never
// leaves an empty square.
export function Avatar({ image, initials }: { image?: string; initials?: string }) {
  const [failed, setFailed] = useState(false);
  const shape = "size-9.75 shrink-0 rounded";
  if (initials === undefined) return <span className={cx("skeleton", shape)} />;
  if (image && !failed) {
    return (
      <img
        alt=""
        className={cx(shape, "border object-cover")}
        onError={() => setFailed(true)}
        src={image}
      />
    );
  }
  return (
    <span
      className={cx(
        shape,
        "grid place-items-center border font-extrabold text-accent",
      )}
    >
      {initials || "?"}
    </span>
  );
}

export function ParentRefs({ parentOids, onJump }: { parentOids: readonly string[]; onJump?: (oid: string) => void }) {
  if (parentOids.length === 0) return null;
  return (
    <div className="ml-auto flex shrink-0 items-baseline gap-1 text-[11px] text-muted">
      <span>{parentOids.length > 1 ? "parents:" : "parent:"}</span>
      <span className="flex flex-col items-end gap-0.5">
        {parentOids.map((oid) => (onJump ? (
          <ShaChip
            align="right"
            ariaLabel={`Jump to parent commit ${oid} in graph`}
            hint="Jump to commit in graph"
            key={oid}
            oid={oid}
            onClick={() => onJump(oid)}
            shortOid={oid.slice(0, 7)}
          />
        ) : (
          <code className="px-0.75 text-accent" key={oid} title={oid}>
            {oid.slice(0, 7)}
          </code>
        )))}
      </span>
    </div>
  );
}

export function CoAuthorRow({ coAuthors }: { coAuthors: readonly Identity[] }) {
  if (coAuthors.length === 0) return null;
  return (
    <div className="flex items-center gap-2 px-3 pb-3">
      <span className="text-[10px] font-[750] uppercase tracking-[0.06em] text-muted">
        Co-authors
      </span>
      <ul className="flex flex-wrap items-center gap-1.5">
        {coAuthors.map((coAuthor, index) => (
          <li key={`${index}:${coAuthor.name}:${coAuthor.email}`}>
            <span
              className={cx(
                "size-6 shrink-0 rounded",
                "grid place-items-center border text-[10px] font-extrabold text-accent",
              )}
              title={coAuthor.email || coAuthor.name}
            >
              {identityInitials(coAuthor.name) || "?"}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function StatsRow({ children }: { children: ReactNode }) {
  return <div className="flex items-center gap-2 px-3 pb-2">{children}</div>;
}

export function FilesPanel({ children }: { children: ReactNode }) {
  return <div className="flex min-h-0 flex-1 flex-col pt-2">{children}</div>;
}
