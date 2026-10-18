# Security Policy

## Reporting a vulnerability

Please **do not open a public issue** for security problems.

Email **[security@litepod.sh](mailto:security@litepod.sh)** instead, or
**[security@sebasgc.xyz](mailto:security@sebasgc.xyz)** as an alternative. Include:

- what you found and what an attacker could do with it;
- steps to reproduce, or a proof of concept;
- the Lite Explorer version (**Lite Explorer → About**) and your OS version.

We aim to:

- acknowledge your report within **3 business days**;
- keep you updated until it's resolved.

Once a fix is released, we'll credit you in the changelog, unless you'd rather stay
anonymous.

## Supported versions

Only the latest release gets security fixes. The app updates itself, so make sure you're
on the newest version before you report.

## Scope

These are especially relevant:

- **Stored credentials**: S3, Azure, Google Drive, SFTP, FTP and SMB credentials, kept in
  the system keychain.
- **Remote connections**: TLS and host-key verification for SFTP, FTP and SMB.
- **File previews**: parsing untrusted files (archives, documents, images, databases),
  and HTML or Markdown rendered in the preview.
- **The updater**: how updates are downloaded and how their signatures are checked.
- **File operations**: copy, move, extract or delete reaching outside the intended paths,
  for example through path traversal in archives.

Out of scope:

- issues that need an attacker to already control your user account;
- the current builds being self-signed and not notarized (this is a known limitation).
