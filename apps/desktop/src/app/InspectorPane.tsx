import { CommitDetails, CommitDetailsSkeleton } from "../components/commit-details";
import type { FolderCollapseTarget } from "../components/file-tree";
import { SidePanel, Spinner } from "../components/ui";
import { WorktreePanel, type CommitDraft } from "../components/worktree";
import { gitcatApi } from "../lib/api";
import type {
    AppSettings,
    ChangedFile,
    ConflictResolution,
    FileViewMode,
    RepositorySnapshot,
    StatusEntry,
} from "../lib/types";
import { currentBranch } from "./branches";
import type { CommitDetailsPanel, RunMutation, RuntimeRepository } from "./state";

export interface InspectorPaneProps {
    abortActiveOperation: () => void;
    activeCommitDraft: CommitDraft;
    activeRepository: RuntimeRepository | undefined;
    autoResolveActiveConflicts: () => void;
    avatarImages: ReadonlyMap<string, string>;
    busy: boolean;
    changeFileViewMode: (mode: FileViewMode) => void;
    continueActiveOperation: () => void;
    copySha: (oid: string) => Promise<void>;
    createPatchFile: (paths: string[], staged: boolean) => Promise<void>;
    details: CommitDetailsPanel | null;
    jumpToCommit: (oid: string) => void;
    openCommitFile: (file: ChangedFile) => void;
    openConflictEditor: (entry: StatusEntry) => Promise<void>;
    openWorktreeDiff: (entry: StatusEntry, staged: boolean) => void;
    overviewLoading: boolean;
    resolveConflictEntry: (entry: StatusEntry, resolution: ConflictResolution) => void;
    resolveConflictPaths: (paths: string[], resolution: ConflictResolution) => void;
    rewordCommit: (oid: string, message: string) => Promise<boolean>;
    rewordRequest: { oid: string; token: number } | null;
    runMutation: RunMutation;
    selectedOid: string | null;
    selectedPath: string | undefined;
    selectedWorktreeFile: { path: string; staged: boolean } | null;
    settings: AppSettings;
    showError: (title: string, error: unknown) => void;
    skipActiveOperation: () => void;
    snapshot: RepositorySnapshot | null;
    stageCollapse: { target: FolderCollapseTarget; staged: boolean; token: number } | null;
    stagePaths: (paths: string[], collapse?: FolderCollapseTarget) => void;
    unstagePaths: (paths: string[], collapse?: FolderCollapseTarget) => void;
    updateActiveCommitDraft: (draft: CommitDraft) => void;
    wipSelected: boolean;
}

export function InspectorPane({
    abortActiveOperation,
    activeCommitDraft,
    activeRepository,
    autoResolveActiveConflicts,
    avatarImages,
    busy,
    changeFileViewMode,
    continueActiveOperation,
    copySha,
    createPatchFile,
    details,
    jumpToCommit,
    openCommitFile,
    openConflictEditor,
    openWorktreeDiff,
    overviewLoading,
    resolveConflictEntry,
    resolveConflictPaths,
    rewordCommit,
    rewordRequest,
    runMutation,
    selectedOid,
    selectedPath,
    selectedWorktreeFile,
    settings,
    showError,
    skipActiveOperation,
    snapshot,
    stageCollapse,
    stagePaths,
    unstagePaths,
    updateActiveCommitDraft,
    wipSelected,
}: InspectorPaneProps) {
    return (
        <>
        {wipSelected && snapshot ? (
            <WorktreePanel
                busy={busy}
                branchName={currentBranch(snapshot)}
                collapse={stageCollapse ?? undefined}
                commitKeybind={settings.keybinds.commit}
                draft={activeCommitDraft}
                fileViewMode={settings.file_view_mode}
                onFileViewModeChange={changeFileViewMode}
                onAutoResolveConflicts={autoResolveActiveConflicts}
                onCommit={(message, amend, signoff) => runMutation(amend ? "Commit amended" : "Commit created", (repository) => gitcatApi.createCommit(repository.repository_id, { message, amend, signoff }))}
                onDraftChange={updateActiveCommitDraft}
                onOpenDiff={openWorktreeDiff}
                onOpenConflict={(entry) => void openConflictEditor(entry)}
                onResolveConflict={resolveConflictEntry}
                onResolveConflicts={resolveConflictPaths}
                onContinueOperation={continueActiveOperation}
                onSkipOperation={skipActiveOperation}
                onAbortOperation={abortActiveOperation}
                operationProgress={snapshot.operation_progress ?? null}
                operationSource={snapshot.operation_source ?? null}
                onStage={stagePaths}
                onUnstage={unstagePaths}
                onDiscard={(paths) => {
                    if (!window.confirm(`Discard all changes to ${paths.length === 1 ? paths[0] : `${paths.length} files`}? This cannot be undone.`)) return;
                    void runMutation("Changes discarded", (repository) => gitcatApi.discardPaths(repository.repository_id, paths));
                }}
                onStashFile={(paths) => void runMutation("File stashed", (repository) => gitcatApi.stashFile(repository.repository_id, paths, null))}
                onIgnore={(patterns) => void runMutation("Updated .gitignore", (repository) => gitcatApi.appendGitignore(repository.repository_id, patterns))}
                onCreatePatch={(paths, staged) => void createPatchFile(paths, staged)}
                onOpenFolder={(path) => {
                    if (!activeRepository) return;
                    void gitcatApi.openRepositoryFolder(activeRepository.repository_id, path)
                        .catch((error) => showError("Could not open folder", error));
                }}
                onRevealFile={(path) => {
                    if (!activeRepository) return;
                    void gitcatApi.openRepositoryFolder(activeRepository.repository_id, path)
                        .catch((error) => showError("Could not show file", error));
                }}
                repositoryRoot={activeRepository?.info.root ?? ""}
                operation={snapshot.operation_state}
                selectedFile={selectedWorktreeFile}
                status={snapshot.status}
            />
        ) : details ? (
            <CommitDetails
                avatarImages={avatarImages}
                busy={busy || overviewLoading}
                details={details}
                fileViewMode={settings.file_view_mode}
                onFileViewModeChange={changeFileViewMode}
                onCopySha={() => void copySha(details.oid)}
                onJumpToCommit={jumpToCommit}
                onReword={snapshot ? (message) => rewordCommit(details.oid, message) : undefined}
                editRequest={rewordRequest}
                onSelectFile={openCommitFile}
                selectedPath={selectedPath}
            />
        ) : selectedOid && !wipSelected ? (
            <CommitDetailsSkeleton />
        ) : (
            <SidePanel className="items-center justify-center gap-2.25 text-muted">
                <Spinner label="Loading commit details" /> Select a commit
            </SidePanel>
        )}
        </>
    );
}
