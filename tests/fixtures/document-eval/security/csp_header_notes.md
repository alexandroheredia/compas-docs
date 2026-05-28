# CSP Header Notes

## Topic

This note explains content security policy headers for the web dashboard.

## Core directives

- default-src 'self'
- img-src 'self' data:
- script-src 'self'
- object-src 'none'

## Why it matters

Restrictive CSP reduces script injection risk, but it does not address email logging or audit-trail privacy concerns.

## Delivery note

The admin dashboard uses the same shell as the reporting views, so CSP guidance is sometimes mentioned alongside dashboard copy discussions even though the actual topic is browser policy.
