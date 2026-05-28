# Backup Strategy Overview

## Layers

- scheduled snapshots every night
- point-in-time recovery for short rollback windows
- monthly restore drills into a separate environment

## Recommendation

Use scheduled backups plus PITR as the default baseline. Full archive exports are helpful for migrations but are not the primary recovery tool.

## Verification

- verify backup retention policy monthly
- verify restore drill evidence quarterly
- document RPO and RTO in the operations handbook

## Common confusion

An archive export is not the same thing as point-in-time recovery. PITR is for rolling the database backward to a narrow time window after operator error.
