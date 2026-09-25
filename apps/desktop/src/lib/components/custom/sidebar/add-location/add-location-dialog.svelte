<script lang="ts">
  import { Dialog } from "bits-ui";
  import { untrack } from "svelte";
  import CheckIcon from "@lucide/svelte/icons/check";
  import ChevronLeftIcon from "@lucide/svelte/icons/chevron-left";
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import CornerDownLeftIcon from "@lucide/svelte/icons/corner-down-left";
  import FingerprintIcon from "@lucide/svelte/icons/fingerprint-pattern";
  import RadarIcon from "@lucide/svelte/icons/radar";
  import ShieldAlertIcon from "@lucide/svelte/icons/shield-alert";
  import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";
  import "./add-location.css";
  import LocationFields from "./location-fields.svelte";
  import NetworkFields from "./network-fields.svelte";
  import ProtocolMark from "./protocol-mark.svelte";
  import { LocationForm } from "./location-form.svelte.js";
  import { LOCATION_GROUPS, LOCATION_TYPES, type LocationKind } from "./location-types.js";
  import { formatLocationUrl } from "$lib/remote/location-url.js";
  import { openLocalNetworkSettings, type DiscoveredServer } from "$lib/remote/network-locations.js";
  import { NetworkScan } from "$lib/remote/network-scan.svelte.js";
  import type { Location } from "$lib/tabs/tabs.js";

  let {
    open = $bindable(false),
    editing = null,
    onAdded,
    onUpdated,
  }: {
    open?: boolean;
    /** Saved network location to edit. Opens straight on the details step. */
    editing?: Location | null;
    onAdded: (location: Location) => void;
    onUpdated: (previousPath: string, location: Location) => void;
  } = $props();

  const form = new LocationForm();
  const scan = new NetworkScan();
  let step = $state<"choose" | "details">("choose");
  let address = $state("");
  let addressInvalid = $state(false);
  const comingSoon = new Set<LocationKind>(["gdrive", "onedrive", "dropbox"]);

  $effect(() => {
    const path = editing?.path;
    if (!open) {
      untrack(() => {
        form.reset();
        scan.reset();
        step = "choose";
        address = "";
        addressInvalid = false;
      });
      return;
    }
    if (path) {
      untrack(() => {
        step = "details";
        void form.edit(path);
      });
    }
  });

  let title = $derived(`${form.editingPath ? "Edit" : "Add"} ${LOCATION_TYPES[form.kind].label}`);

  function pick(kind: LocationKind) {
    form.choose(kind);
    step = "details";
  }

  function pickServer(server: DiscoveredServer) {
    scan.stop();
    form.fromDiscovered(server);
    step = "details";
  }

  function continueWithAddress(event: SubmitEvent) {
    event.preventDefault();
    if (form.applyUrl(address)) step = "details";
    else addressInvalid = true;
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const previousPath = form.editingPath;
    const location = await form.save();
    if (!location) return;
    if (previousPath) onUpdated(previousPath, location);
    else onAdded(location);
    open = false;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay class="loc-overlay add-location-overlay" />
    <Dialog.Content class="loc add-location" interactOutsideBehavior="ignore">
      <div class="loc-shell">
        {#if step === "choose"}
          <div class="loc-core pane">
            <Dialog.Title class="add-location-title">Add location</Dialog.Title>
            <form class="paste" onsubmit={continueWithAddress}>
              <input
                class="loc-input mono"
                class:invalid={addressInvalid}
                placeholder="Paste an address, like smb://nas.local/Photos"
                aria-label="Server address"
                spellcheck="false"
                autocomplete="off"
                bind:value={address}
                oninput={() => (addressInvalid = false)} />
              <button type="submit" class="loc-btn" disabled={!address.trim()}><CornerDownLeftIcon /> Continue</button>
            </form>
            {#if addressInvalid}
              <p class="loc-field-error">
                Not an address we recognize. Try smb://, sftp://, ftp://, nfs://, https:// or \\server\share.
              </p>
            {/if}

            <section class="nearby" aria-label="On your network">
              <div class="nearby-head">
                <p class="loc-section-label">On your network</p>
                {#if scan.scanning}
                  <span class="scan-state"><i></i> Looking for SMB servers…</span>
                {:else if scan.scanned}
                  <button type="button" class="link" onclick={() => void scan.start()}>Scan again</button>
                {/if}
              </div>

              {#if !scan.scanned}
                <div class="scan-intro">
                  <RadarIcon />
                  <p>Find SMB servers on your local network. Your system may ask to allow network access.</p>
                  <button type="button" class="loc-btn" onclick={() => void scan.start()}>Scan</button>
                </div>
              {:else}
                <div class="servers">
                  {#each scan.servers as server (server.host)}
                    <button type="button" class="server" onclick={() => pickServer(server)}>
                      <ProtocolMark kind="smb" />
                      <span class="copy">
                        <strong>{server.name}</strong>
                        <small>{server.source === "bonjour" ? server.host : "SMB on port 445"}</small>
                      </span>
                    </button>
                  {/each}
                  {#if scan.scanning && scan.servers.length < 2}
                    <div class="skeleton"></div>
                  {/if}
                </div>
                {#if scan.permissionDenied}
                  <div class="loc-note warn">
                    <TriangleAlertIcon />
                    <span>
                      Lite Explorer can’t reach your local network. Allow it in System Settings, Privacy &amp; Security,
                      Local Network, then scan again.
                      <button type="button" class="link" onclick={() => void openLocalNetworkSettings()}>Open Settings</button>
                    </span>
                  </div>
                {:else if scan.error}
                  <p class="loc-field-error">{scan.error}</p>
                {:else if !scan.scanning && scan.servers.length === 0}
                  <p class="loc-help">No SMB servers found. Add one by address or pick a type below.</p>
                {/if}
              {/if}
            </section>

            <div class="types">
              {#each LOCATION_GROUPS as group (group.label)}
                <div>
                  <p class="loc-section-label">{group.label}</p>
                  <div class="type-list">
                    {#each group.kinds as kind (kind)}
                      <button type="button" class="type" class:soon={comingSoon.has(kind)} disabled={comingSoon.has(kind)} onclick={() => pick(kind)}>
                        <ProtocolMark {kind} />
                        <span class="copy">
                          <strong>{LOCATION_TYPES[kind].label}{#if comingSoon.has(kind)} <em>Soon</em>{/if}</strong>
                          <small>{LOCATION_TYPES[kind].hint}</small>
                        </span>
                      </button>
                    {/each}
                  </div>
                </div>
              {/each}
            </div>

            <footer class="actions">
              <Dialog.Close type="button" class="loc-btn">Cancel</Dialog.Close>
            </footer>
          </div>
        {:else}
          <form class="loc-core pane" onsubmit={submit} oninput={form.edited}>
            <header class="details-head">
              {#if !form.editingPath}
                <button
                  type="button"
                  class="back"
                  aria-label="Back"
                  disabled={form.busy}
                  onclick={() => (step = "choose")}><ChevronLeftIcon /></button>
              {/if}
              <ProtocolMark kind={form.kind} />
              <div class="head-copy">
                <Dialog.Title class="add-location-title">{title}</Dialog.Title>
                <p class="loc-url">
                  {form.isNetwork ? formatLocationUrl(form.network) : LOCATION_TYPES[form.kind].hint}
                </p>
              </div>
            </header>

            <fieldset disabled={form.busy}>
              {#key form.kind}
                <div class="swap">
                  {#if form.isNetwork}
                    <NetworkFields bind:input={form.network} errors={form.networkErrors} editing={!!form.editingPath} />
                  {:else}
                    <LocationFields bind:input={form.cloud} />
                  {/if}
                </div>
              {/key}
            </fieldset>

            <div class="status" aria-live="polite">
              {#if form.status.state === "ok"}
                <p class="loc-status ok"><CheckIcon /> {form.status.message}</p>
              {:else if form.status.state === "error" && form.status.error.trust}
                {@const trust = form.status.error.trust}
                <div class="trust" class:changed={trust.changed}>
                  <p class="trust-title">
                    {#if trust.changed}<ShieldAlertIcon />{:else}<FingerprintIcon />{/if}
                    {trust.algorithm === "certificate"
                      ? trust.changed
                        ? "Certificate changed"
                        : "Untrusted certificate"
                      : trust.changed
                        ? "Host key changed"
                        : "Verify this server"}
                  </p>
                  <p>{form.status.message}</p>
                  <code>{trust.fingerprint}</code>
                  <div class="trust-actions">
                    <button type="button" class="loc-btn" onclick={form.edited}>Cancel</button>
                    <button type="button" class="loc-btn" class:primary={!trust.changed} class:danger={trust.changed} onclick={form.trust}>
                      {trust.changed ? "Trust the new one" : "Trust and continue"}
                    </button>
                  </div>
                </div>
              {:else if form.status.state === "error"}
                <p class="loc-status error"><CircleAlertIcon /> {form.status.message}</p>
              {/if}
            </div>

            <footer class="actions">
              <Dialog.Close type="button" class="loc-btn">Cancel</Dialog.Close>
              <button
                type="button"
                class="loc-btn"
                class:testing={form.status.state === "testing"}
                disabled={form.busy}
                onclick={form.test}>{form.status.state === "testing" ? "Testing" : "Test"}</button>
              <button
                type="submit"
                class="loc-btn primary"
                class:testing={form.status.state === "saving"}
                disabled={form.busy}>
                {form.status.state === "saving" ? "Saving" : form.editingPath ? "Save" : "Connect"}
              </button>
            </footer>
          </form>
        {/if}
      </div>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>

<style>
  :global(.add-location-overlay) {
    animation: add-location-fade 220ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  :global(.loc.add-location) {
    position: fixed;
    top: 50%;
    left: 50%;
    z-index: 61;
    width: min(600px, calc(100vw - 32px));
    max-height: calc(100dvh - 32px);
    overflow: auto;
    translate: -50% -50%;
    outline: none;
    animation: add-location-in 260ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  :global(.add-location-title) {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  .pane {
    display: grid;
    gap: 16px;
    padding: 18px;
    animation: loc-rise 240ms var(--loc-ease);
  }
  .paste {
    display: flex;
    gap: 8px;
  }
  .paste .loc-input,
  .paste .loc-btn {
    height: 34px;
  }
  .mono {
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 12px;
  }
  .trust {
    display: grid;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 9px;
    background: rgb(10 155 255 / 0.08);
    box-shadow: inset 0 0 0 1px rgb(10 155 255 / 0.28);
    font-size: 12px;
    line-height: 1.45;
    animation: loc-rise 220ms var(--loc-ease);
  }
  .trust.changed {
    background: rgb(255 107 94 / 0.09);
    box-shadow: inset 0 0 0 1px rgb(255 107 94 / 0.32);
  }
  .trust p {
    margin: 0;
    color: var(--loc-muted);
  }
  .trust .trust-title {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--loc-text);
    font-weight: 600;
  }
  .trust-title :global(svg) {
    width: 15px;
    height: 15px;
  }
  .trust code {
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--loc-field);
    color: var(--loc-text);
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 11px;
    word-break: break-all;
  }
  .trust-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
  .loc-btn.danger {
    background: #c2453a;
    color: #fff;
  }
  .nearby {
    display: grid;
    gap: 8px;
  }
  .nearby-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  .nearby-head .loc-section-label {
    margin: 0;
  }
  .scan-state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--loc-faint);
    font-size: 11px;
  }
  .scan-state i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--loc-accent);
    animation: scan-pulse 1.1s var(--loc-ease) infinite;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--loc-accent);
    font: inherit;
    font-size: 11.5px;
  }
  .link:hover {
    text-decoration: underline;
  }
  .scan-intro {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 9px;
    background: rgb(255 255 255 / 0.03);
    box-shadow: inset 0 0 0 1px var(--loc-line);
  }
  .scan-intro :global(svg) {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    color: var(--loc-muted);
  }
  .scan-intro p {
    flex: 1;
    margin: 0;
    color: var(--loc-muted);
    font-size: 12px;
    line-height: 1.4;
  }
  .servers {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px 8px;
    margin: 0 -8px;
  }
  .servers:empty {
    display: none;
  }
  .server {
    display: flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
    padding: 6px 8px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    animation: loc-rise 260ms var(--loc-ease);
    transition: background-color 160ms var(--loc-ease);
  }
  .server:hover,
  .server:focus-visible {
    outline: none;
    background: rgb(255 255 255 / 0.06);
  }
  .skeleton {
    height: 38px;
    margin: 0 8px;
    border-radius: 8px;
    background: linear-gradient(90deg, rgb(255 255 255 / 0.03), rgb(255 255 255 / 0.07), rgb(255 255 255 / 0.03));
    background-size: 200% 100%;
    animation: scan-skeleton 1.2s var(--loc-ease) infinite;
  }
  @keyframes scan-pulse {
    50% {
      opacity: 0.3;
    }
  }
  @keyframes scan-skeleton {
    to {
      background-position: -200% 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .scan-state i,
    .skeleton {
      animation: none;
    }
  }
  .types {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
    padding-top: 14px;
    border-top: 1px solid var(--loc-line);
  }
  .type-list {
    display: grid;
    gap: 2px;
    margin: 0 -8px;
  }
  .type {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 8px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    transition:
      background-color 160ms var(--loc-ease),
      transform 160ms var(--loc-ease);
  }
  .type:hover,
  .type:focus-visible {
    outline: none;
    background: rgb(255 255 255 / 0.06);
  }
  .type:active {
    transform: scale(0.98);
  }
  .type.soon {
    opacity: 0.48;
    cursor: not-allowed;
  }
  .type.soon:hover {
    background: transparent;
  }
  .type em {
    margin-left: 5px;
    color: var(--loc-faint);
    font-size: 9px;
    font-style: normal;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .copy {
    display: grid;
    min-width: 0;
  }
  .copy strong {
    font-size: 12.5px;
    font-weight: 500;
  }
  .copy small {
    overflow: hidden;
    color: var(--loc-faint);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .details-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .head-copy {
    display: grid;
    min-width: 0;
  }
  .back {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin-left: -4px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--loc-muted);
  }
  .back:hover {
    background: rgb(255 255 255 / 0.06);
    color: var(--loc-text);
  }
  .back :global(svg) {
    width: 16px;
    height: 16px;
  }
  fieldset {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }
  .swap {
    animation: loc-rise 240ms var(--loc-ease);
  }
  .status:empty {
    display: none;
  }
  @media (max-width: 560px) {
    .types,
    .servers {
      grid-template-columns: 1fr;
    }
  }
  @keyframes add-location-in {
    from {
      opacity: 0;
      transform: scale(0.96);
    }
  }
  @keyframes add-location-fade {
    from {
      opacity: 0;
    }
  }
</style>
