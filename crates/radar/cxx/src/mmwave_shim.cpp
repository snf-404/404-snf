// SPDX-License-Identifier: Apache-2.0
//
// C++ shim implementation for the cxx bridge. Scaffold only: returns a zeroed
// frame. The real body hands `frame` to the TI mmWave SDK's TLV parser (the
// components catalogued in references/mmwave_sdk_software_manifest.html) and
// maps the decoded detection/vitals output onto `MmwaveFrame`.

#include "mmwave_shim.h"

MmwaveFrame parse_mmwave_frame(rust::Slice<const uint8_t> frame) {
    (void)frame;

    MmwaveFrame out{};
    out.frame_number = 0;
    out.num_detected_points = 0;
    out.breathing_rate_bpm = 0.0f;
    out.heart_rate_bpm = 0.0f;
    return out;
}
