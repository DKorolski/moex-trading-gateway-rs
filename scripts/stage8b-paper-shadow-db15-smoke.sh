#!/usr/bin/env bash
set -euo pipefail

# Isolated, no-network Stage 8B paper projection smoke. The script deliberately
# uses only local Redis database 15 and refuses to delete pre-existing data.

readonly REDIS_HOST="127.0.0.1"
readonly REDIS_PORT="6379"
readonly REDIS_DB="15"
readonly BROKER_CLI="${STAGE8B_BROKER_CLI:-/opt/moex-finam-paper/bin/broker-cli}"
readonly RUNTIME_CONFIG="${STAGE8B_RUNTIME_CONFIG:-/etc/moex-finam-paper/runtime-unseeded.json}"

command -v jq >/dev/null
command -v redis-cli >/dev/null
[[ -x "$BROKER_CLI" ]]
[[ -f "$RUNTIME_CONFIG" ]]

redis15=(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" -n "$REDIS_DB" --raw)
if [[ "$("${redis15[@]}" DBSIZE)" != "0" ]]; then
  echo "REFUSED: Redis DB 15 is not empty" >&2
  exit 2
fi

cleanup() {
  "${redis15[@]}" FLUSHDB >/dev/null
}
trap cleanup EXIT

readonly run_suffix="$(date -u +%Y%m%dT%H%M%SZ)"
readonly prefix="finam_imoexf_paper:stage8b-smoke:${run_suffix}"
readonly source_stream="${prefix}:m1"
readonly target_stream="${prefix}:m10"
readonly runtime_stream="${prefix}:runtime-state"
readonly batch_stream="finam_imoexf_paper:runtime:stage8b-smoke:${run_suffix}:publish-batches"
readonly dlq_stream="${prefix}:dlq"
readonly group="stage8b-paper-smoke-${run_suffix}"
readonly consumer="stage8b-paper-smoke-consumer"
readonly base_epoch="$(date -u -d '2026-09-02T06:00:00Z' +%s)"

operational_source_before="$(redis-cli -n 0 --raw XLEN finam_imoexf_paper:ws:market_data)"
operational_runtime_before="$(redis-cli -n 0 --raw XLEN finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf)"

for i in $(seq 0 9); do
  open_epoch=$((base_epoch + i * 60))
  close_epoch=$((open_epoch + 60))
  open_ts="$(date -u -d "@${open_epoch}" +%Y-%m-%dT%H:%M:%SZ)"
  close_ts="$(date -u -d "@${close_epoch}" +%Y-%m-%dT%H:%M:%SZ)"
  open_price=$((2200 + i))
  payload="$(jq -cn \
    --arg ts "$open_ts" \
    --arg close_ts "$close_ts" \
    --arg open "$open_price" \
    --arg high "$((open_price + 2))" \
    --arg low "$((open_price - 1))" \
    --arg close "$((open_price + 1))" \
    --arg volume "$((100 + i))" \
    '{
      schema_version: 2,
      ts_utc: $ts,
      source: "stage8b-synthetic-db15",
      msg_type: "MarketData",
      payload: {
        Bar: {
          instrument: {
            symbol: "IMOEXF",
            venue_symbol: "IMOEXF@RTSX",
            exchange: "Moex",
            market: "Futures"
          },
          source_kind: "LiveStream",
          timeframe_sec: 60,
          open_ts: $ts,
          close_ts: $close_ts,
          open: $open,
          high: $high,
          low: $low,
          close: $close,
          volume: $volume,
          is_final: true
        }
      }
    }')"
  "${redis15[@]}" XADD "$source_stream" '*' payload "$payload" >/dev/null
done

consumer_summary="$($BROKER_CLI finam-paper-runtime-consume \
  --config "$RUNTIME_CONFIG" \
  --redis-url "redis://${REDIS_HOST}:${REDIS_PORT}/${REDIS_DB}" \
  --source-stream "$source_stream" \
  --target-stream "$target_stream" \
  --runtime-state-stream "$runtime_stream" \
  --publish-batches-stream "$batch_stream" \
  --dlq-stream "$dlq_stream" \
  --strategy-invocation-shadow \
  --strategy-warmup-bars 1 \
  --group-start-id 0 \
  --group "$group" \
  --consumer "$consumer" \
  --max-iterations 10)"

