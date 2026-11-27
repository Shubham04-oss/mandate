/**
 * Tidies the plain-text part: html-to-text writes a link as "words : url", which becomes
 * "words:" with the url alone on the next line, easy to tap or copy; runs of blank lines shrink.
 */
export function tidyText(text: string): string {
  return `${text
    .replace(/ : (\S+)$/gm, ":\n$1")
    .replace(/\n{3,}/g, "\n\n")
    .trim()}\n`;
}
