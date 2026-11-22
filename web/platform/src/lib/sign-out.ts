import { request } from "@miy/sdk";
import { clearAllAppGuards, confirmLeaveAllApps } from "./navigation-guards";

/** Sign out of this surface, asking first when an open app has unsaved work. */
export async function signOut(csrf: string) {
  if (!confirmLeaveAllApps()) return;
  const data = await request<{ redirect: string }>("/auth/logout", csrf, { method: "POST" });
  clearAllAppGuards();
  window.location.assign(data.redirect);
}

/** Leave for another page of the sign-in server (e.g. its password form). */
export function leaveForIdentityProvider(href: string) {
  if (!confirmLeaveAllApps()) return;
  clearAllAppGuards();
  window.location.assign(href);
}
