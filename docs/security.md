# Security policy

## Secrets

Never commit:

- Finam secret token;
- JWT/access tokens;
- account ids in public examples if they identify live accounts;
- broker reports containing personal data;
- private keys or `.env` files.

Use local `.env` files only for development and keep them ignored by git.

## Logging

Logs may include:

- token fingerprints;
- account aliases;
- normalized symbols;
- client order ids;
- broker order ids.

Logs must not include:

- raw secret token;
- raw JWT;
- full personal account identifiers unless explicitly required for local-only diagnostics.

## Live trading guard

Order-emitting functionality must require:

- explicit config flag;
- explicit account id;
- explicit strategy id;
- readiness = live-ready;
- operator pause not active;
- idempotent client order id.

