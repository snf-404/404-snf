# snf-radar

IWR6843 mmWave radar upper layer for 404-snf.

## Design split

| Concern | Where | Notes |
| --- | --- | --- |
| Serial transport | Rust (`tokio-serial`) | Owns the radar **data** UART (default 921 600 baud). |
| Config / data state machine | Rust | Sends the chirp config, tracks frame sync. |
| Framing & validation | Rust | Reassembles magic-word-delimited frames from the byte stream. |
| Raw TLV decode | **C/C++ via `cxx`** | Delegated to the TI mmWave SDK — the only native code in the project. |
| Feature extraction | Rust | Turns decoded detections/vitals into fatigue-model features. |

Everything except the innermost TLV decode is Rust, by design ("offload all other
logic to Rust").

## Building

- **Default (host-checkable):** pure Rust, no C++ toolchain or vendor SDK
  required. `parse_frame` returns a stub.

  ```bash
  cargo check -p snf-radar
  ```

- **With the TI SDK:** enable the `sdk` feature and point `MMWAVE_SDK_PATH` at
  the SDK root. The `cxx` bridge (`src/ffi.rs`) plus the shim
  (`cxx/src/mmwave_shim.cpp`) compile and link against the SDK.

  ```bash
  MMWAVE_SDK_PATH=/opt/ti/mmwave_sdk cargo build -p snf-radar --features sdk
  ```

The SDK components to wrap are catalogued in
[`references/mmwave_sdk_software_manifest.html`](../../references/mmwave_sdk_software_manifest.html).

## Status

Scaffold. No real serial I/O, framing, SDK parsing, or feature extraction yet.
