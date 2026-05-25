# Universal Implementation Plan Template

<!--
AUTHORING MODEL INSTRUCTIONS:
Use this template to produce an execution-ready implementation plan for a weaker execution model.
Replace every placeholder with concrete project-specific content before handing off the plan.
Do not leave TBD, TODO, "investigate", "decide later", or ambiguous alternatives in the final plan.
If information is unknown, add an explicit discovery step that tells the executing model exactly which file, command, symbol, log, issue, or test to inspect and exactly how to use the result.
Every section is marked REQUIRED or CONDITIONAL. Required sections must always be filled. Conditional sections must be filled when their stated condition applies; otherwise write "Not applicable" and one sentence explaining why.
Write instructions as commands to the executing model. Prefer precise file paths, symbol names, commands, expected outputs, and pass/fail criteria over prose.
-->

## 1. Plan Control

<!-- REQUIRED. Fill this table exactly. Use one row per field. Keep values short but specific. -->

| Field | Value |
| --- | --- |
| Plan title | `<concise title>` |
| Plan type | `<Feature implementation / Bug fix / Refactor>` |
| Target project/repository | `<repo or project name>` |
| Target branch | `<branch name or "executor creates branch from ...">` |
| Authoring model | `<model/name if relevant>` |
| Intended executing model | `<model/name or capability level if relevant>` |
| Date authored | `<YYYY-MM-DD>` |
| Expected completion state | `<one sentence describing the final user-visible or codebase state>` |

## 2. Execution Contract

<!-- REQUIRED. This section tells the executing model how strictly to follow the plan. Keep all bullets. Add project-specific constraints where needed. -->

The executing model must follow this plan exactly.

- Do not make architectural decisions not explicitly specified in this plan.
- Do not broaden scope, add convenience features, rename unrelated code, or perform opportunistic cleanup.
- Do not skip validation steps. If a validation step fails, stop and follow the recovery instructions in Section 13.
- Do not mark the plan complete unless every item in Section 14 is satisfied.
- Preserve existing behavior unless this plan explicitly says to change it.
- Preserve user changes and unrelated work in the repository. Do not revert or overwrite files outside this plan.
- If a required file, symbol, command, or behavior differs from this plan, stop at the nearest checkpoint and record the discrepancy using the format in Section 13.4.

## 3. Context And Goal

<!-- REQUIRED. Explain what is being done and why. Use direct, concrete statements. Include the problem, the intended outcome, and why the work matters. -->

### 3.1 Summary

`<One paragraph summarizing the work. State what will be built, fixed, or refactored and the desired end state.>`

### 3.2 Problem Or Opportunity

`<Describe the current problem, defect, missing capability, maintainability issue, or product opportunity.>`

### 3.3 Intended Behavior Or Outcome

`<Describe the exact behavior, user experience, API behavior, internal structure, or maintenance outcome expected after completion.>`

### 3.4 Links And References

<!-- REQUIRED. Fill this table. Include issues, tickets, design docs, prior plans, PRs, logs, monitoring dashboards, support cases, or write "None" with a reason. -->

| Type | Reference | Relevance |
| --- | --- | --- |
| `<Issue/Ticket/PR/Doc/Log/Other>` | `<URL, file path, or identifier>` | `<why this matters>` |

### 3.5 Prior Attempts Or Related Work

<!-- REQUIRED. List previous attempts, partial implementations, reverted commits, related migrations, or similar features. If none, state "None identified" and how that was determined. -->

- `<Prior attempt or related work, with file paths/commit/PR if known>`

## 4. Plan Type Requirements

<!-- REQUIRED. Fill only the subsection matching the Plan type. For the other subsections, write "Not applicable" and one sentence explaining why. -->

### 4.1 Feature Implementation Details

<!-- CONDITIONAL: Required when Plan type is Feature implementation. Specify the user-facing or system-facing feature contract. -->

#### Feature Contract

| Aspect | Required Detail |
| --- | --- |
| Primary user/system actor | `<who or what uses the feature>` |
| Trigger | `<action, event, API call, route, command, job, or condition that starts the behavior>` |
| Inputs | `<input fields, parameters, request body, state, files, or external data>` |
| Processing rules | `<ordered rules the implementation must apply>` |
| Outputs | `<UI state, response, persisted data, emitted event, log, or side effect>` |
| Error behavior | `<exact errors, fallback behavior, status codes, messages, or retry behavior>` |
| Permission/security behavior | `<auth, authorization, privacy, validation, escaping, rate limits, or "none">` |

