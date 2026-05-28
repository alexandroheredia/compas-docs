# Incident Response Runbook

## Purpose

This runbook defines how the team responds to a production outage that affects the payroll export pipeline.

## Severity Levels

- SEV-1: payroll processing blocked for all organizations
- SEV-2: export generation delayed but workaround exists
- SEV-3: partial degradation with no active customer block

## First 30 minutes

1. Confirm whether the export queue is stuck or merely slow.
2. Freeze non-essential deploys.
3. Assign one incident commander and one communications owner.
4. Check the retry worker, dead-letter queue, and database health.

## Recovery targets

- Recovery time objective: 2 hours
- Recovery point objective: 15 minutes

## Evidence to capture

- timeline of actions
- queue depth screenshots
- failed export identifiers
- database restore decisions

## After action review

Run a blameless postmortem and record concrete preventive actions.
