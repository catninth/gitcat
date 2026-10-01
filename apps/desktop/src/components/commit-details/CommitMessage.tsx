import { PencilFilled } from "../ui/PencilFilled";
import type { ReactNode } from "react";

import { cx } from "../../lib";
import { Button, Input, TextArea } from "../ui";

const CARD = "m-2.5 rounded-[5px] border border-border bg-background p-3.25 text-left";

const HEADING = "text-[16px] font-[560] leading-[1.42] text-foreground";

export function MessageCard({ children }: { children: ReactNode }) {
    return <div className={CARD}>{children}</div>;
}

function MessageText({ body }: { body: string }) {
    if (!body) return <></>;
    return (
        <p className="mt-3 whitespace-pre-wrap text-left leading-[1.55] text-[color-mix(in_srgb,var(--gc-text)_80%,var(--gc-muted))]">
            {body}
        </p>
    );
}

export function MessageView({ subject, body, onEdit }: { subject: string; body: string; onEdit?: () => void }) {
    if (!onEdit) {
        return (
            <MessageCard>
                <h2 className={HEADING}>{subject}</h2>
                <MessageText body={body} />
            </MessageCard>
        );
    }
    return (
        <button
            className={cx(
                CARD,
                "group/msg block w-auto cursor-text text-inherit transition-[border-color,background-color] duration-120",
                "hover:border-[color-mix(in_srgb,var(--gc-accent)_45%,var(--gc-border))] hover:bg-[color-mix(in_srgb,var(--gc-accent)_5%,var(--gc-background))]",
            )}
            onClick={onEdit}
            title="Click to edit commit message"
            type="button"
        >
            <h2 className={cx(HEADING, "flex items-start gap-1.5")}>
                {subject}
                <PencilFilled
                    aria-hidden="true"
                    className="ml-auto mt-0.5 shrink-0 text-muted opacity-0 transition-opacity duration-120 group-hover/msg:opacity-100 group-focus-visible/msg:opacity-100"
                    size={13}
                />
            </h2>
            <MessageText body={body} />
        </button>
    );
}

export function MessageEditor({
    body,
    busy,
    canSave,
    subject,
    onBodyChange,
    onCancel,
    onSubjectChange,
    onSubmit,
}: {
    body: string;
    busy: boolean;
    canSave: boolean;
    subject: string;
    onBodyChange: (value: string) => void;
    onCancel: () => void;
    onSubjectChange: (value: string) => void;
    onSubmit: () => void;
}) {
    const field = "rounded-[5px] border border-border bg-surface text-foreground outline-0 focus:border-accent";
    return (
        <form
            className={cx(CARD, "flex flex-col gap-2")}
            onKeyDown={(event) => {
                if (event.key === "Escape") {
                    event.preventDefault();
                    onCancel();
                }
            }}
            onSubmit={(event) => {
                event.preventDefault();
                onSubmit();
            }}
        >
            <div className="flex items-center gap-2">
                <Input
                    aria-label="Commit summary"
                    autoFocus
                    className={cx(field, "min-w-0 flex-1 px-2.25 py-1.75 text-[14px] font-[560]")}
                    disabled={busy}
                    onChange={(event) => onSubjectChange(event.target.value)}
                    placeholder="Summary"
                    value={subject}
                />
                <span className="shrink-0 text-[10px] tabular-nums text-muted">{subject.length}</span>
            </div>
            <TextArea
                aria-label="Commit description"
                className={cx(field, "min-h-19.5 w-full resize-y px-2.25 py-2 leading-normal")}
                disabled={busy}
                onChange={(event) => onBodyChange(event.target.value)}
                placeholder="Description"
                rows={4}
                value={body}
            />
            <div className="flex gap-2">
                <Button className="flex-1" compact disabled={!canSave} tone="accent" type="submit">Update Message</Button>
                <Button className="flex-1" compact disabled={busy} onClick={onCancel} type="button">Cancel</Button>
            </div>
        </form>
    );
}
