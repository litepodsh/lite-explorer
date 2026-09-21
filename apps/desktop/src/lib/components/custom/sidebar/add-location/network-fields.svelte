<script lang="ts">
  import EyeIcon from "@lucide/svelte/icons/eye";
  import EyeOffIcon from "@lucide/svelte/icons/eye-off";
  import InfoIcon from "@lucide/svelte/icons/info";
  import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";
  import {
    allowsGuest,
    defaultPort,
    fallbackName,
    hasAuth,
    needsUsername,
    pathLabel,
    pathPlaceholder,
    type NetworkField,
    type NetworkLocationInput,
  } from "$lib/remote/network-locations.js";

  let {
    input = $bindable(),
    errors = [],
    editing = false,
  }: { input: NetworkLocationInput; errors?: NetworkField[]; editing?: boolean } = $props();

  let revealPassword = $state(false);
  let protocol = $derived(input.protocol);
  const shows = (field: NetworkField) => errors.includes(field);
</script>

<div class="fields">
  <div class="address wide">
    <div>
      <label class="loc-label" for="net-host">Server</label>
      <input
        id="net-host"
        class="loc-input mono"
        class:invalid={shows("host")}
        placeholder={protocol === "webdav" ? "cloud.example.com" : "nas.local or 192.168.1.20"}
        spellcheck="false"
        autocomplete="off"
        bind:value={input.host} />
      {#if shows("host")}<p class="loc-field-error">Enter a server name or IP address.</p>{/if}
    </div>
    <div>
      <label class="loc-label" for="net-port">Port</label>
      <input
        id="net-port"
        class="loc-input mono"
        class:invalid={shows("port")}
        inputmode="numeric"
        placeholder={String(defaultPort(input))}
        bind:value={input.port} />
      {#if shows("port")}<p class="loc-field-error">Use 1 to 65535.</p>{/if}
    </div>
  </div>

  <div class="wide">
    <label class="loc-label" for="net-path">
      {pathLabel(protocol)}
      {#if protocol !== "nfs"}<small>optional</small>{/if}
    </label>
    <input
      id="net-path"
      class="loc-input mono"
      class:invalid={shows("path")}
      placeholder={pathPlaceholder(protocol)}
      spellcheck="false"
      autocomplete="off"
      bind:value={input.path} />
    {#if shows("path")}
      <p class="loc-field-error">NFS needs the export path, like /srv/exports/media.</p>
    {:else if protocol === "smb"}
      <p class="loc-help">Leave empty to browse shares when connecting.</p>
    {/if}
  </div>

  {#if protocol === "webdav"}
    <label class="loc-toggle wide">
      <span>Use HTTPS</span>
      <input
        type="checkbox"
        checked={input.webdavSecurity === "https"}
        onchange={(event) => (input.webdavSecurity = event.currentTarget.checked ? "https" : "http")} />
    </label>
    {#if input.webdavSecurity === "http"}
      <p class="loc-note warn wide"><TriangleAlertIcon /> Without HTTPS, your password and files travel unencrypted.</p>
    {/if}
  {:else if protocol === "ftp"}
    <div class="wide">
      <label class="loc-label" for="net-security">Security</label>
      <select id="net-security" class="loc-input" bind:value={input.ftpSecurity}>
        <option value="explicit">FTPS, explicit TLS (recommended)</option>
        <option value="implicit">FTPS, implicit TLS</option>
        <option value="plain">Plain FTP, no encryption</option>
      </select>
    </div>
    {#if input.ftpSecurity === "plain"}
      <p class="loc-note warn wide">
        <TriangleAlertIcon /> Plain FTP sends your password in clear text. Use it only on networks you trust.
      </p>
    {/if}
  {:else if protocol === "nfs"}
    <p class="loc-note info wide">
      <InfoIcon /> NFS doesn’t ask for a password. The server grants access based on this computer’s address.
    </p>
  {/if}

  {#if allowsGuest(protocol)}
    <div class="wide">
      <span class="loc-label" id="net-auth-label">Connect as</span>
      <div class="loc-segment" role="radiogroup" aria-labelledby="net-auth-label">
        <button type="button" role="radio" aria-checked={input.auth === "guest"} onclick={() => (input.auth = "guest")}
          >{protocol === "smb" ? "System account" : "Guest"}</button>
        <button
          type="button"
          role="radio"
          aria-checked={input.auth === "password"}
          onclick={() => (input.auth = "password")}>Registered user</button>
      </div>
    </div>
  {/if}

  {#if hasAuth(protocol) && needsUsername(input)}
    <div>
      <label class="loc-label" for="net-user">Username</label>
      <input
        id="net-user"
        class="loc-input"
        class:invalid={shows("username")}
        placeholder={protocol === "smb" ? "me or DOMAIN\\me" : "me"}
        spellcheck="false"
        autocomplete="off"
        bind:value={input.username} />
      {#if shows("username")}<p class="loc-field-error">Enter your username.</p>{/if}
    </div>
    <div>
      <label class="loc-label" for="net-password">Password <small>optional</small></label>
      <div class="loc-secret">
        <input
          id="net-password"
          class="loc-input"
          type={revealPassword ? "text" : "password"}
          placeholder={editing ? "Unchanged" : ""}
          autocomplete="off"
          bind:value={input.password} />
        <button
          type="button"
          aria-label={revealPassword ? "Hide password" : "Show password"}
          onclick={() => (revealPassword = !revealPassword)}>
          {#if revealPassword}<EyeOffIcon />{:else}<EyeIcon />{/if}
        </button>
      </div>
    </div>
    <label class="loc-toggle wide">
      <span>Remember password</span>
      <input type="checkbox" bind:checked={input.rememberPassword} />
    </label>
    <p class="loc-help wide tight">
      {input.rememberPassword
        ? "Saved in this computer’s password store."
        : "You’ll be asked for the password each time you connect."}
    </p>
  {/if}

  <div class="wide">
    <label class="loc-label" for="net-name">Name <small>optional</small></label>
    <input id="net-name" class="loc-input" placeholder={fallbackName(input)} autocomplete="off" bind:value={input.name} />
  </div>
</div>

<style>
  .fields {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 10px;
  }
  .wide {
    grid-column: 1 / -1;
  }
  .address {
    display: grid;
    grid-template-columns: 1fr 84px;
    gap: 10px;
  }
  .tight {
    margin-top: -6px;
  }
  .mono {
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 12px;
  }
</style>
