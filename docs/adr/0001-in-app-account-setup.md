# 0001. In-app account setup, not macOS system-account reuse

Date: 2026-06-30
Status: Accepted

## Context

The ideal was to reuse mail accounts already configured in macOS System Settings,
so the user never re-enters credentials. Investigation on the target machine showed
this is not possible for a third-party app:

- `~/Library/Accounts/` is empty / inaccessible; no `Accounts.sqlite` readable.
- Internet Accounts live behind private Apple frameworks with no public API.
- Keychain credentials for Mail.app are ACL-locked to Mail.app; reading them prompts
  the user per item and yields Apple-scoped tokens, not credentials usable by our app.

## Decision

Each account is added **once inside our app** via a connection wizard. Credentials
are stored in our own Keychain entry. Three backends sit behind one connection layer:
Gmail API (OAuth), generic IMAP/SMTP, and Exchange Online.

## Consequences

- One-time setup friction per account; unavoidable OS tax.
- Full local control of credentials and tokens.
- IMAP/SMTP is the universal fallback; Gmail-API and Exchange-native are richer upgrades.
- On-prem Exchange (EWS/NTLM) is explicitly out of scope.
