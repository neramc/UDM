# Security Policy

Universal Drive Manager works with raw storage devices. A bug that writes to
the wrong device, escalates privileges, or reports an unverified result as
successful can destroy data. We treat such bugs as security issues.

## Supported versions

The project is in pre-release development (Phase 0, read-only prototype).
Only the latest commit on the `main` branch receives fixes. There are no
released versions yet.

| Version | Supported |
|---------|-----------|
| `main` (latest) | Yes |
| Any older commit or unofficial build | No |

## Reporting a vulnerability

**Please do not report security issues in public GitHub issues,
discussions or pull requests.**

Report privately through GitHub's private vulnerability reporting:

> https://github.com/neramc/UDM/security/advisories/new

If that page is not available to you, open a public issue titled
"Security contact request" **without any technical details**, and a
maintainer will arrange a private channel.

Please include, as far as you can:

- affected commit or build, operating system and version;
- device type involved (USB stick, SD card, NVMe enclosure, …) — use
  virtual disks or devices without valuable data when reproducing;
- step-by-step reproduction, expected and actual behaviour;
- impact (e.g. wrong device written, privilege gained, data exposed);
- logs with serial numbers and personal paths redacted;
- whether and when you intend to disclose publicly.

## What counts as a security issue

In scope (examples):

- Any write, format, partition or erase operation reaching a device or
  region the user did not authorize, or continuing after the target's
  identity changed.
- Treating a system disk (or a disk whose system status is unknown) as a
  valid destructive target.
- Privilege escalation or confused-deputy behaviour involving the privileged
  helper or its IPC protocol (once it exists; see ADR-0006).
- Execution of commands or programs derived from device contents, image
  files, configuration or IPC messages.
- Memory-safety issues or crashes triggered by crafted partition tables,
  images, filesystem metadata or OS metadata.
- Reporting success when flush or verification failed, or reporting
  cancellation while a write is still running.
- Leaking serial numbers, file paths or other personal data through logs,
  reports or network access (the application must not send telemetry).
- Vulnerable dependencies that are reachable from the application.

Out of scope:

- Attacks that require an already compromised user account or OS kernel.
- Data loss caused by a user explicitly confirming the correct target with
  full information shown (this is intended behaviour; usability reports are
  still welcome as normal issues).
- Issues in unofficial builds or modified versions.
- Findings from automated scanners without a demonstrated impact.

## Our process

This is a volunteer-run project; the following are goals, not guarantees:

| Step | Target |
|------|--------|
| Acknowledge the report | within 7 days |
| Initial assessment and severity | within 14 days |
| Fix or mitigation for critical issues (e.g. wrong-target writes) | as fast as possible, prioritized over all other work |
| Coordinated public disclosure | after a fix is available, normally within 90 days of the report |

We will credit reporters in the advisory unless they prefer to remain
anonymous. There is no bug bounty.

## Good-faith research

We will not pursue or support legal action against research that follows
this policy, avoids harm to others' data and devices, and gives us a
reasonable time to respond. Never test against devices or data you do not
own; prefer the virtual-disk tooling in `crates/udm-testkit`.

## Security design

- Threat model: [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md)
- Device identity and revalidation: [ADR-0003](docs/ADR/0003-device-identity-model.md),
  [ADR-0005](docs/ADR/0005-operation-planning.md)
- Privileged helper boundary: [ADR-0006](docs/ADR/0006-privileged-helper-boundary.md)
- Dependency policy: `deny.toml` (`cargo deny check` runs in CI)
