import { Plus, X } from "lucide-react";
import { useState } from "react";

import { cx } from "../../lib";
import { hostNameError, INTEGRATIONS, selfHostedHosts } from "../../lib/integrations";
import type { Integration } from "../../lib/integrations";
import type { ForgeKind } from "../../lib/types";
import { ForgeConnectPanel } from "../forge";
import { Badge, Button, IconButton, Input } from "../ui";
import { FIELD_INPUT } from "./SettingsField";

/**
 * One page per hosting service: what GitCat can do with it, which hosts it
 * covers, and the connection itself.
 *
 * Naming a self-hosted host is what tells the rest of the application which
 * service lives there, so the host list and the connection belong together
 * rather than in separate sections.
 */
export function IntegrationsPage({
  onOverridesChange,
  overrides,
}: {
  onOverridesChange: (overrides: Record<string, ForgeKind>) => void;
  overrides: Readonly<Record<string, ForgeKind>>;
}) {
  const [selectedId, setSelectedId] = useState(INTEGRATIONS[0].id);
  const selected = INTEGRATIONS.find((integration) => integration.id === selectedId) ?? INTEGRATIONS[0];

  return (
    <div className="grid grid-cols-[210px_minmax(0,1fr)] gap-4.5 max-[840px]:grid-cols-1">
      <nav
        aria-label="Hosting services"
        className="flex flex-col gap-0.5 rounded-[7px] border border-border bg-background/45 p-1.25 max-[840px]:flex-row max-[840px]:overflow-auto"
      >
        {INTEGRATIONS.map((integration) => (
          <button
            aria-current={integration.id === selected.id ? "page" : undefined}
            className={cx(
              "flex w-full cursor-pointer items-center gap-2.25 rounded-[5px] px-2.25 py-1.75 text-left text-[13px] whitespace-nowrap",
              integration.id === selected.id
                ? "bg-accent/12 font-[650] text-accent"
                : "text-muted hover:bg-foreground/6 hover:text-foreground",
            )}
            key={integration.id}
            onClick={() => setSelectedId(integration.id)}
            type="button"
          >
            <integration.icon size={15} />
            <span className="flex-1 overflow-hidden text-ellipsis">{integration.label}</span>
          </button>
        ))}
      </nav>

      <section className="min-w-0">
        <header className="mb-3 flex items-center gap-2">
          <selected.icon className="text-accent" size={17} />
          <h3 className="text-[15px] font-[640]">{selected.label}</h3>
          {selected.support === "links_only" ? <Badge>Coming soon</Badge> : null}
        </header>

        <p className="mb-4 text-[13px] leading-[1.55] text-muted">
          {selected.support === "links_only"
            ? "Name the hosts of this service so GitCat builds their links correctly."
            : "Connecting raises the request limit, reaches private repositories, and lets GitCat list the repositories you can clone. The credential is kept in the operating system credential store and is never part of a settings export."}
        </p>

        {selected.host !== null ? (
          <ForgeConnectPanel allowToken host={selected.host} integration={selected} />
        ) : (
          <SelfHostedHosts
            integration={selected}
            onOverridesChange={onOverridesChange}
            overrides={overrides}
          />
        )}
      </section>
    </div>
  );
}

/**
 * The hosts one self-hosted service is installed on.
 *
 * A host is remembered as a settings override, which is also what tells the
 * graph and the link builder what kind of service answers there -- so removing
 * a host here is what makes GitCat stop treating it as one.
 */
function SelfHostedHosts({
  integration,
  onOverridesChange,
  overrides,
}: {
  integration: Integration;
  onOverridesChange: (overrides: Record<string, ForgeKind>) => void;
  overrides: Readonly<Record<string, ForgeKind>>;
}) {
  const [draft, setDraft] = useState("");
  const hosts = selfHostedHosts(integration, overrides);
  const candidate = draft.trim().toLowerCase();
  const error = hostNameError(candidate);
  const duplicate = candidate.length > 0 && candidate in overrides;

  const add = () => {
    if (!candidate || error || duplicate) return;
    onOverridesChange({ ...overrides, [candidate]: integration.forge });
    setDraft("");
  };

  const remove = (host: string) => {
    const next = { ...overrides };
    delete next[host];
    onOverridesChange(next);
  };

  return (
    <div className="flex flex-col gap-3">
      {hosts.map((host) => (
        <div className="rounded-[7px] border border-border bg-background/45 p-2.5" key={host}>
          <div className="mb-2 flex items-center gap-1.5 text-[13px]">
            <span className="min-w-0 grow truncate font-[650] text-foreground">{host}</span>
            <IconButton
              aria-label={`Remove ${host}`}
              onClick={() => remove(host)}
              title="Remove host"
            >
              <X size={13} />
            </IconButton>
          </div>
          <ForgeConnectPanel allowToken host={host} integration={integration} />
        </div>
      ))}

      {hosts.length === 0 ? (
        <p className="text-[13px] leading-[1.45] text-muted/72">
          No host named yet. GitCat cannot recognise a self-hosted install from its address alone.
        </p>
      ) : null}

      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-1.5">
          <Input
            aria-label={`Host of a ${integration.label} install`}
            className={FIELD_INPUT}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => { if (event.key === "Enter") add(); }}
            placeholder="git.example.com"
            spellCheck={false}
            value={draft}
          />
          <Button
            compact
            disabled={!candidate || Boolean(error) || duplicate}
            icon={<Plus size={13} />}
            onClick={add}
          >
            Add host
          </Button>
        </div>
        {error ? <small className="text-[12px] text-danger">{error}</small> : null}
        {duplicate ? <small className="text-[12px] text-danger">this host is already named</small> : null}
        <small className="text-[12px] text-muted/72">
          Hosts are saved with the dialog; credentials are stored as soon as they are connected.
        </small>
      </div>
    </div>
  );
}