#### Feature Acceptance Scenarios

<!-- Use Given/When/Then rows. Include normal, edge, and failure scenarios. -->

| ID | Given | When | Then |
| --- | --- | --- | --- |
| F-1 | `<initial state>` | `<action>` | `<expected result>` |

### 4.2 Bug Fix Details

<!-- CONDITIONAL: Required when Plan type is Bug fix. Define the observed defect, root cause, and corrected behavior. -->

#### Defect Statement

| Aspect | Required Detail |
| --- | --- |
| Observed broken behavior | `<what happens now>` |
| Expected behavior | `<what should happen instead>` |
| Reproduction reliability | `<always/intermittent/rare plus conditions>` |
| First known affected version/commit | `<version/commit/date or "unknown with discovery step ...">` |
| User/customer impact | `<impact and severity>` |

#### Reproduction Procedure

<!-- REQUIRED for bug fixes. Provide exact commands, clicks, requests, data setup, or test steps. The executing model must be able to reproduce or simulate the defect before applying the fix unless impossible. -->

1. `<Step to create the initial state>`
2. `<Step to trigger the defect>`
3. `<Expected broken result before the fix>`

#### Root Cause Analysis

<!-- REQUIRED for bug fixes. State why the defect occurs, not only where. Reference exact files, functions, data flows, and conditions. -->

| Cause ID | File / Function / Component | Why It Fails | Evidence |
| --- | --- | --- | --- |
| RC-1 | `<path and symbol>` | `<mechanism that causes the bug>` | `<test/log/code path/proof>` |

#### Corrected Behavior

`<Describe exactly how behavior changes after the fix, including boundary cases and unchanged behavior.>`

### 4.3 Refactor Details

<!-- CONDITIONAL: Required when Plan type is Refactor. Define the structural change and the behavior preservation contract. -->

#### Refactor Objective

`<State the maintainability, readability, performance-neutral, dependency, layering, or architecture objective. Do not describe behavior changes unless they are incidental and explicitly allowed.>`

#### Behavior Preservation Contract

<!-- REQUIRED for refactors. List behaviors that must remain identical. Tie each behavior to a test or verification step. -->

| Behavior ID | Existing Behavior To Preserve | Verification Method |
| --- | --- | --- |
| R-1 | `<behavior>` | `<test, command, manual check, snapshot, or code inspection>` |

#### Allowed Structural Changes

<!-- REQUIRED for refactors. List exactly what may be renamed, moved, extracted, consolidated, or deleted. -->

- `<Allowed structural change with exact paths/symbols>`

#### Forbidden Structural Changes

<!-- REQUIRED for refactors. Prevent the executing model from drifting into unrelated cleanup or behavior changes. -->

- `<Forbidden change, e.g. changing public API shape, database schema, dependency versions, UI copy, serialization format>`

## 5. Scope

<!-- REQUIRED. Be explicit. The in-scope list says what must be done. The out-of-scope list prevents unplanned expansion. -->

### 5.1 In Scope

<!-- REQUIRED. Use one bullet per deliverable. Each bullet must be observable in code, tests, docs, config, or runtime behavior. -->

- `<Specific deliverable>`

### 5.2 Out Of Scope

<!-- REQUIRED. Use one bullet per excluded item. Include tempting adjacent work that the executing model must not do. -->

- `<Explicitly excluded item>`

### 5.3 Non-Goals

<!-- REQUIRED. Use this to distinguish deliberate non-objectives from mere omissions. -->

- `<Thing this plan intentionally does not optimize, redesign, support, migrate, or solve>`

### 5.4 Success Criteria

<!-- REQUIRED. Use checkable criteria only. Each criterion must be objectively verifiable by a command, test, inspection, or runtime result. -->

| ID | Criterion | Verification |
| --- | --- | --- |
| S-1 | `<specific completion condition>` | `<exact command, test, inspection, or manual check>` |

## 6. Current State Analysis

<!-- REQUIRED. Describe the existing code and behavior relevant to this plan. Do not summarize the whole project. Reference exact files and symbols. -->

