# Research Note: 30 Hours Over Four Days

## Question

What breaks when a contract stores only weekly minutes but the employee actually works four long days instead of five even weekdays?

## Finding

The system spreads weekly hours across a flat Monday to Friday pattern. That causes the live time balance to drift during the week and can also skew monthly totals.

## Example

- real schedule: Monday to Thursday, 7.5 hours each day
- modeled schedule: Monday to Friday, 6 hours each day

The employee appears ahead of target by Thursday evening even when they are exactly on contract.

## Consequences

- false positive time balance
- incorrect missing-workday detection
- inaccurate payroll summaries for partial ranges
