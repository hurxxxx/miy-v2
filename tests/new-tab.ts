import { expect, type Locator, type Page } from "@playwright/test";

export async function openNewTab(page: Page, link: Locator) {
  const originalUrl = page.url();
  const destination = new URL((await link.getAttribute("href"))!, originalUrl).href;
  await expect(link).toHaveAttribute("target", "_blank");
  await expect(link).toHaveAttribute("rel", /\bnoopener\b/);
  await expect(link).toHaveAttribute("rel", /\bnoreferrer\b/);
  await expect(link).toHaveAccessibleDescription("새 탭에서 열립니다");
  const opened = page.waitForEvent("popup");
  await link.click();
  const tab = await opened;
  await expect(tab).toHaveURL(destination);
  await tab.waitForLoadState("domcontentloaded");
  expect(await tab.evaluate(() => window.opener === null)).toBe(true);
  await expect(page).toHaveURL(originalUrl);
  return tab;
}
