// Lightweight update check against GitHub Releases. No signing keys required:
// it reads the latest published release, compares the tag to this build, and
// hands back the .dmg download URL if newer. (A fully-silent auto-installer
// would need tauri-plugin-updater + a signed release feed + notarization.)

export const APP_VERSION = "0.1.0";

// GitHub repo the "Güncelle" button checks for newer releases. The release.yml
// CI attaches the .dmg to each published Release; the button downloads it when
// the tag is newer than APP_VERSION. (Create this repo + push + publish a
// Release for it to go live.)
export const GITHUB_REPO = "burskozbekov/BudgetCut";

export interface UpdateInfo {
  status: "current" | "available" | "unconfigured" | "error";
  latest?: string;
  url?: string;
}

/** Numeric semver compare: is `a` newer than `b`? */
function isNewer(a: string, b: string): boolean {
  const pa = a.split(".").map((n) => parseInt(n, 10) || 0);
  const pb = b.split(".").map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const x = pa[i] ?? 0;
    const y = pb[i] ?? 0;
    if (x !== y) return x > y;
  }
  return false;
}

export async function checkForUpdate(): Promise<UpdateInfo> {
  if (!GITHUB_REPO) return { status: "unconfigured" };
  try {
    const res = await fetch(`https://api.github.com/repos/${GITHUB_REPO}/releases/latest`, {
      headers: { accept: "application/vnd.github+json" },
    });
    if (!res.ok) return { status: "error" };
    const j: any = await res.json();
    const latest = String(j.tag_name ?? "").replace(/^v/, "");
    const dmg = (j.assets ?? []).find((a: any) => typeof a.name === "string" && a.name.endsWith(".dmg"));
    const url = dmg?.browser_download_url ?? j.html_url;
    if (latest && isNewer(latest, APP_VERSION)) return { status: "available", latest, url };
    return { status: "current" };
  } catch {
    return { status: "error" };
  }
}
