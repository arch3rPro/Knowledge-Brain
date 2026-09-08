# ADR-0020: Update official CLI binaries only after independent release verification

- Status: proposed
- Class: architecture
- Date: 2026-09-08

## Problem

Downloading a newer executable from the same Release that supplies its checksum does not give an installed program an independent authorization signal. Users also need a portable way to update the official CLI without background network activity, while source and Cargo installations must remain under their original package manager's control. Replacing a running executable has different filesystem constraints on Unix and Windows.

## Proposal

Knowledge-Brain provides explicit `kb update check` and `kb update` commands. Neither command runs automatically or in the background. They consider only the latest non-draft, non-prerelease GitHub Release.

Official Release binaries embed a supported target identifier and the canonical Minisign public key. The public key is stored in the repository as a reviewable release asset; the matching private key exists only in the `KB_UPDATE_SIGNING_KEY` GitHub Actions secret. Each release signs `SHA256SUMS` and uploads `SHA256SUMS.minisig` alongside the archives. The release workflow fails rather than publishing an update channel without that signature.

`kb update check` resolves the latest compatible asset and reports whether its version is newer. `kb update` accepts only an official Release binary, downloads the archive, checksum file and signature over HTTPS, verifies the signature using the embedded public key, verifies the archive digest, rejects unsafe archive paths, and validates the staged binary's version and target before replacement.

The running program copies itself to a private helper path. After the main process exits, the helper replaces the installed executable with the verified staged file, preserves a recoverable prior file until replacement succeeds, and then cleans its own temporary state. This gives Windows a process that is not the locked target executable and gives every supported platform the same recovery boundary.

Source checkouts and binaries installed by Cargo are not update targets. An executable without the official Release marker returns an installation-boundary error and tells the user to update through its original installation method.

## Alternatives considered

**Offer only a download link.** This avoids self-replacement complexity but leaves every user to select the correct asset, validate it and update their PATH manually.

**Trust `SHA256SUMS` fetched from the same Release without a signature.** A digest detects accidental corruption, but an attacker able to alter the release assets can alter both the archive and its checksum.

**Use macOS notarization or Windows Authenticode as the updater's trust root.** These are separate operating-system trust systems and require credentials the project does not hold. They do not provide a portable updater protocol.

**Replace every writable `kb`, including Cargo and source installations.** This can silently modify a development checkout or a file managed by Cargo, and makes later package-manager actions surprising.

**Poll for updates in the background.** This changes the offline-by-default contract and performs network traffic without an explicit user request.

**Accept prereleases by default.** Stable users need predictable versions; prerelease channels require a separate selection and rollback policy.

## Acceptance criteria

- `kb update check` performs no file write and only contacts the release endpoint when explicitly invoked.
- `kb update` verifies `SHA256SUMS.minisig` with an embedded public key before it trusts an archive digest.
- The updater accepts only a newer stable release containing an asset for its embedded target.
- Invalid signatures, digest mismatches, malformed archives, target mismatches and replacement failures leave the current executable usable.
- An official binary can replace itself on Linux, macOS and Windows through a helper that outlives the original process.
- Source and Cargo installations are rejected without mutation.
- Release publication fails without the private signing secret and publishes the checksum signature with every supported archive.
- User documentation includes check, update, verification, recovery and public-key rotation procedures.

## Risks

- The signing private key is a high-value GitHub secret; repository write access and workflow changes require careful review.
- The latest-release endpoint and GitHub availability are external dependencies of explicit update commands.
- A public-key rotation requires a release signed by the prior key or a documented manual reinstallation path.
- Self-replacement can be delayed by antivirus or filesystem locks, particularly on Windows; the helper needs bounded retries and a recoverable backup.
