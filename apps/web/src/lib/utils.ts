import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Merge conditional class lists and de-duplicate conflicting Tailwind
 * utilities, the same way shadcn/ui components do.
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
