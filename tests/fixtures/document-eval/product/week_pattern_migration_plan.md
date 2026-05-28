# Week Pattern Migration Plan

## Goal

Make weekday schedules first-class contract data instead of deriving daily hours from weekly minutes.

## Proposed model

Store weekday minutes for Monday through Sunday and derive the weekly total from that schedule.

## Migration rule

Backfill existing contracts to a legacy-compatible five-day weekday split so old records keep their meaning.

## Required follow-up

- route all expected-hours calculations through one shared resolver
- keep stored balance snapshots as mirrors, not the source of truth
- update reports and payroll summaries to use the active weekday map by date
