#!/bin/bash
set -euo pipefail

# Claude Code on the web clones the repo into a fresh container every session,
# so the plugins listed in .claude/settings.json are enabled but never on disk.
# Install them here. Both commands are idempotent and exit 0 when the
# marketplace or plugin is already present.
#
# Keep these lists in sync with "extraKnownMarketplaces" and "enabledPlugins"
# in .claude/settings.json.

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

MARKETPLACES=(
  "mattpocock/skills"
  "DietrichGebert/ponytail"
)

PLUGINS=(
  "mattpocock-skills@mattpocock"
  "ponytail@ponytail"
)

for marketplace in "${MARKETPLACES[@]}"; do
  claude plugin marketplace add "$marketplace"
done

for plugin in "${PLUGINS[@]}"; do
  claude plugin install "$plugin"
done
