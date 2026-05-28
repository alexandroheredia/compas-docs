# Component Selection Guide

Use this guide to answer one practical question quickly: which promoted component should I use in this screen, and why?

## Common choices

- use `PanelCard` for a surfaced card-like block
- use `SectionBlock` for a titled group without a surface
- use `PrimaryButton` for the main call to action
- use `DangerButton` for irreversible actions
- use `AutoCompleteField` when the user should type to narrow a large list
- use `SelectField` for a short closed list of options

## Anti-patterns

- do not assemble whole screens from low-level focus-ring primitives
- do not use tooltip components for rich interactive content
