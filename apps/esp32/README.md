# ESP32-C5 CSI breathing and motion sensing

Pure on-device Wi-Fi CSI sensing for one ESP32-C5. The board joins a 2.4 GHz
network and pings its DHCP default gateway at 100 Hz. ICMP Echo Replies from the
router provide a steady CSI stream; motion and breathing results are computed on
the C5 and printed over serial. There is no second ESP32, MQTT, backend, or
frontend.

## Signal pipeline

- An ESP-IDF ping session sends continuous ICMP Echo Requests to the router.
- CSI callback copies the first HT20 64-subcarrier block from received replies
  into a FreeRTOS queue.
  This also handles the 256-byte STBC packets seen on ESP32-C5 by ignoring the
  second training block.
- The first 4 seconds select 12 stable, non-adjacent data subcarriers.
- The next 6 seconds establish a quiet-room motion baseline. Keep the room still
  for the complete 10-second calibration.
- Motion uses Hampel outlier removal, per-packet spatial coefficient of
  variation, a 100-packet moving variance, a P95-derived threshold, and
  three-hit state debounce.
- Breathing uses all 40 non-guard/non-DC subcarriers independently from motion
  selection. Samples are timestamp-resampled to 10 Hz, evaluated over a
  25.6-second Hamming window, and combined by spectral prominence in the
  0.08-0.60 Hz range (4.8-36 bpm).
- Breathing is withheld during motion and for three seconds afterward. A
  peak-to-band-mean ratio below 6 is reported as `waiting`, not as a rate.

## Hardware layout

Only one ESP32-C5 and a normal Wi-Fi router are required. The firmware forces
2.4 GHz, 802.11n, and HT20/channel width 20 MHz. The router must answer ICMP Echo
Requests from LAN clients; most home routers do by default. Place the person in
the propagation path between the router and C5. A direct or strong reflected
path gives the best breathing result.

## Configure and flash

Use ESP-IDF 5.5.x with ESP32-C5 support. The project defaults to the `esp32c5`
target.

Configure and build:

```powershell
idf.py menuconfig
idf.py build flash monitor
```

Set these values under `ESP32-C5 CSI sensing`:

- `2.4 GHz Wi-Fi SSID` and `Wi-Fi password`
- `Router ping rate`: normally leave at 100 Hz
- `ICMP payload size`: normally leave at 64 bytes

The sensor prints one status line per second:

```text
motion=false score=0.00001234 breath=valid bpm=15.6 confidence=0.84 rssi=-51 frames=4200 drops=0 ping_replies=4200 ping_timeouts=0
```

The first breathing result appears after 10 seconds of calibration plus a full
25.6-second breathing window. `drops` should remain near zero. A high count
means the receiver is getting more traffic than the processing task can drain.
`ping_timeouts` increasing continuously means the router is not answering ICMP,
the link is unstable, or the configured ping rate is too high.

## Validation

Run the deterministic host tests:

```powershell
cmake -G Ninja -B tests/host/build -S tests/host
ninja -C tests/host/build
ctest --test-dir tests/host/build --output-on-failure
```

Motion is also tested against the public ESP32-C5 recordings from
[`francescopace/espectre`](https://github.com/francescopace/espectre), commit
`29e457a0cf4251d681905f0df60832988f2f7559` (GPL-3.0 dataset). The four paired
C5 baseline/movement recordings produce 100% recall and 0% baseline false
positives with this implementation. Its long C5 recording produces 100% recall;
the apparent 9.96% pre-label positives are one continuous run immediately before
the annotated motion boundary, consistent with ESPectre's published result for
that recording.

```powershell
python tools/validate_espectre_dataset.py path/to/espectre/micro-espectre/data
```

Breathing is checked with approximately 107 pps paced-breathing recordings from
[`marcrubii/pulse-wifi-sensing`](https://github.com/marcrubii/pulse-wifi-sensing),
commit `152fecbc25df9c192cc2899b920222609748e412` (MIT). The 6 bpm recording is
estimated at 6.00 bpm; its stable tail is valid for 75.5% of frames, while the
empty-room negative control has 0% valid breathing frames.

```powershell
python tools/validate_breathing_dataset.py path/to/pulse-wifi-sensing/data/breathing
```

Datasets are not copied into this repository. The validators compile and call
the same C implementation used by the firmware.

## Limits

This is a single-link, single-person detector. It does not identify people,
count occupants, distinguish pets, or estimate breathing during gross motion.
Room layout, AP rate control, interference, and sensor placement materially
affect CSI. Reboot and recalibrate after moving the AP, sensor, furniture, or
other large reflectors. Do not treat the output as a medical measurement or an
apnea alarm without a separate clinical validation process.
