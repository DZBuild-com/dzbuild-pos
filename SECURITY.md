# Security policy

DZBuild POS stores a shop's sales, stock, customers and fiscal records. This page explains how to
report a security problem to the DZBuild team and what happens after you do.

## Supported versions

| Version | Supported |
| --- | --- |
| 1.0.x (LTS) | Yes: security fixes |
| Older than 1.0 | No: please update to the latest 1.0.x |

## Report a vulnerability privately

**Please do not open a public issue, discussion or pull request for a security problem.**

1. Use GitHub's private vulnerability reporting: the repository's **Security** tab › **Report a
   vulnerability**, or go straight to
   [the report form](https://github.com/DZBuild-com/dzbuild-pos/security/advisories/new).
2. If you cannot use GitHub, write to us through [dzbuild.com/pos/support](https://dzbuild.com/pos/support)
   and start your message with "Security". We will move the conversation to a private channel.

Please include the version (Settings › About), what an attacker can do, the steps to reproduce, and a
proof of concept if you have one. Do not include real shop or customer data.

## What happens next

These are the times we aim for:

| Step | Target |
| --- | --- |
| Acknowledge your report | within 72 hours |
| First assessment and severity | within 7 days |
| Fix for a **critical** issue | within 14 days |
| Fix for a **high** issue | within 30 days |
| Fix for a **medium** or **low** issue | in the next planned release, or within 90 days |

We keep you informed, agree on a disclosure date with you, publish a GitHub security advisory with the
fixed version, and credit you if you want to be credited.

## Scope

In scope:

- the DZBuild POS desktop app (`src-tauri`, `src`, `crates/*`) and its installers;
- the LAN host role and the till pairing (`crates/pos-server`);
- the updater (manifest, signatures, download and install);
- the DZBuild link client inside the app (OAuth, token storage, sync).

Out of scope: the dzbuild.com website and DZBuild's online services, which have their own security
policy; third-party dependencies (report them upstream, and tell us if DZBuild POS is affected);
attacks that need an administrator account on the shop's PC or physical tampering with the hardware;
social engineering and denial of service by flooding.

## Areas of particular concern

- **Shop data at rest**: the SQLite database, backups and the `.dzpos` export, the diagnostic ZIP (it
  must never carry customer data, PINs or tokens).
- **The tamper-evident books**: the hash-chained audit journal, gap-free numbering, the fiscal archive
  and its verifier. A way to edit or delete a validated sale without leaving a trace is a serious bug.
- **The updater**: anything that installs code without a valid signature.
- **The LAN**: the pairing code, a till's access token, and what one machine on the shop network can
  read or write on another.
- **Local accounts**: PINs, roles and the manager approvals.

## Safe harbour

We will not pursue or support legal action against anyone who researches and reports in good faith
under this policy: you test only on your own installation and data, you avoid harming shops, their
customers or their data, you do not access more than you need to show the problem, and you give us a
reasonable time to fix it before you disclose it.

## Bug bounty

There is no paid bug bounty. Reporters are credited in the advisory and the changelog.
