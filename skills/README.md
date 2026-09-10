# Hamstik Agent Skill

The canonical Hamstik Agent Skill will live in this directory once the Hamstik CLI
Dogfooding Alpha command surface is implemented and stable.

The future skill will teach coding agents to use the official `hamstik` CLI — not to
duplicate the Hamstik REST API. It will cover context discovery, reading Work Items,
safe transitions, commenting, and `--json` output handling.

The Dogfooding Alpha command surface now exists, but no active Agent Skill is provided
yet because the CLI remains pre-alpha and the command/output contracts are still being
stabilized. Publishing the Skill after that stabilization avoids agents treating
pre-release instructions as authoritative.
