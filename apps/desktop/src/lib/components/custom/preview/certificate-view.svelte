<script lang="ts">
  import BadgeCheck from "@lucide/svelte/icons/badge-check";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import { openCertificate, type CertificatePreview } from "./certificate.js";

  type Props = { path: string; name: string };
  let { path, name }: Props = $props();

  let certificate = $state.raw<CertificatePreview | null>(null);
  let error = $state("");
  let token = 0;

  $effect(() => {
    const target = path;
    const request = ++token;
    certificate = null;
    error = "";
    openCertificate(target)
      .then((result) => {
        if (request === token) certificate = result;
      })
      .catch((reason) => {
        if (request === token) error = reason instanceof Error ? reason.message : String(reason);
      });
  });
</script>

<div class="h-full min-h-0 overflow-auto bg-[#1f1d1b] text-[#e8e5e2]" aria-label={`Certificate preview of ${name}`}>
  {#if error}
    <div class="grid h-full place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if certificate}
    <div class="mx-auto flex max-w-2xl flex-col gap-3 p-4">
      {#if certificate.certificates.length === 0}
        <div class="flex items-center gap-3 rounded-xl border border-[#3a3734] bg-[#242220] p-4">
          <KeyRound class="size-5 text-[#9c9895]" />
          <div>
            <p class="text-[14px] font-medium">
              {certificate.kind === "private-key" ? "Clave privada" : certificate.kind === "public-key" ? "Clave pública" : "Archivo PEM"}
            </p>
            <p class="text-[12.5px] text-[#9c9895]">El contenido no se muestra por seguridad.</p>
          </div>
        </div>
      {/if}
      {#each certificate.certificates as cert, index (cert.serial)}
        <article class="rounded-xl border border-[#3a3734] bg-[#242220] p-4">
          <header class="flex items-start gap-2">
            <ShieldCheck class="mt-0.5 size-4 shrink-0 text-[#9c9895]" />
            <div class="min-w-0 flex-1">
              <p class="text-[14px] font-semibold break-words">{cert.subject}</p>
              <p class="text-[12px] break-words text-[#9c9895]">Emitido por {cert.issuer}</p>
            </div>
            {#if index === 0 && certificate.certificates.length > 1}
              <span class="rounded bg-white/5 px-1.5 py-0.5 text-[10.5px] text-[#c0bbb5]">hoja</span>
            {/if}
          </header>
          <div class="mt-2 flex flex-wrap gap-1.5">
            {#if cert.isCa}
              <span class="inline-flex items-center gap-1 rounded-full bg-emerald-500/15 px-2 py-0.5 text-[10.5px] text-emerald-300">
                <BadgeCheck class="size-3" />CA
              </span>
            {/if}
            {#if cert.selfSigned}
              <span class="rounded-full bg-white/5 px-2 py-0.5 text-[10.5px] text-[#c0bbb5]">autofirmado</span>
            {/if}
            {#if cert.keySize}
              <span class="rounded-full bg-white/5 px-2 py-0.5 text-[10.5px] text-[#c0bbb5]">{cert.keySize} bits</span>
            {/if}
          </div>
          <dl class="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[12.5px]">
            <dt class="font-medium text-[#9c9895]">Válido</dt>
            <dd>{cert.notBefore} → {cert.notAfter}</dd>
            <dt class="font-medium text-[#9c9895]">Serial</dt>
            <dd class="font-mono text-[11.5px] break-all">{cert.serial}</dd>
            <dt class="font-medium text-[#9c9895]">Firma</dt>
            <dd class="break-all">{cert.signatureAlgorithm}</dd>
            <dt class="font-medium text-[#9c9895]">Clave</dt>
            <dd class="break-all">{cert.publicKeyAlgorithm}</dd>
            {#if cert.subjectAltNames.length > 0}
              <dt class="font-medium text-[#9c9895]">Nombres</dt>
              <dd class="break-all">{cert.subjectAltNames.join(", ")}</dd>
            {/if}
            {#if cert.keyUsage.length > 0}
              <dt class="font-medium text-[#9c9895]">Usos</dt>
              <dd>{cert.keyUsage.join(", ")}</dd>
            {/if}
          </dl>
        </article>
      {/each}
    </div>
  {/if}
</div>
