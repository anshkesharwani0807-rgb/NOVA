# Security Policy

> Security and privacy are not features of NOVA — they are its identity. See
> Chapter 1 (Principles 1, 2, 5) and the forthcoming **Chapter 15 (Security &
> Privacy)**, which will define the threat model, controls, encryption, key custody,
> egress chokepoint, and plugin sandboxing in detail. This file is the baseline
> policy until Chapter 15 is written.

## Core commitments (from the NOVA Bible)

- **Privacy by default.** User data stays on-device unless there is an explicit,
  comprehensible reason to move it and, where material, explicit consent (D3).
- **Privileged egress.** All network egress is a single, logged, consent-gated
  chokepoint. There is no silent phone-home anywhere in NOVA.
- **Owned, encrypted memory.** User memory is encrypted, inspectable, correctable,
  portable, and never mined or sold.
- **No data monetization.** Ever.

## Reporting a vulnerability

**Do not open a public issue for security vulnerabilities.**

1. Report privately to the maintainers (a dedicated security contact/address will be
   published before public release; until then, contact the repository owner
   directly and privately).
2. Include: a description, reproduction steps, affected component, and potential
   impact — especially any that could cause user data to leave the device.
3. You will receive an acknowledgment and a coordinated-disclosure timeline.

Please give us reasonable time to remediate before any public disclosure.

## Supported versions

The project is at genesis stage; a supported-versions matrix will be published with
the first release (policy defined in Chapter 19).

## Scope of highest concern

Given NOVA's design, the highest-severity classes are:

- Any unintended data **egress** from the device.
- Compromise of the **memory store** or its encryption/keys.
- **Plugin** escape from sandboxing (the primary exfiltration vector — Ch1 §1.7.3).
- Weakening of the **consent gate** on autonomous actions.

## Handling secrets

Never commit secrets. `.env`, keys, and `secrets/` are git-ignored. Any credential
committed by accident must be rotated immediately and reported.
