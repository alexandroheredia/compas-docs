# Testability Matrix

## Purpose

Identify which services are mostly pure logic and which are deeply coupled to Firebase or other infrastructure.

## Heavy infrastructure dependencies

- time balance service reads and writes balance documents and work sessions
- work session controller depends on active-session queries and transactional writes
- organization service is mostly a data-access helper

## Pure or near-pure logic candidates

- weekday mapping helper that converts a date into scheduled hours
- duration formatting utilities
- timestamp parsing helpers
- report row formatting logic

## Refactoring guidance

Extract deterministic calculations first so they can be tested without mocking the database.