### 6.1 Relevant Files And Responsibilities

<!-- REQUIRED. Fill this table with every file the executing model needs to understand before editing. -->

| File | Symbol(s) / Section(s) | Current Responsibility | Planned Action |
| --- | --- | --- | --- |
| `<path>` | `<function/class/component/config section>` | `<what it does today>` | `<read only / modify / add tests / delete / move>` |

### 6.2 Current Data And Control Flow

<!-- REQUIRED. Provide an ordered flow from entry point to output. Include state, persistence, network calls, events, and errors when relevant. -->

1. `<Current step 1: entry point and file/symbol>`
2. `<Current step 2: downstream call/data transformation>`
3. `<Current step 3: output, side effect, or failure point>`

### 6.3 Current Limitations

<!-- REQUIRED. List limitations that motivate this plan. For refactors, include maintainability limitations. For bug fixes, include failure conditions. For features, include missing behavior. -->

- `<Limitation with file/symbol and concrete effect>`

### 6.4 Existing Tests And Coverage

<!-- REQUIRED. List relevant existing tests and what they currently prove. If there are no tests, state that explicitly and identify the closest test location or pattern. -->

| Test File | Test Name / Area | What It Covers Today | Gap This Plan Must Address |
| --- | --- | --- | --- |
| `<path>` | `<test name or describe block>` | `<current coverage>` | `<missing assertion/scenario>` |

## 7. Target Design

<!-- REQUIRED. Specify the final architecture or implementation shape so the executing model does not invent one. -->

### 7.1 Target Data And Control Flow

<!-- REQUIRED. Provide the new intended flow as an ordered list. Every changed or new step must reference files/symbols. -->

1. `<Target step 1: entry point and file/symbol>`
2. `<Target step 2: validation/transformation/call>`
3. `<Target step 3: output, persistence, event, or error behavior>`

### 7.2 API, Interface, Or Contract Changes

<!-- REQUIRED. Use this table for public APIs, internal interfaces, function signatures, component props, CLI options, config keys, schema fields, events, or data contracts. If none, write a single row saying "No contract changes". -->

| Contract | Current | New | Compatibility Notes |
| --- | --- | --- | --- |
| `<function/API/schema/prop/config/event>` | `<current shape>` | `<new shape>` | `<breaking/non-breaking/migration required/none>` |

### 7.3 Data Model, Persistence, Or Migration Changes

<!-- CONDITIONAL: Required if the plan changes database schema, persisted files, caches, indexes, serialized formats, local storage, queues, or external state. Otherwise write "Not applicable". -->

| Store / File / Schema | Current State | Required Change | Migration / Backfill / Cleanup | Rollback Impact |
| --- | --- | --- | --- | --- |
| `<store or schema>` | `<current>` | `<change>` | `<exact migration step or none>` | `<how rollback handles data>` |

### 7.4 Error Handling And Edge Behavior

<!-- REQUIRED. Define exact behavior for invalid input, missing data, external failures, permission failures, concurrency, timeouts, empty states, and boundaries relevant to this plan. -->

| Case | Required Behavior | Verification |
| --- | --- | --- |
| `<edge/error case>` | `<exact behavior, message, status, fallback, retry, or no-op>` | `<test or check>` |

### 7.5 Observability And Diagnostics

<!-- CONDITIONAL: Required if the change affects production behavior, background jobs, API calls, external services, critical user paths, or difficult-to-debug flows. Otherwise write "Not applicable". Specify exact logs, metrics, traces, alerts, or deliberately no new observability. -->

| Signal Type | Location | Required Signal | Sensitive Data Rules |
| --- | --- | --- | --- |
| `<log/metric/trace/alert/event/none>` | `<file/symbol/system>` | `<exact message, metric name, dimensions, or rationale for none>` | `<what must not be logged>` |

## 8. Dependencies And Constraints

<!-- REQUIRED. Identify everything this plan depends on and everything affected by it. -->

### 8.1 Upstream Dependencies

<!-- REQUIRED. Include libraries, services, APIs, schemas, environment variables, generated code, build tools, feature flags, credentials, and prior work. -->

| Dependency | Type | Required State / Version | Verification |
| --- | --- | --- | --- |
| `<dependency>` | `<library/service/API/env/tool/feature flag/prior work>` | `<required condition>` | `<how executor verifies>` |

