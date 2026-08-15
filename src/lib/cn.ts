/** Joins class names, dropping falsy values. Tiny stand-in for clsx to avoid a dependency. */
export function cn(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}
