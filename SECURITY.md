# Security policy

## Supported versions

Only the latest release gets security fixes. Until the first release, that means the `main` branch.

## Report a vulnerability

Don't open a public issue, discussion or pull request for a vulnerability. Report it privately through GitHub:

1. Open the [Security tab](https://github.com/sina96/colima-gpui/security) of this repository.
2. Click **Report a vulnerability**.
3. Fill in the form and submit it.

Include:

- The Colima GPUI version (or commit) and the macOS version.
- The output of `colima version`.
- What an attacker can do, and what access they need first.
- Steps to reproduce the problem, or a proof of concept.

## What to expect

- The maintainer acknowledges the report within one week.
- This is a volunteer project, so there's no fixed time for a fix.
- You're kept informed and told before the fix is public.
- If you agree, the security advisory credits you.

## Scope

These notes describe what Colima GPUI does, to help you decide whether a problem is a vulnerability.

- **CLI calls:**
  - The app runs `colima` and `docker` as your user, with argument lists and no shell.
  - Profile names and Docker IDs come from those tools' own output.
  - A way to make the app run a different program, or pass text from a container or image as a shell command, is a vulnerability.
- **SSH into the VM:** this asks Terminal (through `osascript`) to run `colima ssh -p <profile>`, with the profile name escaped. A profile name that breaks out of that command is a vulnerability.
- **Network:**
  - The app listens on no port.
  - It connects to the internet only when you click a download in the AI tab, which fetches Colima's agent skill from `github.com` / `raw.githubusercontent.com` / `api.github.com` with `curl` over HTTPS.
  - Links (documentation, GitHub) open in your browser.
- **Files:**
  - Settings are a JSON file in `~/Library/Application Support/colima-gpui/`.
  - Saved logs and the downloaded skill go only to a folder you choose. The skill download is staged in a temporary folder and only then moved into place.
  - A way to make the app write outside these places is a vulnerability.
- **Docker access:** the app uses each profile's Docker socket, which is equal to root inside that VM. Any process running as your user can already use that socket, so this access alone isn't a vulnerability.
- **Code signing:** local builds and any release are ad-hoc signed and not notarized by Apple. The signature only shows that the app isn't damaged, not who built it.

Problems in Colima, Lima or Docker are out of scope. Report them to those projects.
