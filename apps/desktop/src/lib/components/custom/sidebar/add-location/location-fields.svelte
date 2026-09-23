<script lang="ts">
  import EyeIcon from "@lucide/svelte/icons/eye";
  import EyeOffIcon from "@lucide/svelte/icons/eye-off";
  import { AWS_REGIONS, isPendingProvider, type RemoteLocationInput } from "$lib/remote/remote-locations.js";

  let { input = $bindable() }: { input: RemoteLocationInput } = $props();

  let revealSecret = $state(false);
</script>

{#if isPendingProvider(input.provider)}
  <div class="provider-pending">
    <strong>{input.provider === "azblob" ? "Azure Blob setup" : "OAuth setup required"}</strong>
    <p>
      {input.provider === "azblob"
        ? "Azure Blob will be available after the OpenDAL connection layer is enabled."
        : "Register this desktop OAuth app first. Lite Explorer will use your own client configuration, never a shared client ID."}
    </p>
  </div>
{:else if input.provider === "gdrive"}
  <div class="fields">
    <div class="wide">
      <label class="loc-label" for="loc-name">Name <small>optional</small></label>
      <input id="loc-name" class="loc-input" placeholder="Google Drive" autocomplete="off" bind:value={input.name} />
    </div>
    <div class="wide">
      <label class="loc-label" for="loc-prefix">Start in folder <small>optional</small></label>
      <input id="loc-prefix" class="loc-input mono" placeholder="Projects/2026/" spellcheck="false" autocomplete="off" bind:value={input.prefix} />
      <p class="loc-help">Selecting Connect opens Google in your default browser. The renewable access token stays in your system keychain.</p>
    </div>
  </div>
{:else}
<div class="fields">
  <div class="wide">
    <label class="loc-label" for="loc-name">Name <small>optional</small></label>
    <input
      id="loc-name"
      class="loc-input"
      placeholder={input.bucket.trim() || "Production media"}
      autocomplete="off"
      bind:value={input.name} />
  </div>

  {#if input.provider === "r2"}
    <div class="wide">
      <label class="loc-label" for="loc-account">Account ID</label>
      <input
        id="loc-account"
        class="loc-input mono"
        placeholder="32-character hex ID"
        spellcheck="false"
        autocomplete="off"
        bind:value={input.accountId} />
      <p class="loc-help">Shown in the Cloudflare dashboard under R2.</p>
    </div>
  {:else if input.provider === "custom" || input.provider === "azblob"}
    <div class="wide">
      <label class="loc-label" for="loc-endpoint">Endpoint URL</label>
      <input
        id="loc-endpoint"
        class="loc-input mono"
        placeholder={input.provider === "azblob" ? "https://account.blob.core.windows.net" : "https://minio.internal:9000"}
        spellcheck="false"
        autocomplete="off"
        bind:value={input.endpoint} />
    </div>
  {/if}

  <div>
    <label class="loc-label" for="loc-key">{input.provider === "azblob" ? "Storage account name" : "Access key ID"}</label>
    <input
      id="loc-key"
      class="loc-input mono"
      spellcheck="false"
      autocomplete="off"
      bind:value={input.accessKeyId} />
  </div>
  <div>
    <label class="loc-label" for="loc-secret">{input.provider === "azblob" ? "Account key" : "Secret access key"}</label>
    <div class="loc-secret">
      <input
        id="loc-secret"
        class="loc-input mono"
        type={revealSecret ? "text" : "password"}
        spellcheck="false"
        autocomplete="off"
        bind:value={input.secretAccessKey} />
      <button
        type="button"
        aria-label={revealSecret ? "Hide secret" : "Show secret"}
        onclick={() => (revealSecret = !revealSecret)}>
        {#if revealSecret}<EyeOffIcon />{:else}<EyeIcon />{/if}
      </button>
    </div>
  </div>
  <p class="loc-help wide keychain">Saved in your system keychain, never in the app database.</p>

  {#if input.provider === "aws"}
    <div>
      <label class="loc-label" for="loc-region">Region</label>
      <select id="loc-region" class="loc-input" bind:value={input.region}>
        {#each AWS_REGIONS as region (region)}
          <option value={region}>{region}</option>
        {/each}
      </select>
    </div>
  {:else if input.provider === "custom"}
    <div>
      <label class="loc-label" for="loc-region-custom">Region</label>
      <input
        id="loc-region-custom"
        class="loc-input mono"
        spellcheck="false"
        bind:value={input.region} />
    </div>
  {/if}

  <div class:wide={input.provider === "r2"}>
    <label class="loc-label" for="loc-bucket">{input.provider === "azblob" ? "Container" : "Bucket"} <small>{input.provider === "azblob" ? "required" : "optional"}</small></label>
    <input
      id="loc-bucket"
      class="loc-input mono"
      placeholder={input.provider === "azblob" ? "media" : "All buckets"}
      spellcheck="false"
      autocomplete="off"
      bind:value={input.bucket} />
  </div>

  {#if input.bucket.trim()}
    <div class="wide">
      <label class="loc-label" for="loc-prefix">Start in folder <small>optional</small></label>
      <input
        id="loc-prefix"
        class="loc-input mono"
        placeholder="uploads/2026/"
        spellcheck="false"
        autocomplete="off"
        bind:value={input.prefix} />
    </div>
  {/if}

  {#if input.provider === "custom"}
    <label class="loc-toggle wide">
      <span>Path-style addressing</span>
      <input type="checkbox" bind:checked={input.pathStyle} />
    </label>
  {/if}
</div>
{/if}

<style>
  .fields {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 10px;
  }
  .wide {
    grid-column: 1 / -1;
  }
  .keychain {
    margin-top: -6px;
  }
  .mono {
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 12px;
  }
  .provider-pending {
    padding: 14px;
    border-radius: 10px;
    background: rgb(10 155 255 / 0.08);
    box-shadow: inset 0 0 0 1px rgb(10 155 255 / 0.2);
    color: var(--loc-text);
  }
  .provider-pending p {
    margin: 6px 0 0;
    color: var(--loc-muted);
    font-size: 12px;
    line-height: 1.45;
  }
</style>
