# M4-3l dry runtime attach / M1-M10 parity

Status: source-only / no-live / no order endpoints.

M4-3l starts the strategy-facing parity layer after M4-3k observability parity.
The goal is to ensure that the FINAM shadow contour can feed the future IMOEXF
hybrid dry runtime with the same 10-minute closed-bar contract that the current
ALOR contour uses.

## ALOR oracle

The ALOR gateway does not locally assemble the strategy 10-minute bars from
1-minute bars. It subscribes to broker bars directly:

```text
opcode = BarsGetAndSubscribe
tf     = cfg.tf_sec
```

For the current 10-minute strategy configurations, `cfg.tf_sec = 600`, and the
Redis bar stream is named as a native 10-minute stream:

```text
md.bars.<portfolio>.10m
```

Therefore ALOR is the native 10-minute oracle for strategy-facing parity.

## FINAM strategy-facing path

The accepted FINAM path for this stage is:

```text
FINAM WS final M1 bars
  -> CanonicalBarAggregator(target = 600s)
  -> canonical final 10m bar
  -> M3h dry runtime shadow consumer
  -> M4-3l timeframe + provenance gate
  -> M3i paper strategy input
```

Raw FINAM 1-minute bars must not become strategy-facing decisions. Even if the
generic M3h shadow consumer observes a final 1-minute bar, the M4-3l gate rejects
it unless `bar.timeframe_sec == 600`.

M4-3l-a hardens this boundary further: `bar.timeframe_sec == 600` is not enough.
The strategy-facing input must carry explicit sidecar provenance:

```text
bar_source_mode       = FinamDerivedM1ToM10
source_timeframe_sec  = 60
target_timeframe_sec  = 600
aggregation_complete  = true
gap_absence_proven    = true
```

This provenance is deliberately separate from the broker-neutral `Bar` payload.
It proves how the 10-minute bar was produced without making every broker-core
bar carry FINAM-specific metadata.

FINAM-native 10-minute bars are intentionally not treated as equivalent yet.
They require a separate active-session characterization before they can replace
or bypass the canonical M1-to-10m path. While that characterization is pending,
FINAM-native 10-minute bars are rejected even if they are final live bars with
`timeframe_sec == 600`.

## Boundary

M4-3l must not:

- send FINAM `POST /orders`;
- send FINAM `DELETE /orders/{id}`;
- enable command-consumer-to-real-FINAM;
- enable continuous runtime live;
- emit `LiveReady`;
- attach a live strategy runtime;
- open or close a position;
- enable Stop/SLTP/bracket/replace/multi-leg;
- cut over automatically from ALOR to FINAM.

## Acceptance

Source acceptance requires:

- ALOR oracle documented as native `BarsGetAndSubscribe(tf=600)`;
- FINAM strategy path documented as canonical M1-to-10m derived bars;
- raw FINAM M1 strategy-facing input rejected by timeframe gate;
- FINAM native M10 strategy-facing input rejected while characterization is
  pending;
- canonical FINAM 10m strategy-facing input accepted only with
  `FinamDerivedM1ToM10` provenance;
- derived M1-to-M10 acceptance requires `aggregation_complete = true` and
  `gap_absence_proven = true`;
- canonical FINAM 10m strategy-facing input accepted in dry mode;
- report flags keep runtime-live, LiveReady, real order endpoints, and real
  command consumer disabled.

Runtime evidence waits for active-session comparison of ALOR native 10m bars
against FINAM derived 10m bars.
