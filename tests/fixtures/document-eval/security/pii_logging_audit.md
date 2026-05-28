# PII Logging Audit

## Summary

Several debug statements exposed user email addresses in logs. The risk is highest where auth listeners or audit events print the email directly.

## Sensitive examples

- sentry user context logging user.email
- audit event logging with actor email
- invitation flow printing normalized email addresses

## Safe pattern

Only emit debug logging behind an explicit debug-mode guard, and avoid logging direct identifiers when a pseudonymous token is enough.

## Follow-up

- replace raw print statements with guarded debug logging
- remove email values from routine diagnostics
- verify sendDefaultPii remains disabled in Sentry setup