### 8.2 Downstream Dependents

<!-- REQUIRED. List callers, consumers, jobs, routes, UI screens, packages, external systems, docs, or tests that rely on the changed area. -->

| Dependent | How It Depends On This Area | Required Protection |
| --- | --- | --- |
| `<dependent>` | `<dependency relationship>` | `<test, compatibility requirement, or manual check>` |

### 8.3 Environmental Constraints

<!-- REQUIRED. Include runtime versions, OS constraints, local services, API keys, network access, test data, permissions, time limits, or write "None known". -->

- `<Constraint and required handling>`

### 8.4 Security, Privacy, And Compliance Constraints

<!-- REQUIRED. State relevant security/privacy rules or explicitly state that none are relevant. Include validation, authorization, secret handling, PII, logging restrictions, and dependency trust. -->

- `<Constraint and how implementation must satisfy it>`

## 9. Risk Assessment

<!-- REQUIRED. Identify likely failure modes before implementation. Include technical, behavioral, operational, and testing risks. -->

| Risk ID | Risk | Why It Could Happen | Impact | Mitigation In This Plan | Verification |
| --- | --- | --- | --- | --- | --- |
| RISK-1 | `<risk>` | `<cause>` | `<impact>` | `<specific plan step or guardrail>` | `<test/check>` |

## 10. Implementation Steps

<!--
REQUIRED. This is the authoritative execution sequence.
Every step must be atomic, ordered, independently verifiable, and specific to file and symbol level.
Do not write vague steps such as "update service layer", "handle errors", "add tests", or "refactor component".
If exact code is known, include before and after code blocks.
If exact code cannot be known until inspection, the step must include a bounded discovery action and a rule that determines the exact edit.
The executing model must complete each step's verification before moving to the next step.
-->

### Step 1: `<imperative title naming exact file/symbol>`

<!-- REQUIRED for every step. Copy this entire step structure for each numbered step. Increment step numbers sequentially. -->

#### 1.1 Purpose

`<One sentence explaining why this step exists and which success criterion or risk it supports.>`

#### 1.2 Files And Symbols

<!-- REQUIRED. List only files touched or inspected by this step. If a generated file is affected, say whether to edit it directly or regenerate it. -->

| Path | Symbol / Section | Action |
| --- | --- | --- |
| `<path>` | `<function/class/component/test/config section>` | `<inspect/modify/create/delete/rename/regenerate>` |

#### 1.3 Preconditions

<!-- REQUIRED. State what must already be true before this step starts. Reference previous step verification IDs when relevant. -->

- `<Precondition>`

#### 1.4 Exact Change

<!-- REQUIRED. Use one of the formats below. Prefer Format A when exact code is known. Use Format B only when exact code depends on repository inspection that cannot be precomputed. -->

**Format A: Known Code Change**

<!-- Include the surrounding function/class/block name. The executing model must apply the after state exactly, adjusting only formatting required by the project's formatter. -->

In `<path>`, replace this code in `<symbol/block>`:

```<language>
<before code>
```

With this code:

```<language>
<after code>
```

**Format B: Bounded Discovery Then Deterministic Edit**

<!-- Use this only when exact code cannot be written in advance. The discovery must be narrow and the edit rule must leave no architectural choice. -->

1. Inspect `<exact file/path/pattern/command>` to locate `<exact symbol or condition>`.
2. If `<condition A>`, make `<specific edit A>`.
3. If `<condition B>`, make `<specific edit B>`.
4. If neither condition is true, stop and use the discrepancy report in Section 13.4.

#### 1.5 Required Local Verification

<!-- REQUIRED. The executing model must run or perform this before continuing. Include exact commands and expected output, or exact inspection criteria. -->

| Verification ID | Command / Inspection | Expected Result |
| --- | --- | --- |
| V1.1 | `<command or inspection>` | `<expected output/result>` |

#### 1.6 Failure Handling For This Step

<!-- REQUIRED. Say what to do if the change or verification fails. Include whether to retry, inspect a specific file, revert this step only, or stop. -->

- `<Failure condition>`: `<required response>`

### Step 2: `<imperative title naming exact file/symbol>`

<!-- REQUIRED if more work remains. Repeat the exact structure from Step 1. -->

