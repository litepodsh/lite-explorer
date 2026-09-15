export type Platform = "macos" | "windows" | "linux" | "unknown";

/** Set by the app shell once the OS is detected. */
export const platformState = $state<{ current: Platform }>({ current: "unknown" });

/** The modifier that adds to a selection and triggers shortcuts: Cmd on macOS, Ctrl elsewhere. */
export function isPrimaryModifier(event: Pick<MouseEvent, "metaKey" | "ctrlKey">): boolean {
  return platformState.current === "macos" ? event.metaKey : event.ctrlKey;
}
