type Translate = (key: string, params?: Record<string, unknown>) => string

const CJK_RE = /[\u3400-\u9fff]/
const CJK_RUN_RE = /[\u3400-\u9fff]+/g

/**
 * Last-resort guard for legacy backend strings that have not yet been
 * converted to a structured event/error. Chinese mode keeps the original
 * message verbatim. English mode removes CJK text while retaining protocol
 * identifiers, addresses, numbers and OS error details as technical context.
 */
export function localizeLegacyBackendText(
  message: string,
  locale: string,
  t: Translate,
  fallbackKey: string,
): string {
  if (!locale.toLowerCase().startsWith('en') || !CJK_RE.test(message)) return message
  const technical = message
    .replace(CJK_RUN_RE, ' ')
    .replace(/\s+/g, ' ')
    .replace(/^[\s:：,，;；()（）-]+|[\s:：,，;；()（）-]+$/g, '')
    .trim()
  return t(fallbackKey, { technical: technical || '-' })
}

export function containsCjk(message: string): boolean {
  return CJK_RE.test(message)
}
