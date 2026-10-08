import { CalendarClock, Check } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { identityInitials, parseCoAuthors } from "../../lib";
import type { ChangedFile, CommitDetails as CommitDetailsType } from "../../lib/types";
import type { FileTreeItem, FileViewMode } from "../file-tree";
import { ChangeCountSummary, FileTree, FileTreeControls, fileChangeCounts, sumChangeCounts } from "../file-tree";
import { SidePanel } from "../ui";
import { MessageEditor, MessageView } from "./CommitMessage";
import { Avatar, CoAuthorRow, FilesPanel, IdentityRow, ParentRefs, StatsRow } from "./CommitSections";
import { ShaBar, ShaCopy } from "./ShaBar";

interface CommitDetailsProps {
    details: CommitDetailsType;
    /** Author pictures keyed by lower-cased email, as the graph rows use. */
    avatarImages?: ReadonlyMap<string, string>;
    selectedPath?: string;
    busy?: boolean;
    fileViewMode: FileViewMode;
    onFileViewModeChange: (mode: FileViewMode) => void;
    onSelectFile: (file: ChangedFile) => void;
    onCopySha: () => void;
    onJumpToCommit?: (oid: string) => void;
    onReword?: (message: string) => Promise<boolean>;
    // Opens the message editor once per token, when the token names this commit.
    editRequest?: { oid: string; token: number } | null;
}

function composeMessage(subject: string, body: string): string {
    const trimmedBody = body.trim();
    const trimmedSubject = subject.trim();
    return trimmedBody ? `${trimmedSubject}\n\n${trimmedBody}` : trimmedSubject;
}

const STATUS_LABEL: Record<string, string> = {
    added: "A",
    modified: "M",
    deleted: "D",
    renamed: "R",
    copied: "C",
    type_changed: "T",
    unmerged: "U",
};

export function CommitDetails({ details, avatarImages, selectedPath, busy = false, fileViewMode, onFileViewModeChange, onSelectFile, onCopySha, onJumpToCommit, onReword, editRequest }: CommitDetailsProps) {
    const [editing, setEditing] = useState(false);
    const [subject, setSubject] = useState(details.subject);
    const [body, setBody] = useState(details.body);
    const appliedEditToken = useRef(0);

    // Reset the editor whenever a different commit loads or the message changes
    // underneath (e.g. after a successful reword reloads details). An unseen
    // edit request for this commit opens it instead; the token keeps a later
    // refresh from reopening an editor the user has closed.
    //
    // Every dependency here is a primitive on purpose: the callers pass a fresh
    // `onReword` closure and `editRequest` object on each render, so depending on
    // their identities re-ran this effect -- and closed the editor the user had
    // just opened -- on any unrelated re-render of the pane.
    const canReword = Boolean(onReword);
    const requestedToken = editRequest && editRequest.oid === details.oid ? editRequest.token : null;
    useEffect(() => {
        const requested = canReword && requestedToken !== null && requestedToken !== appliedEditToken.current;
        if (requested && requestedToken !== null) appliedEditToken.current = requestedToken;
        setEditing(requested);
        setSubject(details.subject);
        setBody(details.body);
    }, [details.oid, details.subject, details.body, requestedToken, canReword]);

    const dirty = subject.trim() !== details.subject.trim() || body.trim() !== details.body.trim();
    const canSave = Boolean(onReword) && subject.trim().length > 0 && dirty && !busy;

    const submitReword = async () => {
        if (!canSave || !onReword) return;
        const ok = await onReword(composeMessage(subject, body));
        if (ok) setEditing(false);
    };
    const cancelReword = () => {
        setEditing(false);
        setSubject(details.subject);
        setBody(details.body);
    };
    const authored = new Date(details.authored_at.seconds * 1000);
    const initials = identityInitials(details.author.name);
    const authorImage = avatarImages?.get(details.author.email.trim().toLowerCase());
    const coAuthors = useMemo(() => parseCoAuthors(details.body), [details.body]);
    const fileItems = useMemo<FileTreeItem<ChangedFile>[]>(() => details.files.map((file) => ({
        id: file.new_path,
        path: file.new_path,
        data: file,
        status: file.status,
        statusLabel: STATUS_LABEL[file.status] ?? "M",
        additions: file.additions,
        deletions: file.deletions,
    })), [details.files]);
    const fileCounts = useMemo(
        () => sumChangeCounts(...details.files.map((file) => fileChangeCounts(file.status))),
        [details.files],
    );

    return (
        <SidePanel aria-label="Commit details">
            <ShaBar>
                <span>commit:</span>
                <ShaCopy oid={details.oid} onCopy={onCopySha} shortOid={details.short_oid} />
            </ShaBar>
            {editing ? (
                <MessageEditor
                    body={body}
                    busy={busy}
                    canSave={canSave}
                    onBodyChange={setBody}
                    onCancel={cancelReword}
                    onSubjectChange={setSubject}
                    onSubmit={() => void submitReword()}
                    subject={subject}
                />
            ) : (
                <MessageView
                    body={details.body}
                    onEdit={onReword ? () => setEditing(true) : undefined}
                    subject={details.subject}
                />
            )}
            <IdentityRow>
                <Avatar image={authorImage} initials={initials} />
                <div className="flex min-w-0 flex-col gap-0.5">
                    <strong className="overflow-hidden text-ellipsis whitespace-nowrap" title={details.author.email}>{details.author.name}</strong>
                    <small className="mt-0.75 flex items-center gap-1 text-[10px] text-muted">
                        <CalendarClock size={12} /> {authored.toLocaleString()}
                    </small>
                </div>
                <ParentRefs onJump={onJumpToCommit} parentOids={details.parent_oids} />
            </IdentityRow>
            <CoAuthorRow coAuthors={coAuthors} />
            <StatsRow>
                <span className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1">
                    <ChangeCountSummary counts={fileCounts} labels size="md" />
                </span>
            </StatsRow>
            <FilesPanel>
                <FileTreeControls mode={fileViewMode} onModeChange={onFileViewModeChange} />
                <FileTree
                    ariaLabel="Changed files"
                    className="flex-1 px-1.5 pb-2 pt-0.75"
                    emptyState={<><Check aria-hidden="true" size={16} /> No changed files</>}
                    items={fileItems}
                    mode={fileViewMode}
                    onSelect={onSelectFile}
                    selectedId={selectedPath}
                />
            </FilesPanel>
        </SidePanel>
    );
}
