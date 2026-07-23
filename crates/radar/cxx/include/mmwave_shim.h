// SPDX-License-Identifier: Apache-2.0
//
// C++ shim declaration for the cxx bridge in `src/ffi.rs`. The shared
// `MmwaveFrame` struct and the `rust::Slice` type are provided by the
// cxx-generated header, included from the .cpp translation unit.

#pragma once

#include "snf-radar/src/ffi.rs.h"

// Decode a validated IWR6843 frame buffer into a POD `MmwaveFrame`.
//
// Scaffold: the body currently returns zeroed fields. The real implementation
// forwards `frame` to the TI mmWave SDK TLV parser (linked via MMWAVE_SDK_PATH).
MmwaveFrame parse_mmwave_frame(rust::Slice<const uint8_t> frame);
