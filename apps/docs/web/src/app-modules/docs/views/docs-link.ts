/**
 * miy: links and clipboard inside the app frame.
 *
 * AI-DO built links from `window.location.origin` and wrote them with
 * `navigator.clipboard.writeText`. In the frame, `window.location.origin` is
 * the frame's own origin (not the portal), and the portal iframe carries no
 * `allow="clipboard-write"` delegation, so Chromium refuses the async
 * clipboard API there (NotAllowedError, permissions policy).
 *
 * - The portal URL is the portal origin the browser reports in
 *   `location.ancestorOrigins` (Chromium and WebKit; no parent access) plus
 *   `runtime.basePath` and the app path. Without it (Firefox) the link is the
 *   portal-relative path.
 * - Copying tries the async clipboard API, then the legacy `copy` command on
 *   a temporary text field (allowed on a user gesture). When both fail the
 *   caller shows the link for manual copying.
 */
export function portalOrigin(
  location: Pick<Location, 'ancestorOrigins'> | null | undefined = typeof window ===
  'undefined'
    ? null
    : window.location,
): string {
  const origins = location?.ancestorOrigins;
  if (!origins || origins.length === 0) return '';
  const origin = origins[0];
  return /^https?:\/\/[^/]+$/.test(origin) ? origin : '';
}

export function portalUrlFor(
  basePath: string,
  appPath: string,
  origin: string = portalOrigin(),
): string {
  const path = appPath.startsWith('/') ? appPath : `/${appPath}`;
  return `${origin}${basePath.replace(/\/+$/, '')}${path}`;
}

export async function copyText(
  text: string,
  doc: Document = document,
  clipboard: Pick<Clipboard, 'writeText'> | undefined = typeof navigator ===
  'undefined'
    ? undefined
    : navigator.clipboard,
): Promise<boolean> {
  try {
    if (clipboard) {
      await clipboard.writeText(text);
      return true;
    }
  } catch {
    // Blocked by the frame's permissions policy; try the legacy command.
  }
  const field = doc.createElement('textarea');
  field.value = text;
  field.setAttribute('readonly', '');
  field.style.position = 'fixed';
  field.style.opacity = '0';
  doc.body.append(field);
  try {
    field.select();
    return typeof doc.execCommand === 'function' && doc.execCommand('copy');
  } catch {
    return false;
  } finally {
    field.remove();
  }
}
