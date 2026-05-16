# macOS Code Signing

This document explains Slab's two macOS signing modes and how to switch between them.

---

## Current state: ad-hoc signed

Slab DMGs uploaded to GitHub Releases are currently **ad-hoc signed**. This means:

- ✅ The app is technically signed (no broken binary errors)
- ✅ Code signature integrity is verified before each launch
- ⚠️ Apple's Gatekeeper does not trust the signer, so users see a security warning on first launch
- ⚠️ Users must **right-click → Open** on the first launch, then click **Open** in the warning dialog

**Why not properly signed?** Apple charges $99/year for the Developer Program, which is required to issue a Developer ID Application certificate. Slab is free and open-source. When/if a Developer ID is available, the CI is already wired to use it (see below).

---

## User instructions for ad-hoc DMGs

Include these instructions in every release:

> **First-launch on macOS:**
> 1. Open the DMG and drag **Slab** to Applications.
> 2. In Finder, right-click (or Control-click) **Slab.app** → **Open**.
> 3. Click **Open** in the security dialog.
> 4. Done — subsequent launches are normal double-clicks.
>
> This is required because Slab is signed ad-hoc, not by an Apple-issued Developer ID. The app and signature are inspectable: run `codesign -dvv /Applications/Slab.app` to verify.

---

## Upgrading to Developer ID + notarization

When you enroll in the [Apple Developer Program](https://developer.apple.com/programs/) ($99/year), CI will automatically pick up signing as soon as the secrets below are configured. **No code changes needed** — the workflow conditionally runs the Developer ID path when `APPLE_CERTIFICATE_BASE64` is present.

### Step 1: Create a Developer ID Application certificate

1. Go to [Apple Developer → Certificates](https://developer.apple.com/account/resources/certificates/list).
2. Click **+** → **Developer ID Application** → Continue.
3. Generate a CSR in Keychain Access: **Keychain Access → Certificate Assistant → Request a Certificate from a Certificate Authority**. Save the CSR to disk; choose "Saved to disk", leave email/name blank or fill in.
4. Upload the CSR back to Apple Developer. Download the resulting `developerID_application.cer`.
5. Double-click the `.cer` to import into your login keychain.

### Step 2: Export as `.p12`

1. In Keychain Access, find **"Developer ID Application: <your-name> (<team-id>)"**.
2. Right-click → **Export**, choose **Personal Information Exchange (.p12)**, set a strong password.
3. Save as `slab-cert.p12`.

### Step 3: Create an app-specific password for notarization

1. Go to [appleid.apple.com](https://appleid.apple.com/) → **Sign-In and Security** → **App-Specific Passwords**.
2. Generate a new password labeled `slab-notarize`.
3. Copy the password — it's shown only once.

### Step 4: Find your Team ID

1. Go to [Apple Developer → Membership](https://developer.apple.com/account/#!/membership).
2. Copy the 10-character **Team ID** (looks like `ABCDE12345`).

### Step 5: Encode the cert as base64

```bash
base64 -i slab-cert.p12 -o slab-cert.p12.base64
# Then: pbcopy < slab-cert.p12.base64
```

### Step 6: Add GitHub Secrets

Go to **Slab repo → Settings → Secrets and variables → Actions → New repository secret**, and add all six:

| Secret name | Value | Notes |
|---|---|---|
| `APPLE_CERTIFICATE_BASE64` | base64-encoded `.p12` contents | from step 5 |
| `APPLE_CERTIFICATE_PASSWORD` | password you set on the `.p12` | from step 2 |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: <Your Name> (TEAMID)` | full identity string, copy exactly from `security find-identity -v -p codesigning` |
| `APPLE_ID` | your Apple ID email | the account that owns the Developer Program |
| `APPLE_PASSWORD` | app-specific password from step 3 | **not** your real Apple ID password |
| `APPLE_TEAM_ID` | 10-char team ID | from step 4 |

### Step 7: Push a release

That's it. Next push to `main` (or tag) will automatically:

1. Import the `.p12` into a temp keychain on the macOS runner
2. Sign the `.app` bundle with the Developer ID identity
3. Submit the DMG to Apple's notary service
4. Wait for notarization to complete (~5-15 min)
5. Staple the notarization ticket to the DMG
6. Upload the signed + notarized DMG as an artifact

Users will be able to double-click the DMG with no security warnings. Gatekeeper will accept it.

---

## Verifying a signed DMG

```bash
# Should show: "Developer ID Application: <name> (<team>)"
codesign -dvv /Applications/Slab.app

# Should show: "accepted, source=Notarized Developer ID"
spctl -a -vv /Applications/Slab.app

# Should show: "The validate action worked!"
xcrun stapler validate /path/to/Slab.dmg
```

---

## CI behavior matrix

| Secret presence | Signing | Notarization | User experience |
|---|---|---|---|
| `APPLE_CERTIFICATE_BASE64` absent | Ad-hoc (`codesign -s -`) | None | Right-click → Open on first launch |
| `APPLE_CERTIFICATE_BASE64` present | Developer ID | Submitted + stapled | Double-click works clean |

---

## Cost / time tradeoffs

| Path | $/year | Setup time | User friction |
|---|---|---|---|
| Ad-hoc (current) | $0 | none | first-launch right-click prompt |
| Developer ID + notarize | $99 | ~30 min once + 24-48h Apple approval | zero |

If/when Slab gets enough users to justify the $99/yr, switching is a 6-secret config change with no code edits.
