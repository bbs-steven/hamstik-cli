# Hamstik Agent Skill

The canonical Hamstik Agent Skill will live in this directory once the Hamstik CLI
Dogfooding Alpha command surface is implemented and stable.

The future skill will teach coding agents to use the official `hamstik` CLI — not to
duplicate the Hamstik REST API. It will cover context discovery, reading Work Items,
safe transitions, commenting, and `--json` output handling.

No active Agent Skill is provided yet because the CLI command surface it must describe
does not exist. Creating one early would risk agents treating incomplete instructions as
authoritative.