runtime_payload="$("${redis15[@]}" XREVRANGE "$runtime_stream" + - COUNT 1 | tail -n 1)"
latest_batch_payload="$("${redis15[@]}" XREVRANGE "$batch_stream" + - COUNT 1 | tail -n 1)"
source_len="$("${redis15[@]}" XLEN "$source_stream")"
runtime_len="$("${redis15[@]}" XLEN "$runtime_stream")"
batch_entries="$("${redis15[@]}" XLEN "$batch_stream")"
dlq_len="$("${redis15[@]}" XLEN "$dlq_stream")"
pending_count="$("${redis15[@]}" XPENDING "$source_stream" "$group" | sed -n '1p')"

operational_source_after="$(redis-cli -n 0 --raw XLEN finam_imoexf_paper:ws:market_data)"
operational_runtime_after="$(redis-cli -n 0 --raw XLEN finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf)"

result="$(jq -n \
  --arg run_id "$run_suffix" \
  --argjson consumer "$consumer_summary" \
  --argjson runtime_state "$runtime_payload" \
  --argjson latest_batch_marker "$latest_batch_payload" \
  --argjson source_len "$source_len" \
  --argjson runtime_len "$runtime_len" \
  --argjson batch_entries "$batch_entries" \
  --argjson dlq_len "$dlq_len" \
  --argjson pending_count "$pending_count" \
  --argjson operational_source_before "$operational_source_before" \
  --argjson operational_source_after "$operational_source_after" \
  --argjson operational_runtime_before "$operational_runtime_before" \
  --argjson operational_runtime_after "$operational_runtime_after" \
  '{
    schema_version: 1,
    stage: "Stage 8B isolated FINAM paper-shadow P0 synthetic VPS smoke",
    run_id: $run_id,
    redis_database: 15,
    input: {m1_count: $source_len},
    consumer: $consumer,
    output: {
      runtime_state_count: $runtime_len,
      publish_batch_marker_entries: $batch_entries,
      dlq_count: $dlq_len,
      pending_count: $pending_count,
      runtime_state: $runtime_state,
      latest_publish_batch_marker: $latest_batch_marker
    },
    operational_db0: {
      source_stream_count_before: $operational_source_before,
      source_stream_count_after: $operational_source_after,
      runtime_state_count_before: $operational_runtime_before,
      runtime_state_count_after: $operational_runtime_after
    },
    safety: {
      finam_requests: 0,
      http_post_delete: 0,
      broker_dispatch: false,
      real_orders: false,
      full_trade_token_used: false,
      generation2_private_material_used: false
    }
  }')"

jq -e '
  .redis_database == 15 and
  .input.m1_count == 10 and
  .consumer.paper_only == true and
  .consumer.live_ready_allowed == false and
  .consumer.command_consumer_to_real_finam_enabled == false and
  .consumer.order_placement_enabled == false and
  .consumer.metrics.bars_seen == 10 and
  .consumer.metrics.bars_buffered == 9 and
  .consumer.metrics.bars_published == 1 and
  .consumer.metrics.xack_count == 10 and
  .consumer.metrics.runtime_batches_published == 1 and
  .consumer.metrics.runtime_records_published == 1 and
  .output.runtime_state_count == 1 and
  .output.publish_batch_marker_entries == 2 and
  .output.dlq_count == 0 and
  .output.pending_count == 0 and
  .output.latest_publish_batch_marker.phase == "Committed" and
  .output.runtime_state.paper_only == true and
  .output.runtime_state.payload.RuntimeState.hybrid_intraday.entry_ready == true and
  .output.runtime_state.payload.RuntimeState.hybrid_intraday.last_bar_close == 2210 and
  .output.runtime_state.payload.RuntimeState.safety_boundary.live_orders_enabled == false and
  .operational_db0.source_stream_count_before == .operational_db0.source_stream_count_after and
  .operational_db0.runtime_state_count_before == .operational_db0.runtime_state_count_after and
  .safety.finam_requests == 0 and
  .safety.real_orders == false
' <<<"$result" >/dev/null

printf '%s\n' "$result"
