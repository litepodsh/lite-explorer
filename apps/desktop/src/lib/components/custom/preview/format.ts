const UNITS = ["KB", "MB", "GB", "TB"];
const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });
/** Mexican grouping: a comma every three digits, like 1,234,567. */
const countFormat = new Intl.NumberFormat("es-MX");

export function formatSize(bytes: number): string {
  if (bytes < 1000) return bytes === 1 ? "1 byte" : `${bytes} bytes`;
  let value = bytes;
  let unit = -1;
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return `${Number(value.toFixed(1))} ${UNITS[unit]}`;
}

export function formatDate(milliseconds: number): string {
  return dateFormat.format(milliseconds);
}

export function formatCount(value: number): string {
  return countFormat.format(value);
}
