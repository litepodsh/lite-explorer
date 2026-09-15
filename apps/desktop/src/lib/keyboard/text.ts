/** A file name without its last extension. Dotfiles like `.bashrc` keep their name. */
export function fileStem(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/** A folder path ready for typing a child name in the palette's path mode. */
export function withTrailingSlash(path: string): string {
  if (!path) return "/";
  return /[\\/]$/.test(path) ? path : `${path}/`;
}
