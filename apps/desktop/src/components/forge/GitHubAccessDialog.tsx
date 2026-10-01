import { useEffect } from "react";

import { credentialFor, useForgeConnections } from "../../app/forgeConnections";
import { integrationById } from "../../lib/integrations";
import { Button, Modal } from "../ui";
import { ForgeConnectPanel } from "./ForgeConnectPanel";

/** The failed command targets GitHub.com, even if the user has switched tabs. */
export function GitHubAccessDialog({ onClose }: { onClose: () => void }) {
  const connections = useForgeConnections();
  const otherSignIn = connections.pending?.host ?? connections.starting;
  const credential = credentialFor(connections, "github.com");
  const missingScopes = credential?.missing_scopes ?? [];
  const completed = connections.notice?.host === "github.com"
    && connections.notice.tone === "success"
    && !connections.pending
    && !connections.starting
    && Boolean(credential)
    && missingScopes.length === 0;

  useEffect(() => {
    if (completed) onClose();
  }, [completed, onClose]);

  if (completed) return null;

  return (
    <Modal
      title="Restore GitHub access"
      description="Sign in with an account that can write to the repository and approve GitCat's requested access on GitHub."
      footer={<Button onClick={onClose}>Close</Button>}
      onClose={onClose}
    >
      <ForgeConnectPanel
        host="github.com"
        integration={integrationById("github")!}
        offerReconnect
      />
      <p className="mt-3 text-[11px] leading-[1.5] text-muted">
        {otherSignIn && otherSignIn !== "github.com"
          ? `Finish or cancel the sign-in to ${otherSignIn} first.`
          : "Approve the requested access on GitHub. Your existing sign-in is kept until the new sign-in finishes."}
      </p>
      <p className="mt-2 text-[11px] leading-[1.5] text-muted">
        If access is still denied, the repository owner or organization may need to grant write
        access or approve GitCat. Signing in cannot grant permissions your account does not have.
      </p>
    </Modal>
  );
}
