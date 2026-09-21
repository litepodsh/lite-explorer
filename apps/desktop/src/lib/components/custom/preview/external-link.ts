export function externalUrl(value: string): string | null {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:" ? url.href : null;
  } catch {
    return null;
  }
}

export function confirmExternalLink(value: string): void {
  const url = externalUrl(value);
  if (!url) return;
  void Promise.all([
    import("@tauri-apps/api/core"),
    import("$lib/components/custom/dialog"),
  ]).then(([{ invoke }, { confirmation }]) => {
    confirmation.ask({
      title: "Open external link?",
      description: url,
      confirmLabel: "Open link",
      variant: "primary",
      onconfirm: () => invoke("open_external_url", { url }),
    });
  });
}
