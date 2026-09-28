#!/usr/bin/env bash
set -euo pipefail

: "${CURRENT_TREE_REAL_CARGO:?missing immutable real cargo path}"
: "${CURRENT_TREE_STAGE8A5_REPLAY_COMPAT_HELPER:?missing replay compatibility helper}"
: "${CURRENT_TREE_STAGE8A5_REPLAY_COMPAT_EVIDENCE:?missing replay compatibility evidence path}"

workspace_test=0
all_targets_test=0
for argument in "$@"; do
  [[ "$argument" == "--workspace" ]] && workspace_test=1
  [[ "$argument" == "--all-targets" ]] && all_targets_test=1
done

if [[ "${CURRENT_TREE_STAGE8A5_REPLAY_COMPAT:-0}" == "1" ]] && \
  [[ "${1:-}" == "test" ]] && \
  [[ "$workspace_test" == "1" ]] && \
  [[ "$all_targets_test" == "1" ]]; then
  workspace="$(git rev-parse --show-toplevel 2>/dev/null || true)"
  if [[ -n "$workspace" ]]; then
    source_ref="$(git -C "$workspace" rev-parse HEAD)"
    case "$source_ref" in
      10e357825a701193d964975bb5769bd0745d4986|\
      2b6371adb905654e0ddd8b6714159bcef737b577|\
      2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64|\
      8418cfb63ecee6702bf8a2873592b7cad1e711ee|\
      8d4c1f437c02cfb023aa75fb4a411b9394d2d293|\
      a1044e0dbe324c722b637498ca80ffafd9f0cbee|\
      bf58b47fdef8af774a4107455dfcc6204e594283|\
      e0bf9b7d9eb209e19b875f199511a493ddcd0da9|\
      e10d8fb0f9e095a849b1e56779a0597606d22111|\
      ec71791563a933889eb825f6f8f0846915ba6415)
        python3 "$CURRENT_TREE_STAGE8A5_REPLAY_COMPAT_HELPER" \
          --root "$workspace" \
          --evidence "$CURRENT_TREE_STAGE8A5_REPLAY_COMPAT_EVIDENCE"
        set +e
        "$CURRENT_TREE_REAL_CARGO" "$@"
        cargo_status=$?
        set -e
        python3 "$CURRENT_TREE_STAGE8A5_REPLAY_COMPAT_HELPER" \
          --root "$workspace" \
          --evidence "$CURRENT_TREE_STAGE8A5_REPLAY_COMPAT_EVIDENCE" \
          --restore
        exit "$cargo_status"
        ;;
    esac
  fi
fi

exec "$CURRENT_TREE_REAL_CARGO" "$@"
