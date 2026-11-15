/** Who makes the app and where to find it — shown under 设置 → 关于. */
export const DEVELOPER = "SinkFish";

/** Product website. Leave empty to hide the link; only http(s) addresses are shown. */
export const WEBSITE_URL = "https://github.com/Sink-Fish/OCR-Trans";

/** The website as a link target and a short label, or null when there is none to show. */
export function websiteLink(url: string = WEBSITE_URL): { href: string; label: string } | null {
  const u = url.trim();
  if (!u) return null;
  try {
    const parsed = new URL(u);
    if (parsed.protocol !== "https:" && parsed.protocol !== "http:") return null;
    const path = parsed.pathname === "/" ? "" : parsed.pathname;
    return { href: parsed.href, label: parsed.host + path };
  } catch {
    return null;
  }
}
