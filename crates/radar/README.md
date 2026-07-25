# snf-radar

Pure-Rust IWR6843 UART transport — both ports — TLV parsing, and transparent
signal indicators. No C/C++ compiler, native shim, or TI host SDK is used.

> The IWR6843 reaches the board over its **USB virtual COM port**, so the radar
> is an ordinary Linux tty and `RadarStream` reads it directly on the CA35 —
> on target and on a dev host alike. That is the `serial` feature, on by default.
>
> The feature exists because the CM33 also carries a `no_std` parser for the same
> packets (`snf-shared`'s `detect` module), for a build where the sensor is wired
> to USART6 instead. That path is not the default; see
> [`crates/mcu`](../mcu/README.md).

The default build supports the factory Out-of-Box point-cloud firmware. TI
Vital Signs support is deliberately opt-in because it requires different
firmware on the radar:

```bash
cargo test -p snf-radar
cargo test -p snf-radar --features vital-signs
```

## The two UARTs

The sensor enumerates **two** ttys and they are not interchangeable:

| Port | Baud | Carries |
|---|---:|---|
| CLI (`/dev/ttyACM0`, `/dev/ttyUSB0`) | 115 200 | the `mmwDemo:/>` prompt — configuration commands |
| Data (`/dev/ttyACM1`, `/dev/ttyUSB1`) | 921 600 | the binary TLV stream |

The IWR6843 boots **idle**. Until a configuration profile has been sent to the
CLI port and its last line, `sensorStart`, has been accepted, the data port
produces nothing at all — which looks exactly like a broken cable. So the
connect sequence is: configure, *then* read.

```rust,no_run
use snf_radar::{RadarCli, RadarCliConfig, RadarConfig, RadarStream};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
// Sends the profile, one command at a time, each awaiting its own `Done`.
let report = RadarCli::configure(&RadarCliConfig {
    cli_port: "/dev/ttyUSB0".into(),
    ..RadarCliConfig::default()
})
.await?;
for note in &report.notes {
    // `sensorStart: Debug: Init Calibration Status = 0x1ffe`
    println!("{note}");
}

let mut radar = RadarStream::open(RadarConfig::default())?;
# let _ = &mut radar;
# Ok(())
# }
```

With no `profile_path`, `RadarCli::configure` sends the profile compiled into
this crate, [`profiles/out-of-box-6843isk.cfg`](profiles/out-of-box-6843isk.cfg)
— the factory Out-of-Box demo at 10 Hz, static clutter retained (the
gross-activity indicator measures radial speed and wants the still body
present). Any other firmware needs its own file.

A profile is read from a TI `.cfg` file **or** from a pasted `mmwDemo:/>`
session transcript; in a transcript only the text after each prompt is taken as
a command, so the sensor's own `Done` and `Debug:` replies cannot be mistaken
for one. `%` and `#` begin comments.

The handshake is one command at a time, each waiting for its `Done`, rather than
the whole file written at the port with a sleep between lines. `Done` is the
only synchronisation the CLI offers. Notes along the way (`Ignored: Sensor is
already stopped`, the calibration status after `sensorStart`) come back in
`ConfigureReport::notes`; a line starting with `Error` fails the command and
aborts the run, because a sensor started under a half-applied profile emits
frames that parse cleanly and mean something else. A command that never answers
fails after `command_timeout_ms` (5 s by default — sized for `sensorStart`, which
replies only after RF calibration).

Re-running a profile is safe on a sensor that is already streaming: the shipped
one, like TI's, begins with `sensorStop` and `flushCfg`.

## Indicator priority

The score weights impact at 50%, development ease at 25%, and deployment ease
at 25%. Inputs are scored from 1 to 5.

| Rank | Indicator | Impact | Dev ease | Deploy ease | Score | Status |
|---:|---|---:|---:|---:|---:|---|
| 1 | Gross activity trend | 4 | 5 | 5 | 4.50 | Implemented |
| 2 | Respiration rate | 5 | 4 | 3 | 4.25 | Implemented, opt-in |
| 3 | Heart rate | 5 | 4 | 3 | 4.25 | Implemented, opt-in |
| 4 | Micro-movements | 3 | 3 | 5 | 3.50 | Deferred |
| 5 | Head/torso pose drift | 4 | 2 | 3 | 3.25 | Deferred |
| 6 | Postural sway | 4 | 2 | 2 | 3.00 | Deferred |

Respiration precedes heart rate because breathing displacement is larger and
normally easier to recover robustly. This milestone implements heart rate, not
HRV. Reliable HRV needs coherent phase/IQ, individual beat times, and
beat-to-beat validation; TI's result TLV supplies an aggregate heart-rate
number, not those inputs.

Point-cloud radial velocity can describe coarse motion but not validated
sub-millimetre micro-movement. Pose drift needs temporal point-cloud fusion,
calibration, and a pose model. Postural sway is especially sensitive to static
clutter removal and CFAR configuration.

## Firmware and deployment modes

### Factory Out-of-Box firmware

The firmware normally shipped on an IWR6843ISK provides Cartesian detected
points and side information. It supports the dot graph and gross-activity
indicator. It does **not** calculate heart or respiration rates.

Use the default build and protocol:

```rust,no_run
use snf_radar::{
    IndicatorEngine, RadarCli, RadarCliConfig, RadarConfig, RadarProtocol, RadarStream,
};
use std::time::Instant;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
// The built-in profile is this firmware's, so the defaults suffice.
RadarCli::configure(&RadarCliConfig::default()).await?;

let config = RadarConfig {
    data_port: "/dev/ttyUSB1".into(),
    protocol: RadarProtocol::OutOfBox,
    ..RadarConfig::default()
};
let mut radar = RadarStream::open(config)?;
let mut indicators = IndicatorEngine::default();

while let Some(frame) = radar.next_frame().await? {
    let snapshot = indicators.update(Instant::now(), &frame);
    println!("{:?}", snapshot.activity);
}
# Ok(())
# }
```

### TI Vital Signs firmware

Heart rate and respiration require TI Radar Toolbox's **Vital Signs With People
Tracking** firmware. For IWR6843ISK, use the externally supplied binary named:

```text
vital_signs_tracking_6843ISK_demo.bin
```

Recent Radar Toolbox releases place it below:

```text
source/ti/examples/Industrial_and_Personal_Electronics/
  Vital_Signs/Vital_Signs_With_People_Tracking/prebuilt_binaries/
```

The repository does not redistribute that binary. Obtain Radar Toolbox from TI,
then:

1. Put the IWR6843ISK into its documented flashing mode.
2. Flash the ISK binary with TI UniFlash.
3. Return the board to functional mode and reset it.
4. Point `RadarCliConfig::profile_path` (`radar.profile_path` in `Repose.toml`)
   at the matching ISK `.cfg` from the same example. The built-in profile
   configures the *factory* demo and must not be sent to this firmware; `snf-app`
   refuses to start rather than send it, and the pairing is why the protocol is
   never auto-detected.
5. Read the data UART at the baud rate that configuration selects (normally
   921600).

Follow the quick-start instructions shipped with the exact Radar Toolbox
release for jumper and UniFlash details. Do not use the AOP binary or AOP
configuration on an ISK board.

Enable and explicitly select the protocol:

```toml
snf-radar = { path = "../radar", features = ["vital-signs"] }
```

```rust,no_run
use snf_radar::{
    IndicatorEngine, RadarCli, RadarCliConfig, RadarConfig, RadarProtocol, RadarStream,
};
use std::time::Instant;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
// This firmware needs the vital-signs example's own `.cfg`.
RadarCli::configure(&RadarCliConfig {
    profile_path: Some("/opt/snf/vital-signs.cfg".into()),
    ..RadarCliConfig::default()
})
.await?;

let mut radar = RadarStream::open(RadarConfig {
    data_port: "/dev/ttyUSB1".into(),
    protocol: RadarProtocol::VitalSigns,
    ..RadarConfig::default()
})?;
let mut indicators = IndicatorEngine::default();

while let Some(frame) = radar.next_frame().await? {
    let result = indicators.update(Instant::now(), &frame);
    if let Some(heart) = result.heart_rate {
        println!("heart: {heart:?}");
    }
    if let Some(respiration) = result.respiration_rate {
        println!("respiration: {respiration:?}");
    }
}
# Ok(())
# }
```

The protocol is never auto-detected: both demos share the same magic word and
40-byte header, so automatic selection can silently give a plausible but wrong
interpretation.

## Supported wire formats

Every packet begins with the standard 40-byte little-endian xWR68xx header. The
stream reader tolerates arbitrary UART read boundaries, leading garbage,
multiple packets in one read, and a malformed packet followed by a valid one.
Declared packet size is capped by `RadarConfig::max_packet_length`.

### Packet padding is not zero, and must not be required to be

The demo rounds `totalPacketLen` up to a multiple of
`MMWDEMO_OUTPUT_MSG_SEGMENT_LEN` (32) and transmits the slack from a **local
array it never initializes** — in `MmwDemo_transmitProcessedOutput`:

```c
uint8_t padding[MMWDEMO_OUTPUT_MSG_SEGMENT_LEN];   /* never assigned */
...
numPaddingBytes = MMWDEMO_OUTPUT_MSG_SEGMENT_LEN - (packetLen & (MMWDEMO_OUTPUT_MSG_SEGMENT_LEN-1));
if (numPaddingBytes < MMWDEMO_OUTPUT_MSG_SEGMENT_LEN)
{
    UART_writePolling (uartHandle, (uint8_t*)padding, numPaddingBytes);
}
```

So the trailing bytes are whatever that stack frame held. TI constrains the
packet *length* only — the SDK documents the macro as "output packet length is a
multiple of this value, must be power of 2" — and says nothing anywhere about
the padding *content*. Nothing here inspects those bytes, and nothing should: an
earlier version of this parser required them to be zero and rejected **71%** of
frames from a live IWR6843, whose padding was recognisable stack debris
(`44 d6 00 08 …`, addresses in the `0x0800xxxx` range).

Out-of-Box parsing supports:

- TLV 1: `x`, `y`, `z`, and radial velocity as four `f32` values per point.
- TLV 2: unsigned Q9 log2 magnitudes for every stationary-scene range bin.
- TLV 6: inter-frame processing/UART timing, processing margins, and CPU load.
- TLV 7: signed SNR and noise values in 0.1 dB units.
- TLV 9: the RadarSS report status/time and all RX, TX, power-management, and
  digital-core temperature sensors in degrees Celsius.
- Forward-compatible skipping of other TLVs, and packet padding of any content
  (see below).

With `vital-signs`, parsing additionally supports:

- TLV 1020: five `f32` compression units followed by 8-byte spherical points.
  Elevation and azimuth are signed 8-bit values, Doppler is signed 16-bit, and
  range/SNR are unsigned 16-bit. Values are scaled and converted into `x`
  lateral, `y` outward, and `z` vertical Cartesian points.
- TLV `0x410`: one exact 136-byte `VitalSignsReading`. Multiple records in a
  packet are retained. Its fields are subject ID, range bin, breathing
  deviation, explicit HR, explicit respiration rate, 15 heart display samples,
  and 15 breath display samples.

The waveform arrays in `0x410` are unitless internal display parameters.
Do not recalculate HR or respiration from them; use the explicit BPM fields.
See TI's [header clarification](https://e2e.ti.com/support/sensors-group/sensors/f/sensors-forum/1238279/iwr6843aop-the-tlv-header-and-frame-definitions-are-mismatch-in-sdk-document-vital-sign-and-people-counting-example-c)
and [vital TLV definition](https://e2e.ti.com/support/sensors-group/sensors/f/sensors-forum/1333066/iwr6843levm-problems-about-getting-data-from-serial-port).

Complete captured packets can also be decoded without opening a port:

```rust,no_run
use snf_radar::{parse_frame_for, RadarProtocol};

# fn example(packet: &[u8]) -> Result<(), snf_radar::ParseError> {
let frame = parse_frame_for(RadarProtocol::OutOfBox, packet)?;
println!("{} dots", frame.points.len());
# Ok(())
# }
```

## Indicator behavior

`IndicatorEngine::update` accepts the host receipt `Instant` so its windows do
not assume a fixed frame rate.

Gross activity:

- Rejects non-finite, out-of-ROI, and optionally low-SNR dots.
- Reports mean squared radial speed (`motion_energy_mps2`), RMS radial speed,
  moving-point fraction, contributing point count, and point-support
  confidence.
- Maintains time-weighted 10-second and 60-second EWMAs.
- Reports rising/falling only when their relative difference exceeds 15%.

Vital rates:

- Locks to `target_subject_id`, or to the first available ID when not set.
- Rejects non-finite or implausible vendor values.
- Uses a five-second rolling median.
- Reports `WarmingUp` for 20 seconds after acquisition.
- Resets on a subject change or a gap longer than two seconds.
- Reports `MotionContaminated` above the configured activity threshold while
  retaining both raw and stabilized BPM for diagnostics.

All thresholds and windows are fields of `IndicatorConfig`. Confidence describes
input support and short-term numerical stability; it is not diagnostic or
clinical confidence.

`snf_bridge::FeatureExtractor` combines the stabilized rates with personal
baselines and the point-cloud activity snapshot:

```text
heart_slowdown              <- heart rate relative to personal baseline
respiration_slowdown        <- respiration relative to personal baseline
motion_quietness            <- activity.rms_radial_speed_mps
moving_point_quietness      <- activity.moving_point_fraction
recent_motion_drop          <- short_term_energy / long_term_energy
cardiorespiratory_agreement <- heart_slowdown * respiration_slowdown
```

Only construct a fatigue sample when the required vital estimates are
`VitalStatus::Valid`; decide explicitly whether motion-contaminated samples are
acceptable for a particular model.

## Writing another parser or indicator

To add a TLV:

1. Register its numeric type in `parser.rs` under the appropriate protocol.
2. Validate the exact payload size before every indexed read. For
   variable-length payloads, use checked/saturating size arithmetic and reject
   incomplete trailing records.
3. Decode signedness and little-endian units exactly as emitted; keep raw vendor
   measurements separate from policy or smoothing.
4. Convert coordinates into the documented `RadarPoint` axes at the parser
   boundary.
5. Preserve unknown TLVs so newer firmware stays observable.
6. Add a byte-level fixture covering every field offset, signed extremes,
   malformed length, truncation, and protocol mismatch.

To add an indicator:

1. Put stateless physical measurements in its output and time-dependent state in
   `IndicatorEngine`.
2. Base windows on the supplied `Instant`, not frame count.
3. Make ROI, thresholds, validity ranges, and reset gaps configurable.
4. Expose unavailable, warming, invalid, and contaminated states instead of
   substituting zero.
5. Test empty input, non-finite values, timing gaps, state reset, rising/falling
   transitions, and deterministic synthetic signals.
6. Document whether the value is measured, vendor-computed, normalized, or
   heuristic and specify its units.

## Field acceptance checklist

- Start-up sends the profile and logs the command count plus `sensorStart`'s
  `Debug: Init Calibration Status = …`. No frames and no such line means the
  sensor is still idle — check the CLI port before suspecting the data port.
- With factory firmware, dots and activity are present; vital outputs are absent.
- With the ISK vital firmware and matching configuration, compressed dots and
  `0x410` records are present.
- HR and respiration initially report `WarmingUp`, then become `Valid` only
  after stable, plausible input.
- Subject loss, a long gap, or a new subject restarts warm-up.
- Deliberate gross movement produces `MotionContaminated`.
- Captured UART bytes can be replayed through `parse_frame_for` for diagnosis.

For best vital results, face the chest toward the radar, remain within the
firmware configuration's range and field of view, and minimize gross motion
during acquisition. The outputs are research indicators and are not medical
measurements or diagnoses.