#### 2.1 Purpose

`<One sentence.>`

#### 2.2 Files And Symbols

| Path | Symbol / Section | Action |
| --- | --- | --- |
| `<path>` | `<symbol/section>` | `<action>` |

#### 2.3 Preconditions

- `<Precondition>`

#### 2.4 Exact Change

**Format A: Known Code Change**

In `<path>`, replace this code in `<symbol/block>`:

```<language>
<before code>
```

With this code:

```<language>
<after code>
```

**Format B: Bounded Discovery Then Deterministic Edit**
1. Inspect `<exact file/path/pattern/command>` to locate `<exact symbol or condition>`.
2. If `<condition A>`, make `<specific edit A>`.
3. If `<condition B>`, make `<specific edit B>`.
4. If neither condition is true, stop and use the discrepancy report in Section 13.4.

#### 2.5 Required Local Verification

| Verification ID | Command / Inspection | Expected Result |
| --- | --- | --- |
| V2.1 | `<command or inspection>` | `<expected output/result>` |

#### 2.6 Failure Handling For This Step

- `<Failure condition>`: `<required response>`

## 11. Testing Requirements

<!-- REQUIRED. Define exact tests to add, update, and run. Testing must prove success criteria and protect against regressions. -->

### 11.1 Test Strategy

<!-- REQUIRED. State the testing approach by level. Include why each level is needed or not needed. -->

| Level | Required? | Reason | Files / Commands |
| --- | --- | --- | --- |
| Unit | `<Yes/No>` | `<why>` | `<test files and commands>` |
| Integration | `<Yes/No>` | `<why>` | `<test files and commands>` |
| End-to-end | `<Yes/No>` | `<why>` | `<test files and commands>` |
| Static analysis / typecheck / lint | `<Yes/No>` | `<why>` | `<commands>` |
| Manual verification | `<Yes/No>` | `<why>` | `<steps>` |

### 11.2 Tests To Add Or Modify

<!-- REQUIRED. Each row must describe one exact test case. Do not write "add tests for X". Include initial state, action, expected result, and file. -->

| Test ID | Test File | Test Name | Initial State / Fixture | Action | Expected Result |
| --- | --- | --- | --- | --- | --- |
| T-1 | `<path>` | `<exact test name>` | `<fixture/setup>` | `<operation>` | `<assertions>` |

### 11.3 Existing Tests That Must Continue Passing

<!-- REQUIRED. List targeted existing tests and full suites. Include exact commands. -->

| Command | Purpose | Expected Passing Result |
| --- | --- | --- |
| `<command>` | `<what this protects>` | `<expected output/status>` |

### 11.4 Edge Cases To Cover

<!-- REQUIRED. List edge cases and tie each to a test or manual verification. -->

| Edge Case | Coverage Method | Expected Result |
| --- | --- | --- |
| `<edge case>` | `<test ID or manual step>` | `<expected behavior>` |

### 11.5 Test Data And Fixtures

<!-- CONDITIONAL: Required if tests need fixtures, factories, seed data, mocks, snapshots, network stubs, clocks, files, or environment variables. Otherwise write "Not applicable". -->

| Fixture / Data | Location | Required Content | Cleanup |
| --- | --- | --- | --- |
| `<fixture>` | `<path>` | `<data>` | `<cleanup or none>` |

## 12. Documentation, Configuration, And Generated Artifacts

<!-- REQUIRED. State whether docs, config, generated files, changelogs, schemas, API docs, snapshots, lockfiles, or examples must change. Do not leave this implicit. -->

### 12.1 Documentation Updates

| Document | Required Change | Reason |
| --- | --- | --- |
| `<path or "None">` | `<change or "No documentation change required">` | `<why>` |

### 12.2 Configuration Updates

| Config File / Setting | Required Change | Deployment / Runtime Impact |
| --- | --- | --- |
| `<path/key or "None">` | `<change or "No configuration change required">` | `<impact>` |

### 12.3 Generated Artifacts

| Artifact | Source Command | Edit Directly? | Expected Result |
| --- | --- | --- | --- |
| `<path or "None">` | `<generation command or "N/A">` | `<Yes/No>` | `<expected generated change>` |

## 13. Rollback And Recovery

<!-- REQUIRED. Give exact instructions for failed steps, partial completion, and safe stop points. -->

### 13.1 Safe Checkpoints

<!-- REQUIRED. Identify points where the executing model can stop without leaving the project broken. -->

| Checkpoint | Reached After | Safe State Description | Validation Command |
| --- | --- | --- | --- |
| C-1 | `<step number>` | `<what is complete and safe>` | `<command/check>` |

### 13.2 Step Failure Recovery

<!-- REQUIRED. Include general recovery rules. Step-specific recovery belongs in Section 10. -->

- If a verification command fails, do not continue to the next implementation step.
- First inspect the failing output and compare it to the expected result listed for that verification.
- If the failure is caused by the step just performed, revert only that step's changes and retry the step once.
- If the failure is caused by pre-existing unrelated work, stop and report the discrepancy using Section 13.4.
- If the failure source cannot be determined within `<timebox or number of attempts>`, stop and report the discrepancy using Section 13.4.

### 13.3 Rollback Procedure

<!-- REQUIRED. Explain how to return to the last known good state without destroying unrelated user changes. Use file-level rollback instructions, not broad destructive commands. -->

1. `<Rollback step 1: identify files changed by this plan>`
2. `<Rollback step 2: restore only those files to the previous state or reverse exact changes>`
3. `<Rollback step 3: remove generated artifacts created only by this plan, if safe>`
4. `<Rollback step 4: rerun validation command to confirm known good state>`

### 13.4 Discrepancy Report Format

<!-- REQUIRED. The executing model must use this exact format if it cannot proceed. -->

```markdown
## Plan Discrepancy Report

Step: <step number and title>
Expected: <what the plan said would be true>
Actual: <what the executor observed>
Files inspected: <paths>
Commands run: <commands and exit statuses>
User/unrelated changes detected: <yes/no/unknown; details>
Recommended next action: <specific recommendation without making unauthorized changes>
```

## 14. Final Verification And Completion Checklist

<!-- REQUIRED. The executing model must complete every item before declaring the plan done. Add plan-specific items as needed. -->

### 14.1 Final Verification Commands

<!-- REQUIRED. Include exact commands in the order they must be run. Passing means exit code 0 and expected output unless otherwise stated. -->

| Order | Command | Expected Result |
| --- | --- | --- |
| 1 | `<command>` | `<expected output/status>` |
| 2 | `<command>` | `<expected output/status>` |

### 14.2 Completion Checklist

<!-- REQUIRED. Keep these items and add specific items for this plan. The executing model must check each box only after verifying it. -->

- [ ] Plan type-specific requirements in Section 4 are satisfied.
- [ ] All in-scope items in Section 5.1 are complete.
- [ ] No out-of-scope items in Section 5.2 were implemented.
- [ ] Every implementation step in Section 10 was completed in order.
- [ ] Every local verification in Section 10 passed before proceeding to the next step.
- [ ] All tests listed in Section 11.2 were added or updated exactly as specified.
- [ ] All commands listed in Section 11.3 and Section 14.1 pass.
- [ ] Edge cases listed in Section 11.4 are covered.
- [ ] Documentation, configuration, and generated artifact requirements in Section 12 are complete.
- [ ] Security, privacy, and compliance constraints in Section 8.4 are satisfied.
- [ ] No unrelated files were modified.
- [ ] The final diff matches the intended files and symbols in this plan.
- [ ] Rollback instructions in Section 13 remain accurate after implementation.

### 14.3 Final Response Requirements For Executing Model

<!-- REQUIRED. Tell the executing model exactly what to report after completion. -->

When the plan is complete, report:

- `<One sentence summary of what changed>`
- `<List of files changed>`
- `<Tests/commands run with pass/fail results>`
- `<Any deviations from the plan, or "No deviations">`
- `<Any follow-up work intentionally left out of scope>`

## 15. Appendix

<!-- CONDITIONAL: Use only for supporting material that is too long for earlier sections but required for execution, such as full API examples, large schemas, logs, screenshots descriptions, migration SQL, or command output. If unused, write "Not applicable". -->

### 15.1 Full Before / After Examples

```<language>
<large before/after snippet if needed>
```

### 15.2 Reference Logs Or Outputs

```text
<log/output if needed>
```

### 15.3 Additional Execution Notes

- `<note required for correct execution>`
